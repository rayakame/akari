use unicode_segmentation::UnicodeSegmentation;

use crate::document::InlineKind;
use crate::parse::{Node, Piece};

const MAX_JUMBO: usize = 30;

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

/// A Unicode emoji's name as Discord's hover card and popout show it, without colons: gemoji's
/// word (`grinning`, `thumbsup`), skin tones as `_tone1`–`_tone5` (two for a pair), a flag as
/// `flag_de`, a lone regional indicator as `regional_indicator_a`, else Unicode's name. `None`
/// for anything the parser doesn't count as exactly one emoji: text-presentation symbols without
/// their variation selector, and ©, ® and ™, which Discord leaves as text.
pub fn emoji_shortcode(emoji: &str) -> Option<String> {
    if exempt(emoji) || !is_emoji(emoji) {
        return None;
    }
    let letters: Vec<char> = emoji.chars().filter(|&c| c != '\u{fe0f}').collect();
    match letters[..] {
        [first] if regional_indicator(first) => {
            return regional_letter(first).map(|a| format!("regional_indicator_{a}"));
        }
        [first, second] => {
            if let (Some(a), Some(b)) = (regional_letter(first), regional_letter(second)) {
                return Some(format!("flag_{a}{b}"));
            }
        }
        _ => {}
    }
    let found = emojis::get(emoji)?;
    let tone = found
        .skin_tone()
        .filter(|&tone| tone != emojis::SkinTone::Default);
    let base = match tone {
        Some(_) => found
            .with_skin_tone(emojis::SkinTone::Default)
            .unwrap_or(found),
        None => found,
    };
    // gemoji lists `+1` before `thumbsup`; Discord's names are words.
    let name = base
        .shortcodes()
        .find(|name| name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .map(str::to_owned)
        .unwrap_or_else(|| unicode_name(base.name()));
    Some(match tone.map(tones) {
        Some((first, Some(second))) => format!("{name}_tone{first}_tone{second}"),
        Some((first, None)) => format!("{name}_tone{first}"),
        None => name,
    })
}

// Light 1 … dark 5; a pair (light and medium-light, …) in emojis' order, each first tone with
// the four others.
fn tones(tone: emojis::SkinTone) -> (u8, Option<u8>) {
    let index = tone as u8;
    if index <= 5 {
        return (index, None);
    }
    let pair = index - 6;
    let first = pair / 4 + 1;
    let others: Vec<u8> = (1..=5).filter(|&tone| tone != first).collect();
    (first, others.get(usize::from(pair % 4)).copied())
}

fn unicode_name(name: &str) -> String {
    name.chars()
        .filter_map(|c| match c {
            ' ' | '-' => Some('_'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

fn regional_letter(c: char) -> Option<char> {
    let offset = u32::from(c).checked_sub(0x1F1E6)?;
    (offset < 26).then(|| char::from(b'a' + offset as u8))
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
