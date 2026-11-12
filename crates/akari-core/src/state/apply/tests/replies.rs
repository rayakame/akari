use super::*;
use crate::model::{MessageReferenceType, MessageType};
use crate::state::windows::StarterFetch;
use crate::state::{MessageReference, ReplyPreview, ReplyTarget, StarterMessage};

const TARGET: u64 = 400_000_000_000_000_005;

fn viewing(channel: u64) -> State {
    let mut state = ready_state();
    state.view_channel(Snowflake::new(channel), &mut Vec::new());
    state
}

// A reply to TARGET in `channel`, with the target as Discord sends it along: an object, `null`
// for a deleted one, or `None` for no key at all.
fn reply(id: u64, channel: u64, target: Option<Value>) -> Value {
    reply_to(id, channel, TARGET, target)
}

fn reply_to(id: u64, channel: u64, target_id: u64, target: Option<Value>) -> Value {
    let mut reply = message(id, channel);
    reply["type"] = 19.into();
    reply["message_reference"] = json!({
        "type": 0, "message_id": target_id.to_string(), "channel_id": channel.to_string(),
    });
    if let Some(target) = target {
        reply["referenced_message"] = target;
    }
    reply
}

fn original(channel: u64, content: &str) -> Value {
    let mut original = message(TARGET, channel);
    original["content"] = content.into();
    original
}

fn target(state: &State, channel: u64, id: u64) -> String {
    match state.reply_target(Snowflake::new(channel), Snowflake::new(id)) {
        ReplyTarget::Loaded(preview) => format!(
            "{}{}",
            preview.content,
            if preview.edited { " (edited)" } else { "" }
        ),
        ReplyTarget::Deleted => "deleted".to_owned(),
        ReplyTarget::NotLoaded => "not loaded".to_owned(),
    }
}

fn changed(channel: u64, id: u64) -> String {
    format!("ReplyTargetChanged({channel}, {id})")
}

fn page(messages: &[Value]) -> Vec<model::Message> {
    messages
        .iter()
        .map(|message| serde_json::from_str(&message.to_string()).unwrap())
        .collect()
}

fn load(state: &mut State, channel: u64, kind: LoadKind, messages: &[Value]) -> Vec<String> {
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(channel), kind, 1, &mut events)
        .unwrap();
    state.finish_load(ticket, page(messages), true, &mut events);
    events.iter().map(describe).collect()
}

#[test]
fn a_reply_brings_its_target_in_three_states() {
    let mut state = viewing(GENERAL);

    let events = apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(original(GENERAL, "the original"))),
    );
    assert_eq!(
        events,
        [changed(GENERAL, TARGET), "MessageInserted(10)".to_owned()]
    );
    assert_eq!(target(&state, GENERAL, TARGET), "the original");

    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(11, GENERAL, 6, Some(Value::Null)),
    );
    assert_eq!(target(&state, GENERAL, 6), "deleted");

    let events = apply(&mut state, "MESSAGE_CREATE", reply_to(12, GENERAL, 7, None));
    assert_eq!(events, ["MessageInserted(12)"], "not loaded is what it was");
    assert_eq!(target(&state, GENERAL, 7), "not loaded");
    assert_eq!(target(&state, GENERAL, 8), "not loaded", "never named");

    let record = state
        .message(Snowflake::new(GENERAL), Snowflake::new(10))
        .unwrap();
    assert_eq!(
        record.replied_to(),
        Some((Snowflake::new(GENERAL), Snowflake::new(TARGET)))
    );
}

fn loaded(state: &State, channel: u64, id: u64) -> Arc<ReplyPreview> {
    match state.reply_target(Snowflake::new(channel), Snowflake::new(id)) {
        ReplyTarget::Loaded(preview) => preview,
        other => panic!("expected the target, got {other:?}"),
    }
}

// (attachments, embeds, sticker)
fn extras(preview: &ReplyPreview) -> (bool, bool, bool) {
    (
        preview.has_attachments,
        preview.has_embeds,
        preview.has_sticker,
    )
}

fn rich() -> Value {
    fixture(include_str!("../../../../tests/fixtures/message.json"))
}

// TARGET in GENERAL with only `part` of what a line names besides its text.
fn carrying(part: &str) -> Value {
    let mut target = message(TARGET, GENERAL);
    target[part] = rich()[part].clone();
    target
}

fn forward(mut message: Value) -> Value {
    message["message_reference"] = json!({
        "type": 1, "channel_id": VOICE.to_string(), "message_id": "400000000000000001",
    });
    message
}

// Every field of the preview, each from a target that would tell it from its neighbour.
fn the_line_shows_each_part(bring: impl Fn(&mut State, Value)) {
    let mut state = viewing(GENERAL);
    let mut command = carrying("attachments");
    command["type"] = 20.into();
    command["flags"] = (1 << 13).into();
    command["edited_timestamp"] = "2026-10-08T12:00:00+00:00".into();
    command["webhook_id"] = "900000000000000001".into();
    bring(&mut state, command);

    let preview = loaded(&state, GENERAL, TARGET);
    assert_eq!(preview.kind, MessageType::ChatInputCommand);
    assert_eq!(preview.author.id.get(), MIRA);
    assert!(preview.webhook);
    assert_eq!(&*preview.content, "hi");
    assert!(preview.edited);
    assert_eq!(preview.flags, Message::IS_VOICE_MESSAGE);
    assert_eq!(extras(&preview), (true, false, false));
    assert!(!preview.forwarded);

    for (part, expected) in [
        ("embeds", (false, true, false)),
        ("sticker_items", (false, false, true)),
    ] {
        let mut state = viewing(GENERAL);
        bring(&mut state, carrying(part));

        let preview = loaded(&state, GENERAL, TARGET);
        assert_eq!(extras(&preview), expected, "{part}");
        assert_eq!(preview.kind, MessageType::Default, "{part}");
        assert!(!preview.webhook, "{part}");
        assert!(!preview.edited, "{part}");
        assert_eq!(preview.flags, 0, "{part}");
        assert!(!preview.forwarded, "{part}");
    }
}

