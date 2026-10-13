use std::ops::Range;

use super::memo::{Frame, Runs};
use super::{Content, Ctx, Input, Match, Node, Parser, char_len};

const FENCE: &str = "```";

fn is_language(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.')
}

impl Parser {
    // ```(?:([A-Za-z0-9_+\-.]*)\n)?(.*?[^\n].*?)```, then newlines stripped from both ends.
    pub(super) fn code_block(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        frame: &mut Frame,
    ) -> Option<Match> {
        let text = &input.text[..range.end];
        if !text[at..].starts_with(FENCE) {
            return None;
        }
        let after = at + FENCE.len();
        let language_end = after
            + text.as_bytes()[after..]
                .iter()
                .take_while(|&&b| is_language(b))
                .count();
        self.steps += (language_end - after) as u64;
        let with_language = (text.as_bytes().get(language_end) == Some(&b'\n'))
            .then(|| {
                self.fence_after(text, language_end + 1, frame)
                    .map(|close| (language_end + 1, close))
            })
            .flatten();
        let (code_start, close, language) = match with_language {
            Some((start, close)) => (
                start,
                close,
                (language_end > after).then_some(after..language_end),
            ),
            None => {
                let close = self.fence_after(text, after, frame)?;
                (after, close, None)
            }
        };
        let code = trim_newlines(text, code_start..close);
        let mut pieces = Vec::new();
        input.pieces(code, |span| pieces.push(span));
        Some(Match::new(
            Node::CodeBlock {
                language: language.map(|l| input.span(l)),
                code: pieces,
                source: input.span(at..close + FENCE.len()),
            },
            close + FENCE.len(),
        ))
    }

    fn fence_after(&mut self, text: &str, start: usize, frame: &mut Frame) -> Option<usize> {
        let first = start
            + text.as_bytes()[start..]
                .iter()
                .take_while(|&&b| b == b'\n')
                .count();
        self.steps += (first - start) as u64;
        if first >= text.len() {
            return None;
        }
        let from = first + char_len(text, first);
        if frame.no_fence_from.is_some_and(|none| from >= none) {
            return None;
        }
        let found = text[from..].find(FENCE).map(|i| from + i);
        self.steps += (found.unwrap_or(text.len()) - from) as u64;
        if found.is_none() {
            frame.no_fence_from = Some(from);
        }
        found
    }

    // (`+)(.+?)(?<!`)\1(?!`): the regex backtracks to a shorter opening run.
    pub(super) fn inline_code(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        if at < frame.code_dead_until {
            return None;
        }
        let text = &input.text[..range.end];
        let bytes = text.as_bytes();
        let opening = bytes[at..].iter().take_while(|&&b| b == b'`').count();
        self.steps += opening as u64;
        let runs = frame
            .runs
            .get_or_insert_with(|| Runs::new(bytes, range.start, &mut self.steps));
        for len in (1..=opening).rev() {
            self.steps += 1;
            if let Some(close) = runs.find(len, at + opening) {
                let content = strip_spaces(text, at + len..close);
                if ctx.testing_link {
                    let inner = self.inner(input, at, range, ctx, false);
                    return Some(Match {
                        node: Node::TestingCode {
                            children: Vec::new(),
                        },
                        next: close + len,
                        nested: vec![Content::new(content, inner)],
                    });
                }
                let mut pieces = Vec::new();
                input.pieces(content, |span| pieces.push(span));
                return Some(Match::new(
                    Node::Code {
                        code: pieces,
                        source: input.span(at..close + len),
                    },
                    close + len,
                ));
            }
        }
        frame.code_dead_until = at + opening;
        None
    }
}

fn trim_newlines(text: &str, range: Range<usize>) -> Range<usize> {
    let s = &text[range.clone()];
    let start = range.start + (s.len() - s.trim_start_matches('\n').len());
    let end = range.end - (s.len() - s.trim_end_matches('\n').len());
    start..end.max(start)
}

// Discord drops one space at an end whose next non-space character is a backtick (R§2).
fn strip_spaces(text: &str, range: Range<usize>) -> Range<usize> {
    let s = &text[range.clone()];
    let inner = s.trim_matches(' ');
    let start = range.start + usize::from(inner.starts_with('`') && s.starts_with(' '));
    let end = range.end - usize::from(inner.ends_with('`') && s.ends_with(' '));
    start..end.max(start)
}
