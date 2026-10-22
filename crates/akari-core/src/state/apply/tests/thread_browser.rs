use super::*;
use crate::model::ThreadSearchResults;
use crate::state::{OlderThreads, ThreadBrowser};

// Threads in GENERAL besides the joined THREAD from READY (newest message 400…030).
const ACTIVE_A: u64 = 300_000_000_000_000_041;
const ACTIVE_B: u64 = 300_000_000_000_000_042;
const OLD_X: u64 = 300_000_000_000_000_051;
const OLD_Y: u64 = 300_000_000_000_000_052;
const OLD_Z: u64 = 300_000_000_000_000_053;

fn thread_object(id: u64, newest: Option<u64>, archived: bool) -> Value {
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    let object = thread.as_object_mut().unwrap();
    object.remove("member");
    object.remove("newly_created");
    thread["id"] = id.to_string().into();
    thread["last_message_id"] = newest.map_or(Value::Null, |id| id.to_string().into());
    thread["thread_metadata"]["archived"] = archived.into();
    thread
}

fn private(mut thread: Value) -> Value {
    thread["type"] = 12.into();
    thread
}

// From text, as the account decodes it: the lenient lists borrow from it.
fn results(body: &Value) -> ThreadSearchResults {
    serde_json::from_str(&body.to_string()).unwrap()
}

fn page(threads: Vec<Value>, has_more: bool) -> ThreadSearchResults {
    results(&json!({
        "threads": threads, "members": [], "has_more": has_more,
        "total_results": 0, "most_recent_messages": []
    }))
}

fn not_indexed() -> ThreadSearchResults {
    results(
        &json!({"message": "Index not yet available. Try again later",
        "code": 110_000, "retry_after": 2}),
    )
}

fn browser(state: &State) -> ThreadBrowser {
    state.thread_browser(Snowflake::new(GENERAL))
}

fn sections(state: &State) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    let browser = browser(state);
    let ids = |threads: &[Arc<Channel>]| threads.iter().map(|thread| thread.id.get()).collect();
    (
        ids(&browser.joined),
        ids(&browser.others),
        ids(&browser.archived),
    )
}

fn load(state: &mut State, threads: Vec<Value>, has_more: bool) -> Vec<String> {
    let parent = Snowflake::new(GENERAL);
    let started = state.begin_older_threads(parent).expect("a page can load");
    let mut events = Vec::new();
    state.finish_older_threads(parent, started, page(threads, has_more), &mut events);
    events.iter().map(describe).collect()
}

fn with_old_threads() -> State {
    let mut state = ready_state();
    for (id, newest) in [
        (OLD_X, 400_000_000_000_000_015),
        (OLD_Y, 400_000_000_000_000_014),
    ] {
        apply(
            &mut state,
            "THREAD_CREATE",
            thread_object(id, Some(newest), true),
        );
    }
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(OLD_Z, Some(400_000_000_000_000_013), true),
    );
    state
}

// Neither the owner nor a moderator: no MANAGE_THREADS.
fn ready_without_moderator_role() -> Value {
    let mut ready = ready_value();
    ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
    ready["merged_members"][0][0]["roles"] = json!([]);
    ready
}

#[test]
fn the_browser_splits_active_threads_by_membership() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );

    assert_eq!(sections(&state), (vec![THREAD], vec![ACTIVE_A], vec![]));
}

#[test]
fn active_threads_follow_their_newest_message() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_B, Some(400_000_000_000_000_050), false),
    );
    assert_eq!(sections(&state).1, vec![ACTIVE_B, ACTIVE_A]);

    let mut message = message(400_000_000_000_000_060, ACTIVE_A);
    message.as_object_mut().unwrap().remove("guild_id");
    apply(&mut state, "MESSAGE_CREATE", message);

    assert_eq!(sections(&state).1, vec![ACTIVE_A, ACTIVE_B]);
}

#[test]
fn a_thread_without_messages_counts_from_its_own_id() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, None, false),
    );
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_B, Some(ACTIVE_A - 1), false),
    );

    assert_eq!(sections(&state).1, vec![ACTIVE_A, ACTIVE_B]);
}