#[test]
fn a_preview_keeps_what_the_line_shows() {
    the_line_shows_each_part(|state, target| {
        apply(state, "MESSAGE_CREATE", reply(10, GENERAL, Some(target)));
    });
}

#[test]
fn a_target_from_the_window_keeps_what_the_line_shows() {
    let from_the_window = |state: &mut State, target: Value| {
        apply(state, "MESSAGE_CREATE", target);
        apply(state, "MESSAGE_CREATE", reply(10, GENERAL, None));
    };
    the_line_shows_each_part(from_the_window);

    let mut state = viewing(GENERAL);
    from_the_window(&mut state, forward(message(TARGET, GENERAL)));
    assert!(loaded(&state, GENERAL, TARGET).forwarded);
}

#[test]
fn a_forwarded_target_sent_along_says_so() {
    let mut state = viewing(GENERAL);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(forward(message(TARGET, GENERAL)))),
    );

    assert!(loaded(&state, GENERAL, TARGET).forwarded);
}

#[test]
fn an_edit_patches_each_part_of_the_preview() {
    let mut state = viewing(GENERAL);
    let mut before = original(GENERAL, "before");
    before["edited_timestamp"] = "2026-10-08T12:00:00+00:00".into();
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(before)),
    );
    let rich = rich();
    let mut other = rich["author"].clone();
    other["id"] = "100000000000000003".into();
    type Holds = fn(&ReplyPreview) -> bool;
    let edits: Vec<(&str, Value, Holds)> = vec![
        ("attachments", rich["attachments"].clone(), |preview| {
            extras(preview) == (true, false, false)
        }),
        ("attachments", json!([]), |preview| {
            extras(preview) == (false, false, false)
        }),
        ("embeds", rich["embeds"].clone(), |preview| {
            extras(preview) == (false, true, false)
        }),
        ("sticker_items", rich["sticker_items"].clone(), |preview| {
            extras(preview) == (false, true, true)
        }),
        ("flags", (1 << 13).into(), |preview| {
            preview.flags == Message::IS_VOICE_MESSAGE
        }),
        ("author", other, |preview| {
            preview.author.id.get() == 100_000_000_000_000_003
        }),
        ("type", 20.into(), |preview| {
            preview.kind == MessageType::ChatInputCommand
        }),
        ("webhook_id", "900000000000000001".into(), |preview| {
            preview.webhook
        }),
        ("edited_timestamp", Value::Null, |preview| !preview.edited),
    ];

    for (field, value, holds) in edits {
        assert!(!holds(&loaded(&state, GENERAL, TARGET)), "{field}, before");
        let mut edit = json!({"id": TARGET.to_string(), "channel_id": GENERAL.to_string()});
        edit[field] = value;

        let events = apply(&mut state, "MESSAGE_UPDATE", edit);

        let told: Vec<&String> = events
            .iter()
            .filter(|event| event.starts_with("ReplyTargetChanged"))
            .collect();
        assert_eq!(told, [&changed(GENERAL, TARGET)], "{field}");
        let after = loaded(&state, GENERAL, TARGET);
        assert!(holds(&after), "{field}: {after:?}");
        assert_eq!(&*after.content, "before", "{field}");
    }
}

#[test]
fn a_second_reply_with_the_same_target_says_nothing() {
    let mut state = viewing(GENERAL);
    let same = || Some(original(GENERAL, "the original"));
    apply(&mut state, "MESSAGE_CREATE", reply(10, GENERAL, same()));
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(11, GENERAL, 6, Some(Value::Null)),
    );

    let again = apply(&mut state, "MESSAGE_CREATE", reply(12, GENERAL, same()));
    let gone_again = apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(13, GENERAL, 6, Some(Value::Null)),
    );

    assert_eq!(again, ["MessageInserted(12)"]);
    assert_eq!(gone_again, ["MessageInserted(13)"]);
}

// A command's result may answer a message of another channel: the target is that channel's.
#[test]
fn a_reply_into_another_channel_keeps_its_target_there() {
    let mut state = viewing(GENERAL);
    let mut command = message(10, GENERAL);
    command["type"] = 23.into();
    command["message_reference"] = json!({
        "type": 0, "message_id": TARGET.to_string(), "channel_id": VOICE.to_string(),
    });
    command["referenced_message"] = original(VOICE, "elsewhere");

    let events = load(&mut state, GENERAL, LoadKind::Latest, &[command]);

    assert!(events.contains(&changed(VOICE, TARGET)), "{events:?}");
    assert!(!events.contains(&changed(GENERAL, TARGET)), "{events:?}");
    assert_eq!(target(&state, VOICE, TARGET), "elsewhere");
    assert_eq!(target(&state, GENERAL, TARGET), "not loaded");
    let row = state
        .message(Snowflake::new(GENERAL), Snowflake::new(10))
        .unwrap();
    assert_eq!(
        row.replied_to(),
        Some((Snowflake::new(VOICE), Snowflake::new(TARGET)))
    );

    let (_, windows) = state.cache_parts(50);
    let general = Snowflake::new(GENERAL);
    let (_, cached) = windows.iter().find(|(id, _)| *id == general).unwrap();
    let targets: Vec<(u64, u64, String)> = cached
        .targets
        .iter()
        .map(|((channel, id), target)| {
            let content = match target {
                ReplyTarget::Loaded(preview) => preview.content.to_string(),
                other => format!("{other:?}"),
            };
            (channel.get(), id.get(), content)
        })
        .collect();
    assert_eq!(targets, [(VOICE, TARGET, "elsewhere".to_owned())]);
}

