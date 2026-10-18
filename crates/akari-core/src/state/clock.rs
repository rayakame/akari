use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use tokio::time::Instant;

use super::apply::wall_millis;

type Wall = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The store's "now", which the badges and the deadlines share: the wall clock at the start,
/// moved on by tokio's clock (which tests pause), and forward when the wall clock jumps ahead.
/// It never steps back, so a wall clock set back can't relist a thread whose window ended.
pub(crate) struct Clock {
    wall: Wall,
    started: Instant,
    // The wall time at `started`, raised when the wall clock jumps ahead.
    base: AtomicI64,
}

impl Clock {
    pub(crate) fn new() -> Self {
        Self::with_wall(Arc::new(wall_millis))
    }

    pub(crate) fn with_wall(wall: Wall) -> Self {
        let base = wall();
        Self {
            wall,
            started: Instant::now(),
            base: AtomicI64::new(base),
        }
    }

    pub(crate) fn now_millis(&self) -> i64 {
        let elapsed = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
        let wall = (self.wall)();
        let base = self.base.load(Ordering::Relaxed);
        if wall > base.saturating_add(elapsed) {
            let raised = self
                .base
                .fetch_max(wall.saturating_sub(elapsed), Ordering::Relaxed);
            return raised.saturating_add(elapsed).max(wall);
        }
        base.saturating_add(elapsed)
    }
}

// States compare by what they hold; two clocks of equal states may differ by a few milliseconds.
#[cfg(test)]
impl PartialEq for Clock {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const HOUR: i64 = 60 * 60 * 1000;

    fn settable(start: i64) -> (Clock, Arc<AtomicI64>) {
        let wall = Arc::new(AtomicI64::new(start));
        let read = wall.clone();
        (
            Clock::with_wall(Arc::new(move || read.load(Ordering::Relaxed))),
            wall,
        )
    }

    #[tokio::test(start_paused = true)]
    async fn the_clock_never_steps_back_when_the_wall_clock_does() {
        let (clock, wall) = settable(10 * HOUR);
        let before = clock.now_millis();

        wall.store(9 * HOUR, Ordering::Relaxed);

        assert!(clock.now_millis() >= before);
    }

    #[tokio::test(start_paused = true)]
    async fn the_clock_moves_on_while_the_wall_clock_is_behind() {
        let (clock, wall) = settable(10 * HOUR);
        wall.store(9 * HOUR, Ordering::Relaxed);
        let before = clock.now_millis();

        tokio::time::advance(Duration::from_secs(10)).await;

        assert_eq!(clock.now_millis(), before + 10_000);
    }

    #[tokio::test(start_paused = true)]
    async fn the_clock_follows_the_wall_clock_forward() {
        let (clock, wall) = settable(10 * HOUR);

        wall.store(11 * HOUR, Ordering::Relaxed);

        assert_eq!(clock.now_millis(), 11 * HOUR);
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(clock.now_millis(), 11 * HOUR + 1_000, "and on from there");
    }
}
