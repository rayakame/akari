use std::time::{Duration, Instant, SystemTime};

use super::*;
use crate::state::ThreadCreation;

const VIEW: u64 = 1 << 10;
const HISTORY: u64 = 1 << 16;
const PUBLIC: u64 = 1 << 35;
const PRIVATE: u64 = 1 << 36;
const ROLE: u64 = 600;
const CATEGORY: u64 = 300_000_000_000_000_001;
const NEWS: u64 = 300_000_000_000_000_004;
// Messages in GENERAL.
const PLAIN: u64 = 400_000_000_000_000_050;
const OTHER: u64 = 400_000_000_000_000_051;

const BOTH: ThreadCreation = ThreadCreation {
    public: true,
    private: true,
};
const ONLY_PUBLIC: ThreadCreation = ThreadCreation {
    public: true,
    private: false,
};
const ONLY_PRIVATE: ThreadCreation = ThreadCreation {
    public: false,
    private: true,
};
const NONE: ThreadCreation = ThreadCreation {
    public: false,
    private: false,
};

fn news_channel() -> Value {
    json!({"id": NEWS.to_string(), "type": 5, "name": "news", "position": 2,
        "parent_id": CATEGORY.to_string(), "permission_overwrites": [], "rate_limit_per_user": 0})
}