#[test]
fn a_target_the_window_has_needs_no_copy_sent_along() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", original(GENERAL, "loaded"));

    apply(&mut state, "MESSAGE_CREATE", reply(10, GENERAL, None));

    assert_eq!(target(&state, GENERAL, TARGET), "loaded");
}

#[test]
fn a_target_that_arrives_live_is_loaded() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", reply(10, GENERAL, None));
    assert_eq!(target(&state, GENERAL, TARGET), "not loaded");

    let events = apply(&mut state, "MESSAGE_CREATE", original(GENERAL, "at last"));

    assert_eq!(
        events[..2],
        [
            changed(GENERAL, TARGET),
            format!("MessageInserted({TARGET})")
        ]
    );
    assert_eq!(target(&state, GENERAL, TARGET), "at last");
}

#[test]
fn a_page_resolves_its_replies_from_itself() {
    let mut state = viewing(GENERAL);

    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[original(GENERAL, "same page"), reply(10, GENERAL, None)],
    );

    assert_eq!(target(&state, GENERAL, TARGET), "same page");
}

#[test]
fn a_target_that_wasnt_loaded_is_once_a_load_brings_it() {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[reply(10, GENERAL, None)],
    );
    assert_eq!(target(&state, GENERAL, TARGET), "not loaded");

    let events = load(
        &mut state,
        GENERAL,
        LoadKind::Older,
        &[original(GENERAL, "older")],
    );

    assert!(events.contains(&changed(GENERAL, TARGET)), "{events:?}");
    assert_eq!(target(&state, GENERAL, TARGET), "older");
}

#[test]
fn an_edit_of_the_target_reaches_its_preview() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(original(GENERAL, "before"))),
    );
    let edit = json!({
        "id": TARGET.to_string(), "channel_id": GENERAL.to_string(), "content": "after",
        "edited_timestamp": "2026-10-08T12:00:00+00:00",
    });

    let events = apply(&mut state, "MESSAGE_UPDATE", edit.clone());
    assert_eq!(
        events,
        [changed(GENERAL, TARGET)],
        "the target isn't in the window"
    );
    assert_eq!(target(&state, GENERAL, TARGET), "after (edited)");
    assert_eq!(apply(&mut state, "MESSAGE_UPDATE", edit), [] as [&str; 0]);

    let unnamed = json!({"id": "77", "channel_id": GENERAL.to_string(), "content": "x"});
    assert_eq!(
        apply(&mut state, "MESSAGE_UPDATE", unnamed),
        [] as [&str; 0]
    );
    assert_eq!(target(&state, GENERAL, 77), "not loaded");
}

#[test]
fn an_edit_never_loads_a_target_that_is_deleted_or_not_loaded() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", reply(10, GENERAL, None));
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(11, GENERAL, 6, Some(Value::Null)),
    );

    for id in [TARGET, 6] {
        let edit = json!({"id": id.to_string(), "channel_id": GENERAL.to_string(), "content": "x"});
        assert_eq!(apply(&mut state, "MESSAGE_UPDATE", edit), [] as [&str; 0]);
    }

    assert_eq!(target(&state, GENERAL, TARGET), "not loaded");
    assert_eq!(target(&state, GENERAL, 6), "deleted");
}

#[test]
fn deleting_the_target_says_so_on_every_reply() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(original(GENERAL, "going"))),
    );
    apply(&mut state, "MESSAGE_CREATE", reply_to(11, GENERAL, 6, None));
    let delete = json!({"id": TARGET.to_string(), "channel_id": GENERAL.to_string()});

    let events = apply(&mut state, "MESSAGE_DELETE", delete.clone());
    assert_eq!(events, [changed(GENERAL, TARGET)]);
    assert_eq!(target(&state, GENERAL, TARGET), "deleted");
    assert_eq!(apply(&mut state, "MESSAGE_DELETE", delete), [] as [&str; 0]);

    let events = apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": ["6", "99"], "channel_id": GENERAL.to_string()}),
    );
    assert_eq!(events, [changed(GENERAL, 6)]);
    assert_eq!(target(&state, GENERAL, 6), "deleted");
    assert_eq!(
        target(&state, GENERAL, 99),
        "not loaded",
        "no reply names it"
    );
}

#[test]
fn a_deleted_target_in_the_window_goes_with_its_row() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", original(GENERAL, "here"));
    apply(&mut state, "MESSAGE_CREATE", reply(10, GENERAL, None));

    let events = apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": TARGET.to_string(), "channel_id": GENERAL.to_string()}),
    );

    assert_eq!(
        events,
        [
            changed(GENERAL, TARGET),
            format!("MessageDeleted({TARGET})")
        ]
    );
    assert_eq!(target(&state, GENERAL, TARGET), "deleted");
}

// A page asked before the target changed brings its old copy along.
#[test]
fn what_happened_to_a_target_during_a_load_is_put_over_the_page() {
    for (name, data, after) in [
        (
            "MESSAGE_DELETE",
            json!({"id": TARGET.to_string(), "channel_id": GENERAL.to_string()}),
            "deleted",
        ),
        (
            "MESSAGE_UPDATE",
            json!({"id": TARGET.to_string(), "channel_id": GENERAL.to_string(), "content": "new"}),
            "new",
        ),
    ] {
        let mut state = viewing(GENERAL);
        let mut events = Vec::new();
        let ticket = state
            .begin_load(Snowflake::new(GENERAL), LoadKind::Latest, 1, &mut events)
            .unwrap();
        apply(&mut state, name, data);

        let stale = [reply(10, GENERAL, Some(original(GENERAL, "old")))];
        state.finish_load(ticket, page(&stale), true, &mut events);

        assert_eq!(target(&state, GENERAL, TARGET), after, "{name}");
    }
}

