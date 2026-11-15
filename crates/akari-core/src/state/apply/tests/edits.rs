use super::*;
use crate::RequestError;
use crate::state::apply::{DeleteStart, EditAsk, WantedEdit};
use crate::state::edits::{EditError, EditOutcome, FAILED_KEPT};
use crate::state::windows::StarterFetch;
use crate::state::{MessageEdit, PostPreview, StarterMessage};

const FORUM: u64 = 300_000_000_000_000_050;
const POST: u64 = 300_000_000_000_000_060;
const ORIGIN: u64 = 400_000_000_000_000_019;
const STARTER_ROW: u64 = 400_000_000_000_000_021;
const EARLIER: &str = "2026-11-15T12:00:00+00:00";
const LATER: &str = "2026-11-15T12:05:00+00:00";

fn id<M>(value: u64) -> Snowflake<M> {
    Snowflake::new(value)
}

fn mine(id: u64, channel: u64) -> Value {
    let mut message = message(id, channel);
    message["author"]["id"] = ME.to_string().into();
    message["author"]["username"] = "akari_tester".into();
    message
}

fn viewing(channel: u64) -> State {
    let mut state = ready_state();
    state.view_channel(id(channel), &mut Vec::new());
    state
}

fn general() -> State {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", mine(10, GENERAL));
    state
}

fn save(
    state: &mut State,
    channel: u64,
    row: u64,
    text: &str,
) -> (Option<WantedEdit>, Vec<String>) {
    let mut events = Vec::new();
    let now = state.now_millis();
    let wanted = state.want_edit(id(channel), id(row), text, now, &mut events);
    (wanted, events.iter().map(describe).collect())
}

fn ask(state: &mut State, channel: u64, message: u64) -> Option<EditAsk> {
    state.ask_edit(id(channel), id(message), &mut Vec::new())
}

fn asked(state: &mut State, channel: u64, message: u64) -> String {
    ask(state, channel, message)
        .expect("nothing to ask")
        .content
        .to_string()
}

fn answer(
    state: &mut State,
    channel: u64,
    message: u64,
    outcome: Result<Option<model::Message>, EditError>,
) -> (EditOutcome, Vec<String>) {
    let mut events = Vec::new();
    let answered = state.edit_answered(id(channel), id(message), outcome, &mut events);
    (answered, events.iter().map(describe).collect())
}

fn edited(id: u64, channel: u64, text: &str, at: &str) -> model::Message {
    let mut message = mine(id, channel);
    message["content"] = text.into();
    message["edited_timestamp"] = at.into();
    wire(&message)
}

fn wire<T: serde::de::DeserializeOwned>(value: &Value) -> T {
    serde_json::from_str(&value.to_string()).unwrap()
}

fn done(outcome: &EditOutcome) -> bool {
    matches!(outcome, EditOutcome::Done(_))
}

fn edit_of(state: &State, channel: u64, message: u64) -> String {
    match state.message_edit(id(channel), id(message)) {
        None => "none".to_owned(),
        Some(MessageEdit::Pending) => "pending".to_owned(),
        Some(MessageEdit::Failed { content, error }) => format!("failed {content:?}: {error}"),
    }
}

fn text(state: &State, channel: u64, message: u64) -> String {
    state
        .message(id(channel), id(message))
        .map(|message| message.content.to_string())
        .unwrap_or_else(|| "(gone)".to_owned())
}

fn update(state: &mut State, channel: u64, message: u64, text: &str, at: &str) -> Vec<String> {
    apply(
        state,
        "MESSAGE_UPDATE",
        json!({
            "id": message.to_string(), "channel_id": channel.to_string(), "content": text,
            "edited_timestamp": at,
        }),
    )
}

fn updated(message: u64) -> String {
    format!("MessageUpdated({message})")
}

