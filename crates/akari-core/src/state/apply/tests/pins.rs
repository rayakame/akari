use super::*;

const PINNED: &str = "2026-10-01T13:00:00+00:00";
const LATER: &str = "2026-10-02T09:30:00.250000+00:00";

fn unread(state: &State, channel: u64) -> bool {
    let now = Timestamp::from_unix_millis(state.now_millis());
    state.pins_unread(Snowflake::new(channel), now)
}

fn changed(channel: u64) -> String {
    format!("PinsUnreadChanged({channel})")
}

fn pins_update(channel: u64, time: Value) -> Value {
    json!({
        "channel_id": channel.to_string(), "guild_id": G1.to_string(),
        "last_pin_timestamp": time,
    })
}

fn pins_ack(channel: u64, time: Value) -> Value {
    json!({"channel_id": channel.to_string(), "timestamp": time, "version": 4400})
}

// GENERAL pinned at PINNED, with what READY says the user acknowledged there.
fn pinned_ready(acked: Value) -> Value {
    let mut ready = ready_value();
    ready["guilds"][0]["channels"][1]["last_pin_timestamp"] = PINNED.into();
    ready["read_state"]["entries"][0]["last_pin_timestamp"] = acked;
    ready
}

#[test]
fn ready_says_which_channels_have_pins_the_user_hasnt_seen() {
    let never = state_from(&pinned_ready("1970-01-01T00:00:00+00:00".into()));
    let seen = state_from(&pinned_ready(PINNED.into()));
    let older = state_from(&pinned_ready("2026-09-30T08:00:00+00:00".into()));
    let no_entry = state_from(&pinned_ready(Value::Null));

    assert!(unread(&never, GENERAL));
    assert!(!unread(&seen, GENERAL));
    assert!(unread(&older, GENERAL));
    assert!(unread(&no_entry, GENERAL), "never acknowledged");
    assert!(!unread(&never, VOICE), "no pin");
    assert!(!unread(&never, 999), "no channel");
}

#[test]
fn a_new_pin_shows_the_dot_and_an_ack_takes_it() {
    let mut state = state_from(&pinned_ready(PINNED.into()));

    let events = apply(
        &mut state,
        "CHANNEL_PINS_UPDATE",
        pins_update(GENERAL, LATER.into()),
    );
    assert_eq!(events, [changed(GENERAL)]);
    assert!(unread(&state, GENERAL));
    let repeated = apply(
        &mut state,
        "CHANNEL_PINS_UPDATE",
        pins_update(GENERAL, LATER.into()),
    );
    assert_eq!(repeated, [] as [&str; 0]);

    let events = apply(
        &mut state,
        "CHANNEL_PINS_ACK",
        pins_ack(GENERAL, LATER.into()),
    );
    assert_eq!(events, [changed(GENERAL)]);
    assert!(!unread(&state, GENERAL));
    assert_eq!(
        apply(
            &mut state,
            "CHANNEL_PINS_ACK",
            pins_ack(GENERAL, LATER.into())
        ),
        [] as [&str; 0]
    );
}

// The official client takes an ack without a time to be for the channel's newest pin.
#[test]
fn an_ack_without_a_time_is_for_the_newest_pin() {
    let mut state = state_from(&pinned_ready(Value::Null));
    assert!(unread(&state, GENERAL));

    let events = apply(
        &mut state,
        "CHANNEL_PINS_ACK",
        pins_ack(GENERAL, Value::Null),
    );

    assert_eq!(events, [changed(GENERAL)]);
    assert!(!unread(&state, GENERAL));
}

#[test]
fn an_older_ack_leaves_the_dot() {
    let mut state = state_from(&pinned_ready(Value::Null));

    let events = apply(
        &mut state,
        "CHANNEL_PINS_ACK",
        pins_ack(GENERAL, "2026-09-30T08:00:00+00:00".into()),
    );

    assert_eq!(events, [] as [&str; 0]);
    assert!(unread(&state, GENERAL));
}

