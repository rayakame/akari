use std::ops::Range;

use super::{Content, Ctx, Input, LineStart, Match, Node, Parser};
use crate::document::HeadingLevel;
use crate::emoji::py_space;

// Discord's limit; the next level's bullet stays text (R§6).
const MAX_LIST_DEPTH: u8 = 11;
const MAX_LIST_NUMBER: u64 = 1_000_000_000;

fn trim_end(text: &str, range: Range<usize>, strip: impl Fn(char) -> bool) -> Range<usize> {
    let kept = text[range.clone()].trim_end_matches(strip).len();
    range.start..range.start + kept
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    from + bytes[from..]
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(bytes.len() - from)
}

impl Parser {
    // The regexes' ^.
    fn at_line_start(bytes: &[u8], at: usize, range: &Range<usize>, ctx: Ctx) -> bool {
        if at == range.start {
            ctx.line_start == LineStart::Nothing
        } else {
            bytes[at - 1] == b'\n'
        }
    }

    // The regexes' (?<=^ *).
    fn after_indent(&mut self, bytes: &[u8], at: usize, range: &Range<usize>, ctx: Ctx) -> bool {
        let mut i = at;
        while i > range.start {
            i -= 1;
            self.steps += 1;
            match bytes[i] {
                b' ' => {}
                b'\n' => return true,
                _ => return false,
            }
        }
        ctx.line_start != LineStart::Text
    }

    // ^-# +(?!-#)([^\n]+)\n?
    pub(super) fn subtext(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        if !text[at..].starts_with("-#") || !Self::at_line_start(bytes, at, range, ctx) {
            return None;
        }
        let spaces = bytes[at + 2..].iter().take_while(|&&b| b == b' ').count();
        if spaces == 0 {
            return None;
        }
        let mut start = at + 2 + spaces;
        if text[start..].starts_with("-#") || bytes.get(start).is_none_or(|&b| b == b'\n') {
            if spaces < 2 {
                return None;
            }
            start -= 1;
        }
        let end = line_end(bytes, start);
        self.steps += (end - at) as u64;
        let next = end + usize::from(end < bytes.len());
        let content = trim_end(text, start..end, py_space);
        let inner = self.inner(input, at, range, ctx, next > end);
        Some(Match {
            node: Node::Subtext {
                children: Vec::new(),
                source: input.span(at..next),
            },
            next,
            nested: vec![Content::new(content, inner)],
        })
    }

    // ^(#{1,3})\s+(?!#)([^\n]*)\s* (parse-discord's verbose regex makes its #*\s*\n? a comment).
    pub(super) fn heading(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        if ctx.list_depth > 0 || !Self::at_line_start(bytes, at, range, ctx) {
            return None;
        }
        let hashes = bytes[at..].iter().take_while(|&&b| b == b'#').count();
        let level = match hashes {
            1 => HeadingLevel::One,
            2 => HeadingLevel::Two,
            3 => HeadingLevel::Three,
            _ => return None,
        };
        let space_start = at + hashes;
        let space_end = space_start
            + text[space_start..]
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
        self.steps += (space_end - at) as u64;
        if space_end == space_start {
            return None;
        }
        let mut start = space_end;
        if bytes.get(start) == Some(&b'#') {
            let last = text[space_start..space_end]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            if last == 0 {
                return None;
            }
            start = space_start + last;
        }
        let end = line_end(bytes, start);
        let next = end
            + text[end..]
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
        self.steps += (next - start) as u64;
        let title = trim_end(text, start..end, py_space);
        let title = trim_end(text, title, |c| c == '#');
        let title = trim_end(text, title, py_space);
        let inner = self.inner(input, at, range, ctx, text[..next].ends_with('\n'));
        Some(Match {
            node: Node::Heading {
                level,
                children: Vec::new(),
                source: input.span(at..next),
            },
            next,
            nested: vec![Content::new(title, inner)],
        })
    }

    // (?:(?<=^ *)> ([^\n]*)\n?)+ or (?<=^ *)>>> (.*)
    pub(super) fn quote(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        if ctx.quote || !self.after_indent(bytes, at, range, ctx) {
            return None;
        }
        let (lines, next) = if text[at..].starts_with("> ") {
            let mut lines = Vec::new();
            let mut pos = at;
            while text[pos..].starts_with("> ") {
                let end = line_end(bytes, pos + 2);
                self.steps += (end - pos) as u64;
                let newline = usize::from(end < bytes.len());
                lines.push(pos + 2..end + newline);
                pos = end + newline;
            }
            if let Some(last) = lines.last_mut()
                && text[last.clone()].ends_with('\n')
            {
                last.end -= 1;
            }
            (lines, pos)
        } else if text[at..].starts_with(">>> ") {
            (std::iter::once(at + 4..range.end).collect(), range.end)
        } else {
            return None;
        };
        let inner = Ctx {
            quote: true,
            ..self.inner(input, at, range, ctx, text[..next].ends_with('\n'))
        };
        let content = self.content(input, &lines, inner);
        Some(Match {
            node: Node::Quote {
                children: Vec::new(),
                source: input.span(at..next),
            },
            next,
            nested: vec![content],
        })
    }