fn starter_row(origin: Option<Value>) -> Value {
    let mut starter = message(STARTER_ROW, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({
        "message_id": ORIGIN.to_string(), "channel_id": GENERAL.to_string(),
    });
    if let Some(origin) = origin {
        starter["referenced_message"] = origin;
    }
    starter
}

fn starter_text(state: &State) -> String {
    match state.starter_message(id(THREAD)) {
        Some(StarterMessage::Loaded(message)) => message.content.to_string(),
        Some(StarterMessage::Unavailable) => "(unavailable)".to_owned(),
        other => panic!("expected a starter, got {other:?}"),
    }
}

fn forum_post() -> State {
    let mut state = ready_state();
    let mut forum = fixture(include_str!(
        "../../../../tests/fixtures/channel_guild_text.json"
    ));
    forum["id"] = FORUM.to_string().into();
    forum["type"] = 15.into();
    apply(&mut state, "CHANNEL_CREATE", forum);
    let mut post = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    post["id"] = POST.to_string().into();
    post["parent_id"] = FORUM.to_string().into();
    apply(&mut state, "THREAD_CREATE", post);
    state.open_forum(Some(id(FORUM)), &mut Vec::new());
    state
}

fn reply_to_ten(id: u64) -> Value {
    let mut reply = message(id, GENERAL);
    reply["type"] = 19.into();
    reply["message_reference"] = json!({
        "message_id": "10", "channel_id": GENERAL.to_string(),
    });
    reply
}

struct Copy {
    name: &'static str,
    state: State,
    row: (u64, u64),
    message: (u64, u64),
    read: fn(&State) -> String,
}

fn copies() -> Vec<Copy> {
    let window = general();

    let mut latest = viewing(THREAD);
    apply(&mut latest, "MESSAGE_CREATE", mine(10, THREAD));
    assert!(latest.latest_message(id(THREAD)).is_some());

    let mut preview = forum_post();
    preview.view_channel(id(POST), &mut Vec::new());
    apply(&mut preview, "MESSAGE_CREATE", mine(POST, POST));
    assert!(matches!(
        preview.post_preview(id(POST)),
        Some(PostPreview::Loaded(_))
    ));
    assert!(preview.message(id(POST), id(POST)).is_some());

    let mut fetched = viewing(THREAD);
    apply(&mut fetched, "MESSAGE_CREATE", starter_row(None));
    assert!(fetched.starter_to_fetch(id(THREAD)).is_some());
    let origin = wire(&mine(ORIGIN, GENERAL));
    fetched.settle_starter(
        id(THREAD),
        StarterFetch::Found(Box::new(origin)),
        &mut Vec::new(),
    );

    let mut sent_along = viewing(THREAD);
    apply(
        &mut sent_along,
        "MESSAGE_CREATE",
        starter_row(Some(mine(ORIGIN, GENERAL))),
    );

    // Held in the parent's stale window; the thread's starter row shows it.
    let mut held = viewing(THREAD);
    apply(
        &mut held,
        "MESSAGE_CREATE",
        starter_row(Some(mine(ORIGIN, GENERAL))),
    );
    held.view_channel(id(GENERAL), &mut Vec::new());
    held.windows.mark_stale(&mut Vec::new());
    apply(&mut held, "MESSAGE_CREATE", mine(ORIGIN, GENERAL));
    assert_eq!(held.windows.held(id(GENERAL)).len(), 1);
    let mut pins = ready_state();
    pinned_only(&mut pins, GENERAL, &[mine(10, GENERAL)]);

    let mut target = general();
    apply(&mut target, "MESSAGE_CREATE", reply_to_ten(11));
    assert!(matches!(
        target.reply_target(id(GENERAL), id(10)),
        ReplyTarget::Loaded(_)
    ));

    vec![
        Copy {
            name: "window",
            state: window,
            row: (GENERAL, 10),
            message: (GENERAL, 10),
            read: |state| text(state, GENERAL, 10),
        },
        Copy {
            name: "thread box latest",
            state: latest,
            row: (THREAD, 10),
            message: (THREAD, 10),
            read: |state| {
                state
                    .latest_message(id(THREAD))
                    .unwrap()
                    .content
                    .to_string()
            },
        },
        Copy {
            name: "post preview",
            state: preview,
            row: (POST, POST),
            message: (POST, POST),
            read: |state| match state.post_preview(id(POST)) {
                Some(PostPreview::Loaded(message)) => message.content.to_string(),
                other => panic!("expected a preview, got {other:?}"),
            },
        },
        Copy {
            name: "fetched starter",
            state: fetched,
            row: (THREAD, STARTER_ROW),
            message: (GENERAL, ORIGIN),
            read: starter_text,
        },
        Copy {
            name: "starter sent along",
            state: sent_along,
            row: (THREAD, STARTER_ROW),
            message: (GENERAL, ORIGIN),
            read: starter_text,
        },
        Copy {
            name: "held",
            state: held,
            row: (THREAD, STARTER_ROW),
            message: (GENERAL, ORIGIN),
            read: |state| state.windows.held(id(GENERAL))[0].content.to_string(),
        },
        Copy {
            name: "reply target",
            state: target,
            row: (GENERAL, 10),
            message: (GENERAL, 10),
            read: |state| match state.reply_target(id(GENERAL), id(10)) {
                ReplyTarget::Loaded(preview) => preview.content.to_string(),
                other => panic!("expected a loaded target, got {other:?}"),
            },
        },
        Copy {
            name: "pins list",
            state: pins,
            row: (GENERAL, 10),
            message: (GENERAL, 10),
            read: |state| state.pins(id(GENERAL)).items[0].message.content.to_string(),
        },
    ]
}

#[test]
fn every_copy_shows_a_save_at_once_its_refusal_and_then_discords_answer() {
    for mut copy in copies() {
        let (name, read) = (copy.name, copy.read);
        let ((row_channel, row), (channel, message)) = (copy.row, copy.message);
        let state = &mut copy.state;

        let (wanted, _) = save(state, row_channel, row, "saved");
        let wanted = wanted.unwrap_or_else(|| panic!("{name}: not editable"));
        assert_eq!(
            (wanted.channel.get(), wanted.message.get(), wanted.changed),
            (channel, message, true),
            "{name}"
        );
        assert_eq!(read(state), "saved", "{name}: at once");
        assert_eq!(edit_of(state, channel, message), "pending", "{name}");

        assert_eq!(asked(state, channel, message), "saved", "{name}");
        let (refused, _) = answer(state, channel, message, Err(EditError::Forbidden));
        assert!(matches!(refused, EditOutcome::Refused(..)), "{name}");
        assert_eq!(read(state), "hi", "{name}: Discord's text back");
        assert_eq!(
            edit_of(state, channel, message),
            "failed \"saved\": editing this message isn't allowed",
            "{name}"
        );

        save(state, row_channel, row, "again");
        assert_eq!(asked(state, channel, message), "again", "{name}");
        let answered = edited(message, channel, "again", EARLIER);
        let (outcome, _) = answer(state, channel, message, Ok(Some(answered)));
        assert!(done(&outcome), "{name}");
        assert_eq!(read(state), "again", "{name}: the answer");
        assert_eq!(edit_of(state, channel, message), "none", "{name}");
    }
}

#[test]
fn each_copy_tells_what_shows_it_of_a_save() {
    let events: Vec<(&str, Vec<String>)> = copies()
        .into_iter()
        .map(|mut copy| {
            let mut events = save(&mut copy.state, copy.row.0, copy.row.1, "saved").1;
            events.sort();
            (copy.name, events)
        })
        .collect();

    assert_eq!(
        events,
        [
            ("window", vec![updated(10)]),
            (
                "thread box latest",
                vec![updated(10), format!("ThreadActivity({GENERAL}, {THREAD})")]
            ),
            (
                "post preview",
                vec![updated(POST), format!("PostPreviewChanged({POST})")]
            ),
            ("fetched starter", vec![updated(STARTER_ROW)]),
            ("starter sent along", vec![updated(STARTER_ROW)]),
            ("held", vec![updated(STARTER_ROW)]),
            (
                "reply target",
                vec![updated(10), format!("ReplyTargetChanged({GENERAL}, 10)")]
            ),
            ("pins list", vec![format!("PinsChanged({GENERAL})")]),
        ]
    );
}

// The parent's window has the newest copy: the starter row reads it, and is told.
#[test]
fn a_starter_rows_edit_reaches_the_parents_window_and_the_row() {
    let mut state = viewing(THREAD);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        starter_row(Some(mine(ORIGIN, GENERAL))),
    );
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", mine(ORIGIN, GENERAL));

    let (wanted, events) = save(&mut state, THREAD, STARTER_ROW, "saved");

    assert_eq!(wanted.unwrap().channel.get(), GENERAL);
    assert_eq!(events, [updated(ORIGIN), updated(STARTER_ROW)]);
    assert_eq!(text(&state, GENERAL, ORIGIN), "saved");
    assert_eq!(starter_text(&state), "saved");
}