#[test]
fn a_pins_update_without_a_time_takes_the_dot() {
    let mut state = state_from(&pinned_ready(Value::Null));

    let events = apply(
        &mut state,
        "CHANNEL_PINS_UPDATE",
        pins_update(GENERAL, Value::Null),
    );

    assert_eq!(events, [changed(GENERAL)]);
    assert!(!unread(&state, GENERAL));
    assert_eq!(
        apply(
            &mut state,
            "CHANNEL_PINS_UPDATE",
            pins_update(999, LATER.into())
        ),
        [] as [&str; 0],
        "no channel"
    );
}

#[test]
fn a_channel_update_brings_its_pin_time() {
    let mut state = state_from(&pinned_ready(PINNED.into()));
    let update = |time: Value| {
        json!({
            "id": GENERAL.to_string(), "guild_id": G1.to_string(), "last_pin_timestamp": time,
        })
    };

    let events = apply(&mut state, "CHANNEL_UPDATE", update(LATER.into()));
    assert!(events.contains(&changed(GENERAL)), "{events:?}");
    assert!(unread(&state, GENERAL));

    let unchanged = apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({"id": GENERAL.to_string(), "guild_id": G1.to_string(), "name": "renamed"}),
    );
    assert!(!unchanged.contains(&changed(GENERAL)), "{unchanged:?}");
    assert!(
        unread(&state, GENERAL),
        "a missing time keeps the known one"
    );

    let events = apply(&mut state, "CHANNEL_UPDATE", update(Value::Null));
    assert!(events.contains(&changed(GENERAL)), "{events:?}");
    assert!(!unread(&state, GENERAL));
}

#[test]
fn a_passive_update_brings_pin_times() {
    let mut state = state_from(&pinned_ready(PINNED.into()));

    let events = apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        json!({
            "guild_id": G1.to_string(),
            "updated_channels": [
                {"id": GENERAL.to_string(), "last_message_id": null, "last_pin_timestamp": LATER},
                {"id": VOICE.to_string(), "last_message_id": "400000000000000009"},
            ],
        }),
    );

    assert!(events.contains(&changed(GENERAL)), "{events:?}");
    assert!(!events.contains(&changed(VOICE)), "{events:?}");
    assert!(unread(&state, GENERAL));
}

// The official client's canBeUnread: a muted channel and a thread the user hasn't joined
// show no dot.
#[test]
fn a_muted_channel_and_an_unjoined_thread_have_no_dot() {
    let mut ready = pinned_ready(Value::Null);
    let general = &mut ready["user_guild_settings"]["entries"][0]["channel_overrides"][1];
    assert_eq!(general["channel_id"], GENERAL.to_string());
    general["muted"] = true.into();
    general["mute_config"] = Value::Null;
    let muted = state_from(&ready);
    assert!(!unread(&muted, GENERAL));

    let mut state = ready_state();
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread.as_object_mut().unwrap().remove("member");
    thread["id"] = "300000000000000031".into();
    thread["last_pin_timestamp"] = PINNED.into();
    apply(&mut state, "THREAD_CREATE", thread.clone());
    assert!(!unread(&state, 300_000_000_000_000_031));

    thread["member"] = json!({"flags": 0, "join_timestamp": PINNED});
    apply(&mut state, "THREAD_CREATE", thread);
    assert!(unread(&state, 300_000_000_000_000_031));
}

#[test]
fn a_later_ready_says_which_dots_changed() {
    let mut state = state_from(&pinned_ready(PINNED.into()));
    let mut later = pinned_ready(PINNED.into());
    later["guilds"][0]["channels"][1]["last_pin_timestamp"] = LATER.into();

    let events = apply(&mut state, "READY", later);

    assert!(events.contains(&changed(GENERAL)), "{events:?}");
    assert!(unread(&state, GENERAL));
    let again = apply(&mut state, "READY", pinned_ready(LATER.into()));
    assert!(again.contains(&changed(GENERAL)), "{again:?}");
    assert!(!unread(&state, GENERAL));
}

#[test]
fn a_pin_time_alone_is_no_channel_update_at_ready() {
    let mut state = state_from(&pinned_ready(PINNED.into()));
    let mut later = pinned_ready(PINNED.into());
    later["guilds"][0]["channels"][1]["last_pin_timestamp"] = LATER.into();

    let events = apply(&mut state, "READY", later);

    assert!(
        !events.contains(&format!("ChannelUpdated({GENERAL})")),
        "{events:?}"
    );
}
