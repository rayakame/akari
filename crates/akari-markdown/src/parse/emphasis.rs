use std::ops::Range;

use super::memo::{Doomed, Frame};
use super::{Content, Ctx, Input, Match, Node, Parser, char_len};
use crate::document::Style;

fn is_space(c: char) -> bool {
    c.is_whitespace()
}

impl Parser {
    // Bold wins when it starts here too and is longer (parse-discord's get_match).
    pub(super) fn star(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let italic = self.italic_star(input, at, range, frame);
        let bold =
            |parser: &mut Parser, frame: &mut Frame| parser.double(input, at, range, b'*', frame);
        let (style, content, end) = match italic {
            Some((content, end)) => match bold(self, frame) {
                Some((b, b_end)) if b_end > end => (Style::BOLD, b, b_end),
                _ => (Style::ITALIC, content, end),
            },
            None => {
                let (content, end) = bold(self, frame)?;
                (Style::BOLD, content, end)
            }
        };
        Some(self.style(input, range, ctx, at, (style, content, end)))
    }

    // Underline wins when it starts here too and is longer (parse-discord's get_match).
    pub(super) fn underscore(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let italic = self.italic_underscore(input, at, range, frame);
        let underline =
            |parser: &mut Parser, frame: &mut Frame| parser.double(input, at, range, b'_', frame);
        let (style, content, end) = match italic {
            Some((content, end)) => match underline(self, frame) {
                Some((u, u_end)) if u_end > end => (Style::UNDERLINE, u, u_end),
                _ => (Style::ITALIC, content, end),
            },
            None => {
                let (content, end) = underline(self, frame)?;
                (Style::UNDERLINE, content, end)
            }
        };
        Some(self.style(input, range, ctx, at, (style, content, end)))
    }

    // ~~(.+?)~~(?!_)
    pub(super) fn strike(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let (content, end) = self.any_until(
            input,
            at,
            range,
            "~~",
            Some(b'_'),
            &mut frame.no_strike_from,
        )?;
        Some(self.style(input, range, ctx, at, (Style::STRIKE, content, end)))
    }

    // \|\|(.+?)\|\|
    pub(super) fn spoiler(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let (content, end) =
            self.any_until(input, at, range, "||", None, &mut frame.no_spoiler_from)?;
        let inner = self.inner(input, at, range, ctx, false);
        Some(Match {
            node: Node::Spoiler {
                children: Vec::new(),
                source: input.span(at..end),
            },
            next: end,
            nested: vec![Content::new(content, inner)],
        })
    }

    fn style(
        &mut self,
        input: &Input,
        range: &Range<usize>,
        ctx: Ctx,
        at: usize,
        (style, content, end): (Style, Range<usize>, usize),
    ) -> Match {
        let inner = self.inner(input, at, range, ctx, false);
        Match {
            node: Node::Style {
                style,
                children: Vec::new(),
                source: input.span(at..end),
            },
            next: end,
            nested: vec![Content::new(content, inner)],
        }
    }