#[test]
fn a_message_in_a_thread_is_thread_activity() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_061, THREAD),
    );

    assert!(
        events.contains(&format!("ThreadActivity({GENERAL}, {THREAD})")),
        "{events:?}"
    );
}

#[test]
fn a_message_in_a_channel_is_not_thread_activity() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "MESSAGE_CREATE",
        message(400_000_000_000_000_061, GENERAL),
    );

    assert!(
        !events
            .iter()
            .any(|event| event.starts_with("ThreadActivity")),
        "{events:?}"
    );
}

#[test]
fn unjoined_threads_kept_by_ready_are_unconfirmed() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );

    apply(&mut state, "READY", ready_value());

    assert!(state.channel(Snowflake::new(ACTIVE_A)).is_some());
    assert_eq!(sections(&state), (vec![THREAD], vec![], vec![]));
}

#[test]
fn a_sync_confirms_a_kept_thread() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );
    apply(&mut state, "READY", ready_value());

    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        json!({"guild_id": G1.to_string(), "channel_ids": [GENERAL.to_string()],
               "threads": [thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false)],
               "members": []}),
    );

    assert_eq!(sections(&state).1, vec![ACTIVE_A]);
}

#[test]
fn a_thread_update_confirms_a_kept_thread() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );
    apply(&mut state, "READY", ready_value());

    apply(
        &mut state,
        "THREAD_UPDATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );

    assert_eq!(sections(&state).1, vec![ACTIVE_A]);
}

#[test]
fn nothing_is_unconfirmed_before_the_first_ready() {
    let mut source = ready_state();
    apply(
        &mut source,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );
    let cached = source.entities.clone();
    let mut state = State::new();
    let derived = Derived::build(&cached, state.now_millis());

    assert!(state.restore(cached, derived, &mut Vec::new()));

    assert_eq!(sections(&state), (vec![THREAD], vec![ACTIVE_A], vec![]));
}

#[test]
fn archived_threads_are_none_before_the_first_page() {
    let state = with_old_threads();

    assert!(sections(&state).2.is_empty());
    assert_eq!(browser(&state).older, OlderThreads::NotLoaded);
}

#[test]
fn archived_threads_stop_at_the_oldest_loaded() {
    let mut state = with_old_threads();

    load(
        &mut state,
        vec![
            thread_object(OLD_X, Some(400_000_000_000_000_015), true),
            thread_object(OLD_Y, Some(400_000_000_000_000_014), true),
        ],
        true,
    );

    assert_eq!(sections(&state).2, vec![OLD_X, OLD_Y]);
    assert_eq!(browser(&state).older, OlderThreads::More);
}

#[test]
fn all_archived_threads_once_there_are_no_more() {
    let mut state = with_old_threads();

    load(
        &mut state,
        vec![thread_object(OLD_X, Some(400_000_000_000_000_015), true)],
        false,
    );

    assert_eq!(sections(&state).2, vec![OLD_X, OLD_Y, OLD_Z]);
    assert_eq!(browser(&state).older, OlderThreads::All);
}

#[test]
fn a_thread_archived_later_joins_the_archived_section() {
    let mut state = with_old_threads();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );
    load(
        &mut state,
        vec![thread_object(OLD_X, Some(400_000_000_000_000_015), true)],
        true,
    );

    apply(
        &mut state,
        "THREAD_UPDATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), true),
    );

    assert_eq!(
        sections(&state),
        (vec![THREAD], vec![], vec![ACTIVE_A, OLD_X])
    );
}

#[test]
fn a_private_thread_needs_membership_or_manage_threads() {
    let mut state = state_from(&ready_without_moderator_role());
    apply(
        &mut state,
        "THREAD_CREATE",
        private(thread_object(
            ACTIVE_A,
            Some(400_000_000_000_000_040),
            false,
        )),
    );
    assert!(sections(&state).1.is_empty());

    let mut joined = private(thread_object(
        ACTIVE_A,
        Some(400_000_000_000_000_040),
        false,
    ));
    joined["member"] = json!({"id": ACTIVE_A.to_string(), "user_id": ME.to_string(),
        "join_timestamp": "2026-10-08T12:00:00.000000+00:00", "flags": 1});
    apply(&mut state, "THREAD_CREATE", joined);
    assert_eq!(sections(&state).0, vec![ACTIVE_A, THREAD]);

    let mut moderator = ready_state();
    apply(
        &mut moderator,
        "THREAD_CREATE",
        private(thread_object(
            ACTIVE_B,
            Some(400_000_000_000_000_040),
            false,
        )),
    );
    assert_eq!(sections(&moderator).1, vec![ACTIVE_B]);
}

