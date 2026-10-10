//! Rules for sending every UI needs: how long a message may be, and slowmode.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use crate::model::{ChannelId, MessageId, PremiumType};

pub(crate) const DEFAULT_LENGTH_LIMIT: usize = 2000;

/// How long `content` is as Discord counts it: Unicode code points.
pub fn message_length(content: &str) -> usize {
    // The API counts code points, not UTF-16 units or graphemes.
    content.chars().count()
}

pub(crate) fn length_limit(premium: PremiumType) -> usize {
    match premium {
        PremiumType::Tier2 => 4000,
        _ => DEFAULT_LENGTH_LIMIT,
    }
}

// The first number in Discord's "Must be 2000 or fewer in length.", in any locale's grouping.
pub(crate) fn limit_in(message: &str) -> Option<usize> {
    let start = message.find(|c: char| c.is_ascii_digit())?;
    let mut digits = String::new();
    let mut chars = message[start..].chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if matches!(c, ',' | '.' | ' ' | '\u{a0}' | '\u{202f}')
            && chars.peek().is_some_and(char::is_ascii_digit)
        {
            continue;
        } else {
            break;
        }
    }
    digits.parse().ok()
}

/// A channel's slowmode as it applies to the current user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Slowmode {
    pub interval: Duration,
    /// The user's permissions bypass it.
    pub exempt: bool,
    /// When the user may send again; `None` when they may now.
    pub until: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cooldown {
    deadline: Instant,
    started_by: Option<MessageId>,
}

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct Cooldowns(HashMap<ChannelId, Cooldown>);

impl Cooldowns {
    pub(crate) fn start(
        &mut self,
        channel: ChannelId,
        send: MessageId,
        now: Instant,
        interval: Duration,
    ) {
        let deadline = self.later(channel, now + interval);
        self.0.insert(
            channel,
            Cooldown {
                deadline,
                started_by: Some(send),
            },
        );
    }

    // Discord didn't count a send that failed, unless something moved the cooldown since.
    pub(crate) fn drop_started_by(&mut self, channel: ChannelId, send: MessageId) {
        if self
            .0
            .get(&channel)
            .is_some_and(|cooldown| cooldown.started_by == Some(send))
        {
            self.0.remove(&channel);
        }
    }

    pub(crate) fn hold(&mut self, channel: ChannelId, now: Instant, wait: Duration) {
        let deadline = self.later(channel, now + wait);
        self.0.insert(
            channel,
            Cooldown {
                deadline,
                started_by: None,
            },
        );
    }

    // `nonce` is the echo's; our own send's echo doesn't restart the cooldown it started.
    pub(crate) fn seen(
        &mut self,
        channel: ChannelId,
        nonce: Option<MessageId>,
        now: Instant,
        interval: Duration,
    ) {
        if let Some(cooldown) = self.0.get(&channel)
            && nonce.is_some()
            && cooldown.started_by == nonce
        {
            return;
        }
        self.hold(channel, now, interval);
    }

    pub(crate) fn until(
        &self,
        channel: ChannelId,
        now: Instant,
        wall: SystemTime,
    ) -> Option<SystemTime> {
        let left = self.0.get(&channel)?.deadline.checked_duration_since(now)?;
        (!left.is_zero()).then(|| wall + left)
    }