#[test]
fn only_replies_and_command_results_have_targets() {
    let mut state = viewing(GENERAL);
    let mut command = reply(10, GENERAL, Some(original(GENERAL, "the message")));
    command["type"] = 23.into();
    let mut pin = reply_to(11, GENERAL, 6, Some(original(GENERAL, "pinned")));
    pin["type"] = 6.into();
    let mut forward = reply_to(12, GENERAL, 7, Some(original(GENERAL, "forwarded")));
    forward["type"] = 0.into();

    apply(&mut state, "MESSAGE_CREATE", command);
    let events = apply(&mut state, "MESSAGE_CREATE", pin);
    apply(&mut state, "MESSAGE_CREATE", forward);

    assert_eq!(target(&state, GENERAL, TARGET), "the message");
    assert_eq!(events, ["MessageInserted(11)"]);
    assert_eq!(target(&state, GENERAL, 6), "not loaded");
    assert_eq!(target(&state, GENERAL, 7), "not loaded");
    let line = state
        .message(Snowflake::new(GENERAL), Snowflake::new(11))
        .unwrap();
    assert_eq!(line.replied_to(), None);
}

// The official client keeps 100 targets a channel; Akari keeps those too, and besides them
// every target a loaded reply names, so no line falls back to "not loaded" under its reader.
#[test]
fn targets_are_bounded_but_never_dropped_under_a_loaded_reply() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    let replies: Vec<Value> = (0..150_u64)
        .map(|index| {
            let target = 1000 + index;
            let mut original = message(target, GENERAL);
            original["content"] = format!("original {target}").into();
            reply_to(2000 + index, GENERAL, target, Some(original))
        })
        .collect();

    load(&mut state, GENERAL, LoadKind::Latest, &replies);
    assert_eq!(state.windows.targets_len(general), 150);
    assert_eq!(target(&state, GENERAL, 1000), "original 1000");

    // A jump replaces the window: the replies are gone, and their targets may go.
    let mut events = Vec::new();
    let ticket = state
        .begin_load(
            general,
            LoadKind::Around(Snowflake::new(5000)),
            2,
            &mut events,
        )
        .unwrap();
    let around = [
        reply_to(5000, GENERAL, 4000, Some(original(GENERAL, "far away"))),
        reply_to(5001, GENERAL, 1149, None),
    ];
    state.finish_load(ticket, page(&around), false, &mut events);

    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(target(&state, GENERAL, 4000), "far away");
    assert_eq!(
        target(&state, GENERAL, 1000),
        "not loaded",
        "the longest unnamed went"
    );
    assert_eq!(
        target(&state, GENERAL, 1050),
        "not loaded",
        "and the next, down to the bound"
    );
    assert_eq!(target(&state, GENERAL, 1051), "original 1051");
    assert_eq!(
        target(&state, GENERAL, 1149),
        "original 1149",
        "named by a loaded reply"
    );
}

// What stays is what was named last, not what came first.
#[test]
fn a_target_named_again_outlives_older_ones() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    let replies: Vec<Value> = (0..150_u64)
        .map(|index| {
            let target = 1000 + index;
            let mut original = message(target, GENERAL);
            original["content"] = format!("original {target}").into();
            reply_to(2000 + index, GENERAL, target, Some(original))
        })
        .collect();
    load(&mut state, GENERAL, LoadKind::Latest, &replies);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(3000, GENERAL, 1000, None),
    );
    assert_eq!(target(&state, GENERAL, 1000), "original 1000");

    let mut events = Vec::new();
    let ticket = state
        .begin_load(
            general,
            LoadKind::Around(Snowflake::new(5000)),
            2,
            &mut events,
        )
        .unwrap();
    let around = [reply_to(
        5000,
        GENERAL,
        4000,
        Some(original(GENERAL, "far away")),
    )];
    state.finish_load(ticket, page(&around), false, &mut events);

    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(
        target(&state, GENERAL, 1000),
        "original 1000",
        "no row names it any more; it was named after the others"
    );
    assert_eq!(target(&state, GENERAL, 1001), "not loaded");
    assert_eq!(target(&state, GENERAL, 1051), "not loaded");
    assert_eq!(target(&state, GENERAL, 1052), "original 1052");
}

const KEPT: u64 = 500;

fn with_own_target(id: u64) -> Value {
    reply_to(
        id,
        GENERAL,
        id + 10_000,
        Some(message(id + 10_000, GENERAL)),
    )
}

// 150 replies come and go in GENERAL, each with its target, then one more comes: more targets
// than the bound, all newer than what the tests below named first.
fn churn(state: &mut State) {
    let ids: Vec<u64> = (3001..=3150).collect();
    for id in &ids {
        apply(state, "MESSAGE_CREATE", with_own_target(*id));
    }
    let gone: Vec<String> = ids.iter().map(u64::to_string).collect();
    apply(
        state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": gone, "channel_id": GENERAL.to_string()}),
    );
    apply(state, "MESSAGE_CREATE", with_own_target(9000));
}