    // \*(?!\s)((?:\\.|\*\*|[^*\\])+?)(?<!\s) {0,2}\*(?!\*)
    fn italic_star(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        frame: &mut Frame,
    ) -> Option<(Range<usize>, usize)> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        let start = at + 1;
        if text[start..].chars().next().is_none_or(is_space) {
            return None;
        }
        let doomed = frame
            .doomed_star
            .get_or_insert_with(|| Doomed::new(range.clone()));
        let mut pos = start;
        loop {
            self.steps += 1;
            if pos > start {
                if doomed.is(pos) {
                    return None;
                }
                doomed.set(pos);
                let before = text[..pos].chars().next_back().unwrap_or(' ');
                if !is_space(before) {
                    let spaces = bytes[pos..]
                        .iter()
                        .take(3)
                        .take_while(|&&b| b == b' ')
                        .count();
                    let close = pos + spaces;
                    if spaces <= 2
                        && bytes.get(close) == Some(&b'*')
                        && bytes.get(close + 1) != Some(&b'*')
                    {
                        return Some((start..pos, close + 1));
                    }
                }
            }
            match bytes.get(pos)? {
                b'\\' if pos + 1 < bytes.len() => pos += 1 + char_len(text, pos + 1),
                b'*' if bytes.get(pos + 1) == Some(&b'*') => pos += 2,
                b'\\' | b'*' => return None,
                _ => pos += char_len(text, pos),
            }
        }
    }

    // _((?:\\.|__|[^_\\])+?)_(?![a-zA-Z0-9_])
    fn italic_underscore(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        frame: &mut Frame,
    ) -> Option<(Range<usize>, usize)> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        let start = at + 1;
        let doomed = frame
            .doomed_underscore
            .get_or_insert_with(|| Doomed::new(range.clone()));
        let mut pos = start;
        loop {
            self.steps += 1;
            if pos > start {
                if doomed.is(pos) {
                    return None;
                }
                doomed.set(pos);
                if bytes.get(pos) == Some(&b'_')
                    && !bytes
                        .get(pos + 1)
                        .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Some((start..pos, pos + 1));
                }
            }
            match bytes.get(pos)? {
                b'\\' if pos + 1 < bytes.len() => pos += 1 + char_len(text, pos + 1),
                b'_' if bytes.get(pos + 1) == Some(&b'_') => pos += 2,
                b'\\' | b'_' => return None,
                _ => pos += char_len(text, pos),
            }
        }
    }

    // \*\*((?:\\.|[^\\])+?)\*\*(?!\*), and the same with underscores.
    fn double(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        delimiter: u8,
        frame: &mut Frame,
    ) -> Option<(Range<usize>, usize)> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        if bytes.get(at + 1) != Some(&delimiter) {
            return None;
        }
        let start = at + 2;
        let none_from = if delimiter == b'*' {
            &mut frame.no_bold_from
        } else {
            &mut frame.no_underline_from
        };
        if none_from.is_some_and(|none| start >= none) {
            return None;
        }
        let mut pos = start;
        loop {
            self.steps += 1;
            if pos > start
                && bytes.get(pos) == Some(&delimiter)
                && bytes.get(pos + 1) == Some(&delimiter)
                && bytes.get(pos + 2) != Some(&delimiter)
            {
                return Some((start..pos, pos + 2));
            }
            match bytes.get(pos) {
                Some(b'\\') if pos + 1 < bytes.len() => pos += 1 + char_len(text, pos + 1),
                Some(b'\\') | None => break,
                Some(_) => pos += char_len(text, pos),
            }
        }
        *none_from = Some(none_from.map_or(start, |none| none.min(start)));
        None
    }

    fn any_until(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        delimiter: &str,
        not_after: Option<u8>,
        none_from: &mut Option<usize>,
    ) -> Option<(Range<usize>, usize)> {
        let text = &input.text[..range.end];
        if !text[at..].starts_with(delimiter) {
            return None;
        }
        let start = at + delimiter.len();
        let from = start + char_len(text, start);
        if from > text.len() || from == start || none_from.is_some_and(|none| from >= none) {
            return None;
        }
        let mut search = from;
        while let Some(i) = text[search..].find(delimiter) {
            let close = search + i;
            self.steps += (close - search) as u64 + 1;
            let after = close + delimiter.len();
            if not_after.is_none_or(|b| text.as_bytes().get(after) != Some(&b)) {
                return Some((start..close, after));
            }
            search = close + 1;
        }
        self.steps += (text.len() - search) as u64;
        *none_from = Some(none_from.map_or(from, |none| none.min(from)));
        None
    }

    // parse-discord's Context.update.
    pub(super) fn inner(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        ends_with_newline: bool,
    ) -> Ctx {
        use super::LineStart;
        let bytes = input.text.as_bytes();
        let mut spaces = false;
        let mut i = at;
        let line_start = loop {
            if i == range.start {
                break match ctx.line_start {
                    LineStart::Text => LineStart::Text,
                    _ if spaces => LineStart::Space,
                    other => other,
                };
            }
            i -= 1;
            self.steps += 1;
            match bytes[i] {
                b'\n' => {
                    break if spaces {
                        LineStart::Space
                    } else {
                        LineStart::Nothing
                    };
                }
                b' ' => spaces = true,
                _ => break LineStart::Text,
            }
        };
        Ctx {
            line_start,
            trailing_newline: ends_with_newline,
            depth: ctx.depth + 1,
            ..ctx
        }
    }
}
