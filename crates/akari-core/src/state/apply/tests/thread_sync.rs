use super::*;

// Active threads in GENERAL the user hasn't joined.
const SYNCED: u64 = 300_000_000_000_000_031;
const LATEST: u64 = 400_000_000_000_000_060;

fn thread_object(id: u64) -> Value {
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    let object = thread.as_object_mut().unwrap();
    object.remove("member");
    object.remove("newly_created");
    thread["id"] = id.to_string().into();
    thread
}

fn in_thread(id: u64, thread: u64, content: &str) -> Value {
    let mut message = message(id, thread);
    message["mentions"] = json!([]);
    message["content"] = content.into();
    message.as_object_mut().unwrap().remove("guild_id");
    message
}

fn sync(channels: Option<&[u64]>, threads: &[u64], latest: Vec<Value>) -> Value {
    let mut sync = json!({
        "guild_id": G1.to_string(),
        "threads": threads.iter().map(|id| thread_object(*id)).collect::<Vec<_>>(),
        "members": [],
        "most_recent_messages": latest,
    });
    if let Some(channels) = channels {
        sync["channel_ids"] = channels.iter().map(u64::to_string).collect();
    }
    sync
}

fn latest_content(state: &State, thread: u64) -> Option<String> {
    state
        .latest_message(Snowflake::new(thread))
        .map(|message| message.content.to_string())
}

fn joined(state: &State, thread: u64) -> Option<bool> {
    state
        .channel(Snowflake::new(thread))
        .and_then(|channel| channel.thread.as_deref().map(|info| info.joined()))
}

#[test]
fn a_sync_brings_unjoined_threads_with_their_latest_messages() {
    let mut state = ready_state();

    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "the newest")],
        ),
    );

    assert_eq!(joined(&state, SYNCED), Some(false));
    assert!(!state.listed(Snowflake::new(SYNCED)));
    assert_eq!(
        latest_content(&state, SYNCED).as_deref(),
        Some("the newest")
    );
}

#[test]
fn a_sync_never_drops_or_unjoins_a_joined_thread() {
    let mut state = ready_state();

    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED], Vec::new()),
    );
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[THREAD], Vec::new()),
    );

    assert_eq!(joined(&state, THREAD), Some(true));
}

#[test]
fn a_threads_latest_message_follows_live_messages() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "from the sync")],
        ),
    );

    apply(
        &mut state,
        "MESSAGE_CREATE",
        in_thread(LATEST + 1, SYNCED, "live"),
    );
    assert_eq!(latest_content(&state, SYNCED).as_deref(), Some("live"));

    apply(
        &mut state,
        "MESSAGE_UPDATE",
        json!({"id": (LATEST + 1).to_string(), "channel_id": SYNCED.to_string(),
               "content": "edited", "edited_timestamp": "2026-10-21T10:00:00.000000+00:00"}),
    );
    assert_eq!(latest_content(&state, SYNCED).as_deref(), Some("edited"));
}

#[test]
fn an_older_message_in_a_sync_doesnt_replace_a_newer_one() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", thread_object(SYNCED));
    apply(
        &mut state,
        "MESSAGE_CREATE",
        in_thread(LATEST + 1, SYNCED, "live"),
    );

    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "older")],
        ),
    );

    assert_eq!(latest_content(&state, SYNCED).as_deref(), Some("live"));
}

#[test]
fn a_deleted_thread_takes_its_latest_message_along() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "the newest")],
        ),
    );

    apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": SYNCED.to_string(), "guild_id": G1.to_string(),
               "parent_id": GENERAL.to_string(), "type": 11}),
    );

    assert!(state.latest_message(Snowflake::new(SYNCED)).is_none());
}

#[test]
fn a_new_latest_message_rereads_the_box() {
    let mut state = viewing_general();
    let mut started = message(SYNCED, GENERAL);
    started["flags"] = 32.into();
    started["thread"] = thread_object(SYNCED);
    apply(&mut state, "MESSAGE_CREATE", started);

    let mut events = Vec::new();
    state.apply(
        dispatch(
            "THREAD_LIST_SYNC",
            &sync(
                Some(&[GENERAL]),
                &[SYNCED],
                vec![in_thread(LATEST, SYNCED, "the newest")],
            ),
        ),
        &mut events,
    );

    assert!(
        events.iter().any(
            |event| matches!(event, StoreEvent::MessageUpdated(row) if row.id.get() == SYNCED)
        )
    );
}