    // Discord's November 2025 lists (R§6): continuation lines need two spaces.
    pub(super) fn list(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        if ctx.list_depth >= MAX_LIST_DEPTH
            || !self.after_indent(bytes, at, range, ctx)
            || bullet(bytes, at, &mut self.steps).is_none()
        {
            return None;
        }
        let mut items = Vec::new();
        let mut first_number = None;
        let mut pos = at;
        while let Some(width) = bullet(bytes, pos, &mut self.steps) {
            if first_number.is_none() {
                first_number = Some(number(bytes, pos));
            }
            let start = pos + width;
            let mut end = start;
            if bytes.get(end) == Some(&b'\n') {
                let after = end + 1 + usize::from(bytes.get(end + 1) == Some(&b' '));
                if bullet(bytes, after, &mut self.steps).is_none() {
                    end += 1;
                }
            }
            end = line_end(bytes, end);
            while bytes.get(end) == Some(&b'\n') && bytes.get(end + 1..end + 3) == Some(b"  ") {
                end = line_end(bytes, end + 1);
            }
            self.steps += (end - pos) as u64 + 1;
            items.push((width, start..end));
            pos = end;
            if bytes.get(pos) == Some(&b'\n') {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b' ') {
                pos += 1;
            }
        }
        let next = pos;
        let inner = Ctx {
            list_depth: ctx.list_depth + 1,
            ..self.inner(input, at, range, ctx, text[..next].ends_with('\n'))
        };
        let mut parsed = Vec::with_capacity(items.len());
        let mut nested = Vec::with_capacity(items.len());
        for (width, item) in items {
            let lines = dedent(bytes, item.clone(), width);
            parsed.push((Vec::new(), input.span(item)));
            nested.push(self.content(input, &lines, inner));
        }
        Some(Match {
            node: Node::List {
                start: first_number
                    .flatten()
                    .map(|n| n.clamp(1, MAX_LIST_NUMBER) as u32),
                items: parsed,
                source: input.span(at..next),
            },
            next,
            nested,
        })
    }

    fn content(&mut self, input: &Input, pieces: &[Range<usize>], ctx: Ctx) -> Content {
        match pieces {
            [one] => Content::new(one.clone(), ctx),
            _ => {
                let joined = input.derive(pieces);
                self.steps += joined.text.len() as u64;
                Content {
                    range: 0..joined.text.len(),
                    input: Some(joined),
                    ctx,
                }
            }
        }
    }
}

fn bullet(bytes: &[u8], at: usize, steps: &mut u64) -> Option<usize> {
    let marker = match bytes.get(at)? {
        b'-' | b'*' => 1,
        b'0'..=b'9' => {
            let digits = bytes[at..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            *steps += digits as u64;
            if bytes.get(at + digits) != Some(&b'.') {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let spaces = bytes[at + marker..]
        .iter()
        .take_while(|&&b| b == b' ')
        .count();
    (spaces > 0).then_some(marker + spaces)
}

fn number(bytes: &[u8], at: usize) -> Option<u64> {
    let digits = bytes[at..].iter().take_while(|b| b.is_ascii_digit());
    let mut n: Option<u64> = None;
    for &d in digits {
        n = Some(
            n.unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(u64::from(d - b'0')),
        );
    }
    n
}

// parse-discord removes ^ {1,width} from each line before parsing the item.
fn dedent(bytes: &[u8], item: Range<usize>, width: usize) -> Vec<Range<usize>> {
    let mut pieces: Vec<Range<usize>> = Vec::new();
    let mut line = item.start;
    loop {
        let indent = bytes[line..item.end]
            .iter()
            .take(width)
            .take_while(|&&b| b == b' ')
            .count();
        let end = bytes[line..item.end]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(item.end, |i| line + i + 1);
        let start = line + indent;
        match pieces.last_mut() {
            Some(last) if indent == 0 && last.end == start => last.end = end,
            _ => pieces.push(start..end),
        }
        if end >= item.end {
            break;
        }
        line = end;
    }
    pieces
}
