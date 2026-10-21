use super::*;

// Started from MESSAGE in GENERAL, so it has the message's ID.
const MESSAGE: u64 = 400_000_000_000_000_050;
const OTHER_THREAD: u64 = 300_000_000_000_000_030;

fn thread_object(id: u64, count: u32) -> Value {
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    let object = thread.as_object_mut().unwrap();
    object.remove("member");
    object.remove("newly_created");
    thread["id"] = id.to_string().into();
    thread["message_count"] = count.into();
    thread
}

fn with_thread(mut message: Value, count: u32) -> Value {
    let id: u64 = message["id"].as_str().unwrap().parse().unwrap();
    message["flags"] = 32.into();
    message["thread"] = thread_object(id, count);
    message
}

// A message from Mira that mentions nobody.
fn plain(id: u64, channel: u64) -> Value {
    let mut message = message(id, channel);
    message["mentions"] = json!([]);
    message
}

// A thread started without a message posts this line with the thread's ID and HAS_THREAD; one
// started from an older message posts it with an ID of its own and no flag.
fn line(id: u64, thread: u64) -> Value {
    let mut line = plain(id, GENERAL);
    line["type"] = 18.into();
    if id == thread {
        line["flags"] = 32.into();
    }
    line["content"] = "Bug triage".into();
    line["message_reference"] =
        json!({"channel_id": thread.to_string(), "guild_id": G1.to_string()});
    line
}

fn apply_events(state: &mut State, name: &str, data: Value) -> Vec<StoreEvent> {
    let mut events = Vec::new();
    state.apply(dispatch(name, &data), &mut events);
    events
}

fn updated(events: &[StoreEvent]) -> Vec<u64> {
    events
        .iter()
        .filter_map(|event| match event {
            StoreEvent::MessageUpdated(message) => Some(message.id.get()),
            _ => None,
        })
        .collect()
}

fn channel_updated(events: &[StoreEvent], channel: u64) -> bool {
    events.iter().any(
        |event| matches!(event, StoreEvent::ChannelUpdated(updated) if updated.id.get() == channel),
    )
}

fn count(state: &State, thread: u64) -> Option<u32> {
    state.channel(Snowflake::new(thread)).and_then(|channel| {
        channel
            .thread
            .as_deref()
            .and_then(|info| info.message_count)
    })
}

// GENERAL viewed, with MESSAGE and the thread started from it.
fn with_a_box() -> State {
    let mut state = viewing_general();
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_thread(plain(MESSAGE, GENERAL), 3),
    );
    state
}

#[test]
fn a_message_with_a_thread_brings_the_thread() {
    let mut state = viewing_general();

    let events = apply(
        &mut state,
        "MESSAGE_CREATE",
        with_thread(plain(MESSAGE, GENERAL), 3),
    );

    let thread = state.channel(Snowflake::new(MESSAGE)).expect("the thread");
    assert_eq!(thread.name.as_deref(), Some("Bug triage"));
    assert_eq!(thread.parent_id, Some(Snowflake::new(GENERAL)));
    assert!(!thread.thread.as_deref().unwrap().joined());
    assert_eq!(count(&state, MESSAGE), Some(3));
    assert!(events.contains(&format!("ChannelAdded({MESSAGE})")));
}

#[test]
fn a_page_with_a_thread_brings_the_thread() {
    let mut state = viewing_general();
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(GENERAL), LoadKind::Latest, 1, &mut events)
        .unwrap();
    let page = [
        plain(MESSAGE - 1, GENERAL),
        with_thread(plain(MESSAGE, GENERAL), 7),
    ]
    .iter()
    .map(|message| serde_json::from_str(&message.to_string()).unwrap())
    .collect();

    state.finish_load(ticket, page, false, &mut events);

    assert_eq!(count(&state, MESSAGE), Some(7));
}

#[test]
fn a_new_thread_on_a_message_arrives_with_its_update() {
    let mut state = viewing_general();
    apply(&mut state, "MESSAGE_CREATE", plain(MESSAGE, GENERAL));

    let update = json!({
        "id": MESSAGE.to_string(), "channel_id": GENERAL.to_string(),
        "guild_id": G1.to_string(), "flags": 32, "thread": thread_object(MESSAGE, 0)
    });
    apply(&mut state, "MESSAGE_UPDATE", update);

    assert!(state.channel(Snowflake::new(MESSAGE)).is_some());
}

#[test]
fn a_thread_without_its_guild_takes_its_parents() {
    let mut state = viewing_general();
    let mut message = with_thread(plain(MESSAGE, GENERAL), 3);
    message["thread"]
        .as_object_mut()
        .unwrap()
        .remove("guild_id");

    apply(&mut state, "MESSAGE_CREATE", message);

    let thread = state.channel(Snowflake::new(MESSAGE)).expect("the thread");
    assert_eq!(thread.guild_id, Some(Snowflake::new(G1)));
}

#[test]
fn a_reply_in_an_unjoined_thread_raises_its_count() {
    let mut state = with_a_box();

    apply(&mut state, "MESSAGE_CREATE", plain(MESSAGE + 1, MESSAGE));
    assert_eq!(count(&state, MESSAGE), Some(4));

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": (MESSAGE + 1).to_string(), "channel_id": MESSAGE.to_string(),
               "guild_id": G1.to_string()}),
    );
    assert_eq!(count(&state, MESSAGE), Some(3));
}