#[test]
fn a_later_ready_keeps_unjoined_threads() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "the newest")],
        ),
    );

    let events = apply(&mut state, "READY", ready_value());

    assert_eq!(joined(&state, SYNCED), Some(false));
    assert!(!events.contains(&format!("ChannelRemoved({SYNCED})")));
    assert_eq!(
        latest_content(&state, SYNCED).as_deref(),
        Some("the newest")
    );
}

#[test]
fn unjoined_threads_without_a_cached_box_arent_cached() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED], Vec::new()),
    );

    let (entities, _) = state.cache_parts(50);

    assert!(!entities.channels.contains_key(&Snowflake::new(SYNCED)));
    assert!(entities.channels.contains_key(&Snowflake::new(THREAD)));
}

// As the official client: a sync adds and updates, and keeps what it doesn't list.
#[test]
fn a_sync_keeps_threads_it_doesnt_list() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED], Vec::new()),
    );

    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED + 1], Vec::new()),
    );
    apply(&mut state, "THREAD_LIST_SYNC", sync(None, &[], Vec::new()));

    assert_eq!(joined(&state, SYNCED), Some(false));
    assert_eq!(joined(&state, SYNCED + 1), Some(false));
}

#[test]
fn a_syncs_member_joins_its_thread() {
    let mut state = ready_state();
    let mut sync = sync(Some(&[GENERAL]), &[SYNCED], Vec::new());
    sync["members"] = json!([{
        "id": SYNCED.to_string(), "user_id": "100000000000000001",
        "join_timestamp": "2026-10-20T10:00:00.000000+00:00", "flags": 1,
        "muted": false, "mute_config": null
    }]);

    apply(&mut state, "THREAD_LIST_SYNC", sync);

    assert_eq!(joined(&state, SYNCED), Some(true));
}

#[test]
fn a_starter_message_never_becomes_the_latest() {
    let mut state = ready_state();
    let mut starter = in_thread(LATEST, SYNCED, "");
    starter["type"] = 21.into();

    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED], vec![starter.clone()]),
    );
    starter["id"] = (LATEST + 1).to_string().into();
    apply(&mut state, "MESSAGE_CREATE", starter);

    assert!(state.latest_message(Snowflake::new(SYNCED)).is_none());
}

#[test]
fn deleting_the_latest_message_leaves_none() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "the newest")],
        ),
    );

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": LATEST.to_string(), "channel_id": SYNCED.to_string()}),
    );

    assert!(state.latest_message(Snowflake::new(SYNCED)).is_none());
}

#[test]
fn a_thread_payload_naming_a_newer_message_drops_the_latest() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "the newest")],
        ),
    );
    let mut update = thread_object(SYNCED);
    update["last_message_id"] = (LATEST + 5).to_string().into();

    apply(&mut state, "THREAD_UPDATE", update);

    assert!(state.latest_message(Snowflake::new(SYNCED)).is_none());
}

fn load_thread_page(state: &mut State, ids: &[u64]) {
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(SYNCED), LoadKind::Latest, 1, &mut events)
        .unwrap();
    let page = ids
        .iter()
        .map(|id| {
            serde_json::from_str(&in_thread(*id, SYNCED, &format!("page {id}")).to_string())
                .unwrap()
        })
        .collect();
    state.finish_load(ticket, page, true, &mut events);
}

#[test]
fn a_threads_newest_page_sets_its_latest_message() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", thread_object(SYNCED));

    load_thread_page(&mut state, &[LATEST, LATEST + 1]);

    assert_eq!(
        latest_content(&state, SYNCED),
        Some(format!("page {}", LATEST + 1))
    );
}

#[test]
fn an_empty_newest_page_leaves_no_latest_and_no_count() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(
            Some(&[GENERAL]),
            &[SYNCED],
            vec![in_thread(LATEST, SYNCED, "the newest")],
        ),
    );

    load_thread_page(&mut state, &[]);

    assert!(state.latest_message(Snowflake::new(SYNCED)).is_none());
    let count = state
        .channel(Snowflake::new(SYNCED))
        .and_then(|channel| channel.thread.as_deref()?.message_count);
    assert_eq!(count, Some(0));
}

