use super::*;
use crate::state::{Ack, Badge, LoadKind};

const FORUM: u64 = 300_000_000_000_000_030;
const MEDIA: u64 = 300_000_000_000_000_032;
const NEWEST: u64 = 400_000_000_000_000_003;
const AFTER_JOIN: u64 = 1_300_000_000_000_000_000;

fn id(channel: u64) -> ChannelId {
    Snowflake::new(channel)
}

fn read_state(state: &State, channel: u64) -> Option<(Option<u64>, u32)> {
    state
        .read_state(id(channel))
        .map(|read| (read.acked.map(|id| id.get()), read.mentions))
}

fn badge(state: &State, channel: u64) -> Badge {
    state.channel_badge(id(channel))
}

fn at(channel: u64, cursor: u64) -> Option<Ack> {
    Some(Ack {
        channel: id(channel),
        cursor: Some(Snowflake::new(cursor)),
        now: false,
    })
}

fn at_now(channel: u64, cursor: u64) -> Option<Ack> {
    Some(Ack {
        channel: id(channel),
        cursor: Some(Snowflake::new(cursor)),
        now: true,
    })
}

// A message from Mira that mentions nobody.
fn plain(id: u64, channel: u64) -> Value {
    let mut message = message(id, channel);
    message["mentions"] = json!([]);
    message
}

fn passive(channel: u64, last: u64) -> Value {
    json!({
        "guild_id": G1.to_string(),
        "updated_channels": [{"id": channel.to_string(), "last_message_id": last.to_string()}]
    })
}

fn general_read() -> State {
    let mut state = ready_state();
    apply(
        &mut state,
        "MESSAGE_ACK",
        json!({"channel_id": GENERAL.to_string(), "message_id": NEWEST.to_string(), "version": 1}),
    );
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
    state
}

fn reading(state: &mut State, channel: Option<u64>) -> (Option<Ack>, Vec<String>) {
    let mut events = Vec::new();
    let ack = state.set_reading(channel.map(id), &mut events);
    (ack, events.iter().map(describe).collect())
}

fn mark_read(state: &mut State, channel: u64) -> Option<Ack> {
    state.mark_read(id(channel), &mut Vec::new())
}

fn channel_of(kind: u8, channel: u64, last: u64) -> Value {
    json!({
        "id": channel.to_string(), "type": kind, "guild_id": G1.to_string(), "name": "ideas",
        "position": 3, "permission_overwrites": [], "last_message_id": last.to_string()
    })
}

#[test]
fn reading_an_unread_channel_acks_it_locally() {
    let mut state = ready_state();
    assert_eq!(
        badge(&state, GENERAL),
        Badge {
            unread: true,
            mentions: 2
        }
    );

    let (ack, events) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, at(GENERAL, NEWEST));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
    assert_eq!(badge(&state, GENERAL), Badge::default());
    assert_eq!(state.guild_badge(Snowflake::new(G1)), Badge::default());
    assert!(
        events.contains(&format!("BadgeChanged({GENERAL})"))
            && events.contains(&format!("GuildBadgeChanged({G1})")),
        "{events:?}"
    );
}

#[test]
fn reading_a_read_channel_changes_nothing() {
    let mut state = general_read();

    let (ack, events) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, None);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
    assert_eq!(state.take_reading_ack(), None);
}

#[test]
fn reading_a_channel_with_only_mentions_acks_it() {
    let mut state = ready_where(|ready| {
        ready["read_state"]["entries"][0]["last_message_id"] = NEWEST.to_string().into();
    });
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 2)));

    let (ack, _) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, at(GENERAL, NEWEST));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
}

#[test]
fn a_message_arriving_while_reading_is_read_at_once() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));

    let events = apply(&mut state, "MESSAGE_CREATE", message(NEWEST + 1, GENERAL));

    assert_eq!(state.take_reading_ack(), at(GENERAL, NEWEST + 1));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 1), 0)));
    assert_eq!(badge(&state, GENERAL), Badge::default());
    assert!(
        !events.contains(&format!("GuildBadgeChanged({G1})")),
        "{events:?}"
    );
}

#[test]
fn the_users_own_message_while_reading_needs_no_ack() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));
    let mut own = plain(NEWEST + 1, GENERAL);
    own["author"]["id"] = ME.to_string().into();

    apply(&mut state, "MESSAGE_CREATE", own);

    assert_eq!(state.take_reading_ack(), None);
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 1), 0)));
}

#[test]
fn a_message_arriving_after_reading_stopped_makes_the_channel_unread() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));
    reading(&mut state, None);

    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 1, GENERAL));

    assert_eq!(state.take_reading_ack(), None);
    assert!(badge(&state, GENERAL).unread);
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
}

#[test]
fn a_message_ack_from_elsewhere_while_reading_changes_nothing_more() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));

    apply(
        &mut state,
        "MESSAGE_ACK",
        json!({"channel_id": GENERAL.to_string(), "message_id": (NEWEST - 1).to_string(),
               "manual": true, "mention_count": 1, "version": 2}),
    );
    apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({"id": GENERAL.to_string(), "topic": "t"}),
    );

    assert_eq!(state.take_reading_ack(), None);
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST - 1), 1)));
}

