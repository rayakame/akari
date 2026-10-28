use super::*;
use crate::state::{Ack, AckBody, Badge, LoadKind, PostCount, ReadState};

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
        before: None,
        after: None,
        body: AckBody::Channel,
    })
}

fn at_now(channel: u64, cursor: u64) -> Option<Ack> {
    Some(Ack {
        channel: id(channel),
        cursor: Some(Snowflake::new(cursor)),
        now: true,
        before: None,
        after: None,
        body: AckBody::Channel,
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

// The ack without the state from before it, which has tests of its own.
fn target(ack: Ack) -> Ack {
    Ack {
        before: None,
        after: None,
        ..ack
    }
}

// One channel read: at most one ack.
fn single(mut acks: Vec<Ack>) -> Option<Ack> {
    assert!(acks.len() <= 1, "{acks:?}");
    acks.pop().map(target)
}

fn taken(state: &mut State) -> Option<Ack> {
    single(state.take_reading_acks())
}

fn reading(state: &mut State, channel: Option<u64>) -> (Option<Ack>, Vec<String>) {
    let mut events = Vec::new();
    let ack = single(state.set_reading(channel.map(id).as_slice(), &mut events));
    (ack, events.iter().map(describe).collect())
}

fn mark_read(state: &mut State, channel: u64) -> Option<Ack> {
    state.mark_read(id(channel), &mut Vec::new()).map(target)
}

// The channel's list, loaded at the present.
fn showing(state: &mut State, channel: u64, ids: &[u64]) {
    load_page(state, channel, LoadKind::Latest, ids, true);
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
    showing(&mut state, GENERAL, &[NEWEST]);
    assert_eq!(
        badge(&state, GENERAL),
        Badge {
            unread: true,
            mentions: 2,
            posts: PostCount::None,
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
    showing(&mut state, GENERAL, &[NEWEST]);

    let (ack, events) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, None);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
    assert_eq!(taken(&mut state), None);
}

#[test]
fn reading_a_channel_with_only_mentions_acks_it() {
    let mut state = ready_where(|ready| {
        ready["read_state"]["entries"][0]["last_message_id"] = NEWEST.to_string().into();
    });
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 2)));
    showing(&mut state, GENERAL, &[NEWEST]);

    let (ack, _) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, at(GENERAL, NEWEST));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
}

// The split view: the channel and the thread beside it are read at once.
#[test]
fn reading_two_channels_acks_both_locally() {
    let mut state = ready_where(thread_unread);
    let thread_newest = 400_000_000_000_000_030;
    showing(&mut state, GENERAL, &[NEWEST]);
    showing(&mut state, THREAD, &[thread_newest]);
    let thread_ack = |cursor| Ack {
        body: AckBody::Thread { joined: true },
        ..at(THREAD, cursor).unwrap()
    };

    let acks: Vec<Ack> = state
        .set_reading(&[id(GENERAL), id(THREAD)], &mut Vec::new())
        .into_iter()
        .map(target)
        .collect();

    assert_eq!(
        acks,
        [at(GENERAL, NEWEST).unwrap(), thread_ack(thread_newest)]
    );
    assert_eq!(badge(&state, GENERAL), Badge::default());
    assert_eq!(badge(&state, THREAD), Badge::default());
    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 1, GENERAL));
    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 1));
    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(thread_newest + 1, THREAD),
    );
    assert_eq!(taken(&mut state), Some(thread_ack(thread_newest + 1)));
    assert_eq!(
        read_state(&state, THREAD),
        Some((Some(thread_newest + 1), 0))
    );
}

#[test]
fn a_message_arriving_while_reading_is_read_at_once() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
    reading(&mut state, Some(GENERAL));

    let events = apply(&mut state, "MESSAGE_CREATE", message(NEWEST + 1, GENERAL));

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 1));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 1), 0)));
    assert_eq!(badge(&state, GENERAL), Badge::default());
    assert!(
        !events.contains(&format!("GuildBadgeChanged({G1})")),
        "{events:?}"
    );
}