fn synced_with_latest(state: &mut State) {
    let mut thread = thread_object(SYNCED);
    thread["message_count"] = 4.into();
    let mut sync = sync(
        Some(&[GENERAL]),
        &[],
        vec![in_thread(LATEST, SYNCED, "the newest")],
    );
    sync["threads"] = json!([thread]);
    apply(state, "THREAD_LIST_SYNC", sync);
}

fn count_of(state: &State, thread: u64) -> Option<u32> {
    state
        .channel(Snowflake::new(thread))
        .and_then(|channel| channel.thread.as_deref()?.message_count)
}

fn page_of(ids: &[u64]) -> Vec<model::Message> {
    ids.iter()
        .map(|id| {
            serde_json::from_str(&in_thread(*id, SYNCED, &format!("page {id}")).to_string())
                .unwrap()
        })
        .collect()
}

fn load(state: &mut State, kind: LoadKind, ids: &[u64], reached_end: bool) {
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(SYNCED), kind, 1, &mut events)
        .unwrap();
    state.finish_load(ticket, page_of(ids), reached_end, &mut events);
}

// Only the thread's newest page says it has no messages; an empty page after the newest doesn't.
#[test]
fn an_empty_page_after_the_newest_leaves_the_latest_and_the_count() {
    let mut state = ready_state();
    synced_with_latest(&mut state);
    load(
        &mut state,
        LoadKind::Around(Snowflake::new(LATEST)),
        &[LATEST - 1, LATEST],
        false,
    );

    load(&mut state, LoadKind::Newer, &[], true);

    assert_eq!(
        latest_content(&state, SYNCED).as_deref(),
        Some("the newest")
    );
    assert_eq!(count_of(&state, SYNCED), Some(4));
}

#[test]
fn a_live_message_during_the_newest_page_stays_the_latest() {
    let mut state = ready_state();
    synced_with_latest(&mut state);
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(SYNCED), LoadKind::Latest, 1, &mut events)
        .unwrap();

    apply(
        &mut state,
        "MESSAGE_CREATE",
        in_thread(LATEST + 5, SYNCED, "live"),
    );
    state.finish_load(ticket, page_of(&[LATEST]), true, &mut events);

    assert_eq!(latest_content(&state, SYNCED).as_deref(), Some("live"));
}

#[test]
fn a_message_deleted_during_the_newest_page_isnt_the_latest() {
    let mut state = ready_state();
    synced_with_latest(&mut state);
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(SYNCED), LoadKind::Latest, 1, &mut events)
        .unwrap();

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": (LATEST + 1).to_string(), "channel_id": SYNCED.to_string()}),
    );
    state.finish_load(ticket, page_of(&[LATEST, LATEST + 1]), true, &mut events);

    assert_eq!(
        latest_content(&state, SYNCED),
        Some(format!("page {LATEST}"))
    );
}

#[test]
fn a_newest_page_that_changes_nothing_rereads_no_box() {
    let mut state = viewing_general();
    let mut started = message(SYNCED, GENERAL);
    started["flags"] = 32.into();
    started["thread"] = thread_object(SYNCED);
    apply(&mut state, "MESSAGE_CREATE", started);
    load(&mut state, LoadKind::Latest, &[LATEST], true);
    let mut events = Vec::new();
    state.view_channel(Snowflake::new(SYNCED), &mut events);
    let ticket = state.begin_load(Snowflake::new(SYNCED), LoadKind::Latest, 2, &mut events);
    let ticket = ticket.unwrap();

    state.finish_load(ticket, page_of(&[LATEST]), true, &mut events);

    assert!(
        !events.iter().any(
            |event| matches!(event, StoreEvent::MessageUpdated(row) if row.id.get() == SYNCED)
        )
    );
}