// The row reads the parent's window, so it is told of a change there that its own copy
// already had.
#[test]
fn a_starter_row_is_told_when_only_the_parents_copy_changes() {
    let mut state = viewing(THREAD);
    let mut origin = mine(ORIGIN, GENERAL);
    origin["content"] = "elsewhere".into();
    origin["edited_timestamp"] = EARLIER.into();
    apply(&mut state, "MESSAGE_CREATE", starter_row(Some(origin)));
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", mine(ORIGIN, GENERAL));
    assert_eq!(starter_text(&state), "hi");

    let events = update(&mut state, GENERAL, ORIGIN, "elsewhere", EARLIER);

    assert_eq!(events, [updated(ORIGIN), updated(STARTER_ROW)]);
    assert_eq!(starter_text(&state), "elsewhere");
}

// As a page: a starter fetched again while a save of its message waits shows the saved text.
#[test]
fn a_starter_fetched_while_a_save_waits_shows_the_saved_text() {
    let mut copy = copies().remove(3);
    assert_eq!(copy.name, "fetched starter");
    let state = &mut copy.state;
    save(state, THREAD, STARTER_ROW, "saved");
    asked(state, GENERAL, ORIGIN);
    // The thread's list let go, and opened again: its starter is fetched anew.
    for other in 0..10 {
        state.view_channel(id(900 + other), &mut Vec::new());
    }
    state.view_channel(id(THREAD), &mut Vec::new());
    apply(state, "MESSAGE_CREATE", starter_row(None));
    assert!(state.starter_to_fetch(id(THREAD)).is_some());

    let origin = wire(&mine(ORIGIN, GENERAL));
    state.settle_starter(
        id(THREAD),
        StarterFetch::Found(Box::new(origin)),
        &mut Vec::new(),
    );

    assert_eq!(starter_text(state), "saved");
}

#[test]
fn an_edit_from_the_gateway_reaches_a_fetched_starter_and_tells_its_row() {
    let mut copy = copies().remove(3);
    assert_eq!(copy.name, "fetched starter");

    let events = update(&mut copy.state, GENERAL, ORIGIN, "elsewhere", EARLIER);

    assert_eq!(events, [updated(STARTER_ROW)]);
    assert_eq!(starter_text(&copy.state), "elsewhere");
}

#[test]
fn an_update_that_changes_nothing_tells_no_starter_row() {
    let mut copy = copies().remove(3);
    assert_eq!(copy.name, "fetched starter");

    let events = apply(
        &mut copy.state,
        "MESSAGE_UPDATE",
        json!({"id": ORIGIN.to_string(), "channel_id": GENERAL.to_string(), "content": "hi"}),
    );

    assert_eq!(events, [] as [&str; 0]);
}

#[test]
fn an_unchanged_save_asks_nothing_and_shows_nothing() {
    let mut state = general();

    let (wanted, events) = save(&mut state, GENERAL, 10, "hi");

    assert!(!wanted.unwrap().changed);
    assert_eq!(events, [] as [&str; 0]);
    assert!(ask(&mut state, GENERAL, 10).is_none());
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
}

#[test]
fn a_blank_save_is_refused_unless_an_attachment_or_a_sticker_stays() {
    let mut state = general();
    let mut with_file = mine(11, GENERAL);
    with_file["attachments"] = json!([{
        "id": "700000000000000001", "filename": "a.png", "size": 3,
        "url": "https://cdn.discordapp.com/a.png", "proxy_url": "https://media.discordapp.net/a.png",
    }]);
    apply(&mut state, "MESSAGE_CREATE", with_file);
    let mut with_sticker = mine(12, GENERAL);
    with_sticker["sticker_items"] =
        json!([{"id": "800000000000000001", "name": "wave", "format_type": 1}]);
    apply(&mut state, "MESSAGE_CREATE", with_sticker);

    assert!(save(&mut state, GENERAL, 10, " \n ").0.is_none());
    assert_eq!(text(&state, GENERAL, 10), "hi");
    for kept in [11, 12] {
        let (wanted, _) = save(&mut state, GENERAL, kept, "");
        assert!(wanted.unwrap().changed, "{kept}");
        assert_eq!(asked(&mut state, GENERAL, kept), "", "{kept}");
    }
}

#[test]
fn a_save_where_editing_isnt_offered_changes_nothing() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    let (wanted, events) = save(&mut state, GENERAL, 10, "mine now");

    assert!(wanted.is_none());
    assert_eq!(events, [] as [&str; 0]);
    assert_eq!(text(&state, GENERAL, 10), "hi");
}

#[test]
fn the_latest_save_is_asked_once_the_one_on_its_way_is_answered() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    assert_eq!(asked(&mut state, GENERAL, 10), "first");

    let (wanted, _) = save(&mut state, GENERAL, 10, "second");
    assert!(wanted.unwrap().changed);
    assert_eq!(text(&state, GENERAL, 10), "second");
    let first = edited(10, GENERAL, "first", EARLIER);
    let (outcome, _) = answer(&mut state, GENERAL, 10, Ok(Some(first)));

    assert!(matches!(outcome, EditOutcome::Again));
    assert_eq!(text(&state, GENERAL, 10), "second", "the newer save stays");
    assert_eq!(edit_of(&state, GENERAL, 10), "pending");
    assert_eq!(asked(&mut state, GENERAL, 10), "second");
    let second = edited(10, GENERAL, "second", LATER);
    assert!(done(&answer(&mut state, GENERAL, 10, Ok(Some(second))).0));
    assert_eq!(text(&state, GENERAL, 10), "second");
}

// Saved back to Discord's text while another save is on its way: that one is undone.
#[test]
fn a_save_back_to_the_old_text_while_a_save_runs_is_asked_too() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    assert_eq!(asked(&mut state, GENERAL, 10), "first");

    save(&mut state, GENERAL, 10, "hi");
    let first = edited(10, GENERAL, "first", EARLIER);
    let (outcome, _) = answer(&mut state, GENERAL, 10, Ok(Some(first)));

    assert!(matches!(outcome, EditOutcome::Again));
    assert_eq!(asked(&mut state, GENERAL, 10), "hi");
}