// The fixture's user owns the guild; here they're a plain member whose one role allows `allow`.
fn member_ready(allow: u64, edit: impl FnOnce(&mut Value)) -> Value {
    let mut ready = ready_value();
    let guild = &mut ready["guilds"][0];
    guild["properties"]["owner_id"] = "100000000000000099".into();
    guild["roles"][0]["permissions"] = "0".into();
    guild["roles"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": ROLE.to_string(), "name": "r", "permissions": allow.to_string(), "position": 2}));
    guild["channels"]
        .as_array_mut()
        .unwrap()
        .push(news_channel());
    ready["merged_members"][0][0]["roles"] = json!([ROLE.to_string()]);
    edit(&mut ready);
    ready
}

fn member_state(allow: u64) -> State {
    state_from(&member_ready(allow, |_| {}))
}

fn owner_state() -> State {
    let mut ready = ready_value();
    ready["guilds"][0]["channels"]
        .as_array_mut()
        .unwrap()
        .push(news_channel());
    state_from(&ready)
}

fn creation(state: &State, parent: u64) -> ThreadCreation {
    state.thread_creation(Snowflake::new(parent), state.now_millis())
}

fn can_start(state: &State, message: u64) -> bool {
    state.can_start_thread(
        Snowflake::new(GENERAL),
        Snowflake::new(message),
        state.now_millis(),
    )
}

// GENERAL viewed, with `messages` loaded as Discord sent them.
fn with_messages(mut state: State, messages: &[Value]) -> State {
    let general = Snowflake::new(GENERAL);
    let mut events = Vec::new();
    state.view_channel(general, &mut events);
    let ticket = state
        .begin_load(general, LoadKind::Latest, 1, &mut events)
        .unwrap();
    let page = messages
        .iter()
        .map(|message| serde_json::from_str(&message.to_string()).unwrap())
        .collect();
    state.finish_load(ticket, page, true, &mut events);
    state
}

fn plain_message(id: u64, kind: u64, flags: u64) -> Value {
    let mut message = message(id, GENERAL);
    message["type"] = kind.into();
    message["flags"] = flags.into();
    message
}

fn thread_with_id(id: u64, name: &str) -> Value {
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread["id"] = id.to_string().into();
    thread["name"] = name.into();
    thread["member"]["id"] = id.to_string().into();
    thread
}

#[test]
fn the_owner_may_create_public_and_private_threads() {
    assert_eq!(creation(&owner_state(), GENERAL), BOTH);
}

#[test]
fn public_threads_need_create_public_threads_and_read_message_history() {
    assert_eq!(
        creation(&member_state(VIEW | PUBLIC | HISTORY), GENERAL),
        ONLY_PUBLIC
    );
    assert_eq!(creation(&member_state(VIEW | PUBLIC), GENERAL), NONE);
}

#[test]
fn private_threads_need_create_private_threads() {
    assert_eq!(
        creation(&member_state(VIEW | PRIVATE), GENERAL),
        ONLY_PRIVATE
    );
    assert_eq!(
        creation(&member_state(VIEW | PUBLIC | PRIVATE | HISTORY), GENERAL),
        BOTH
    );
}

#[test]
fn announcement_channels_have_only_public_threads() {
    assert_eq!(creation(&owner_state(), NEWS), ONLY_PUBLIC);
    assert_eq!(creation(&member_state(VIEW | PRIVATE), NEWS), NONE);
}

#[test]
fn threads_voice_channels_categories_and_dms_have_none() {
    let state = owner_state();
    for channel in [THREAD, VOICE, CATEGORY, DM, GROUP, 1] {
        assert_eq!(creation(&state, channel), NONE, "{channel}");
    }
}

#[test]
fn a_timeout_takes_both() {
    let all = VIEW | PUBLIC | PRIVATE | HISTORY;
    let timed_out = |until: &str| {
        let until = until.to_owned();
        state_from(&member_ready(all, move |ready| {
            ready["merged_members"][0][0]["communication_disabled_until"] = until.into();
        }))
    };

    assert_eq!(
        creation(&timed_out("2999-01-01T00:00:00+00:00"), GENERAL),
        NONE
    );
    assert_eq!(
        creation(&timed_out("2001-01-01T00:00:00+00:00"), GENERAL),
        BOTH
    );
}

#[test]
fn a_channel_the_user_cant_view_has_none() {
    let ready = member_ready(VIEW | PUBLIC | PRIVATE | HISTORY, |ready| {
        ready["guilds"][0]["channels"][1]["permission_overwrites"] =
            json!([{"id": ROLE.to_string(), "type": 0, "allow": "0", "deny": VIEW.to_string()}]);
    });

    assert_eq!(creation(&state_from(&ready), GENERAL), NONE);
}

#[test]
fn a_users_message_or_reply_can_start_a_thread() {
    let state = with_messages(
        owner_state(),
        &[plain_message(PLAIN, 0, 0), plain_message(OTHER, 19, 0)],
    );

    assert!(can_start(&state, PLAIN));
    assert!(can_start(&state, OTHER));
}

#[test]
fn system_messages_cant_start_a_thread() {
    for kind in [7, 18, 21, 6] {
        let state = with_messages(owner_state(), &[plain_message(PLAIN, kind, 0)]);
        assert!(!can_start(&state, PLAIN), "type {kind}");
    }
}

#[test]
fn a_message_with_a_thread_cant_start_another() {
    let flagged = with_messages(owner_state(), &[plain_message(PLAIN, 0, 1 << 5)]);
    assert!(!can_start(&flagged, PLAIN));

    // Someone's thread from it, before the message's update says so.
    let mut state = with_messages(owner_state(), &[plain_message(PLAIN, 0, 0)]);
    apply(&mut state, "THREAD_CREATE", thread_with_id(PLAIN, "theirs"));
    assert!(!can_start(&state, PLAIN));
}

#[test]
fn ephemeral_unknown_and_unsent_messages_cant_start_a_thread() {
    let ephemeral = with_messages(owner_state(), &[plain_message(PLAIN, 0, 1 << 6)]);
    assert!(!can_start(&ephemeral, PLAIN));
    assert!(!can_start(&ephemeral, OTHER));

    let mut state = with_messages(owner_state(), &[]);
    let general = Snowflake::new(GENERAL);
    let me = state.current_user().unwrap();
    let pending = Snowflake::new(OTHER);
    let now = state.now_millis();
    state.queue(
        general,
        Arc::new(Message::pending(
            pending,
            general,
            Arc::new(me.user.clone()),
            "hi".to_owned(),
            0,
        )),
        now,
        &mut Vec::new(),
    );
    assert!(!can_start(&state, OTHER));
}

#[test]
fn only_the_public_permission_lets_a_message_start_a_thread() {
    let message = [plain_message(PLAIN, 0, 0)];
    assert!(!can_start(
        &with_messages(member_state(VIEW | PRIVATE | HISTORY), &message),
        PLAIN
    ));
    assert!(can_start(
        &with_messages(member_state(VIEW | PUBLIC | HISTORY), &message),
        PLAIN
    ));
}

fn slowmode_ready(allow: u64, seconds: u32) -> State {
    state_from(&member_ready(allow, move |ready| {
        ready["guilds"][0]["channels"][1]["rate_limit_per_user"] = seconds.into();
    }))
}

fn thread_slowmode(
    state: &State,
    now: Instant,
    wall: SystemTime,
) -> Option<crate::state::Slowmode> {
    state.thread_slowmode(Snowflake::new(GENERAL), state.now_millis(), now, wall)
}

#[test]
fn a_parent_without_slowmode_has_no_thread_slowmode() {
    let state = owner_state();
    assert_eq!(
        thread_slowmode(&state, Instant::now(), SystemTime::now()),
        None
    );
}

#[test]
fn a_new_thread_starts_its_parents_thread_cooldown_and_not_its_message_one() {
    let mut state = slowmode_ready(VIEW | PUBLIC | HISTORY, 30);
    let wall = SystemTime::now();
    let before = thread_slowmode(&state, Instant::now(), wall).unwrap();
    assert_eq!(before.interval, Duration::from_secs(30));
    assert_eq!(before.until, None);

    let started = Instant::now();
    state.start_thread_cooldown(Snowflake::new(GENERAL), state.now_millis());

    let until = thread_slowmode(&state, started, wall)
        .unwrap()
        .until
        .unwrap();
    // Discord's client waits the interval and 100 ms more.
    let left = until.duration_since(wall).unwrap();
    assert!(left > Duration::from_millis(30_000), "{left:?}");
    assert!(left < Duration::from_millis(31_000), "{left:?}");
    let message = state.slowmode(Snowflake::new(GENERAL), state.now_millis(), started, wall);
    assert_eq!(message.unwrap().until, None);
}

#[test]
fn sending_doesnt_start_the_thread_cooldown() {
    let mut state = with_messages(slowmode_ready(VIEW | PUBLIC | HISTORY | (1 << 11), 30), &[]);
    let general = Snowflake::new(GENERAL);
    let me = state.current_user().unwrap();
    let now = state.now_millis();
    state.queue(
        general,
        Arc::new(Message::pending(
            Snowflake::new(OTHER),
            general,
            Arc::new(me.user.clone()),
            "hi".to_owned(),
            0,
        )),
        now,
        &mut Vec::new(),
    );
    let (now, wall) = (Instant::now(), SystemTime::now());

    assert!(
        state
            .slowmode(general, state.now_millis(), now, wall)
            .unwrap()
            .until
            .is_some()
    );
    assert_eq!(thread_slowmode(&state, now, wall).unwrap().until, None);
}

#[test]
fn bypassing_slowmode_skips_the_thread_cooldown() {
    let mut state = slowmode_ready(VIEW | PUBLIC | HISTORY | (1 << 52), 30);
    state.start_thread_cooldown(Snowflake::new(GENERAL), state.now_millis());
    state.hold_thread_cooldown(
        Snowflake::new(GENERAL),
        Duration::from_secs(20),
        state.now_millis(),
    );

    let slowmode = thread_slowmode(&state, Instant::now(), SystemTime::now()).unwrap();
    assert!(slowmode.exempt);
    assert_eq!(slowmode.until, None);
}

#[test]
fn discords_wait_holds_the_thread_cooldown() {
    let mut state = slowmode_ready(VIEW | PUBLIC | HISTORY, 30);
    let (now, wall) = (Instant::now(), SystemTime::now());
    state.hold_thread_cooldown(
        Snowflake::new(GENERAL),
        Duration::from_secs(12),
        state.now_millis(),
    );

    let left = thread_slowmode(&state, now, wall)
        .unwrap()
        .until
        .unwrap()
        .duration_since(wall)
        .unwrap();
    assert!(left > Duration::from_secs(11), "{left:?}");
    assert!(left <= Duration::from_secs(13), "{left:?}");
}

fn answer(value: Value) -> model::Channel {
    serde_json::from_str(&value.to_string()).unwrap()
}

#[test]
fn a_created_thread_is_kept_as_discord_answers_it() {
    let mut state = owner_state();
    let mut events = Vec::new();
    let thread = Snowflake::new(PLAIN);

    let kept = state.keep_created_thread(answer(thread_with_id(PLAIN, "mine")), &mut events);

    assert_eq!(kept, Some(thread));
    let stored = state.channel(thread).unwrap();
    assert_eq!(stored.name.as_deref(), Some("mine"));
    assert!(stored.thread.as_deref().unwrap().joined());
    assert!(
        events
            .iter()
            .map(describe)
            .any(|event| event.starts_with("ChannelAdded"))
    );
}

#[test]
fn the_gateways_copy_wins_over_the_answer() {
    let mut state = owner_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_with_id(PLAIN, "from the gateway"),
    );

    let kept = state.keep_created_thread(answer(thread_with_id(PLAIN, "answer")), &mut Vec::new());

    assert_eq!(kept, Some(Snowflake::new(PLAIN)));
    let stored = state.channel(Snowflake::new(PLAIN)).unwrap();
    assert_eq!(stored.name.as_deref(), Some("from the gateway"));
}

#[test]
fn an_answer_that_isnt_a_thread_keeps_nothing() {
    let mut state = owner_state();
    let mut channel = thread_with_id(PLAIN, "odd");
    channel["type"] = 0.into();

    assert_eq!(
        state.keep_created_thread(answer(channel), &mut Vec::new()),
        None
    );
    assert!(state.channel(Snowflake::new(PLAIN)).is_none());
}

#[test]
fn a_channel_keeps_its_default_archive_duration() {
    let mut ready = ready_value();
    ready["guilds"][0]["channels"][1]["default_auto_archive_duration"] = 1440.into();
    let mut state = state_from(&ready);
    let general = Snowflake::new(GENERAL);
    let duration = |state: &State| {
        state
            .channel(general)
            .unwrap()
            .default_auto_archive_duration
    };
    assert_eq!(duration(&state), Some(1440));

    apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({"id": GENERAL.to_string(), "default_auto_archive_duration": 60}),
    );
    assert_eq!(duration(&state), Some(60));

    apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({"id": GENERAL.to_string(), "name": "renamed"}),
    );
    assert_eq!(duration(&state), Some(60));
    assert_eq!(duration(&ready_state()), None);
}
