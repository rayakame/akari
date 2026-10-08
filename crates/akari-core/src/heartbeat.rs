use std::time::Duration;

use tokio::time::Instant;

// Shared by the gateway and the remote auth gateway, which use the same heartbeat rules.
pub(crate) struct Heartbeat {
    interval: Duration,
    next: Instant,
    awaiting_ack: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Beat {
    NotYet,
    Send,
    // The previous beat was never acknowledged: the connection is dead.
    Zombie,
}

impl Heartbeat {
    pub(crate) fn start(interval: Duration, now: Instant, jitter: f64) -> Self {
        Self {
            interval,
            next: now + interval.mul_f64(jitter),
            awaiting_ack: false,
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.next
    }

    pub(crate) fn poll(&mut self, now: Instant) -> Beat {
        if now < self.next {
            return Beat::NotYet;
        }
        if self.awaiting_ack {
            return Beat::Zombie;
        }
        self.awaiting_ack = true;
        // From now, not from the missed deadline: after a sleep, one beat instead of a burst.
        self.next = now + self.interval;
        Beat::Send
    }

    pub(crate) fn acked(&mut self) {
        self.awaiting_ack = false;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::*;

    const INTERVAL: Duration = Duration::from_millis(40_000);

    #[test]
    fn first_beat_is_jittered_into_the_interval() {
        let start = Instant::now();

        let heartbeat = Heartbeat::start(INTERVAL, start, 0.25);

        assert_eq!(heartbeat.deadline(), start + INTERVAL / 4);
    }

    #[test]
    fn beats_when_due_and_schedules_the_next_one() {
        let start = Instant::now();
        let mut heartbeat = Heartbeat::start(INTERVAL, start, 0.5);

        assert_eq!(heartbeat.poll(start), Beat::NotYet);
        let due = start + INTERVAL / 2;
        assert_eq!(heartbeat.poll(due), Beat::Send);
        assert_eq!(heartbeat.deadline(), due + INTERVAL);
    }

    #[test]
    fn a_missing_ack_makes_the_next_beat_a_zombie() {
        let start = Instant::now();
        let mut heartbeat = Heartbeat::start(INTERVAL, start, 0.0);

        assert_eq!(heartbeat.poll(start), Beat::Send);
        assert_eq!(heartbeat.poll(start + INTERVAL), Beat::Zombie);
    }

    #[test]
    fn an_ack_keeps_the_connection_alive() {
        let start = Instant::now();
        let mut heartbeat = Heartbeat::start(INTERVAL, start, 0.0);

        assert_eq!(heartbeat.poll(start), Beat::Send);
        heartbeat.acked();
        assert_eq!(heartbeat.poll(start + INTERVAL), Beat::Send);
    }

    #[test]
    fn waking_up_late_beats_once() {
        let start = Instant::now();
        let mut heartbeat = Heartbeat::start(INTERVAL, start, 0.0);
        assert_eq!(heartbeat.poll(start), Beat::Send);
        heartbeat.acked();

        let woke = start + INTERVAL * 10;
        assert_eq!(heartbeat.poll(woke), Beat::Send);
        heartbeat.acked();
        assert_eq!(heartbeat.poll(woke), Beat::NotYet);
        assert_eq!(heartbeat.deadline(), woke + INTERVAL);
    }
}