// Saved back to Discord's text before its request went: nothing is asked, and the edit ends.
#[test]
fn a_save_back_to_the_old_text_before_the_request_ends_the_edit_without_one() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    save(&mut state, GENERAL, 10, "hi");

    let mut events = Vec::new();
    let asked = state.ask_edit(id(GENERAL), id(10), &mut events);

    assert!(asked.is_none());
    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        [updated(10)]
    );
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
    assert_eq!(text(&state, GENERAL, 10), "hi");
    assert!(matches!(
        answer(&mut state, GENERAL, 10, Ok(None)).0,
        EditOutcome::Gone
    ));
}

#[test]
fn a_refused_save_that_a_newer_one_replaced_asks_for_the_newer_one() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    asked(&mut state, GENERAL, 10);
    save(&mut state, GENERAL, 10, "second");

    let blocked = EditError::MessageBlocked {
        reason: "No".to_owned(),
    };
    let (outcome, events) = answer(&mut state, GENERAL, 10, Err(blocked));

    assert!(matches!(outcome, EditOutcome::Again));
    assert_eq!(events, [] as [&str; 0]);
    assert_eq!(text(&state, GENERAL, 10), "second");
    assert_eq!(asked(&mut state, GENERAL, 10), "second");
}

// D162 "whichever comes first": the gateway's word ends the pending look before the answer.
#[test]
fn the_gateway_showing_the_saved_text_ends_the_pending_look_before_the_answer() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);

    let events = update(&mut state, GENERAL, 10, "saved", EARLIER);

    assert_eq!(events, [updated(10)], "the edit time");
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
    let message = state.message(id(GENERAL), id(10)).unwrap();
    assert!(message.edited_timestamp.is_some());
    let (outcome, events) = answer(&mut state, GENERAL, 10, Ok(None));
    assert!(done(&outcome));
    assert_eq!(events, [] as [&str; 0], "nothing more to show");
}

// A lost answer: the gateway already said Discord has it.
#[test]
fn a_failure_after_the_gateway_showed_the_text_counts_as_done() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    update(&mut state, GENERAL, 10, "saved", EARLIER);

    let lost = EditError::Request(RequestError::UnexpectedResponse);
    let (outcome, _) = answer(&mut state, GENERAL, 10, Err(lost));

    assert!(done(&outcome));
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
    assert_eq!(text(&state, GENERAL, 10), "saved");
}

// The echo usually beats the PATCH's answer: a later save looks pending again.
#[test]
fn a_save_after_the_echo_of_the_one_on_its_way_is_pending_again() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    asked(&mut state, GENERAL, 10);
    update(&mut state, GENERAL, 10, "first", EARLIER);
    assert_eq!(edit_of(&state, GENERAL, 10), "none");

    save(&mut state, GENERAL, 10, "second");

    assert_eq!(edit_of(&state, GENERAL, 10), "pending");
    update(&mut state, GENERAL, 10, "first", EARLIER);
    assert_eq!(
        text(&state, GENERAL, 10),
        "second",
        "a repeated echo doesn't show"
    );
}

// The gateway is in Discord's order: an edit from elsewhere after the earlier answer wins.
#[test]
fn an_unreadable_answer_after_an_edit_from_elsewhere_never_brings_back_an_earlier_answer() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    asked(&mut state, GENERAL, 10);
    save(&mut state, GENERAL, 10, "second");
    let first = edited(10, GENERAL, "first", EARLIER);
    answer(&mut state, GENERAL, 10, Ok(Some(first)));
    asked(&mut state, GENERAL, 10);
    update(&mut state, GENERAL, 10, "elsewhere", LATER);

    answer(&mut state, GENERAL, 10, Ok(None));

    assert_eq!(text(&state, GENERAL, 10), "elsewhere");
}

#[test]
fn an_earlier_saves_echo_never_shows_while_a_later_one_waits() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    asked(&mut state, GENERAL, 10);
    save(&mut state, GENERAL, 10, "second");

    update(&mut state, GENERAL, 10, "first", EARLIER);

    assert_eq!(text(&state, GENERAL, 10), "second");
    assert_eq!(edit_of(&state, GENERAL, 10), "pending");
    assert!(matches!(
        answer(&mut state, GENERAL, 10, Ok(None)).0,
        EditOutcome::Again
    ));
    assert_eq!(asked(&mut state, GENERAL, 10), "second");
    update(&mut state, GENERAL, 10, "second", LATER);
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
    assert!(done(&answer(&mut state, GENERAL, 10, Ok(None)).0));
    assert_eq!(text(&state, GENERAL, 10), "second");
}

// Another device's edit after the user's: the newer by Discord's edit time shows at the end.
#[test]
fn a_newer_edit_from_elsewhere_shows_once_the_requests_end() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    update(&mut state, GENERAL, 10, "saved", EARLIER);
    update(&mut state, GENERAL, 10, "elsewhere", LATER);

    let answered = edited(10, GENERAL, "saved", EARLIER);
    let (outcome, _) = answer(&mut state, GENERAL, 10, Ok(Some(answered)));

    assert!(done(&outcome));
    assert_eq!(text(&state, GENERAL, 10), "elsewhere");
}

#[test]
fn an_edit_from_elsewhere_while_a_save_waits_shows_if_the_answer_is_older() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);

    update(&mut state, GENERAL, 10, "elsewhere", LATER);
    assert_eq!(text(&state, GENERAL, 10), "saved", "still the save, dimmed");
    assert_eq!(edit_of(&state, GENERAL, 10), "pending");
    let answered = edited(10, GENERAL, "saved", EARLIER);
    answer(&mut state, GENERAL, 10, Ok(Some(answered)));

    assert_eq!(text(&state, GENERAL, 10), "elsewhere");
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
}

