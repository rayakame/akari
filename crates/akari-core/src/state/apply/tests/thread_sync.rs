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