#[test]
fn a_count_never_goes_below_zero() {
    let mut state = viewing_general();
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_thread(plain(MESSAGE, GENERAL), 1),
    );
    let ids: Vec<String> = (1..=3).map(|n| (MESSAGE + n).to_string()).collect();

    apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": ids, "channel_id": MESSAGE.to_string(), "guild_id": G1.to_string()}),
    );

    assert_eq!(count(&state, MESSAGE), Some(0));
}

#[test]
fn a_count_change_rereads_the_box_and_not_the_channel() {
    let mut state = with_a_box();

    let events = apply_events(&mut state, "MESSAGE_CREATE", plain(MESSAGE + 1, MESSAGE));

    assert_eq!(updated(&events), [MESSAGE]);
    assert!(!channel_updated(&events, MESSAGE));
}

#[test]
fn a_started_a_thread_line_rereads_with_its_thread() {
    let mut state = viewing_general();
    apply(&mut state, "THREAD_CREATE", thread_object(OTHER_THREAD, 0));
    apply(
        &mut state,
        "MESSAGE_CREATE",
        line(OTHER_THREAD, OTHER_THREAD),
    );

    let events = apply_events(
        &mut state,
        "MESSAGE_CREATE",
        plain(MESSAGE + 6, OTHER_THREAD),
    );

    assert_eq!(updated(&events), [OTHER_THREAD]);
    assert_eq!(count(&state, OTHER_THREAD), Some(1));
}

// The box goes under the message the thread started from, not under the line about it.
#[test]
fn a_line_about_an_older_messages_thread_isnt_its_box() {
    let mut state = with_a_box();
    apply(&mut state, "MESSAGE_CREATE", line(MESSAGE + 5, MESSAGE));

    let events = apply_events(&mut state, "MESSAGE_CREATE", plain(MESSAGE + 6, MESSAGE));

    assert_eq!(updated(&events), [MESSAGE]);
}

#[test]
fn a_starter_message_isnt_counted() {
    let mut state = with_a_box();
    let mut starter = plain(MESSAGE + 1, MESSAGE);
    starter["type"] = 21.into();
    starter["message_reference"] =
        json!({"message_id": MESSAGE.to_string(), "channel_id": GENERAL.to_string()});

    let events = apply_events(&mut state, "MESSAGE_CREATE", starter);

    assert_eq!(count(&state, MESSAGE), Some(3));
    assert!(updated(&events).is_empty());
}

// A forum post's first message has the post's ID.
#[test]
fn a_posts_first_message_isnt_counted() {
    let mut state = with_a_box();

    apply(&mut state, "MESSAGE_CREATE", plain(MESSAGE, MESSAGE));
    assert_eq!(count(&state, MESSAGE), Some(3));

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": MESSAGE.to_string(), "channel_id": MESSAGE.to_string()}),
    );
    assert_eq!(count(&state, MESSAGE), Some(3));
}

#[test]
fn a_thread_without_a_count_counts_from_zero() {
    let mut state = viewing_general();
    let mut thread = thread_object(OTHER_THREAD, 0);
    thread.as_object_mut().unwrap().remove("message_count");
    apply(&mut state, "THREAD_CREATE", thread);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(MESSAGE + 6, OTHER_THREAD),
    );

    assert_eq!(count(&state, OTHER_THREAD), Some(1));
}

#[test]
fn a_count_stops_at_100k() {
    let mut state = viewing_general();
    apply(
        &mut state,
        "THREAD_CREATE",
        thread_object(OTHER_THREAD, 100_000),
    );

    apply(
        &mut state,
        "MESSAGE_CREATE",
        plain(MESSAGE + 6, OTHER_THREAD),
    );

    assert_eq!(count(&state, OTHER_THREAD), Some(100_000));
}

#[test]
fn a_thread_whose_parent_isnt_viewed_changes_without_events() {
    let mut state = ready_state();
    apply(&mut state, "THREAD_CREATE", thread_object(OTHER_THREAD, 0));

    let events = apply_events(
        &mut state,
        "MESSAGE_CREATE",
        plain(MESSAGE + 6, OTHER_THREAD),
    );

    assert!(updated(&events).is_empty());
    assert!(!channel_updated(&events, OTHER_THREAD));
    assert_eq!(count(&state, OTHER_THREAD), Some(1));
}

#[test]
fn unjoined_threads_are_never_listed_or_counted() {
    let mut state = with_a_box();
    let before = state.guild_badge(Snowflake::new(G1));

    // The fixture's message mentions the user.
    apply(&mut state, "MESSAGE_CREATE", message(MESSAGE + 1, MESSAGE));

    assert!(!state.listed(Snowflake::new(MESSAGE)));
    assert!(
        state
            .listed_threads(Snowflake::new(GENERAL))
            .iter()
            .all(|thread| thread.id.get() != MESSAGE)
    );
    assert_eq!(state.guild_badge(Snowflake::new(G1)), before);
}

#[test]
fn a_message_with_the_threads_id_but_without_the_flag_isnt_its_box() {
    let mut state = viewing_general();
    apply(&mut state, "THREAD_CREATE", thread_object(MESSAGE, 3));
    apply(&mut state, "MESSAGE_CREATE", plain(MESSAGE, GENERAL));

    let events = apply_events(&mut state, "MESSAGE_CREATE", plain(MESSAGE + 1, MESSAGE));

    assert!(updated(&events).is_empty());
    assert_eq!(count(&state, MESSAGE), Some(4));
}