#[test]
fn an_ack_carries_the_read_state_from_before_it() {
    let mut state = ready_state();
    showing(&mut state, GENERAL, &[NEWEST]);
    let ready = state.read_state(id(GENERAL));
    assert_eq!(ready.map(|read| read.mentions), Some(2));

    let read = state.set_reading(&[id(GENERAL)], &mut Vec::new()).pop();
    assert_eq!(read.map(|ack| ack.before), Some(ready));
    assert_eq!(
        read.map(|ack| ack.after),
        Some(state.read_state(id(GENERAL))),
        "and the one it left"
    );
    apply(&mut state, "MESSAGE_CREATE", message(NEWEST + 1, GENERAL));
    assert_eq!(
        state.take_reading_acks().pop().map(|ack| ack.before),
        Some(Some(ReadState {
            acked: Some(Snowflake::new(NEWEST)),
            mentions: 1,
            estimated: false
        })),
        "Akari's own position, with the new message's mention"
    );

    let mut state = ready_where(thread_unread);
    let ready = state.read_state(id(GENERAL));
    let marked = state.mark_read(id(GENERAL), &mut Vec::new());
    assert_eq!(marked.map(|ack| ack.before), Some(ready));
    assert_eq!(
        marked.map(|ack| ack.after),
        Some(state.read_state(id(GENERAL)))
    );

    let mut state = ready_where(thread_unread);
    let befores: Vec<_> = [GENERAL, THREAD]
        .into_iter()
        .map(|channel| (id(channel), state.read_state(id(channel))))
        .collect();
    let acks = state.mark_guilds_read(&[Snowflake::new(G1)], wall_millis(), &mut Vec::new());
    let carried: Vec<_> = acks.iter().map(|ack| (ack.channel, ack.before)).collect();
    assert_eq!(carried, befores);
    for ack in acks {
        assert_eq!(ack.after, state.read_state(ack.channel));
    }
}

#[test]
fn the_users_own_message_while_reading_needs_no_ack() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
    reading(&mut state, Some(GENERAL));
    let mut own = plain(NEWEST + 1, GENERAL);
    own["author"]["id"] = ME.to_string().into();

    apply(&mut state, "MESSAGE_CREATE", own);

    assert_eq!(taken(&mut state), None);
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 1), 0)));
}

#[test]
fn a_message_arriving_after_reading_stopped_makes_the_channel_unread() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
    reading(&mut state, Some(GENERAL));
    reading(&mut state, None);

    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 1, GENERAL));

    assert_eq!(taken(&mut state), None);
    assert!(badge(&state, GENERAL).unread);
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
}

#[test]
fn a_message_ack_from_elsewhere_while_reading_changes_nothing_more() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
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

    assert_eq!(taken(&mut state), None);
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST - 1), 1)));
}

#[test]
fn a_ready_acks_nothing_for_the_channel_being_read() {
    let mut state = State::new();
    showing(&mut state, GENERAL, &[NEWEST]);
    let (ack, _) = reading(&mut state, Some(GENERAL));
    assert_eq!(ack, None);

    state.apply(dispatch("READY", &ready_value()), &mut Vec::new());
    assert_eq!(taken(&mut state), None);
    assert!(badge(&state, GENERAL).unread);
    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 1, GENERAL));

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 1));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 1), 0)));
}

// A new session's READY leaves the list stale until its refresh comes.
#[test]
fn a_ready_acks_the_channel_being_read_once_its_list_is_refreshed() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
    reading(&mut state, Some(GENERAL));
    let mut next = ready_value();
    next["guilds"][0]["channels"][1]["last_message_id"] = (NEWEST + 2).to_string().into();

    state.apply(dispatch("READY", &next), &mut Vec::new());
    assert_eq!(taken(&mut state), None);
    assert!(badge(&state, GENERAL).unread);
    load_page(
        &mut state,
        GENERAL,
        LoadKind::Refresh,
        &[NEWEST, NEWEST + 1, NEWEST + 2],
        false,
    );

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 2));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 2), 0)));
}

#[test]
fn a_history_load_while_reading_is_read_at_once() {
    let mut state = general_read();
    reading(&mut state, Some(GENERAL));

    load_page(&mut state, GENERAL, LoadKind::Latest, &[NEWEST + 4], true);

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 4));
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
fn a_message_without_a_mention_clears_a_stale_cursor() {
    let mut state = general_read();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );

    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 5, GENERAL));
    assert_eq!(
        mark_read(&mut state, GENERAL),
        at_now(GENERAL, NEWEST + 6),
        "an older message leaves the cursor"
    );
    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 7, GENERAL));

    assert_eq!(mark_read(&mut state, GENERAL), at(GENERAL, NEWEST + 7));
}

