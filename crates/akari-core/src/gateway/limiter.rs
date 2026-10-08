use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

const LIMIT: usize = 120;
// Identify or Resume, the first (jittered) heartbeat and two heartbeats Discord asks for.
const EXTRA: usize = 4;

pub(crate) struct CommandLimiter {
    window: Duration,
    sent: VecDeque<Instant>,
    reserved: usize,
}

impl CommandLimiter {
    pub(crate) fn new(window: Duration) -> Self {
        Self {
            window,
            sent: VecDeque::with_capacity(LIMIT),
            reserved: EXTRA,
        }
    }

    pub(crate) fn reserve_for(&mut self, heartbeat_interval: Duration) {
        let beats = if heartbeat_interval.is_zero() {
            LIMIT
        } else {
            usize::try_from(
                self.window
                    .as_nanos()
                    .div_ceil(heartbeat_interval.as_nanos()),
            )
            .unwrap_or(LIMIT)
        };
        self.reserved = beats.saturating_add(EXTRA).min(LIMIT);
    }

    #[allow(dead_code)]
    pub(crate) fn allows_command(&mut self, now: Instant) -> bool {
        self.expire(now);
        self.sent.len() + self.reserved < LIMIT
    }

    // Heartbeats, Identify and Resume are recorded too, but never refused.
    pub(crate) fn record(&mut self, now: Instant) {
        self.expire(now);
        self.sent.push_back(now);
    }

    #[allow(dead_code)]
    pub(crate) fn next_free(&self) -> Instant {
        self.sent
            .front()
            .map_or_else(Instant::now, |first| *first + self.window)
    }

    fn expire(&mut self, now: Instant) {
        while self
            .sent
            .front()
            .is_some_and(|sent| *sent + self.window <= now)
        {
            self.sent.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(60);

    fn fill(limiter: &mut CommandLimiter, now: Instant) -> usize {
        let mut sent = 0;
        while limiter.allows_command(now) {
            limiter.record(now);
            sent += 1;
        }
        sent
    }

    #[test]
    fn commands_get_what_heartbeats_leave() {
        let start = Instant::now();
        let mut limiter = CommandLimiter::new(WINDOW);
        limiter.reserve_for(Duration::from_millis(41_250));

        assert_eq!(fill(&mut limiter, start), 114);
        for _ in 0..6 {
            limiter.record(start);
        }
        assert_eq!(limiter.sent.len(), LIMIT);
    }

    #[test]
    fn the_window_slides() {
        let start = Instant::now();
        let mut limiter = CommandLimiter::new(WINDOW);
        limiter.reserve_for(Duration::from_millis(41_250));
        fill(&mut limiter, start);

        assert!(!limiter.allows_command(start + WINDOW - Duration::from_millis(1)));
        assert_eq!(limiter.next_free(), start + WINDOW);
        assert!(limiter.allows_command(start + WINDOW));
    }

    #[test]
    fn short_heartbeat_intervals_reserve_more() {
        let mut limiter = CommandLimiter::new(Duration::from_secs(1));
        limiter.reserve_for(Duration::from_millis(200));

        assert_eq!(fill(&mut limiter, Instant::now()), LIMIT - 9);
    }

    #[test]
    fn a_zero_interval_leaves_nothing_for_commands() {
        let mut limiter = CommandLimiter::new(WINDOW);
        limiter.reserve_for(Duration::ZERO);

        assert!(!limiter.allows_command(Instant::now()));
    }
}