    fn later(&self, channel: ChannelId, deadline: Instant) -> Instant {
        self.0
            .get(&channel)
            .map_or(deadline, |cooldown| cooldown.deadline.max(deadline))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use super::*;
    use crate::model::{PremiumType, Snowflake};

    fn channel(raw: u64) -> ChannelId {
        Snowflake::new(raw)
    }

    fn send(raw: u64) -> MessageId {
        Snowflake::new(raw)
    }

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn message_length_counts_code_points() {
        assert_eq!(message_length("hello"), 5);
        assert_eq!(message_length("\u{e9}"), 1);
        assert_eq!(message_length("e\u{301}"), 2);
        assert_eq!(message_length("👍🏽"), 2);
        assert_eq!(message_length("👨‍👩‍👧"), 5);
        assert_eq!(message_length("<:akari:123>"), 12);
        assert_eq!(message_length(""), 0);
    }

    #[test]
    fn the_limit_follows_the_plan() {
        assert_eq!(length_limit(PremiumType::None), 2000);
        assert_eq!(length_limit(PremiumType::Tier1), 2000);
        assert_eq!(length_limit(PremiumType::Tier2), 4000);
        assert_eq!(length_limit(PremiumType::Tier3), 2000);
    }

    #[test]
    fn discords_limit_is_read_from_its_message() {
        assert_eq!(limit_in("Must be 2000 or fewer in length."), Some(2000));
        assert_eq!(limit_in("Must be 4,000 or fewer in length."), Some(4000));
        assert_eq!(
            limit_in("Darf höchstens 2.000 Zeichen lang sein."),
            Some(2000)
        );
        assert_eq!(limit_in("Must be fewer in length."), None);
    }

    #[test]
    fn cooldowns_count_monotonic_time() {
        let mut cooldowns = Cooldowns::default();
        let start = Instant::now();
        let wall = SystemTime::UNIX_EPOCH + 1_000_000 * SECOND;
        cooldowns.start(channel(1), send(10), start, 30 * SECOND);

        let later = start + 10 * SECOND;
        assert_eq!(
            cooldowns.until(channel(1), later, wall),
            Some(wall + 20 * SECOND)
        );
        // A clock set back an hour moves the end with it; the time left stays 20 s.
        let set_back = wall - 3600 * SECOND;
        assert_eq!(
            cooldowns.until(channel(1), later, set_back),
            Some(set_back + 20 * SECOND)
        );
        assert_eq!(cooldowns.until(channel(1), start + 30 * SECOND, wall), None);
        assert_eq!(cooldowns.until(channel(2), later, wall), None);
    }

    #[test]
    fn a_failed_send_only_drops_its_own_cooldown() {
        let now = Instant::now();
        let wall = SystemTime::now();
        let mut cooldowns = Cooldowns::default();
        cooldowns.start(channel(1), send(10), now, 30 * SECOND);
        cooldowns.seen(channel(1), None, now + SECOND, 30 * SECOND);
        cooldowns.drop_started_by(channel(1), send(10));
        assert!(cooldowns.until(channel(1), now, wall).is_some());

        let mut cooldowns = Cooldowns::default();
        cooldowns.start(channel(1), send(10), now, 30 * SECOND);
        cooldowns.start(channel(1), send(11), now, 30 * SECOND);
        cooldowns.drop_started_by(channel(1), send(10));
        assert!(cooldowns.until(channel(1), now, wall).is_some());
        cooldowns.drop_started_by(channel(1), send(11));
        assert_eq!(cooldowns.until(channel(1), now, wall), None);
    }

    #[test]
    fn holding_never_shortens_a_cooldown() {
        let now = Instant::now();
        let wall = SystemTime::now();
        let mut cooldowns = Cooldowns::default();
        cooldowns.start(channel(1), send(10), now, 30 * SECOND);
        cooldowns.hold(channel(1), now, 5 * SECOND);
        assert_eq!(
            cooldowns.until(channel(1), now, wall),
            Some(wall + 30 * SECOND)
        );
        cooldowns.hold(channel(1), now, 60 * SECOND);
        assert_eq!(
            cooldowns.until(channel(1), now, wall),
            Some(wall + 60 * SECOND)
        );
    }

    #[test]
    fn the_echo_of_our_send_doesnt_restart_it() {
        let now = Instant::now();
        let wall = SystemTime::now();
        let mut cooldowns = Cooldowns::default();
        cooldowns.start(channel(1), send(10), now, 30 * SECOND);
        cooldowns.seen(channel(1), Some(send(10)), now + 2 * SECOND, 30 * SECOND);
        assert_eq!(
            cooldowns.until(channel(1), now, wall),
            Some(wall + 30 * SECOND)
        );
        cooldowns.seen(channel(1), Some(send(99)), now + 2 * SECOND, 30 * SECOND);
        assert_eq!(
            cooldowns.until(channel(1), now, wall),
            Some(wall + 32 * SECOND)
        );
    }
}