// In a large guild Discord doesn't send every message to, these come anyway.
#[test]
fn a_mentioning_message_keeps_a_stale_cursor() {
    let mut everyone = plain(NEWEST + 8, GENERAL);
    everyone["mention_everyone"] = true.into();
    let mut role = plain(NEWEST + 9, GENERAL);
    role["mention_roles"] = json!(["500000000000000002"]);
    let mut state = general_read();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );

    for (mentioning, id) in [
        (message(NEWEST + 7, GENERAL), NEWEST + 7),
        (everyone, NEWEST + 8),
        (role, NEWEST + 9),
    ] {
        apply(&mut state, "MESSAGE_CREATE", mentioning);
        assert_eq!(mark_read(&mut state, GENERAL), at_now(GENERAL, id));
    }
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

// Never past the rows the list shows, and never at "now", even with a stale cursor.
#[test]
fn reading_acks_only_up_to_the_window() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
    reading(&mut state, Some(GENERAL));
    let mut merged = ready_value()["guilds"][0].clone();
    merged["channels"][1]["last_message_id"] = (NEWEST + 8).to_string().into();

    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );
    assert_eq!(taken(&mut state), None, "a passive update");
    apply(&mut state, "GUILD_CREATE", merged);
    assert_eq!(taken(&mut state), None, "a GUILD_CREATE merge");
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
    assert!(badge(&state, GENERAL).unread);

    apply(&mut state, "MESSAGE_CREATE", message(NEWEST + 7, GENERAL));
    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 7));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 7), 0)));
    assert!(badge(&state, GENERAL).unread, "the cursor is further");
    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 9, GENERAL));

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 9));
    assert_eq!(badge(&state, GENERAL), Badge::default());
}

#[test]
fn reading_a_channel_without_a_present_window_acks_nothing_until_it_loads() {
    let mut state = ready_state();

    let (ack, _) = reading(&mut state, Some(GENERAL));
    assert_eq!(ack, None, "no list");
    state.view_channel(id(GENERAL), &mut Vec::new());
    load_page(
        &mut state,
        GENERAL,
        LoadKind::Around(Snowflake::new(NEWEST - 1)),
        &[NEWEST - 1],
        false,
    );
    assert_eq!(taken(&mut state), None, "a list away from the present");
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST - 1), 2)));
    showing(&mut state, GENERAL, &[NEWEST - 1, NEWEST]);

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST));
    assert_eq!(badge(&state, GENERAL), Badge::default());

    let mut state = ready_state();
    showing(&mut state, GENERAL, &[NEWEST - 1]);
    state.apply(dispatch("READY", &ready_value()), &mut Vec::new());
    let (ack, _) = reading(&mut state, Some(GENERAL));
    assert_eq!(ack, None, "a stale list");
    load_page(
        &mut state,
        GENERAL,
        LoadKind::Refresh,
        &[NEWEST - 1, NEWEST],
        true,
    );

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST));
}

fn delete(state: &mut State, id: u64, channel: u64) {
    apply(
        state,
        "MESSAGE_DELETE",
        json!({"id": id.to_string(), "channel_id": channel.to_string()}),
    );
}

// Posted and deleted while the user was elsewhere: Discord keeps it as the newest message.
#[test]
fn reading_a_channel_whose_newest_message_was_deleted_reads_it() {
    let mut state = general_read();
    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 1, GENERAL));
    delete(&mut state, NEWEST + 1, GENERAL);
    assert!(badge(&state, GENERAL).unread);
    showing(&mut state, GENERAL, &[NEWEST]);

    let (ack, _) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, at(GENERAL, NEWEST + 1));
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 1), 0)));
    assert_eq!(badge(&state, GENERAL), Badge::default());
    assert_eq!(state.guild_badge(Snowflake::new(G1)), Badge::default());
}

#[test]
fn a_newest_row_deleted_while_reading_paused_is_read_once_it_resumes() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
    reading(&mut state, Some(GENERAL));
    reading(&mut state, None);
    apply(&mut state, "MESSAGE_CREATE", plain(NEWEST + 1, GENERAL));
    delete(&mut state, NEWEST + 1, GENERAL);
    assert!(badge(&state, GENERAL).unread);

    let (ack, _) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, at(GENERAL, NEWEST + 1));
    assert_eq!(badge(&state, GENERAL), Badge::default());
}

#[test]
fn reading_a_dm_whose_mentioning_message_was_deleted_reads_it() {
    let (dm_newest, deleted) = (400_000_000_000_000_020, 400_000_000_000_000_022);
    let mut state = ready_state();
    assert_eq!(read_state(&state, DM), Some((Some(dm_newest), 0)));
    apply(&mut state, "MESSAGE_CREATE", dm_message(deleted, DM));
    delete(&mut state, deleted, DM);
    assert!(state.unread(id(DM)));
    assert_eq!(badge(&state, DM).mentions, 1);
    showing(&mut state, DM, &[dm_newest]);

    let (ack, _) = reading(&mut state, Some(DM));

    assert_eq!(ack, at(DM, deleted));
    assert_eq!(read_state(&state, DM), Some((Some(deleted), 0)));
    assert!(!state.unread(id(DM)));
    assert_eq!(badge(&state, DM), Badge::default());
}