// Discord keeps a deleted message as the thread's last_message_id; the one before it stays.
#[test]
fn a_payload_naming_a_deleted_newest_message_keeps_the_latest() {
    let mut state = viewing_general();
    let mut thread = thread_object(SYNCED);
    thread["last_message_id"] = (LATEST + 9).to_string().into();
    let mut sync = sync(
        Some(&[GENERAL]),
        &[],
        vec![in_thread(LATEST, SYNCED, "the newest left")],
    );
    sync["threads"] = json!([thread.clone()]);
    apply(&mut state, "THREAD_LIST_SYNC", sync);

    apply(&mut state, "THREAD_UPDATE", thread.clone());
    let mut started = message(SYNCED, GENERAL);
    started["flags"] = 32.into();
    started["thread"] = thread;
    apply(&mut state, "MESSAGE_CREATE", started);

    assert_eq!(
        latest_content(&state, SYNCED).as_deref(),
        Some("the newest left")
    );
}

#[test]
fn a_repeated_guild_create_keeps_unjoined_threads() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED], Vec::new()),
    );

    let events = apply(
        &mut state,
        "GUILD_CREATE",
        ready_value()["guilds"][0].clone(),
    );

    assert_eq!(joined(&state, SYNCED), Some(false));
    assert!(!events.contains(&format!("ChannelRemoved({SYNCED})")));
}

// Without its guild or a known parent there's nowhere to put it yet.
#[test]
fn a_thread_without_a_guild_or_a_known_parent_waits_unstored() {
    let mut state = ready_state();
    let mut thread = thread_object(SYNCED);
    thread.as_object_mut().unwrap().remove("guild_id");
    thread["parent_id"] = "300000000000000099".into();
    let mut message = message(LATEST, GENERAL);
    message["flags"] = 32.into();
    message["thread"] = thread;

    apply(&mut state, "MESSAGE_CREATE", message);

    assert!(state.channel(Snowflake::new(SYNCED)).is_none());
    assert!(
        state
            .private_channels()
            .iter()
            .all(|channel| channel.id.get() != SYNCED)
    );
}

fn general_page_with_a_box(state: &mut State) {
    let mut started = message(LATEST, GENERAL);
    started["flags"] = 32.into();
    let mut thread = thread_object(LATEST);
    thread["thread_metadata"]["archived"] = true.into();
    started["thread"] = thread;
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(GENERAL), LoadKind::Latest, 1, &mut events)
        .unwrap();
    let page = vec![serde_json::from_str(&started.to_string()).unwrap()];
    state.finish_load(ticket, page, true, &mut events);
}

// A relaunch shows the cached message with its box before the gateway brings the thread again.
#[test]
fn the_thread_of_a_cached_messages_box_is_cached() {
    let mut state = ready_state();
    general_page_with_a_box(&mut state);
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED], Vec::new()),
    );

    let (entities, windows) = state.cache_parts(50);

    assert!(windows.iter().any(|(channel, _)| channel.get() == GENERAL));
    let cached = entities
        .channels
        .get(&Snowflake::new(LATEST))
        .expect("the box's thread");
    assert!(!cached.thread.as_deref().unwrap().joined());
    assert!(!entities.channels.contains_key(&Snowflake::new(SYNCED)));
}

// The snapshot evicts every window whose channel it doesn't write.
#[test]
fn an_unjoined_thread_with_a_cached_window_is_cached() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        sync(Some(&[GENERAL]), &[SYNCED], Vec::new()),
    );
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(SYNCED), LoadKind::Latest, 1, &mut events)
        .unwrap();
    let page = vec![serde_json::from_value(in_thread(LATEST, SYNCED, "read")).unwrap()];
    state.finish_load(ticket, page, true, &mut events);

    let (entities, windows) = state.cache_parts(50);

    assert!(windows.iter().any(|(channel, _)| channel.get() == SYNCED));
    assert!(entities.channels.contains_key(&Snowflake::new(SYNCED)));
}

// Akari loads the last channel while it connects; before READY the store has no guild to put a
// message's thread in.
#[test]
fn a_page_before_its_guild_is_known_keeps_its_threads_for_ready() {
    let mut state = State::new();
    general_page_with_a_box(&mut state);
    assert!(state.channel(Snowflake::new(LATEST)).is_none());

    apply(&mut state, "READY", ready_value());

    let thread = state
        .channel(Snowflake::new(LATEST))
        .expect("the page's thread");
    assert!(!thread.thread.as_deref().unwrap().joined());
}
