//! Unicode's emoji as the picker lists them: groups in CLDR order, each emoji with the short name
//! Discord's users type, from the `emojis` crate (Unicode's emoji data and gemoji's short codes).

use std::borrow::Cow;
use std::fmt;

/// An emoji version (UTS #51): when an emoji came. A host leaves out emoji newer than its font
/// draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmojiVersion {
    pub major: u16,
    pub minor: u16,
}

impl EmojiVersion {
    /// The newest version the catalog has.
    pub const NEWEST: Self = Self::new(18, 0);

    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

/// A CLDR emoji group, the picker's categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmojiGroup {
    SmileysAndEmotion,
    PeopleAndBody,
    AnimalsAndNature,
    FoodAndDrink,
    TravelAndPlaces,
    Activities,
    Objects,
    Symbols,
    Flags,
}

impl EmojiGroup {
    /// Every group, in CLDR's order.
    pub const ALL: [Self; 9] = [
        Self::SmileysAndEmotion,
        Self::PeopleAndBody,
        Self::AnimalsAndNature,
        Self::FoodAndDrink,
        Self::TravelAndPlaces,
        Self::Activities,
        Self::Objects,
        Self::Symbols,
        Self::Flags,
    ];
}

/// A Unicode emoji the catalog knows.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnicodeEmoji(&'static emojis::Emoji);

impl fmt::Debug for UnicodeEmoji {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("UnicodeEmoji").field(&self.as_str()).finish()
    }
}

impl UnicodeEmoji {
    /// The emoji `emoji` is, skin-toned ones too; an unqualified form (❤ without its variation
    /// selector) gives the fully qualified one.
    pub fn get(emoji: &str) -> Option<Self> {
        emojis::get(emoji).map(Self)
    }

    /// The emoji itself, fully qualified.
    pub fn as_str(self) -> &'static str {
        self.0.as_str()
    }

    /// The short name Discord's users know it by, without colons: gemoji's word (`thumbsup`,
    /// not `+1`), a flag as `flag_de`, skin tones as `_tone1`–`_tone5`, else Unicode's name with
    /// underscores (`cracking_face`). akari-markdown's `emoji_shortcode` names it the same.
    pub fn name(self) -> Cow<'static, str> {
        let letters: Vec<char> = self.as_str().chars().filter(|&c| c != '\u{fe0f}').collect();
        if let [first, second] = letters[..]
            && let (Some(a), Some(b)) = (regional_letter(first), regional_letter(second))
        {
            return Cow::Owned(format!("flag_{a}{b}"));
        }
        let tone = self
            .0
            .skin_tone()
            .filter(|&tone| tone != emojis::SkinTone::Default);
        let base = match tone {
            Some(_) => self
                .0
                .with_skin_tone(emojis::SkinTone::Default)
                .unwrap_or(self.0),
            None => self.0,
        };
        let name =
            word_code(base).map_or_else(|| Cow::Owned(unicode_name(base.name())), Cow::Borrowed);
        match tone.map(tones) {
            Some((first, Some(second))) => Cow::Owned(format!("{name}_tone{first}_tone{second}")),
            Some((first, None)) => Cow::Owned(format!("{name}_tone{first}")),
            None => name,
        }
    }

    /// gemoji's other short codes (`+1` for 👍).
    pub fn shortcodes(self) -> impl Iterator<Item = &'static str> {
        let name = self.name();
        self.0.shortcodes().filter(move |&code| code != name)
    }

    /// Unicode's English name (`thumbs up`, `flag: Germany`).
    pub fn cldr_name(self) -> &'static str {
        self.0.name()
    }

    pub fn group(self) -> EmojiGroup {
        match self.0.group() {
            emojis::Group::SmileysAndEmotion => EmojiGroup::SmileysAndEmotion,
            emojis::Group::PeopleAndBody => EmojiGroup::PeopleAndBody,
            emojis::Group::AnimalsAndNature => EmojiGroup::AnimalsAndNature,
            emojis::Group::FoodAndDrink => EmojiGroup::FoodAndDrink,
            emojis::Group::TravelAndPlaces => EmojiGroup::TravelAndPlaces,
            emojis::Group::Activities => EmojiGroup::Activities,
            emojis::Group::Objects => EmojiGroup::Objects,
            emojis::Group::Symbols => EmojiGroup::Symbols,
            emojis::Group::Flags => EmojiGroup::Flags,
        }
    }

    /// Whether it comes in skin tones (Akari lists the default only).
    pub fn has_skin_tones(self) -> bool {
        self.0.skin_tone().is_some()
    }

    pub fn version(self) -> EmojiVersion {
        let version = self.0.emoji_version();
        EmojiVersion::new(
            u16::try_from(version.major()).unwrap_or(u16::MAX),
            u16::try_from(version.minor()).unwrap_or(u16::MAX),
        )
    }
}

