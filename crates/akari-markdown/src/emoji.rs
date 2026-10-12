use unicode_segmentation::UnicodeSegmentation;

use crate::document::InlineKind;
use crate::parse::{Node, Piece};

const MAX_JUMBO: usize = 27;

// Longer than any emoji in the list (a test checks), so the search looks no further.
pub(crate) const MAX_EMOJI_LEN: usize = 64;

pub(crate) fn could_start(c: char) -> bool {
    matches!(c, '#' | '*' | '0'..='9')
        || matches!(
            u32::from(c),
            0xA9 | 0xAE
                | 0x203C
                | 0x2049
                | 0x2122
                | 0x2139
                | 0x2194..=0x21AA
                | 0x231A..=0x23FF
                | 0x24C2
                | 0x25AA..=0x27BF
                | 0x2934..=0x2935
                | 0x2B05..=0x2B55
                | 0x3030
                | 0x303D
                | 0x3297
                | 0x3299
                | 0x1F000..=0x1FAFF
        )
}

fn regional_indicator(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

// Discord's list has fully-qualified forms only, and the lone regional indicators.
fn is_emoji(s: &str) -> bool {
    emojis::get(s).is_some_and(|e| e.as_str() == s) || {
        let mut chars = s.chars();
        chars.next().is_some_and(regional_indicator) && chars.next().is_none()
    }
}

pub(crate) fn emoji_len(text: &str, at: usize, steps: &mut u64) -> Option<usize> {
    let mut chars = text[at..].chars();
    let first = chars.next()?;
    if !could_start(first) {
        return None;
    }
    // A keycap base is an emoji only with its variation selector or keycap mark after it.
    if first.is_ascii() && !matches!(chars.next(), Some('\u{fe0f}' | '\u{20e3}')) {
        return None;
    }
    let window = text.floor_char_boundary((at + MAX_EMOJI_LEN).min(text.len()));
    let grapheme = text[at..window].graphemes(true).next()?;
    *steps += grapheme.len() as u64;
    let mut len = grapheme.len();
    while len > 0 {
        *steps += 1;
        if is_emoji(&grapheme[..len]) {
            return Some(len);
        }
        len = grapheme[..len]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
    }
    None
}

// parse-discord's EXEMPT_EMOJI.
pub(crate) fn exempt(emoji: &str) -> bool {
    matches!(emoji.trim_end_matches('\u{fe0f}'), "©" | "®" | "™")
}

// Shortcodes show as text (D66); only masked-link checks count them as emoji.
pub(crate) fn is_shortcode(name: &str) -> bool {
    emojis::get_by_shortcode(name).is_some_and(|e| !exempt(e.as_str()))
}

// Python's str.isspace, which parse-discord uses.
pub(crate) fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

pub(crate) fn jumbo(nodes: &[Node], source: &str) -> bool {
    let mut count = 0;
    for node in nodes {
        match node {
            Node::Emoji {
                shortcode: false, ..
            }
            | Node::Token {
                kind: InlineKind::CustomEmoji { .. },
                ..
            } => count += 1,
            Node::Text(pieces) => {
                let blank = pieces.iter().all(|piece| match piece {
                    Piece::Plain(span) => source[span.range()].chars().all(py_space),
                    Piece::Break(_) => true,
                    Piece::Escape { .. } => false,
                });
                if !blank {
                    return false;
                }
            }
            _ => return false,
        }
        if count > MAX_JUMBO {
            return false;
        }
    }
    count > 0
}