#[test]
fn live_replies_keep_the_bound() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);

    for id in 1001..=1350_u64 {
        apply(&mut state, "MESSAGE_CREATE", with_own_target(id));
    }

    let rows = message_ids(&state, GENERAL).unwrap();
    assert_eq!(rows.len(), 200, "the window's own bound");
    assert_eq!(
        state.windows.targets_len(general),
        200,
        "every row's target and no other"
    );
    assert_eq!(target(&state, GENERAL, 11_001), "not loaded");
    assert_ne!(target(&state, GENERAL, 11_350), "not loaded");
}

#[test]
fn a_cache_fill_keeps_the_bound() {
    use crate::state::CachedMessages;

    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    let ids: Vec<u64> = (3001..=3150).collect();
    let replies: Vec<Value> = ids.iter().map(|id| with_own_target(*id)).collect();
    load(&mut state, GENERAL, LoadKind::Latest, &replies);
    let gone: Vec<String> = ids.iter().map(u64::to_string).collect();
    apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": gone, "channel_id": GENERAL.to_string()}),
    );
    assert_eq!(state.windows.targets_len(general), 150);

    let mut events = Vec::new();
    let ticket = state
        .begin_load(general, LoadKind::Latest, 2, &mut events)
        .unwrap();
    let cached = CachedMessages {
        messages: vec![stored(reply(10, GENERAL, None))],
        targets: [(
            (general, Snowflake::new(TARGET)),
            ReplyTarget::Loaded(Arc::new(ReplyPreview::of(&stored(original(
                GENERAL,
                "from the file",
            ))))),
        )]
        .into(),
    };
    state.fill_from_cache(ticket, cached, &mut events);

    assert_eq!(message_ids(&state, GENERAL).unwrap(), [10]);
    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(target(&state, GENERAL, TARGET), "from the file");
}

// Live messages wait behind a stale window; they are rows as soon as its page lands.
#[test]
fn a_held_replys_target_is_never_trimmed() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    state.windows.mark_stale(&mut Vec::new());
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(10, GENERAL, KEPT, Some(original(GENERAL, "kept"))),
    );
    assert_eq!(state.windows.held(general).len(), 1);
    assert!(message_ids(&state, GENERAL).unwrap().is_empty());

    churn(&mut state);

    assert_eq!(state.windows.held(general).len(), 2);
    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(target(&state, GENERAL, KEPT), "kept");
    assert_eq!(target(&state, GENERAL, 13_001), "not loaded");
}

#[test]
fn an_unsent_replys_target_is_never_trimmed() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(10, GENERAL, KEPT, Some(original(GENERAL, "kept"))),
    );
    let me = state.current_user().unwrap();
    let unsent = Message {
        kind: MessageType::Reply,
        reference: Some(Box::new(MessageReference {
            kind: MessageReferenceType::Default,
            message_id: Some(Snowflake::new(KEPT)),
            channel_id: Some(general),
            guild_id: None,
        })),
        ..Message::pending(
            Snowflake::new(20),
            general,
            Arc::new(me.user.clone()),
            "hi".to_owned(),
            0,
        )
    };
    let now = state.now_millis();
    state.queue(general, unsent, None, now, &mut Vec::new());
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "10", "channel_id": GENERAL.to_string()}),
    );

    churn(&mut state);

    assert_eq!(message_ids(&state, GENERAL).unwrap(), [9000]);
    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(target(&state, GENERAL, KEPT), "kept");
    assert_eq!(target(&state, GENERAL, 13_001), "not loaded");
}

#[test]
fn a_shown_starters_target_is_never_trimmed() {
    let answering = || reply_to(19, GENERAL, KEPT, Some(original(GENERAL, "kept")));
    let sent_along = |state: &mut State| {
        apply(state, "MESSAGE_CREATE", starter_row(Some(answering())));
    };
    let fetched = |state: &mut State| {
        apply(state, "MESSAGE_CREATE", starter_row(None));
        let thread = Snowflake::new(THREAD);
        assert!(state.starter_to_fetch(thread).is_some());
        let found = page(&[answering()]).remove(0);
        state.settle_starter(
            thread,
            StarterFetch::Found(Box::new(found)),
            &mut Vec::new(),
        );
    };
    type Bring<'a> = &'a dyn Fn(&mut State);
    let cases: [(&str, Bring); 2] = [("sent along", &sent_along), ("fetched", &fetched)];

    for (name, bring) in cases {
        let mut state = viewing(THREAD);
        let general = Snowflake::new(GENERAL);
        state.view_channel(general, &mut Vec::new());
        bring(&mut state);
        assert_eq!(target(&state, GENERAL, KEPT), "kept", "{name}");

        churn(&mut state);

        assert_eq!(state.windows.targets_len(general), 100, "{name}");
        assert_eq!(target(&state, GENERAL, KEPT), "kept", "{name}");
        assert_eq!(target(&state, GENERAL, 13_001), "not loaded", "{name}");
    }
}

#[test]
fn replies_naming_another_channel_dont_protect_this_ones_targets() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    state.view_channel(Snowflake::new(VOICE), &mut Vec::new());
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(10, GENERAL, KEPT, Some(original(GENERAL, "here"))),
    );
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "10", "channel_id": GENERAL.to_string()}),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(20, VOICE, KEPT, Some(original(VOICE, "there"))),
    );

    churn(&mut state);

    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(target(&state, GENERAL, KEPT), "not loaded");
    assert_eq!(target(&state, VOICE, KEPT), "there");
}

#[test]
fn a_window_that_goes_takes_its_targets() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(original(GENERAL, "kept while viewed"))),
    );
    assert_eq!(state.windows.targets_len(general), 1);

    apply(
        &mut state,
        "CHANNEL_DELETE",
        json!({"id": GENERAL.to_string(), "guild_id": G1.to_string()}),
    );

    assert_eq!(state.windows.targets_len(general), 0);
}

