//! Akari's own count of the emoji the user reacts with: how often and when last, at most
//! [`RECENT_REACTIONS`] of them, ranked as Discord's frecency ranks (uses weighted by age).

use std::cmp::Reverse;

use super::picker::RECENT_REACTIONS;
use super::reactions::ReactionEmoji;
use crate::emoji::UnicodeEmoji;

const DAY: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactionUse {
    pub(crate) emoji: ReactionEmoji,
    pub(crate) count: u32,
    pub(crate) last_used: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ReactionUses(Vec<ReactionUse>);

// Discord's frecency bonus by the age of a use: 100 within 3 days down to 10 within 80. Akari
// keeps only the last use, and an older one still ranks by its count.
fn bonus(age_millis: i64) -> u64 {
    match age_millis / DAY {
        ..=3 => 100,
        4..=15 => 70,
        16..=30 => 50,
        31..=45 => 30,
        46..=80 => 10,
        _ => 1,
    }
}

impl ReactionUse {
    fn score(&self, now_millis: i64) -> u64 {
        u64::from(self.count) * bonus(now_millis.saturating_sub(self.last_used))
    }
}

// A Unicode emoji by its fully qualified form, so ❤ and ❤️ count as one.
fn key(emoji: &ReactionEmoji) -> Option<ReactionEmoji> {
    match emoji.id {
        Some(_) => Some(emoji.clone()),
        None => {
            let name = emoji.name.as_deref().filter(|name| !name.is_empty())?;
            Some(ReactionEmoji::unicode(
                UnicodeEmoji::get(name).map_or(name, |found| found.as_str()),
            ))
        }
    }
}

impl ReactionUses {
    pub(crate) fn from_uses(uses: Vec<ReactionUse>) -> Self {
        let mut kept = Self(Vec::with_capacity(uses.len()));
        for used in uses {
            if let Some(emoji) = key(&used.emoji) {
                kept.add(ReactionUse { emoji, ..used });
            }
        }
        kept
    }

    pub(crate) fn uses(&self) -> &[ReactionUse] {
        &self.0
    }

    pub(crate) fn note(&mut self, emoji: &ReactionEmoji, now_millis: i64) {
        if let Some(emoji) = key(emoji) {
            self.add(ReactionUse {
                emoji,
                count: 1,
                last_used: now_millis,
            });
        }
    }

    // Uses noted while the cache wasn't read yet, added to the cache's.
    pub(crate) fn merge(&mut self, cached: &Self) {
        for used in &cached.0 {
            self.add(used.clone());
        }
    }

    // A new emoji in a full list takes the weakest other's place, so a use always shows.
    fn add(&mut self, used: ReactionUse) {
        if let Some(known) = self
            .0
            .iter_mut()
            .find(|known| known.emoji.same(&used.emoji))
        {
            known.count = known.count.saturating_add(used.count);
            if used.last_used >= known.last_used {
                known.last_used = used.last_used;
                if used.emoji.name.is_some() {
                    known.emoji = used.emoji;
                }
            }
            return;
        }
        if self.0.len() >= RECENT_REACTIONS {
            let now = used.last_used.max(
                self.0
                    .iter()
                    .map(|known| known.last_used)
                    .max()
                    .unwrap_or(0),
            );
            if let Some(weakest) = self
                .0
                .iter()
                .enumerate()
                .min_by_key(|(_, known)| (known.score(now), known.last_used))
                .map(|(index, _)| index)
            {
                self.0.swap_remove(weakest);
            }
        }
        self.0.push(used);
    }

    // Best first.
    pub(crate) fn ranked(&self, now_millis: i64) -> Vec<&ReactionUse> {
        let mut ranked: Vec<&ReactionUse> = self.0.iter().collect();
        ranked.sort_by_key(|used| (Reverse(used.score(now_millis)), Reverse(used.last_used)));
        ranked
    }
}

#[cfg(test)]
mod tests;