#[test]
fn threads_the_user_cant_view_are_left_out() {
    let mut ready = ready_without_moderator_role();
    ready["guilds"][0]["channels"][1]["permission_overwrites"] = json!([{
        "id": G1.to_string(), "type": 0, "allow": "0", "deny": "1024"
    }]);
    let mut state = state_from(&ready);
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );

    assert_eq!(sections(&state), (vec![], vec![], vec![]));
}

#[test]
fn ready_clears_the_pages() {
    let mut state = with_old_threads();
    load(
        &mut state,
        vec![thread_object(OLD_X, Some(400_000_000_000_000_015), true)],
        false,
    );

    apply(&mut state, "READY", ready_value());

    assert_eq!(browser(&state).older, OlderThreads::NotLoaded);
    assert!(sections(&state).2.is_empty());
}

#[test]
fn a_page_ready_overtook_keeps_its_threads_but_not_its_place() {
    let mut state = ready_state();
    let parent = Snowflake::new(GENERAL);
    let started = state.begin_older_threads(parent).unwrap();
    apply(&mut state, "READY", ready_value());

    state.finish_older_threads(
        parent,
        started,
        page(
            vec![thread_object(OLD_X, Some(400_000_000_000_000_015), true)],
            false,
        ),
        &mut Vec::new(),
    );

    assert!(state.channel(Snowflake::new(OLD_X)).is_some());
    assert_eq!(browser(&state).older, OlderThreads::NotLoaded);
}

#[test]
fn only_one_page_loads_at_a_time_and_none_after_the_last() {
    let mut state = ready_state();
    let parent = Snowflake::new(GENERAL);

    let first = state.begin_older_threads(parent).unwrap();
    assert_eq!(first.offset, 0);
    assert_eq!(browser(&state).older, OlderThreads::Loading);
    assert!(state.begin_older_threads(parent).is_none());

    state.finish_older_threads(
        parent,
        first,
        page(
            vec![thread_object(OLD_X, Some(400_000_000_000_000_015), true)],
            true,
        ),
        &mut Vec::new(),
    );
    let second = state.begin_older_threads(parent).unwrap();
    assert_eq!(second.offset, 25);
    state.finish_older_threads(parent, second, page(Vec::new(), false), &mut Vec::new());

    assert!(state.begin_older_threads(parent).is_none());
}

#[test]
fn a_failed_or_unindexed_page_fails_the_section() {
    let parent = Snowflake::new(GENERAL);
    let mut failed = with_old_threads();
    let started = failed.begin_older_threads(parent).unwrap();
    failed.fail_older_threads(parent, started);

    let mut unindexed = with_old_threads();
    let started = unindexed.begin_older_threads(parent).unwrap();
    unindexed.finish_older_threads(parent, started, not_indexed(), &mut Vec::new());

    for state in [&mut failed, &mut unindexed] {
        assert_eq!(browser(state).older, OlderThreads::Failed);
        assert!(sections(state).2.is_empty());
        assert!(state.begin_older_threads(parent).is_none());
    }
}

#[test]
fn a_page_doesnt_override_the_gateways_copy() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );

    let mut events = Vec::new();
    let found = state.keep_found_threads(
        page(
            vec![thread_object(ACTIVE_A, Some(400_000_000_000_000_040), true)],
            false,
        ),
        &mut events,
    );

    assert_eq!(found, Some(vec![Snowflake::new(ACTIVE_A)]));
    assert_eq!(sections(&state).1, vec![ACTIVE_A]);
}

