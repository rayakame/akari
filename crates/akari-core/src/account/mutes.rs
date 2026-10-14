use std::time::Duration;

use tokio::time::Instant;

use crate::model::Timestamp;
use crate::state::{Store, StoreEvent, Subscription, wall_millis};

// Discord doesn't say when a timed mute ends, so lists learn it from here: the guild's
// settings change when its mute, or one of its channels', runs out.
pub(crate) async fn end_mutes(store: Store, changes: Subscription) {
    let clock = Clock::start();
    let mut checked = clock.now();
    loop {
        let deadline = store
            .next_mute_end(checked)
            .map(|end| clock.instant_at(end));
        loop {
            tokio::select! {
                event = changes.next() => match event {
                    None => return,
                    Some(event) if rearms(&event) => break,
                    Some(_) => {}
                },
                () = until(deadline) => break,
            }
        }
        let now = clock.now();
        store.end_mutes(checked, now);
        checked = now;
    }
}

// New settings can move the next end. A reconnect can follow a sleep of the computer,
// during which the monotonic clock behind the deadline stood still.
fn rearms(event: &StoreEvent) -> bool {
    matches!(
        event,
        StoreEvent::GuildSettingsChanged { .. }
            | StoreEvent::Ready
            | StoreEvent::Restored
            | StoreEvent::Connection(_)
    )
}

async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

// The wall clock as of the start, moved on by tokio's clock (which tests pause), unless the
// wall clock is ahead.
struct Clock {
    wall: i64,
    started: Instant,
}

impl Clock {
    fn start() -> Self {
        Self {
            wall: wall_millis(),
            started: Instant::now(),
        }
    }

    fn now(&self) -> Timestamp {
        let elapsed = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
        Timestamp::from_unix_millis(wall_millis().max(self.wall.saturating_add(elapsed)))
    }

    fn instant_at(&self, end: Timestamp) -> Instant {
        let ahead = end.unix_millis().saturating_sub(self.now().unix_millis());
        Instant::now() + Duration::from_millis(u64::try_from(ahead).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::gateway::{ChannelOverride, DispatchEvent, MuteConfig, UserGuildSettings};
    use crate::model::{MessageNotificationLevel, Snowflake, Timestamp};
    use crate::state::{DEFAULT_LIMITS, Store, StoreEvent, Subscription};

    const G1: u64 = 200_000_000_000_000_001;
    const GENERAL: u64 = 300_000_000_000_000_002;

    fn wall() -> i64 {
        crate::state::wall_millis()
    }

    fn mute(end: Option<i64>) -> Option<MuteConfig> {
        Some(MuteConfig {
            end_time: end.map(Timestamp::from_unix_millis),
            selected_time_window: 900,
        })
    }

    fn settings(muted: bool, end: Option<i64>, channels: Vec<ChannelOverride>) -> DispatchEvent {
        DispatchEvent::UserGuildSettingsUpdate(Box::new(UserGuildSettings {
            guild_id: Some(Snowflake::new(G1)),
            muted,
            mute_config: mute(end),
            message_notifications: MessageNotificationLevel::Inherit,
            suppress_everyone: false,
            suppress_roles: false,
            hide_muted_channels: false,
            flags: 0,
            channel_overrides: channels,
        }))
    }

    fn channel_muted_until(end: i64) -> ChannelOverride {
        ChannelOverride {
            channel_id: Snowflake::new(GENERAL),
            collapsed: false,
            message_notifications: MessageNotificationLevel::Inherit,
            muted: true,
            mute_config: mute(Some(end)),
            flags: 0,
        }
    }

    // Ends come from the real wall clock while tokio's is paused, so the tests allow a
    // second either side of an end.
    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
    }

    // The settings events waiting in `observer`, read without waiting.
    async fn settings_events(observer: &Subscription) -> usize {
        let mut count = 0;
        for _ in 0..observer.buffered() {
            if let Some(StoreEvent::GuildSettingsChanged { .. }) = observer.next().await {
                count += 1;
            }
        }
        count
    }

    fn start(store: &Store) -> tokio::task::JoinHandle<()> {
        tokio::spawn(end_mutes(store.clone(), store.subscribe()))
    }

    #[tokio::test(start_paused = true)]
    async fn a_mute_ending_sends_guild_settings_changed_at_its_end() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(settings(true, Some(wall() + 60_000), Vec::new()));
        let observer = store.subscribe();
        let _timer = start(&store);
        settle().await;

        tokio::time::advance(Duration::from_secs(59)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 0);

        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 1);

        tokio::time::advance(Duration::from_secs(600)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 0, "only once");
    }

    #[tokio::test(start_paused = true)]
    async fn a_channel_mute_ending_reports_its_guild() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(settings(
            false,
            None,
            vec![channel_muted_until(wall() + 10_000)],
        ));
        let observer = store.subscribe();
        let _timer = start(&store);
        settle().await;

        tokio::time::advance(Duration::from_secs(9)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 0);

        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_settings_update_moves_the_timer() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(settings(true, Some(wall() + 60_000), Vec::new()));
        let _timer = start(&store);
        settle().await;
        let observer = store.subscribe();
        store.apply(settings(true, Some(wall() + 30_000), Vec::new()));
        settle().await;
        assert_eq!(settings_events(&observer).await, 1, "the update itself");

        tokio::time::advance(Duration::from_secs(29)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 0);

        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 1);

        tokio::time::advance(Duration::from_secs(60)).await;
        settle().await;
        assert_eq!(settings_events(&observer).await, 0, "the old end is gone");
    }

    #[tokio::test(start_paused = true)]
    async fn unmuting_cancels_the_end() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(settings(true, Some(wall() + 60_000), Vec::new()));
        let _timer = start(&store);
        settle().await;
        store.apply(settings(false, None, Vec::new()));
        let observer = store.subscribe();

        tokio::time::advance(Duration::from_secs(120)).await;
        settle().await;

        assert_eq!(settings_events(&observer).await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_mute_already_over_sends_nothing() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(settings(true, Some(wall() - 1_000), Vec::new()));
        let observer = store.subscribe();
        let _timer = start(&store);

        tokio::time::advance(Duration::from_secs(3600)).await;
        settle().await;

        assert_eq!(settings_events(&observer).await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_timer_ends_with_the_store() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(settings(true, Some(wall() + 60_000), Vec::new()));
        let timer = start(&store);
        settle().await;

        store.finish();

        assert!(
            tokio::time::timeout(Duration::from_secs(1), timer)
                .await
                .is_ok()
        );
    }
}
