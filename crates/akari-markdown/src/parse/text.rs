use std::ops::Range;

use super::{Ctx, Input, Node, Parser, Piece, char_len};

const SHRUG: &str = "¯\\_(ツ)_/¯";

impl Parser {
    /// Text between matches: escapes, the shrug, and spaces before a newline, which aren't shown
    /// (parse-discord's `advance`). `last`: the region's final text, which a newline may follow.
    pub(super) fn text(
        &mut self,
        input: &Input,
        range: Range<usize>,
        ctx: Ctx,
        last: bool,
        nodes: &mut Vec<Node>,
    ) {
        if range.is_empty() {
            return;
        }
        let text = &input.text[..];
        let bytes = text.as_bytes();
        let escapes = ctx.escapes();
        let mut pieces = Vec::new();
        let mut plain_from = range.start;
        let mut at = range.start;
        while at < range.end {
            self.steps += 1;
            match bytes[at] {
                b'\\' if escapes && at + 1 < range.end => {
                    let len = char_len(text, at + 1);
                    let escaped = text[at + 1..].chars().next().unwrap_or('a');
                    if escaped.is_ascii_alphanumeric() || escaped.is_whitespace() {
                        at += 1;
                        continue;
                    }
                    plain(input, plain_from..at, &mut pieces);
                    pieces.push(Piece::Escape {
                        display: input.span(at + 1..at + 1 + len),
                        source: input.span(at..at + 1 + len),
                    });
                    at += 1 + len;
                    plain_from = at;
                }
                b' ' => {
                    let run_end = at
                        + bytes[at..range.end]
                            .iter()
                            .take_while(|&&b| b == b' ')
                            .count();
                    if run_end < range.end && bytes[run_end] == b'\n' {
                        plain(input, plain_from..at, &mut pieces);
                        pieces.push(Piece::Break(input.span(at..run_end + 1)));
                        at = run_end + 1;
                        plain_from = at;
                    } else if run_end == range.end && last && ctx.trailing_newline {
                        plain(input, plain_from..at, &mut pieces);
                        at = run_end;
                        plain_from = at;
                    } else {
                        at = run_end;
                    }
                }
                b'\n' => {
                    let start = if at > plain_from && bytes[at - 1] == b'\r' {
                        at - 1
                    } else {
                        at
                    };
                    plain(input, plain_from..start, &mut pieces);
                    pieces.push(Piece::Break(input.span(start..at + 1)));
                    at += 1;
                    plain_from = at;
                }
                _ if escapes && text[at..range.end].starts_with(SHRUG) => at += SHRUG.len(),
                _ => at += char_len(text, at),
            }
        }
        plain(input, plain_from..range.end, &mut pieces);
        if pieces.is_empty() {
            return;
        }
        match nodes.last_mut() {
            Some(Node::Text(before)) => before.extend(pieces),
            _ => nodes.push(Node::Text(pieces)),
        }
    }
}

fn plain(input: &Input, range: Range<usize>, pieces: &mut Vec<Piece>) {
    if !range.is_empty() {
        input.pieces(range, |span| pieces.push(Piece::Plain(span)));
    }
}