// gemoji lists `+1` before `thumbsup`; Discord's names are words.
fn word_code(emoji: &'static emojis::Emoji) -> Option<&'static str> {
    emoji
        .shortcodes()
        .find(|code| code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

// Light 1 … dark 5; a pair (light and medium-light, …) in the crate's order, each first tone with
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

/// A group's emoji as the picker lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnicodeGroup {
    pub group: EmojiGroup,
    pub emojis: Vec<UnicodeEmoji>,
}

/// The picker's Unicode emoji: groups in CLDR's order, each group's emoji in Unicode's order,
/// without skin-toned variants and without emoji newer than `newest`; a group left empty is left
/// out.
pub fn unicode_groups(newest: EmojiVersion) -> Vec<UnicodeGroup> {
    let mut groups: Vec<UnicodeGroup> = Vec::with_capacity(EmojiGroup::ALL.len());
    for emoji in listed() {
        if emoji.version() > newest {
            continue;
        }
        match groups.last_mut() {
            Some(last) if last.group == emoji.group() => last.emojis.push(emoji),
            _ => groups.push(UnicodeGroup {
                group: emoji.group(),
                emojis: vec![emoji],
            }),
        }
    }
    groups
}

/// How well a name matches a search, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Match {
    Exact,
    Prefix,
    WordPrefix,
    Contains,
}

/// What the user typed into the picker's search.
pub(crate) struct Query {
    // As typed, for codes such as `+1`.
    raw: String,
    // As names are compared: words joined by underscores.
    words: String,
}

// Lowercase, words joined by `_`, other punctuation dropped: `flag: Germany` is `flag_germany`.
pub(crate) fn search_key(name: &str) -> String {
    name.chars()
        .filter_map(|c| match c {
            ' ' | '-' | '_' => Some('_'),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

impl Query {
    /// `None` for nothing to look for; colons around a name are left out (`:smile:`).
    pub(crate) fn new(text: &str) -> Option<Self> {
        let raw = text.trim().trim_matches(':').trim().to_lowercase();
        let words = search_key(&raw);
        (!raw.is_empty() && !words.is_empty()).then_some(Self { raw, words })
    }

    /// How `key`, lowercase, matches.
    pub(crate) fn rank(&self, key: &str) -> Option<Match> {
        let raw = rank(key, &self.raw);
        if self.words == self.raw {
            return raw;
        }
        raw.into_iter().chain(rank(key, &self.words)).min()
    }

    /// How the best of a Unicode emoji's names matches: its short name, other short codes and
    /// Unicode's name.
    pub(crate) fn rank_unicode(&self, emoji: UnicodeEmoji) -> Option<Match> {
        [emoji.name(), Cow::Owned(search_key(emoji.cldr_name()))]
            .into_iter()
            .chain(emoji.shortcodes().map(Cow::Borrowed))
            .filter_map(|key| self.rank(&key))
            .min()
    }
}

fn rank(key: &str, query: &str) -> Option<Match> {
    if key == query {
        return Some(Match::Exact);
    }
    if key.starts_with(query) {
        return Some(Match::Prefix);
    }
    let mut found = None;
    for (at, _) in key.match_indices(query) {
        if key[..at].ends_with('_') {
            return Some(Match::WordPrefix);
        }
        found = Some(Match::Contains);
    }
    found
}

/// The catalog's emoji in its order, without skin-toned variants.
pub(crate) fn listed() -> impl Iterator<Item = UnicodeEmoji> {
    emojis::iter().map(UnicodeEmoji)
}

#[cfg(test)]
mod tests;