// A message that arrives after the page was asked for may be missing from it.
#[test]
fn a_page_asked_for_before_a_message_arrived_doesnt_read_it() {
    let mut state = general_read();
    let mut events = Vec::new();
    let ticket = state
        .begin_load(id(GENERAL), LoadKind::Latest, 1, &mut events)
        .unwrap();
    apply(
        &mut state,
        "PASSIVE_UPDATE_V2",
        passive(GENERAL, NEWEST + 6),
    );
    let page = vec![serde_json::from_str(&plain(NEWEST, GENERAL).to_string()).unwrap()];
    state.finish_load(ticket, page, true, &mut events);

    let (ack, _) = reading(&mut state, Some(GENERAL));

    assert_eq!(ack, None);
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST), 0)));
    assert!(badge(&state, GENERAL).unread);
}

fn page_of(ids: &[u64]) -> Vec<model::Message> {
    ids.iter()
        .map(|id| serde_json::from_str(&plain(*id, GENERAL).to_string()).unwrap())
        .collect()
}

fn create(state: &mut State, id: u64) {
    apply(state, "MESSAGE_CREATE", plain(id, GENERAL));
}

// GENERAL's list away from the present, at NEWEST - 1, read up to NEWEST and being read.
fn reading_in_history() -> State {
    let mut state = general_read();
    load_page(
        &mut state,
        GENERAL,
        LoadKind::Around(Snowflake::new(NEWEST - 1)),
        &[NEWEST - 1],
        false,
    );
    assert_eq!(reading(&mut state, Some(GENERAL)).0, None);
    state
}

// The refresh after READY holds live messages back until its page is in.
#[test]
fn a_message_created_and_deleted_during_a_held_load_is_read_once_it_lands() {
    let mut state = general_read();
    showing(&mut state, GENERAL, &[NEWEST]);
    let mut ready = ready_value();
    ready["read_state"]["entries"][0]["last_message_id"] = NEWEST.to_string().into();
    ready["read_state"]["entries"][0]["mention_count"] = 0.into();
    state.apply(dispatch("READY", &ready), &mut Vec::new());
    assert_eq!(reading(&mut state, Some(GENERAL)).0, None, "a stale list");
    let refresh = state
        .begin_load(id(GENERAL), LoadKind::Refresh, 2, &mut Vec::new())
        .unwrap();
    create(&mut state, NEWEST + 1);
    delete(&mut state, NEWEST + 1, GENERAL);

    state.finish_load(refresh, page_of(&[NEWEST]), true, &mut Vec::new());

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 1));
    assert_eq!(badge(&state, GENERAL), Badge::default());
}

// The list reached the present with a newer page while the jump to the present still holds
// messages: it can be read, but not past a message that isn't a row yet.
#[test]
fn a_deleted_message_below_a_held_one_reads_nothing_past_it_before_it_shows() {
    let mut state = reading_in_history();
    let present = state
        .begin_load(id(GENERAL), LoadKind::Latest, 2, &mut Vec::new())
        .unwrap();
    let newer = state
        .begin_load(id(GENERAL), LoadKind::Newer, 3, &mut Vec::new())
        .unwrap();
    create(&mut state, NEWEST + 1);
    create(&mut state, NEWEST + 2);
    create(&mut state, NEWEST + 3);
    delete(&mut state, NEWEST + 2, GENERAL);

    state.finish_load(newer, page_of(&[NEWEST]), true, &mut Vec::new());
    assert_eq!(taken(&mut state), None);
    assert_eq!(reading(&mut state, Some(GENERAL)).0, None, "two are held");
    state.finish_load(
        present,
        page_of(&[NEWEST - 1, NEWEST]),
        true,
        &mut Vec::new(),
    );

    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 3));
    assert_eq!(badge(&state, GENERAL), Badge::default());
}

