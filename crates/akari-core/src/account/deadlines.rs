use std::time::Duration;

use tokio::time::Instant;

use crate::model::Timestamp;
use crate::state::{Store, StoreEvent, Subscription, wall_millis};

// Discord doesn't say when a timed mute ends or a thread's inactivity hides it, so lists learn
// it from here: the guild's settings change when its mute, or one of its channels' or threads',
// runs out, and a read thread leaves its list when its window ends.
pub(crate) async fn run_deadlines(store: Store, changes: Subscription) {
    let clock = Clock::start();
    let mut checked = clock.now();
    loop {
        let deadline = store
            .next_deadline(checked)
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
        store.end_deadlines(checked, now);
        checked = now;
    }
}

// New settings can move the next mute end; a thread joined, read or written in can bring a
// window end. A reconnect can follow a sleep of the computer, during which the monotonic clock
// behind the deadline stood still.
fn rearms(event: &StoreEvent) -> bool {
    matches!(
        event,
        StoreEvent::GuildSettingsChanged { .. }
            | StoreEvent::ChannelAdded(_)
            | StoreEvent::ChannelUpdated(_)
            | StoreEvent::BadgeChanged { .. }
            | StoreEvent::ThreadListingChanged { .. }
            | StoreEvent::Ready
            | StoreEvent::Restored
            | StoreEvent::Connection(_)
    )
}

pub(super) async fn until(deadline: Option<Instant>) {
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
        tokio::spawn(run_deadlines(store.clone(), store.subscribe()))
    }

    const THREAD: u64 = 300_000_000_000_000_020;
    const MINUTE: i64 = 60_000;

    fn dispatch(name: &str, data: &serde_json::Value) -> DispatchEvent {
        let payload = serde_json::json!({"op": 0, "s": 1, "t": name, "d": data});
        match crate::gateway::decode(payload.to_string().as_bytes()) {
            Ok(crate::gateway::GatewayEvent::Dispatch { event, .. }) => event,
            other => panic!("expected a dispatch, got {other:?}"),
        }
    }

    fn snowflake_at(unix_millis: i64) -> String {
        Snowflake::<crate::model::GenericMarker>::from_unix_millis(unix_millis, 0)
            .get()
            .to_string()
    }

    // READY whose thread's window of an hour ends at `ends`, read up to its newest message.
    fn ready_with_thread_ending(ends: i64) -> DispatchEvent {
        let mut ready: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ready.json")).unwrap();
        let data = &mut ready["d"];
        let last = snowflake_at(ends - 60 * MINUTE);
        let thread = &mut data["guilds"][0]["threads"][0];
        thread["last_message_id"] = last.clone().into();
        thread["thread_metadata"]["auto_archive_duration"] = 60.into();
        data["read_state"]["entries"][1]["last_message_id"] = last.into();
        dispatch("READY", data)
    }

    fn own_message_now(channel: u64) -> DispatchEvent {
        let mut message: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/message_create.json")).unwrap();
        message["id"] = snowflake_at(wall()).into();
        message["channel_id"] = channel.to_string().into();
        message["author"]["id"] = "100000000000000001".into();
        message["mentions"] = serde_json::json!([]);
        dispatch("MESSAGE_CREATE", &message)
    }

    // The settings and listing events waiting in `observer`, read without waiting.
    async fn deadline_events(observer: &Subscription) -> (usize, usize) {
        let (mut settings, mut listings) = (0, 0);
        for _ in 0..observer.buffered() {
            match observer.next().await {
                Some(StoreEvent::GuildSettingsChanged { .. }) => settings += 1,
                Some(StoreEvent::ThreadListingChanged { .. }) => listings += 1,
                _ => {}
            }
        }
        (settings, listings)
    }

    #[tokio::test(start_paused = true)]
    async fn a_read_thread_leaves_the_list_when_its_window_ends() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(ready_with_thread_ending(wall() + 60_000));
        let observer = store.subscribe();
        let _timer = start(&store);
        settle().await;

        tokio::time::advance(Duration::from_secs(59)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.1, 0);

        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.1, 1);

        tokio::time::advance(Duration::from_secs(600)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.1, 0, "only once");
    }

    #[tokio::test(start_paused = true)]
    async fn a_message_moves_the_window() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(ready_with_thread_ending(wall() + 60_000));
        let _timer = start(&store);
        settle().await;
        store.apply(own_message_now(THREAD));
        let observer = store.subscribe();
        settle().await;

        tokio::time::advance(Duration::from_secs(61)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.1, 0, "the old end is gone");

        tokio::time::advance(Duration::from_secs(60 * 60)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.1, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_mute_end_and_a_window_end_share_the_timer() {
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(ready_with_thread_ending(wall() + 20_000));
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
        assert_eq!(deadline_events(&observer).await, (0, 0));

        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await, (1, 0));

        tokio::time::advance(Duration::from_secs(10)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await, (0, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn a_thread_mute_ends_on_its_own() {
        let mut ready: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ready.json")).unwrap();
        let data = &mut ready["d"];
        let member = &mut data["guilds"][0]["threads"][0]["member"];
        member["muted"] = true.into();
        member["mute_config"] = serde_json::json!({
            "selected_time_window": 900, "end_time": crate::model::iso(wall() + 10_000)
        });
        data["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
        let store = Store::new(DEFAULT_LIMITS);
        store.apply(dispatch("READY", data));
        let thread = Snowflake::new(THREAD);
        assert!(!store.channel_badge(thread).unread, "muted");
        let observer = store.subscribe();
        let _timer = start(&store);
        settle().await;

        tokio::time::advance(Duration::from_secs(9)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.0, 0);

        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.0, 1);
        assert!(store.channel_badge(thread).unread, "the badge is back");
    }

    #[tokio::test(start_paused = true)]
    async fn a_thread_added_later_arms_the_timer() {
        let store = Store::new(DEFAULT_LIMITS);
        let mut ready: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ready.json")).unwrap();
        ready["d"]["guilds"][0]["threads"] = serde_json::json!([]);
        store.apply(dispatch("READY", &ready["d"]));
        let _timer = start(&store);
        settle().await;
        let mut thread: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/thread_create.json")).unwrap();
        thread["thread_metadata"]["auto_archive_duration"] = 60.into();
        thread["thread_metadata"]["archive_timestamp"] =
            crate::model::iso(wall() + 30_000 - 60 * MINUTE).into();
        store.apply(dispatch("THREAD_CREATE", &thread));
        let observer = store.subscribe();
        settle().await;

        tokio::time::advance(Duration::from_secs(29)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.1, 0);

        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(deadline_events(&observer).await.1, 1);
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