// The answer comes before the gateway: its edit time and mentions show with it.
#[test]
fn the_answer_shows_its_edit_time_and_mentions_before_the_gateway() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "for <@100000000000000002>");
    asked(&mut state, GENERAL, 10);
    let mut answered = edited(10, GENERAL, "for <@100000000000000002>", EARLIER);
    answered.mentions = vec![wire(&json!({
        "id": MIRA.to_string(), "username": "mira", "discriminator": "0", "avatar": null,
    }))];
    answered.mention_everyone = true;
    answered.mention_roles = vec![id(77)];

    let (outcome, events) = answer(&mut state, GENERAL, 10, Ok(Some(answered)));

    assert!(done(&outcome));
    assert_eq!(events, [updated(10)]);
    let message = state.message(id(GENERAL), id(10)).unwrap();
    assert!(message.edited_timestamp.is_some());
    let mentioned: Vec<u64> = message.mentions.iter().map(|user| user.id.get()).collect();
    assert_eq!(mentioned, [MIRA]);
    assert!(message.mentions_user(id(MIRA)));
    assert!(message.mention_everyone);
    assert_eq!(&*message.mention_roles, [id(77)]);
}

// Ruling 3: a link preview the gateway added stays when the answer comes after it.
#[test]
fn the_answer_leaves_the_embeds_the_gateway_added_since() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "see https://example.com");
    asked(&mut state, GENERAL, 10);
    apply(
        &mut state,
        "MESSAGE_UPDATE",
        json!({
            "id": "10", "channel_id": GENERAL.to_string(),
            "embeds": [{"type": "link", "url": "https://example.com", "title": "Example"}],
        }),
    );

    let answered = edited(10, GENERAL, "see https://example.com", EARLIER);
    answer(&mut state, GENERAL, 10, Ok(Some(answered)));

    let message = state.message(id(GENERAL), id(10)).unwrap();
    assert_eq!(message.embeds.len(), 1);
    assert_eq!(&*message.content, "see https://example.com");
}

// The gateway is in Discord's order: what it said since the request is newer than a guess.
#[test]
fn an_answer_that_isnt_a_message_after_an_edit_from_elsewhere_leaves_that_edit() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    update(&mut state, GENERAL, 10, "elsewhere", LATER);

    answer(&mut state, GENERAL, 10, Ok(None));

    assert_eq!(text(&state, GENERAL, 10), "elsewhere");
}

// An echo the HTTP answer overtook is older than it: it never brings back an older text.
#[test]
fn a_late_echo_older_than_an_answer_never_undoes_it() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "first");
    asked(&mut state, GENERAL, 10);
    save(&mut state, GENERAL, 10, "second");
    let first = edited(10, GENERAL, "first", LATER);
    answer(&mut state, GENERAL, 10, Ok(Some(first)));

    update(&mut state, GENERAL, 10, "older", EARLIER);
    asked(&mut state, GENERAL, 10);
    answer(&mut state, GENERAL, 10, Err(EditError::Forbidden));

    assert_eq!(text(&state, GENERAL, 10), "first");
}

#[test]
fn an_unreadable_answer_after_an_answer_shows_the_latest_save_also_after_an_older_echo() {
    for echo in [false, true] {
        let mut state = general();
        save(&mut state, GENERAL, 10, "first");
        asked(&mut state, GENERAL, 10);
        save(&mut state, GENERAL, 10, "second");
        let first = edited(10, GENERAL, "first", LATER);
        answer(&mut state, GENERAL, 10, Ok(Some(first)));
        asked(&mut state, GENERAL, 10);

        if echo {
            update(&mut state, GENERAL, 10, "older", EARLIER);
        }
        let (outcome, _) = answer(&mut state, GENERAL, 10, Ok(None));

        assert!(done(&outcome), "echo {echo}");
        assert_eq!(text(&state, GENERAL, 10), "second", "echo {echo}");
    }
}

#[test]
fn an_answer_that_isnt_a_message_leaves_the_saved_text() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);

    let (outcome, events) = answer(&mut state, GENERAL, 10, Ok(None));

    assert!(done(&outcome));
    assert_eq!(text(&state, GENERAL, 10), "saved");
    assert_eq!(events, [updated(10)], "no longer pending");
}

#[test]
fn a_refused_edit_keeps_the_saved_text_until_saved_again_or_dismissed() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    let (_, events) = answer(&mut state, GENERAL, 10, Err(EditError::Deleted));
    assert_eq!(events, [updated(10)]);

    update(&mut state, GENERAL, 10, "elsewhere", LATER);
    assert_eq!(
        text(&state, GENERAL, 10),
        "elsewhere",
        "Discord's text shows"
    );
    assert_eq!(
        edit_of(&state, GENERAL, 10),
        "failed \"saved\": the message was deleted"
    );

    let mut events = Vec::new();
    state.dismiss_edit(id(GENERAL), id(10), &mut events);
    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        [updated(10)]
    );
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
    let mut again = Vec::new();
    state.dismiss_edit(id(GENERAL), id(10), &mut again);
    assert!(again.is_empty());
}

#[test]
fn dismissing_an_edit_on_its_way_changes_nothing() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);

    let mut events = Vec::new();
    state.dismiss_edit(id(GENERAL), id(10), &mut events);

    assert!(events.is_empty());
    assert_eq!(edit_of(&state, GENERAL, 10), "pending");
    assert_eq!(text(&state, GENERAL, 10), "saved");
}

#[test]
fn saving_a_refused_edit_again_asks_again() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    answer(&mut state, GENERAL, 10, Err(EditError::Forbidden));

    let (wanted, events) = save(&mut state, GENERAL, 10, "saved");

    assert!(wanted.unwrap().changed);
    assert_eq!(events, [updated(10)]);
    assert_eq!(text(&state, GENERAL, 10), "saved");
    assert_eq!(edit_of(&state, GENERAL, 10), "pending");
    assert_eq!(asked(&mut state, GENERAL, 10), "saved");
}

// Discord has the other device's text since the refusal, not the text from before the edit.
#[test]
fn a_save_back_to_the_old_text_after_a_refusal_and_an_edit_from_elsewhere_is_asked() {
    for elsewhere_first in [true, false] {
        let mut state = general();
        save(&mut state, GENERAL, 10, "saved");
        asked(&mut state, GENERAL, 10);
        if elsewhere_first {
            update(&mut state, GENERAL, 10, "elsewhere", LATER);
        }
        answer(&mut state, GENERAL, 10, Err(EditError::Forbidden));
        if !elsewhere_first {
            update(&mut state, GENERAL, 10, "elsewhere", LATER);
        }
        assert_eq!(text(&state, GENERAL, 10), "elsewhere");

        let (wanted, _) = save(&mut state, GENERAL, 10, "hi");

        assert!(wanted.unwrap().changed, "elsewhere first {elsewhere_first}");
        assert_eq!(text(&state, GENERAL, 10), "hi");
        assert_eq!(asked(&mut state, GENERAL, 10), "hi");
    }
}