// The reply's ping is in its mentions: the line's "@".
#[test]
fn a_reply_says_whether_it_pinged() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    let mut silent = message(11, GENERAL);
    silent["mentions"] = json!([]);
    apply(&mut state, "MESSAGE_CREATE", silent);
    let read = |id: u64| {
        state
            .message(Snowflake::new(GENERAL), Snowflake::new(id))
            .unwrap()
    };

    assert!(read(10).mentions_user(Snowflake::new(ME)));
    assert!(!read(10).mentions_user(Snowflake::new(MIRA)));
    assert!(!read(11).mentions_user(Snowflake::new(ME)));
}

// Thread starters keep their own path: the message a thread started from is no reply target,
// and one that is itself a reply brings its target for the row that shows it.
#[test]
fn a_thread_starter_shows_its_message_and_that_messages_reply_line() {
    let mut state = viewing(THREAD);
    let mut starter = message(21, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({
        "message_id": "19", "channel_id": GENERAL.to_string(),
    });
    let mut origin = reply_to(19, GENERAL, TARGET, Some(original(GENERAL, "answered")));
    origin["content"] = "started here".into();
    starter["referenced_message"] = origin;

    apply(&mut state, "MESSAGE_CREATE", starter);

    let Some(StarterMessage::Loaded(shown)) = state.starter_message(Snowflake::new(THREAD)) else {
        panic!("expected the starter");
    };
    assert_eq!(&*shown.content, "started here");
    assert_eq!(
        target(&state, GENERAL, 19),
        "not loaded",
        "no reply names it"
    );
    assert_eq!(
        shown.replied_to(),
        Some((Snowflake::new(GENERAL), Snowflake::new(TARGET)))
    );
    assert_eq!(target(&state, GENERAL, TARGET), "answered");
}

#[test]
fn a_starter_discord_says_is_gone_isnt_fetched() {
    let mut state = viewing(THREAD);
    let mut starter = message(21, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({
        "message_id": "19", "channel_id": GENERAL.to_string(),
    });
    starter["referenced_message"] = Value::Null;

    apply(&mut state, "MESSAGE_CREATE", starter);

    let thread = Snowflake::new(THREAD);
    assert_eq!(
        state.starter_message(thread),
        Some(StarterMessage::Unavailable)
    );
    assert!(!state.wants_starter(thread));
}

#[test]
fn a_fetched_starter_that_is_a_reply_brings_its_target() {
    let mut state = viewing(THREAD);
    let mut starter = message(21, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({
        "message_id": "19", "channel_id": GENERAL.to_string(),
    });
    apply(&mut state, "MESSAGE_CREATE", starter);
    let thread = Snowflake::new(THREAD);
    assert!(state.starter_to_fetch(thread).is_some());

    let origin = reply_to(19, GENERAL, TARGET, Some(original(GENERAL, "answered")));
    let found = page(&[origin]).remove(0);
    let mut events = Vec::new();
    state.settle_starter(thread, StarterFetch::Found(Box::new(found)), &mut events);

    assert_eq!(target(&state, GENERAL, TARGET), "answered");
}

// The least recently viewed window goes when an eleventh is viewed.
#[test]
fn an_evicted_window_takes_its_targets() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(original(GENERAL, "kept while viewed"))),
    );

    for other in 1..=10_u64 {
        state.view_channel(Snowflake::new(900 + other), &mut Vec::new());
    }

    assert!(state.messages(general).is_none());
    assert_eq!(state.windows.targets_len(general), 0);
}

// The disk cache's copy of a window brings what its replies answered; what this session
// learned since stays.
#[test]
fn a_cached_window_brings_its_targets() {
    use crate::state::{CachedMessages, ReplyPreview};

    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    let convert = |value: Value| {
        let wire = page(&[value]).remove(0);
        Arc::new(Message::from_wire(wire, &mut |user| {
            Arc::new(crate::state::User::from_wire(user))
        }))
    };
    let cached_preview = |content: &str| {
        ReplyTarget::Loaded(Arc::new(ReplyPreview::of(&convert(original(
            GENERAL, content,
        )))))
    };
    let mut events = Vec::new();
    let ticket = state
        .begin_load(general, LoadKind::Latest, 1, &mut events)
        .unwrap();
    // Held: the window waits for its page, and the target is this session's.
    state.windows.mark_stale(&mut Vec::new());
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(9, GENERAL, 6, Some(original(GENERAL, "live"))),
    );
    assert!(state.messages(general).unwrap().messages.is_empty());

    let cached = CachedMessages {
        messages: vec![
            convert(reply(10, GENERAL, None)),
            convert(reply_to(11, GENERAL, 6, None)),
        ],
        targets: [
            (
                (general, Snowflake::new(TARGET)),
                cached_preview("from the file"),
            ),
            (
                (general, Snowflake::new(6)),
                cached_preview("older than the session"),
            ),
        ]
        .into(),
    };
    let mut events = Vec::new();
    state.fill_from_cache(ticket, cached, &mut events);

    assert_eq!(target(&state, GENERAL, TARGET), "from the file");
    assert_eq!(target(&state, GENERAL, 6), "live");
}

#[test]
fn a_window_is_cached_with_what_its_replies_answer() {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[
            reply(10, GENERAL, Some(original(GENERAL, "kept"))),
            reply_to(11, GENERAL, 6, Some(Value::Null)),
            reply_to(12, GENERAL, 7, None),
            message(13, GENERAL),
        ],
    );

    let (_, windows) = state.cache_parts(50);

    let general = Snowflake::new(GENERAL);
    let (_, cached) = windows.iter().find(|(id, _)| *id == general).unwrap();
    let mut targets: Vec<(u64, bool)> = cached
        .targets
        .iter()
        .map(|((_, id), target)| (id.get(), matches!(target, ReplyTarget::Loaded(_))))
        .collect();
    targets.sort_unstable();
    assert_eq!(targets, [(6, false), (TARGET, true)]);
}