#[test]
fn reading_waits_for_ready() {
    let mut state = State::new();

    let (ack, _) = reading(&mut state, Some(GENERAL));
    assert_eq!(ack, None);
    state.apply(dispatch("READY", &ready_value()), &mut Vec::new());

    assert_eq!(state.take_reading_ack(), at(GENERAL, NEWEST));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
}

#[test]
fn every_ready_acks_the_channel_being_read() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));
    let mut next = ready_value();
    next["guilds"][0]["channels"][1]["last_message_id"] = (NEWEST + 2).to_string().into();

    state.apply(dispatch("READY", &next), &mut Vec::new());

    assert_eq!(state.take_reading_ack(), at(GENERAL, NEWEST + 2));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 2), 0)));
}

#[test]
fn a_history_load_while_reading_is_read_at_once() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));

    load_page(&mut state, GENERAL, LoadKind::Latest, &[NEWEST + 4], true);

    assert_eq!(state.take_reading_ack(), at(GENERAL, NEWEST + 4));
    assert_eq!(badge(&state, GENERAL), Badge::default());
}

#[test]
fn a_passive_update_marks_the_cursor_stale() {
    let mut state = general_read();

    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );

    assert_eq!(mark_read(&mut state, GENERAL), at_now(GENERAL, NEWEST + 6));
}

#[test]
fn a_channel_unread_update_marks_the_cursor_stale() {
    let mut state = general_read();

    apply(
        &mut state,
        "CHANNEL_UNREAD_UPDATE",
        json!({
            "guild_id": G1.to_string(),
            "channel_unread_updates": [{"id": GENERAL.to_string(), "last_message_id": (NEWEST + 6).to_string()}]
        }),
    );

    assert_eq!(mark_read(&mut state, GENERAL), at_now(GENERAL, NEWEST + 6));
}

#[test]
fn ready_clears_stale_cursors() {
    let mut state = general_read();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );

    apply(&mut state, "READY", ready_value());

    assert_eq!(mark_read(&mut state, GENERAL), at(GENERAL, NEWEST));
}

#[test]
fn a_load_at_the_present_clears_a_stale_cursor() {
    let mut state = general_read();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );

    load_page(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[NEWEST + 5, NEWEST + 6],
        false,
    );

    assert_eq!(mark_read(&mut state, GENERAL), at(GENERAL, NEWEST + 6));
}

#[test]
fn an_older_page_leaves_a_cursor_stale() {
    let mut state = general_read();
    load_page(&mut state, GENERAL, LoadKind::Latest, &[NEWEST + 5], false);
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );

    load_page(&mut state, GENERAL, LoadKind::Older, &[NEWEST + 4], false);

    assert_eq!(mark_read(&mut state, GENERAL), at_now(GENERAL, NEWEST + 6));
}

#[test]
fn forums_and_media_channels_are_acked_at_now() {
    let mut state = ready_state();
    apply(
        &mut state,
        "CHANNEL_CREATE",
        channel_of(15, FORUM, AFTER_JOIN + 1),
    );
    apply(
        &mut state,
        "CHANNEL_CREATE",
        channel_of(16, MEDIA, AFTER_JOIN + 2),
    );

    assert_eq!(mark_read(&mut state, FORUM), at_now(FORUM, AFTER_JOIN + 1));
    assert_eq!(mark_read(&mut state, MEDIA), at_now(MEDIA, AFTER_JOIN + 2));
    assert_eq!(read_state(&state, FORUM), Some((Some(AFTER_JOIN + 1), 0)));
}

#[test]
fn a_stale_cursor_is_acked_at_now_above_the_cursor() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));

    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );

    assert_eq!(state.take_reading_ack(), at_now(GENERAL, NEWEST + 6));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 6), 0)));
}

// From the disk cache, before the first READY.
fn restored_state() -> State {
    let DispatchEvent::Ready(ready) = dispatch("READY", &ready_value()) else {
        panic!("not a READY");
    };
    let cached = Entities::from_ready(*ready);
    let derived = Derived::build(&cached, wall_millis());
    let mut state = State::new();
    assert!(state.restore(cached, derived, &mut Vec::new()));
    state
}

// The cache's cursors may be behind Discord's.
#[test]
fn before_ready_every_cursor_is_acked_at_now() {
    let mut state = restored_state();
    assert_eq!(mark_read(&mut state, GENERAL), at_now(GENERAL, NEWEST));

    let mut state = restored_state();
    let (acks, _) = mark_guilds_read(&mut state, &[G1]);
    assert_eq!(acks.first().copied(), at_now(GENERAL, NEWEST));
    assert!(acks.iter().all(|ack| ack.now), "{acks:?}");
}

#[test]
fn mark_read_reads_the_channel_and_clears_its_mentions() {
    let mut state = ready_state();

    assert_eq!(mark_read(&mut state, GENERAL), at(GENERAL, NEWEST));

    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
    assert_eq!(badge(&state, GENERAL), Badge::default());
    assert_eq!(mark_read(&mut state, GENERAL), None, "read now");
}

