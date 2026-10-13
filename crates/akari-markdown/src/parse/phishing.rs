// parse-discord's is_link_admissable; like Discord's, it doesn't look into lists.

use super::{Ctx, Input, Node, Parser, Piece};
use crate::document::InlineKind;
use crate::emoji::py_space;

// parse-discord's bad_whitespace_regex.
pub(crate) fn special_whitespace(c: char) -> bool {
    let n = u32::from(c);
    (c.is_control() && c != '\n')
        || (c.is_whitespace() && c != ' ' && c != '\n')
        || matches!(
            n,
            0xAD | 0x34F
                | 0x600..=0x605
                | 0x61C
                | 0x6DD
                | 0x70F
                | 0x890..=0x891
                | 0x8E2
                | 0x1160
                | 0x17B4
                | 0x17B5
                | 0x180E
                | 0x200B..=0x200F
                | 0x202A..=0x202E
                | 0x2060..=0x2064
                | 0x2066..=0x206F
                | 0x3164
                | 0xFEFF
                | 0xFFA0
                | 0xFFF9..=0xFFFB
                | 0x110BD
                | 0x110CD
                | 0x13430..=0x1343F
                | 0x1BCA0..=0x1BCA3
                | 0x1D173..=0x1D17A
                | 0xE0001
                | 0xE0020..=0xE007F
        )
}

// parse-discord's scheme confusables.
fn unconfuse(c: char) -> char {
    match u32::from(c) {
        0x48 | 0x4BB | 0x570 | 0x13C2 | 0x210E | 0x1D421 | 0x1D489 | 0x1D4BD | 0x1D4F1
        | 0x1D525 | 0x1D559 | 0x1D58D | 0x1D5C1 | 0x1D5F5 | 0x1D629 | 0x1D65D | 0x1D691
        | 0xFF48 => 'h',
        0x54 | 0x1D42D | 0x1D461 | 0x1D495 | 0x1D4C9 | 0x1D4FD | 0x1D531 | 0x1D565 | 0x1D599
        | 0x1D5CD | 0x1D601 | 0x1D635 | 0x1D669 | 0x1D69D => 't',
        0x50 | 0x3C1 | 0x3F1 | 0x440 | 0x2374 | 0x2CA3 | 0x1D429 | 0x1D45D | 0x1D491 | 0x1D4C5
        | 0x1D4F9 | 0x1D52D | 0x1D561 | 0x1D595 | 0x1D5C9 | 0x1D5FD | 0x1D631 | 0x1D665
        | 0x1D699 | 0x1D6D2 | 0x1D6E0 | 0x1D70C | 0x1D71A | 0x1D746 | 0x1D754 | 0x1D780
        | 0x1D78E | 0x1D7BA | 0x1D7C8 | 0xFF50 | 0x48F => 'p',
        0x53 | 0x1BD | 0x455 | 0xA731 | 0xABAA | 0x10448 | 0x118C1 | 0x1D42C | 0x1D460
        | 0x1D494 | 0x1D4C8 | 0x1D4FC | 0x1D530 | 0x1D564 | 0x1D598 | 0x1D5CC | 0x1D600
        | 0x1D634 | 0x1D668 | 0x1D69C | 0xFF53 => 's',
        0x2D0 | 0x2F8 | 0x589 | 0x5C3 | 0x703 | 0x704 | 0x903 | 0xA83 | 0x16EC | 0x1803
        | 0x1809 | 0x205A | 0x2236 | 0xA4FD | 0xA789 | 0xFE30 | 0xFF1A | 0x3B | 0x37E => ':',
        0x1735 | 0x2041 | 0x2044 | 0x2215 | 0x2571 | 0x27CB | 0x29F8 | 0x2CC6 | 0x2F03 | 0x3033
        | 0x30CE | 0x31D3 | 0x4E3F | 0x1D23A => '/',
        _ => c,
    }
}

impl Parser {
    // None: refused; otherwise whether it shows more than whitespace (empty text is refused too).
    pub(super) fn admissible(&mut self, text: &str, ctx: Ctx, allow_emoji: bool) -> Option<bool> {
        let cleaned: String = text
            .chars()
            .filter(|&c| !special_whitespace(c))
            .map(unconfuse)
            .collect();
        self.steps += cleaned.len() as u64;
        let input = Input::source(&cleaned);
        let nodes = self.region(&input, 0..cleaned.len(), ctx);
        walk(&nodes, &cleaned, allow_emoji)
    }
}

fn walk(nodes: &[Node], text: &str, allow_emoji: bool) -> Option<bool> {
    let mut has_text = false;
    for node in nodes {
        let shows = match node {
            Node::Text(pieces) => !pieces.iter().all(|piece| match piece {
                Piece::Plain(span) => text[span.range()].chars().all(py_space),
                Piece::Break(_) => true,
                Piece::Escape { .. } => false,
            }),
            Node::TestingCode { children } => walk(children, text, true)?,
            Node::Style { children, .. }
            | Node::Spoiler { children, .. }
            | Node::Quote { children, .. }
            | Node::Heading { children, .. }
            | Node::Subtext { children, .. } => walk(children, text, allow_emoji)?,
            Node::List { .. }
            | Node::Token {
                kind: InlineKind::Timestamp { .. },
                ..
            } => true,
            Node::Emoji { .. }
            | Node::Token {
                kind: InlineKind::CustomEmoji { .. },
                ..
            } if allow_emoji => true,
            _ => return None,
        };
        has_text |= shows;
    }
    Some(has_text)
}
