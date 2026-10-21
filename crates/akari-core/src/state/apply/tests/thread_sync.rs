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
fn unjoined_threads_are_never_cached() {
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