#[test]
fn a_page_replaces_an_unconfirmed_copy() {
    let mut state = ready_state();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false),
    );
    apply(&mut state, "READY", ready_value());

    state.keep_found_threads(
        page(
            vec![thread_object(ACTIVE_A, Some(400_000_000_000_000_040), true)],
            false,
        ),
        &mut Vec::new(),
    );

    let archived = state
        .channel(Snowflake::new(ACTIVE_A))
        .and_then(|thread| thread.thread.as_deref().map(|info| info.archived));
    assert_eq!(archived, Some(true));
}

#[test]
fn found_threads_keep_their_members_owners_and_latest_messages() {
    let mut state = ready_state();
    let mut results: ThreadSearchResults = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/thread_search.json"
    ))
    .unwrap();
    // A user READY didn't bring.
    let owner = results.threads.as_mut().unwrap()[0].owner.as_mut().unwrap();
    owner.user.as_mut().unwrap().id = Snowflake::new(100_000_000_000_000_009);

    let found = state.keep_found_threads(results, &mut Vec::new());

    assert_eq!(
        found,
        Some(vec![
            Snowflake::new(300_000_000_000_000_041),
            Snowflake::new(300_000_000_000_000_042)
        ])
    );
    let first = state
        .channel(Snowflake::new(300_000_000_000_000_041))
        .unwrap();
    let info = first.thread.as_deref().unwrap();
    assert!(info.joined());
    assert_eq!(
        info.member_ids_preview.as_ref(),
        &[
            Snowflake::new(MIRA),
            Snowflake::new(100_000_000_000_000_003)
        ]
    );
    assert!(
        state
            .user(Snowflake::new(100_000_000_000_000_009))
            .is_some()
    );
    assert_eq!(
        state
            .latest_message(Snowflake::new(300_000_000_000_000_041))
            .map(|message| message.content.to_string())
            .as_deref(),
        Some("the last word")
    );
}

#[test]
fn an_unindexed_search_finds_nothing() {
    let mut state = ready_state();

    assert_eq!(
        state.keep_found_threads(not_indexed(), &mut Vec::new()),
        None
    );
}

#[test]
fn a_thread_update_keeps_the_preview_without_one() {
    let mut state = ready_state();
    let mut thread = thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false);
    thread["member_ids_preview"] = json!([MIRA.to_string()]);
    apply(&mut state, "THREAD_CREATE", thread);

    apply(
        &mut state,
        "THREAD_UPDATE",
        json!({"id": ACTIVE_A.to_string(), "guild_id": G1.to_string(), "name": "renamed"}),
    );

    let preview = state.channel(Snowflake::new(ACTIVE_A)).and_then(|thread| {
        thread
            .thread
            .as_deref()
            .map(|info| info.member_ids_preview.clone())
    });
    assert_eq!(preview.as_deref(), Some(&[Snowflake::new(MIRA)][..]));
}

#[test]
fn a_thread_payload_without_a_preview_keeps_the_known_one() {
    let mut state = ready_state();
    let mut thread = thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false);
    thread["member_ids_preview"] = json!([MIRA.to_string()]);
    apply(&mut state, "THREAD_CREATE", thread);

    apply(
        &mut state,
        "THREAD_LIST_SYNC",
        json!({"guild_id": G1.to_string(), "channel_ids": [GENERAL.to_string()],
               "threads": [thread_object(ACTIVE_A, Some(400_000_000_000_000_040), false)],
               "members": []}),
    );

    let preview = state.channel(Snowflake::new(ACTIVE_A)).and_then(|thread| {
        thread
            .thread
            .as_deref()
            .map(|info| info.member_ids_preview.clone())
    });
    assert_eq!(preview.as_deref(), Some(&[Snowflake::new(MIRA)][..]));
}

#[test]
fn a_threads_starter_message_is_thread_activity_too() {
    let mut state = ready_state();
    let mut starter = message(400_000_000_000_000_061, THREAD);
    starter["type"] = 21.into();

    let events = apply(&mut state, "MESSAGE_CREATE", starter);

    assert!(
        events.contains(&format!("ThreadActivity({GENERAL}, {THREAD})")),
        "{events:?}"
    );
}
