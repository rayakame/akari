use std::ops::Range;

use super::memo::{Brackets, Frame, UrlPart};
use super::phishing::special_whitespace;
use super::{Content, Ctx, Input, LineStart, Match, Node, Parser, literal};
use crate::document::LinkKind;

// A link's text is checked at this depth or deeper, so checks nest at most 8 deep and so do
// styles inside one; each check parses its text again (D73).
const CHECK_DEPTH: u8 = super::MAX_DEPTH - 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkData {
    pub(crate) url: String,
    pub(crate) kind: LinkKind,
    pub(crate) suppress_embed: bool,
}

fn valid_url(text: &str) -> Option<String> {
    let url = url::Url::parse(text).ok()?;
    matches!(url.scheme(), "http" | "https" | "discord").then(|| url.as_str().to_owned())
}

fn is_space(c: char) -> bool {
    c.is_whitespace()
}

fn whitespace_end(text: &str, from: usize) -> usize {
    from + text[from..]
        .chars()
        .take_while(|&c| is_space(c))
        .map(char::len_utf8)
        .sum::<usize>()
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
            && !(next.is_ascii_alphanumeric() || next.is_whitespace())
        {
            out.push(next);
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

impl Parser {
    fn link(
        &mut self,
        input: &Input,
        at: usize,
        display: Range<usize>,
        end: usize,
        data: LinkData,
    ) -> Match {
        self.steps += (end - at) as u64;
        let mut shown = Vec::new();
        input.pieces(display, |span| shown.push(span));
        Match::new(
            Node::Link {
                data,
                shown,
                children: Vec::new(),
                source: input.span(at..end),
            },
            end,
        )
    }

    // (?:https?|steam)://[^\s<]+[^<.,:;"'\]\s], minus one ")" when they outnumber "(".
    pub(super) fn plain_url(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let rest = &text[at..];
        let scheme = ["https://", "http://", "steam://"]
            .into_iter()
            .find(|s| rest.starts_with(s))?;
        let after = at + scheme.len();
        let run_end = after
            + text[after..]
                .chars()
                .take_while(|&c| !is_space(c) && c != '<')
                .map(char::len_utf8)
                .sum::<usize>();
        self.steps += (run_end - at) as u64;
        let mut end = run_end;
        while let Some(last) = text[after..end].chars().next_back()
            && matches!(last, '.' | ',' | ':' | ';' | '"' | '\'' | ']')
        {
            end -= last.len_utf8();
        }
        if text[after..end].chars().count() < 2 {
            return None;
        }
        let url = &text[at..end];
        let closing = url.len() - url.trim_end_matches(')').len();
        if closing > url.matches('(').count() {
            end -= 1;
        }
        let url = &text[at..end];
        // While a masked link's text is checked, a URL whose host rust-url refuses but other
        // parsers take (IDNA) still counts as a URL, so the text can't pass for an address.
        let refused_host =
            || ctx.testing_link && matches!(url::Url::parse(url), Err(url::ParseError::IdnaError));
        Some(
            match valid_url(url).or_else(|| refused_host().then(|| url.to_owned())) {
                Some(target) => self.link(
                    input,
                    at,
                    at..end,
                    end,
                    LinkData {
                        url: target,
                        kind: LinkKind::Plain,
                        suppress_embed: false,
                    },
                ),
                None => Match::new(literal(input, at..end), end),
            },
        )
    }

    // <([^: >]+:/[^ >]+)>; without parse-discord's parenthesis trim, which loses a character.
    pub(super) fn angle_link(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        frame: &mut Frame,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        let start = at + 1;
        let stop = frame.next_colon_space_or_gt(bytes, start, &mut self.steps);
        if stop == start || !text[stop..].starts_with(":/") {
            return None;
        }
        let rest = stop + 2;
        let close = frame.next_space_or_gt(bytes, rest, &mut self.steps);
        if close == rest || bytes.get(close) != Some(&b'>') {
            return None;
        }
        let end = close + 1;
        Some(match valid_url(&text[start..close]) {
            Some(url) => self.link(
                input,
                at,
                start..close,
                end,
                LinkData {
                    url,
                    kind: LinkKind::Plain,
                    suppress_embed: true,
                },
            ),
            None => Match::new(literal(input, start..close), end),
        })
    }

    // docs.discord.food gives only examples; the grammar beyond them is Akari's.
    pub(super) fn email(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        frame: &mut Frame,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let start = at + 1;
        let address = text[start..]
            .strip_prefix("mailto:")
            .map_or(start, |_| start + 7);
        let bytes = text.as_bytes();
        let local = bytes[address..]
            .iter()
            .take_while(|&&b| b.is_ascii_alphanumeric() || b"!#$%&'*+/=^_`{|}~.-".contains(&b))
            .count();
        let at_sign = address + local;
        if local == 0 || bytes.get(at_sign) != Some(&b'@') {
            return None;
        }
        let domain = at_sign + 1;
        let domain_len = bytes[domain..]
            .iter()
            .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            .count();
        let labels = &text[domain..domain + domain_len];
        if labels.split('.').count() < 2 || labels.split('.').any(str::is_empty) {
            return None;
        }
        let mut close = domain + domain_len;
        self.steps += (close - at) as u64;
        if bytes.get(close) == Some(&b'?') {
            close = frame.next_gt_or_space(bytes, close, &mut self.steps);
        }
        if bytes.get(close) != Some(&b'>') {
            return None;
        }
        let url = url::Url::parse(&format!("mailto:{}", &text[address..close])).ok()?;
        Some(self.link(
            input,
            at,
            start..close,
            close + 1,
            LinkData {
                url: url.as_str().to_owned(),
                kind: LinkKind::Email,
                suppress_embed: false,
            },
        ))
    }

    // docs.discord.food gives only examples; the grammar beyond them is Akari's.
    pub(super) fn phone(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let start = at + 1;
        let (scheme, number) = if text[start..].starts_with("tel:") {
            ("tel:", start + 4)
        } else if text[start..].starts_with("sms:") {
            ("sms:", start + 4)
        } else {
            ("tel:", start)
        };
        let bytes = text.as_bytes();
        let plus = bytes.get(number) == Some(&b'+');
        if !plus && number == start {
            return None;
        }
        let body = number + usize::from(plus);
        let len = bytes[body..]
            .iter()
            .take_while(|&&b| b.is_ascii_digit() || b" ()-.".contains(&b))
            .count();
        let close = body + len;
        self.steps += (close - at) as u64;
        let digits: String = text[body..close]
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        if bytes.get(close) != Some(&b'>') || !(3..=15).contains(&digits.len()) {
            return None;
        }
        let url = format!("{scheme}{}{digits}", if plus { "+" } else { "" });
        Some(self.link(
            input,
            at,
            start..close,
            close + 1,
            LinkData {
                url,
                kind: LinkKind::Phone,
                suppress_embed: false,
            },
        ))
    }

    pub(super) fn masked_link(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        let (close, part) = self.link_parts(text, at, range, frame)?;
        let url = valid_url(&unescape(&text[part.url.clone()]));
        let check = Ctx {
            testing_link: true,
            trailing_newline: false,
            depth: ctx.depth.max(CHECK_DEPTH) + 1,
            ..self.inner(input, at, range, ctx, false)
        };
        let allowed = url.is_some()
            && self.admissible(&text[at + 1..close], check, false) == Some(true)
            && part
                .title
                .clone()
                .filter(|t| !t.is_empty())
                .is_none_or(|t| self.admissible(&text[t], check, true).is_some());
        Some(self.masked_match(input, at..close, part, url.filter(|_| allowed), ctx))
    }

    fn link_parts(
        &mut self,
        text: &str,
        at: usize,
        range: &Range<usize>,
        frame: &mut Frame,
    ) -> Option<(usize, UrlPart)> {
        let brackets = frame
            .brackets
            .get_or_insert_with(|| Brackets::new(text.as_bytes(), range.clone(), &mut self.steps));
        let text_end = brackets.text_end(at + 1, &mut self.steps);
        match frame.links_from.get(&text_end) {
            Some(found) => found.clone(),
            None => {
                let found = self.link_close(text, text_end, frame);
                frame.links_from.insert(text_end, found.clone());
                found
            }
        }
    }

    // Apart from masked_link, whose frame is on the stack while its text is checked.
    fn masked_match(
        &mut self,
        input: &Input,
        open_close: Range<usize>,
        part: UrlPart,
        url: Option<String>,
        ctx: Ctx,
    ) -> Match {
        let (at, end) = (open_close.start, part.end);
        let Some(url) = url else {
            return Match::new(literal(input, at..end), end);
        };
        let text = &input.text;
        let shown = clean_ranges(text, at + 1..open_close.end);
        let shown = input.derive(&shown);
        let display = Ctx {
            line_start: LineStart::Nothing,
            trailing_newline: false,
            quote: false,
            list_depth: 0,
            testing_link: false,
            link_text: true,
            depth: ctx.depth + 1,
        };
        self.steps += shown.text.len() as u64;
        let display = Content {
            range: 0..shown.text.len(),
            input: Some(shown),
            ctx: display,
        };
        let title = part.title.map(|t| {
            text[t]
                .chars()
                .filter(|&c| !special_whitespace(c))
                .collect::<String>()
                .into_boxed_str()
        });
        Match {
            node: Node::Link {
                data: LinkData {
                    url,
                    kind: LinkKind::Masked { title },
                    suppress_embed: part.suppress,
                },
                shown: Vec::new(),
                children: Vec::new(),
                source: input.span(at..end),
            },
            next: end,
            nested: vec![display],
        }
    }

    // The regex's backtracking order: the last "](" before the next "[" whose URL part matches.
    fn link_close(
        &mut self,
        text: &str,
        text_end: usize,
        frame: &mut Frame,
    ) -> Option<(usize, UrlPart)> {
        let bytes = text.as_bytes();
        let brackets = frame.brackets.as_ref()?;
        let limit = brackets.next_open(text_end).unwrap_or(bytes.len());
        let candidates: Vec<usize> = brackets
            .closes_in(text_end..limit)
            .filter(|&c| bytes.get(c + 1) == Some(&b'('))
            .collect();
        for close in candidates.into_iter().rev() {
            self.steps += 1;
            if let Some(part) = self.url_part(text, close + 2, frame) {
                return Some((close, part));
            }
        }
        None
    }

    // \(\s*(<)?((?:\([^)]*\)|[^\s\\]|\\[^\n])*?)(>)?(?:\s+['"](.*?)['"])?\s*\)
    fn url_part(&mut self, text: &str, from: usize, frame: &mut Frame) -> Option<UrlPart> {
        let start = whitespace_end(text, from);
        self.steps += (start - from) as u64;
        if text[start..].starts_with('<')
            && let Some(part) = self.url_units(text, start + 1, true, frame)
        {
            return Some(part);
        }
        self.url_units(text, start, false, frame)
    }

    // The lazy URL in the regex's order; boundaries from which nothing matched are remembered.
    fn url_units(
        &mut self,
        text: &str,
        start: usize,
        angle: bool,
        frame: &mut Frame,
    ) -> Option<UrlPart> {
        if frame.url_failed.contains(&start) {
            return None;
        }
        let bytes = text.as_bytes();
        // A boundary and the next alternative to try there; 0: the rest wasn't tried yet.
        let mut stack: Vec<(usize, u8)> = vec![(start, 0)];
        while let Some(&mut (pos, ref mut next_alternative)) = stack.last_mut() {
            self.steps += 1;
            if *next_alternative == 0 {
                *next_alternative = 1;
                if let Some((suppress, title, end)) = self.url_finish(text, pos, frame) {
                    return Some(UrlPart {
                        url: start..pos,
                        suppress: suppress || angle,
                        title,
                        end,
                    });
                }
                continue;
            }
            let alternative = *next_alternative;
            *next_alternative += 1;
            let c = text[pos..].chars().next();
            let next = match (alternative, c) {
                // \([^)]*\)
                (1, Some('(')) => frame
                    .next_close_paren(bytes, pos + 1, &mut self.steps)
                    .map(|close| close + 1),
                // [^\s\\]
                (2, Some(c)) if c != '\\' && !is_space(c) => Some(pos + c.len_utf8()),
                // \\[^\n]
                (3, Some('\\')) => text[pos + 1..]
                    .chars()
                    .next()
                    .filter(|&c| c != '\n')
                    .map(|c| pos + 1 + c.len_utf8()),
                (1..=3, _) => None,
                _ => {
                    frame.url_failed.insert(pos);
                    stack.pop();
                    continue;
                }
            };
            if let Some(next) = next
                && !frame.url_failed.contains(&next)
            {
                stack.push((next, 0));
            }
        }
        None
    }

    // (>)?(?:\s+['"](.*?)['"])?\s*\)
    fn url_finish(
        &mut self,
        text: &str,
        pos: usize,
        frame: &mut Frame,
    ) -> Option<(bool, Option<Range<usize>>, usize)> {
        let bytes = text.as_bytes();
        let gt = bytes.get(pos) == Some(&b'>');
        let after = pos + usize::from(gt);
        let spaces = whitespace_end(text, after);
        self.steps += (spaces - after) as u64 + 1;
        if spaces > after
            && matches!(bytes.get(spaces), Some(b'\'' | b'"'))
            && let Some((close, end)) = frame.title_close(bytes, text, spaces + 1, &mut self.steps)
        {
            return Some((gt, Some(spaces + 1..close), end));
        }
        (bytes.get(spaces) == Some(&b')')).then_some((gt, None, spaces + 1))
    }
}

fn clean_ranges(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut from = range.start;
    for (i, c) in text[range.clone()].char_indices() {
        if special_whitespace(c) {
            let at = range.start + i;
            if at > from {
                out.push(from..at);
            }
            from = at + c.len_utf8();
        }
    }
    if from < range.end || out.is_empty() {
        out.push(from..range.end);
    }
    out
}