#[test]
fn a_deleted_message_past_every_held_one_is_read_once_they_show() {
    let mut state = reading_in_history();
    let present = state
        .begin_load(id(GENERAL), LoadKind::Latest, 2, &mut Vec::new())
        .unwrap();
    create(&mut state, NEWEST + 1);
    create(&mut state, NEWEST + 2);
    delete(&mut state, NEWEST + 2, GENERAL);

    state.finish_load(
        present,
        page_of(&[NEWEST - 1, NEWEST]),
        true,
        &mut Vec::new(),
    );

    assert_eq!(
        message_ids(&state, GENERAL),
        Some(vec![NEWEST - 1, NEWEST, NEWEST + 1])
    );
    assert_eq!(taken(&mut state), at(GENERAL, NEWEST + 2));
    assert_eq!(badge(&state, GENERAL), Badge::default());
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
    (
        acks.into_iter().map(target).collect(),
        events.iter().map(describe).collect(),
    )
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
            Ack {
                body: AckBody::Thread { joined: true },
                ..at(THREAD, 400_000_000_000_000_030).unwrap()
            }
        ]
    );
    assert_eq!(
        state.guild_badge(Snowflake::new(G1)),
        Badge::default(),
        "an unjoined thread's ping waits for the join"
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
                now: true,
                before: None,
                after: None,
                body: AckBody::Channel,
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

fn undo(state: &mut State, ack: Ack) -> Vec<String> {
    let mut events = Vec::new();
    state.undo_ack(ack.channel, ack.before, ack.after, &mut events);
    events.iter().map(describe).collect()
}

#[test]
fn a_refused_ack_restores_the_channel() {
    let mut state = ready_state();
    let ready = state.read_state(id(GENERAL));
    let ack = state.mark_read(id(GENERAL), &mut Vec::new()).unwrap();
    assert_eq!(badge(&state, GENERAL), Badge::default());

    let events = undo(&mut state, ack);

    assert_eq!(state.read_state(id(GENERAL)), ready);
    assert_eq!(
        badge(&state, GENERAL),
        Badge {
            unread: true,
            mentions: 2,
            posts: PostCount::None,
        }
    );
    assert!(
        events.contains(&format!("BadgeChanged({GENERAL})"))
            && events.contains(&format!("GuildBadgeChanged({G1})")),
        "{events:?}"
    );

    let mut state = ready_where(|ready| {
        ready["read_state"]["entries"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        ready["guilds"][0]["channels"][1]["last_message_id"] = AFTER_JOIN.to_string().into();
    });
    let ack = state.mark_read(id(GENERAL), &mut Vec::new()).unwrap();
    undo(&mut state, ack);
    assert_eq!(state.read_state(id(GENERAL)), None, "no read state before");
    assert!(badge(&state, GENERAL).unread);
}

#[test]
fn a_refused_reading_ack_isnt_taken_again() {
    let mut state = ready_state();
    showing(&mut state, GENERAL, &[NEWEST]);
    let ack = state
        .set_reading(&[id(GENERAL)], &mut Vec::new())
        .pop()
        .unwrap();

    undo(&mut state, ack);
    apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({"id": GENERAL.to_string(), "topic": "t"}),
    );

    assert_eq!(taken(&mut state), None);
    assert!(badge(&state, GENERAL).unread);
}

#[test]
fn a_refused_ack_after_a_newer_read_changes_nothing() {
    let mut state = ready_state();
    let ack = state.mark_read(id(GENERAL), &mut Vec::new()).unwrap();
    apply(
        &mut state,
        "MESSAGE_ACK",
        json!({"channel_id": GENERAL.to_string(), "message_id": (NEWEST - 1).to_string(),
               "manual": true, "mention_count": 1, "version": 2}),
    );
    assert!(undo(&mut state, ack).is_empty(), "a MESSAGE_ACK");
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST - 1), 1)));

    let mut state = ready_state();
    let first = state.mark_read(id(GENERAL), &mut Vec::new()).unwrap();
    apply(&mut state, "MESSAGE_CREATE", message(NEWEST + 1, GENERAL));
    state.mark_read(id(GENERAL), &mut Vec::new()).unwrap();
    assert!(undo(&mut state, first).is_empty(), "another local ack");
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST + 1), 0)));

    let mut state = ready_state();
    let ack = state.mark_read(id(GENERAL), &mut Vec::new()).unwrap();
    let mut next = ready_value();
    next["read_state"]["entries"][0]["last_message_id"] = (NEWEST - 2).to_string().into();
    next["read_state"]["entries"][0]["mention_count"] = 0.into();
    state.apply(dispatch("READY", &next), &mut Vec::new());
    assert!(undo(&mut state, ack).is_empty(), "a READY");
    assert_eq!(read_state(&state, GENERAL), Some((Some(NEWEST - 2), 0)));
}