#[test]
fn a_channel_without_a_read_state_is_marked_read_at_its_cursor() {
    let mut state = ready_where(|ready| {
        ready["read_state"]["entries"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        ready["guilds"][0]["channels"][1]["last_message_id"] = AFTER_JOIN.to_string().into();
    });
    assert!(state.unread(id(GENERAL)));

    assert_eq!(mark_read(&mut state, GENERAL), at(GENERAL, AFTER_JOIN));

    assert_eq!(read_state(&state, GENERAL), Some((Some(AFTER_JOIN), 0)));
    assert!(!state.read_state(id(GENERAL)).unwrap().estimated);
}

fn ready_where(edit: impl FnOnce(&mut Value)) -> State {
    let mut ready = ready_value();
    edit(&mut ready);
    state_from(&ready)
}

fn unjoined_thread(state: &mut State, mentioned: bool) -> u64 {
    let thread = 300_000_000_000_000_021;
    let mut created = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    created.as_object_mut().unwrap().remove("member");
    apply(state, "THREAD_CREATE", created);
    let message = if mentioned {
        message(AFTER_JOIN + 3, thread)
    } else {
        plain(AFTER_JOIN + 3, thread)
    };
    apply(state, "MESSAGE_CREATE", message);
    thread
}

fn thread_unread(ready: &mut Value) {
    ready["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
}

fn mark_guilds_read(state: &mut State, guilds: &[u64]) -> (Vec<Ack>, Vec<String>) {
    let mut events = Vec::new();
    let guilds: Vec<GuildId> = guilds.iter().map(|guild| Snowflake::new(*guild)).collect();
    let acks = state.mark_guilds_read(&guilds, wall_millis(), &mut events);
    (acks, events.iter().map(describe).collect())
}

#[test]
fn marking_a_guild_read_acks_its_unread_channels_and_joined_threads() {
    let mut state = ready_where(thread_unread);
    let unjoined = unjoined_thread(&mut state, true);
    assert!(badge(&state, unjoined).mentions > 0, "the ping shows");

    let (acks, _) = mark_guilds_read(&mut state, &[G1]);

    assert_eq!(
        acks,
        [
            at(GENERAL, NEWEST).unwrap(),
            at(THREAD, 400_000_000_000_000_030).unwrap()
        ]
    );
    assert_eq!(
        state.guild_badge(Snowflake::new(G1)),
        Badge {
            unread: false,
            mentions: 1
        }
    );
    assert_eq!(
        read_state(&state, THREAD),
        Some((Some(400_000_000_000_000_030), 0))
    );
    assert!(
        badge(&state, unjoined).mentions > 0,
        "unjoined threads stay"
    );
}

#[test]
fn every_viewable_forum_goes_into_a_guilds_ack() {
    let hidden = 300_000_000_000_000_033;
    let mut state = ready_where(|ready| {
        ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
        ready["merged_members"][0][0]["roles"] = json!([]);
    });
    let mut forum = channel_of(15, FORUM, AFTER_JOIN + 1);
    forum["last_message_id"] = Value::Null;
    apply(&mut state, "CHANNEL_CREATE", forum);
    apply(
        &mut state,
        "CHANNEL_CREATE",
        channel_of(16, MEDIA, BEFORE_JOIN),
    );
    let mut secret = channel_of(15, hidden, AFTER_JOIN + 2);
    secret["permission_overwrites"] =
        json!([{"id": G1.to_string(), "type": 0, "allow": "0", "deny": "1024"}]);
    apply(&mut state, "CHANNEL_CREATE", secret);

    let (acks, _) = mark_guilds_read(&mut state, &[G1]);

    assert_eq!(
        acks,
        [
            at(GENERAL, NEWEST).unwrap(),
            Ack {
                channel: id(FORUM),
                cursor: None,
                now: true
            },
            at_now(MEDIA, BEFORE_JOIN).unwrap(),
        ]
    );
}

const BEFORE_JOIN: u64 = 1_100_000_000_000_000_000;

#[test]
fn a_guild_with_nothing_but_forums_to_ack_changes_nothing() {
    let mut state = general_read();
    apply(
        &mut state,
        "CHANNEL_CREATE",
        channel_of(15, FORUM, AFTER_JOIN + 1),
    );
    assert!(state.unread(id(FORUM)));

    let (acks, events) = mark_guilds_read(&mut state, &[G1]);

    assert!(acks.is_empty(), "{acks:?}");
    assert!(events.is_empty(), "{events:?}");
    assert!(state.unread(id(FORUM)));
}

#[test]
fn marking_guilds_read_leaves_other_guilds_and_dms_alone() {
    let mut state = ready_state();
    apply(
        &mut state,
        "MESSAGE_CREATE",
        dm_message(400_000_000_000_000_022, GROUP),
    );

    let (acks, _) = mark_guilds_read(&mut state, &[G2]);

    assert!(acks.is_empty(), "{acks:?}");
    assert!(badge(&state, GENERAL).unread);
    assert!(state.unread(id(GROUP)));
}