#[test]
fn a_refused_edit_saved_as_discords_text_is_forgotten_without_a_request() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    answer(&mut state, GENERAL, 10, Err(EditError::Forbidden));

    let (wanted, events) = save(&mut state, GENERAL, 10, "hi");

    assert!(!wanted.unwrap().changed);
    assert_eq!(events, [updated(10)]);
    assert_eq!(edit_of(&state, GENERAL, 10), "none");
    assert!(ask(&mut state, GENERAL, 10).is_none());
}

#[test]
fn refused_edits_are_kept_up_to_a_limit_and_across_a_new_session() {
    let mut state = viewing(GENERAL);
    // Ruling 5's 16, one more.
    let count = 17;
    for message in 10..10 + count {
        apply(&mut state, "MESSAGE_CREATE", mine(message, GENERAL));
        save(&mut state, GENERAL, message, "saved");
        asked(&mut state, GENERAL, message);
        answer(&mut state, GENERAL, message, Err(EditError::Forbidden));
    }

    assert_eq!(edit_of(&state, GENERAL, 10), "none", "the oldest went");
    assert!(edit_of(&state, GENERAL, 11).starts_with("failed"));
    state.apply(dispatch("READY", &ready_value()), &mut Vec::new());
    assert!(edit_of(&state, GENERAL, 10 + count - 1).starts_with("failed"));
}

// The refusal that just came is the user's latest word; the oldest other one goes, and its row is
// told.
#[test]
fn a_refusal_past_the_limit_keeps_itself_and_tells_the_row_whose_refusal_goes() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", mine(10, GENERAL));
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    let last = 10 + u64::try_from(FAILED_KEPT).unwrap();
    for message in 11..=last {
        apply(&mut state, "MESSAGE_CREATE", mine(message, GENERAL));
        save(&mut state, GENERAL, message, "saved");
        asked(&mut state, GENERAL, message);
        answer(&mut state, GENERAL, message, Err(EditError::Forbidden));
    }

    let (outcome, mut events) = answer(&mut state, GENERAL, 10, Err(EditError::Forbidden));

    assert!(matches!(outcome, EditOutcome::Refused(..)));
    assert!(edit_of(&state, GENERAL, 10).starts_with("failed"));
    assert_eq!(
        edit_of(&state, GENERAL, 11),
        "none",
        "the oldest other went"
    );
    assert!(edit_of(&state, GENERAL, last).starts_with("failed"));
    events.sort();
    assert_eq!(events, [updated(10), updated(11)]);
}

#[test]
fn an_edit_of_a_reply_whose_target_it_doesnt_ping_keeps_the_ping_off() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    let reply = |id: u64, mentions: Value| {
        let mut reply = reply_to_ten(id);
        reply["author"]["id"] = ME.to_string().into();
        reply["mentions"] = mentions;
        reply
    };
    let mira = json!([{"id": MIRA.to_string(), "username": "mira", "discriminator": "0"}]);
    apply(&mut state, "MESSAGE_CREATE", reply(11, json!([])));
    apply(&mut state, "MESSAGE_CREATE", reply(12, mira));
    let mut plain = mine(13, GENERAL);
    plain["mentions"] = json!([]);
    apply(&mut state, "MESSAGE_CREATE", plain);
    let mut command = reply(14, json!([]));
    command["type"] = 23.into();
    apply(&mut state, "MESSAGE_CREATE", command);

    let quiet: Vec<bool> = [11, 12, 13, 14]
        .into_iter()
        .map(|message| {
            save(&mut state, GENERAL, message, "edited");
            ask(&mut state, GENERAL, message).unwrap().quiet_reply
        })
        .collect();

    assert_eq!(quiet, [true, false, false, false], "replies only");
}

#[test]
fn a_reply_whose_target_isnt_loaded_is_edited_as_the_official_client_does() {
    let mut state = viewing(GENERAL);
    let mut reply = reply_to_ten(11);
    reply["author"]["id"] = ME.to_string().into();
    reply["mentions"] = json!([]);
    apply(&mut state, "MESSAGE_CREATE", reply);

    save(&mut state, GENERAL, 11, "edited");

    assert!(!ask(&mut state, GENERAL, 11).unwrap().quiet_reply);
}

fn begin_delete(state: &mut State, channel: u64, row: u64) -> (Option<DeleteStart>, Vec<String>) {
    let mut events = Vec::new();
    let now = state.now_millis();
    let start = state.begin_delete(id(channel), id(row), now, &mut events);
    (start, events.iter().map(describe).collect())
}

fn delete_answered(state: &mut State, channel: u64, message: u64, deleted: bool) -> Vec<String> {
    let mut events = Vec::new();
    state.delete_answered(id(channel), id(message), deleted, &mut events);
    events.iter().map(describe).collect()
}

fn actions(state: &State, channel: u64, message: u64) -> (bool, bool) {
    let actions = state.message_actions(id(channel), id(message), state.now_millis());
    (actions.edit, actions.delete)
}

#[test]
fn a_delete_dims_the_row_and_neither_edit_nor_delete_is_offered_meanwhile() {
    let mut state = general();
    assert_eq!(actions(&state, GENERAL, 10), (true, true));

    let (start, events) = begin_delete(&mut state, GENERAL, 10);

    assert_eq!(
        start,
        Some(DeleteStart::Request {
            channel: id(GENERAL),
            message: id(10)
        })
    );
    assert_eq!(events, [updated(10)]);
    assert!(state.deleting(id(GENERAL), id(10)));
    assert_eq!(actions(&state, GENERAL, 10), (false, false));
    assert!(begin_delete(&mut state, GENERAL, 10).0.is_none());
    assert!(save(&mut state, GENERAL, 10, "edited").0.is_none());
}

#[test]
fn a_delete_from_a_starter_row_dims_the_parents_message_and_the_row() {
    let mut state = viewing(THREAD);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        starter_row(Some(mine(ORIGIN, GENERAL))),
    );

    let (start, events) = begin_delete(&mut state, THREAD, STARTER_ROW);

    assert_eq!(
        start,
        Some(DeleteStart::Request {
            channel: id(GENERAL),
            message: id(ORIGIN)
        })
    );
    assert_eq!(events, [updated(STARTER_ROW)]);
    assert!(state.deleting(id(GENERAL), id(ORIGIN)));
    assert_eq!(actions(&state, THREAD, STARTER_ROW), (false, false));
}