fn starter_row(origin: Option<Value>) -> Value {
    let mut starter = message(21, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({
        "message_id": "19", "channel_id": GENERAL.to_string(),
    });
    if let Some(origin) = origin {
        starter["referenced_message"] = origin;
    }
    starter
}

fn stored(value: Value) -> Arc<Message> {
    let wire = page(&[value]).remove(0);
    Arc::new(Message::from_wire(wire, &mut |user| {
        Arc::new(crate::state::User::from_wire(user))
    }))
}

// The row itself is the same in the file and in the page: only what it shows changed.
#[test]
fn a_page_that_brings_a_cached_rows_starter_says_so_for_the_row() {
    use crate::state::CachedMessages;

    let mut state = viewing(THREAD);
    let thread = Snowflake::new(THREAD);
    let mut events = Vec::new();
    let ticket = state
        .begin_load(thread, LoadKind::Latest, 1, &mut events)
        .unwrap();
    let cached = CachedMessages {
        messages: vec![stored(starter_row(None))],
        ..CachedMessages::default()
    };
    state.fill_from_cache(ticket, cached, &mut events);
    assert_eq!(state.starter_message(thread), Some(StarterMessage::Loading));

    let mut events = Vec::new();
    let fresh = starter_row(Some(message(19, GENERAL)));
    state.finish_load(ticket, page(&[fresh]), true, &mut events);

    let events: Vec<String> = events.iter().map(describe).collect();
    assert!(
        events.contains(&"MessageUpdated(21)".to_owned()),
        "{events:?}"
    );
    assert!(matches!(
        state.starter_message(thread),
        Some(StarterMessage::Loaded(_))
    ));
}

#[test]
fn a_refresh_that_finds_the_starter_gone_says_so_for_the_row() {
    let mut state = viewing(THREAD);
    let thread = Snowflake::new(THREAD);
    load(
        &mut state,
        THREAD,
        LoadKind::Latest,
        &[starter_row(Some(message(19, GENERAL)))],
    );
    state.windows.mark_stale(&mut Vec::new());

    let events = load(
        &mut state,
        THREAD,
        LoadKind::Refresh,
        &[starter_row(Some(Value::Null))],
    );

    assert!(
        events.contains(&"MessageUpdated(21)".to_owned()),
        "{events:?}"
    );
    assert_eq!(
        state.starter_message(thread),
        Some(StarterMessage::Unavailable)
    );
}

#[test]
fn a_page_with_the_same_starter_says_nothing_for_the_row() {
    let mut state = viewing(THREAD);
    let row = starter_row(Some(message(19, GENERAL)));
    let rows = [row];
    load(&mut state, THREAD, LoadKind::Latest, &rows);
    state.windows.mark_stale(&mut Vec::new());

    let events = load(&mut state, THREAD, LoadKind::Refresh, &rows);

    assert!(
        !events.contains(&"MessageUpdated(21)".to_owned()),
        "{events:?}"
    );
}

#[test]
fn an_evicted_thread_takes_its_starter_and_that_messages_target() {
    let mut state = viewing(THREAD);
    let thread = Snowflake::new(THREAD);
    let origin = reply_to(19, GENERAL, TARGET, Some(original(GENERAL, "answered")));
    apply(&mut state, "MESSAGE_CREATE", starter_row(Some(origin)));
    assert_eq!(state.windows.targets_len(Snowflake::new(GENERAL)), 1);

    for other in 1..=10_u64 {
        state.view_channel(Snowflake::new(900 + other), &mut Vec::new());
    }
    assert!(state.messages(thread).is_none());

    assert_eq!(state.windows.targets_len(Snowflake::new(GENERAL)), 0);
    state.view_channel(thread, &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", starter_row(None));
    assert_eq!(
        state.starter_message(thread),
        Some(StarterMessage::Loading),
        "asked for again"
    );
    assert!(state.wants_starter(thread));
}

// A refresh's page is the truth within its range: a row it lacks was deleted meanwhile.
#[test]
fn a_target_a_refresh_finds_deleted_says_so_on_its_replies() {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[
            message(10, GENERAL),
            message(11, GENERAL),
            reply_to(12, GENERAL, 11, None),
        ],
    );
    assert_ne!(target(&state, GENERAL, 11), "deleted");
    state.windows.mark_stale(&mut Vec::new());

    let events = load(
        &mut state,
        GENERAL,
        LoadKind::Refresh,
        &[message(10, GENERAL), reply_to(12, GENERAL, 11, None)],
    );

    assert!(
        events.contains(&"MessageDeleted(11)".to_owned()),
        "{events:?}"
    );
    assert!(events.contains(&changed(GENERAL, 11)), "{events:?}");
    assert_eq!(target(&state, GENERAL, 11), "deleted");
}

// The preview's "@": the reply's mentions name the author of the loaded target.
#[test]
fn a_reply_says_whether_it_pinged_its_targets_author() {
    let mut state = viewing(GENERAL);
    let by = |author: u64, id: u64| {
        let mut target = message(id, GENERAL);
        target["author"]["id"] = author.to_string().into();
        target
    };
    let silent = |mut reply: Value| {
        reply["mentions"] = json!([]);
        reply
    };
    let replies = [
        reply_to(10, GENERAL, 1, Some(by(ME, 1))),
        silent(reply_to(11, GENERAL, 1, Some(by(ME, 1)))),
        reply_to(12, GENERAL, 2, Some(by(MIRA, 2))),
        reply_to(13, GENERAL, 3, None),
        reply_to(14, GENERAL, 4, Some(Value::Null)),
        message(15, GENERAL),
    ];
    for reply in replies {
        apply(&mut state, "MESSAGE_CREATE", reply);
    }
    let pinged = |id: u64| {
        let reply = state
            .message(Snowflake::new(GENERAL), Snowflake::new(id))
            .unwrap();
        state.reply_mentions_author(&reply)
    };

    assert!(pinged(10));
    assert!(!pinged(11), "no mention");
    assert!(!pinged(12), "mentions someone else");
    assert!(!pinged(13), "not loaded");
    assert!(!pinged(14), "deleted");
    assert!(!pinged(15), "no reply");
}

#[test]
fn many_targets_are_read_at_once_in_the_order_asked() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply(10, GENERAL, Some(original(GENERAL, "kept"))),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(11, GENERAL, 6, Some(Value::Null)),
    );
    let ids = [6, 99, TARGET, 6].map(Snowflake::new);

    let targets = state.reply_targets(Snowflake::new(GENERAL), &ids);

    assert_eq!(
        targets,
        ids.map(|id| state.reply_target(Snowflake::new(GENERAL), id))
    );
    assert_eq!(targets[0], ReplyTarget::Deleted);
    assert_eq!(targets[1], ReplyTarget::NotLoaded);
    assert!(matches!(targets[2], ReplyTarget::Loaded(_)));
}