#[test]
fn a_refused_delete_undims_the_row() {
    let mut state = general();
    begin_delete(&mut state, GENERAL, 10);

    let events = delete_answered(&mut state, GENERAL, 10, false);

    assert_eq!(events, [updated(10)]);
    assert!(!state.deleting(id(GENERAL), id(10)));
    assert_eq!(actions(&state, GENERAL, 10), (true, true));
    assert_eq!(text(&state, GENERAL, 10), "hi");
}

#[test]
fn a_delete_where_it_isnt_offered_changes_nothing() {
    let mut state = viewing(GENERAL);
    let mut renamed = mine(10, GENERAL);
    renamed["type"] = 4.into();
    apply(&mut state, "MESSAGE_CREATE", renamed);

    let (start, events) = begin_delete(&mut state, GENERAL, 10);

    assert!(start.is_none(), "a channel name change can't be deleted");
    assert_eq!(events, [] as [&str; 0]);
    assert!(!state.deleting(id(GENERAL), id(10)));
}

#[test]
fn a_failed_send_is_deleted_without_a_request() {
    let mut state = viewing(GENERAL);
    let me = state.entities.current_user.clone().unwrap();
    let pending = Message::pending(
        id(77),
        id(GENERAL),
        Arc::new(me.user.clone()),
        "x".into(),
        0,
    );
    let now = state.now_millis();
    assert!(state.queue(id(GENERAL), pending, None, now, &mut Vec::new()));
    state.fail(id(GENERAL), id(77), &mut Vec::new());

    let (start, events) = begin_delete(&mut state, GENERAL, 77);

    assert_eq!(start, Some(DeleteStart::Discarded));
    assert_eq!(events, ["MessageDeleted(77)"]);
    assert!(state.messages(id(GENERAL)).unwrap().pending.is_empty());
}

#[test]
fn a_confirmed_delete_takes_the_message_from_every_copy_at_once() {
    for mut copy in copies() {
        let name = copy.name;
        let ((row_channel, row), (channel, message)) = (copy.row, copy.message);
        let state = &mut copy.state;
        save(state, row_channel, row, "saved");

        assert!(begin_delete(state, row_channel, row).0.is_some(), "{name}");
        let events = delete_answered(state, channel, message, true);

        assert!(!events.is_empty(), "{name}: told");
        assert!(
            state.message_edit(id(channel), id(message)).is_none(),
            "{name}"
        );
        assert!(!state.deleting(id(channel), id(message)), "{name}");
        match name {
            "window" => assert_eq!(text(state, GENERAL, 10), "(gone)"),
            "thread box latest" => assert!(state.latest_message(id(THREAD)).is_none()),
            "post preview" => assert!(matches!(
                state.post_preview(id(POST)),
                Some(PostPreview::Deleted)
            )),
            "fetched starter" | "starter sent along" => {
                assert_eq!(starter_text(state), "(unavailable)", "{name}");
            }
            "held" => assert!(state.windows.held(id(GENERAL)).is_empty()),
            "reply target" => assert_eq!(
                state.reply_target(id(GENERAL), id(10)),
                ReplyTarget::Deleted
            ),
            "pins list" => assert!(state.pins(id(GENERAL)).items.is_empty()),
            _ => unreachable!(),
        }
    }
}

// The gateway's MESSAGE_DELETE comes with the answer: the thread's count goes down once.
#[test]
fn a_confirmed_delete_leaves_the_threads_count_to_the_gateway() {
    let mut state = viewing(THREAD);
    for message in [10, 11] {
        apply(&mut state, "MESSAGE_CREATE", mine(message, THREAD));
    }
    let count = |state: &State| {
        state
            .channel(id(THREAD))
            .and_then(|thread| thread.thread.as_deref().and_then(|info| info.message_count))
    };
    let before = count(&state);
    begin_delete(&mut state, THREAD, 11);

    delete_answered(&mut state, THREAD, 11, true);
    assert_eq!(count(&state), before);
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "11", "channel_id": THREAD.to_string()}),
    );

    assert_eq!(count(&state), before.map(|count| count - 1));
    assert_eq!(text(&state, THREAD, 11), "(gone)");
}

#[test]
fn the_gateways_delete_first_leaves_the_answer_nothing_to_do() {
    let mut state = general();
    begin_delete(&mut state, GENERAL, 10);

    let events = apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "10", "channel_id": GENERAL.to_string()}),
    );

    assert_eq!(events, ["MessageDeleted(10)"]);
    assert!(!state.deleting(id(GENERAL), id(10)));
    assert_eq!(
        delete_answered(&mut state, GENERAL, 10, true),
        [] as [&str; 0]
    );
    assert_eq!(
        delete_answered(&mut state, GENERAL, 10, false),
        [] as [&str; 0]
    );
}

#[test]
fn a_delete_from_the_gateway_forgets_the_messages_edit_single_or_bulk() {
    for bulk in [false, true] {
        let mut state = viewing(GENERAL);
        for message in [10, 11] {
            apply(&mut state, "MESSAGE_CREATE", mine(message, GENERAL));
        }
        save(&mut state, GENERAL, 10, "pending");
        save(&mut state, GENERAL, 11, "failed");
        asked(&mut state, GENERAL, 11);
        answer(&mut state, GENERAL, 11, Err(EditError::Forbidden));

        if bulk {
            apply(
                &mut state,
                "MESSAGE_DELETE_BULK",
                json!({"ids": ["10", "11"], "channel_id": GENERAL.to_string()}),
            );
        } else {
            for message in ["10", "11"] {
                apply(
                    &mut state,
                    "MESSAGE_DELETE",
                    json!({"id": message, "channel_id": GENERAL.to_string()}),
                );
            }
        }

        for message in [10, 11] {
            assert_eq!(edit_of(&state, GENERAL, message), "none", "bulk {bulk}");
        }
        assert!(matches!(
            answer(&mut state, GENERAL, 10, Ok(None)).0,
            EditOutcome::Gone
        ));
        assert!(ask(&mut state, GENERAL, 10).is_none());
    }
}

#[test]
fn a_deleted_message_a_thread_started_from_shows_as_unavailable_single_or_bulk() {
    for bulk in [false, true] {
        let mut state = viewing(THREAD);
        apply(
            &mut state,
            "MESSAGE_CREATE",
            starter_row(Some(mine(ORIGIN, GENERAL))),
        );
        assert_eq!(starter_text(&state), "hi");

        let delete = if bulk {
            (
                "MESSAGE_DELETE_BULK",
                json!({"ids": [ORIGIN.to_string()], "channel_id": GENERAL.to_string()}),
            )
        } else {
            (
                "MESSAGE_DELETE",
                json!({"id": ORIGIN.to_string(), "channel_id": GENERAL.to_string()}),
            )
        };
        let events = apply(&mut state, delete.0, delete.1);

        assert_eq!(events, [updated(STARTER_ROW)], "bulk {bulk}");
        assert_eq!(starter_text(&state), "(unavailable)", "bulk {bulk}");
    }
}

// The composer's pending reply ends on its target's deletion, which Discord's client misses
// for bulk deletes; the target held without a window reads as deleted too.
#[test]
fn a_held_replys_target_reads_deleted_after_a_bulk_delete_without_a_window() {
    let mut state = general();
    let now = state.now_millis();
    assert!(state.hold_reply(id(GENERAL), id(10), now, &mut Vec::new()));
    for other in 1..=10 {
        state.view_channel(id(300_000_000_000_001_000 + other), &mut Vec::new());
    }
    assert!(state.messages(id(GENERAL)).is_none(), "the window went");

    let events = apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": ["10"], "channel_id": GENERAL.to_string()}),
    );

    assert_eq!(events, [format!("ReplyTargetChanged({GENERAL}, 10)")]);
    assert_eq!(
        state.reply_target(id(GENERAL), id(10)),
        ReplyTarget::Deleted
    );
    assert_eq!(state.held_reply(id(GENERAL)), Some(id(10)));
    let pending = Message::pending(
        id(78),
        id(GENERAL),
        Arc::new(state.entities.current_user.clone().unwrap().user.clone()),
        "late".into(),
        0,
    );
    let reply = ReplyTo {
        message: id(10),
        mention: true,
    };
    assert!(!state.queue(id(GENERAL), pending, Some(reply), now, &mut Vec::new()));
}

#[test]
fn up_arrow_picks_the_newest_loaded_message_the_user_may_edit() {
    let mut state = viewing(GENERAL);
    assert_eq!(state.newest_editable_message(id(GENERAL), 0), None);
    apply(&mut state, "MESSAGE_CREATE", mine(10, GENERAL));
    let mut sticker_alone = mine(11, GENERAL);
    sticker_alone["content"] = "".into();
    sticker_alone["sticker_items"] =
        json!([{"id": "800000000000000001", "name": "wave", "format_type": 1}]);
    apply(&mut state, "MESSAGE_CREATE", sticker_alone);
    let mut pin_line = mine(12, GENERAL);
    pin_line["type"] = 6.into();
    apply(&mut state, "MESSAGE_CREATE", pin_line);
    apply(&mut state, "MESSAGE_CREATE", message(13, GENERAL));

    let now = state.now_millis();
    assert_eq!(
        state.newest_editable_message(id(GENERAL), now),
        Some(id(10))
    );
    apply(&mut state, "MESSAGE_CREATE", mine(14, GENERAL));
    assert_eq!(
        state.newest_editable_message(id(GENERAL), now),
        Some(id(14))
    );
    begin_delete(&mut state, GENERAL, 14);
    assert_eq!(
        state.newest_editable_message(id(GENERAL), now),
        Some(id(10))
    );
    assert_eq!(state.newest_editable_message(id(THREAD), now), None);
}

#[test]
fn a_message_with_a_pending_edit_is_cached_with_discords_text() {
    let mut state = viewing(GENERAL);
    load_page(&mut state, GENERAL, LoadKind::Latest, &[], true);
    for message in [10, 11] {
        apply(&mut state, "MESSAGE_CREATE", mine(message, GENERAL));
    }
    apply(&mut state, "MESSAGE_CREATE", reply_to_ten(12));
    save(&mut state, GENERAL, 10, "unconfirmed");
    save(&mut state, GENERAL, 11, "refused");
    asked(&mut state, GENERAL, 11);
    answer(&mut state, GENERAL, 11, Err(EditError::Forbidden));

    let (_, windows) = state.cache_parts(50);

    let cached: Vec<String> = windows[0]
        .1
        .messages
        .iter()
        .map(|message| message.content.to_string())
        .collect();
    assert_eq!(cached, ["hi", "hi", "hi"]);
    match windows[0].1.targets.get(&(id(GENERAL), id(10))) {
        Some(ReplyTarget::Loaded(preview)) => assert_eq!(&*preview.content, "hi", "the reply line"),
        other => panic!("expected the reply's target, got {other:?}"),
    }
    assert_eq!(
        text(&state, GENERAL, 10),
        "unconfirmed",
        "the window keeps it"
    );
}

// D162: every copy shows the saved text until Discord answers, also one a page brings meanwhile.
#[test]
fn a_page_loaded_while_a_save_waits_shows_the_saved_text_in_its_rows_and_reply_lines() {
    let mut state = general();
    save(&mut state, GENERAL, 10, "saved");
    asked(&mut state, GENERAL, 10);
    let mut reply = reply_to_ten(11);
    reply["referenced_message"] = mine(10, GENERAL);

    let mut events = Vec::new();
    let ticket = state
        .begin_load(id(GENERAL), LoadKind::Latest, 1, &mut events)
        .unwrap();
    state.finish_load(
        ticket,
        vec![wire(&mine(10, GENERAL)), wire(&reply)],
        true,
        &mut events,
    );

    assert_eq!(text(&state, GENERAL, 10), "saved");
    assert_eq!(edit_of(&state, GENERAL, 10), "pending");
    match state.reply_target(id(GENERAL), id(10)) {
        ReplyTarget::Loaded(preview) => assert_eq!(&*preview.content, "saved"),
        other => panic!("expected a loaded target, got {other:?}"),
    }
    let answered = edited(10, GENERAL, "saved", EARLIER);
    answer(&mut state, GENERAL, 10, Ok(Some(answered)));
    assert_eq!(text(&state, GENERAL, 10), "saved");
}