fn queue_reply(state: &mut State, id: u64, target: u64, mention: bool) -> (bool, Vec<String>) {
    let me = state.current_user().unwrap();
    let general = Snowflake::new(GENERAL);
    let pending = Message::pending(
        Snowflake::new(id),
        general,
        Arc::new(me.user.clone()),
        "an answer".to_owned(),
        0,
    );
    let reply = Some(crate::state::ReplyTo {
        message: Snowflake::new(target),
        mention,
    });
    let now = state.now_millis();
    let mut events = Vec::new();
    let queued = state.queue(general, pending, reply, now, &mut events);
    (queued, events.iter().map(describe).collect())
}

#[test]
fn a_queued_reply_is_a_reply_row_and_loads_its_target() {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[original(GENERAL, "the original")],
    );
    assert_eq!(target(&state, GENERAL, TARGET), "not loaded");

    let (queued, events) = queue_reply(&mut state, 9000, TARGET, true);

    assert!(queued);
    assert_eq!(
        events,
        [changed(GENERAL, TARGET), "MessageInserted(9000)".to_owned()]
    );
    assert_eq!(target(&state, GENERAL, TARGET), "the original");
    let general = Snowflake::new(GENERAL);
    let outgoing = state.pending(general, Snowflake::new(9000)).unwrap();
    assert!(outgoing.mention);
    let row = outgoing.message;
    assert_eq!(row.kind, MessageType::Reply);
    assert_eq!(row.replied_to(), Some((general, Snowflake::new(TARGET))));
    let author = state
        .message(general, Snowflake::new(TARGET))
        .unwrap()
        .author
        .id;
    assert!(row.mentions_user(author));
    assert!(state.reply_mentions_author(&row));

    let (queued, events) = queue_reply(&mut state, 9001, 4242, true);
    assert!(!queued, "the store has no such message");
    assert!(events.is_empty());
    assert!(state.pending(general, Snowflake::new(9001)).is_none());
}

#[test]
fn a_pending_replys_target_and_ping_choice_outlast_its_window() {
    let mut state = viewing(GENERAL);
    let general = Snowflake::new(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[original(GENERAL, "the original")],
    );
    assert!(queue_reply(&mut state, 9000, TARGET, false).0);
    let row = state
        .pending(general, Snowflake::new(9000))
        .unwrap()
        .message;
    assert!(row.mentions.is_empty());

    // A jump replaces the window with more targets than the bound keeps.
    let mut events = Vec::new();
    let ticket = state
        .begin_load(
            general,
            LoadKind::Around(Snowflake::new(5000)),
            2,
            &mut events,
        )
        .unwrap();
    let replies: Vec<Value> = (0..150_u64)
        .map(|index| {
            let mut original = message(1000 + index, GENERAL);
            original["content"] = "another".into();
            reply_to(5000 + index, GENERAL, 1000 + index, Some(original))
        })
        .collect();
    state.finish_load(ticket, page(&replies), false, &mut events);

    assert_eq!(state.windows.targets_len(general), 151);
    assert_eq!(target(&state, GENERAL, TARGET), "the original");
    assert!(
        !state
            .pending(general, Snowflake::new(9000))
            .unwrap()
            .mention
    );

    // Failed, it still names its target; discarded, it lets go of it.
    state.fail(general, Snowflake::new(9000), &mut events);
    let ticket = state
        .begin_load(general, LoadKind::Latest, 3, &mut events)
        .unwrap();
    let present = [reply_to(6000, GENERAL, 7000, Some(message(7000, GENERAL)))];
    state.finish_load(ticket, page(&present), true, &mut events);
    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(target(&state, GENERAL, TARGET), "the original");
    let retried = state
        .retry(general, Snowflake::new(9000), 0, &mut events)
        .unwrap();
    assert!(!retried.mention);
    state.fail(general, Snowflake::new(9000), &mut events);
    state.discard(general, Snowflake::new(9000), &mut events);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        reply_to(6001, GENERAL, 7001, Some(message(7001, GENERAL))),
    );
    assert_eq!(state.windows.targets_len(general), 100);
    assert_eq!(target(&state, GENERAL, TARGET), "not loaded");
}
