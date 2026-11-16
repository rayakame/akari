use super::*;
use crate::model::iso;
use crate::state::windows::{PinsAsk, PinsLoad};
use crate::state::{PinsPage, PinsState, ReplyTarget};

// Pin times, a minute apart: `at(5)` is newer than `at(4)`.
const START: i64 = 1_790_000_000_000;

fn at(minute: i64) -> i64 {
    START + minute * 60_000
}

fn id<M>(value: u64) -> Snowflake<M> {
    Snowflake::new(value)
}

fn pinned(message_id: u64, channel: u64) -> Value {
    let mut message = message(message_id, channel);
    message["pinned"] = true.into();
    message
}

fn pin(message_id: u64, minute: i64) -> Value {
    json!({"pinned_at": iso(at(minute)), "message": pinned(message_id, GENERAL)})
}

// As the REST answer is read, which borrows its lists.
fn wire(page: &Value) -> model::MessagePins {
    serde_json::from_str(&page.to_string()).unwrap()
}

fn changed(channel: u64) -> String {
    format!("PinsChanged({channel})")
}

fn begin(state: &mut State, load: PinsLoad) -> (Option<PinsAsk>, Vec<String>) {
    let mut events = Vec::new();
    let ask = state.begin_pins(id(GENERAL), load, &mut events);
    (ask, events.iter().map(describe).collect())
}

fn finish(state: &mut State, ask: PinsAsk, pins: &[Value], more: bool) -> Vec<String> {
    let page = wire(&json!({"items": pins, "has_more": more}));
    let mut events = Vec::new();
    state.finish_pins(ask, page, &mut events);
    events.iter().map(describe).collect()
}

fn load(state: &mut State, load: PinsLoad, pins: &[Value], more: bool) -> Vec<String> {
    let (ask, mut events) = begin(state, load);
    let ask = ask.expect("nothing asked");
    events.extend(finish(state, ask, pins, more));
    events
}

fn page(state: &State) -> PinsPage {
    state.pins(id(GENERAL))
}

fn listed(state: &State) -> Vec<u64> {
    page(state)
        .items
        .iter()
        .map(|item| item.message.id.get())
        .collect()
}

fn text_of(state: &State, message_id: u64) -> String {
    page(state)
        .items
        .iter()
        .find(|item| item.message.id.get() == message_id)
        .map_or_else(
            || "(not pinned)".to_owned(),
            |item| item.message.content.to_string(),
        )
}

fn last_pin(state: &mut State, minute: i64) -> Vec<String> {
    apply(
        state,
        "CHANNEL_PINS_UPDATE",
        json!({
            "channel_id": GENERAL.to_string(), "guild_id": G1.to_string(),
            "last_pin_timestamp": iso(at(minute)),
        }),
    )
}

fn update(state: &mut State, message_id: u64, fields: Value) -> Vec<String> {
    let mut update = json!({"id": message_id.to_string(), "channel_id": GENERAL.to_string()});
    for (key, value) in fields.as_object().unwrap() {
        update[key] = value.clone();
    }
    apply(state, "MESSAGE_UPDATE", update)
}

fn whole(state: &mut State, message_id: u64, pinned: bool, text: &str) -> Vec<String> {
    let mut update = message(message_id, GENERAL);
    update["pinned"] = pinned.into();
    update["content"] = text.into();
    apply(state, "MESSAGE_UPDATE", update)
}

fn react(state: &mut State, message_id: u64) -> Vec<String> {
    apply(
        state,
        "MESSAGE_REACTION_ADD",
        json!({
            "channel_id": GENERAL.to_string(), "message_id": message_id.to_string(),
            "guild_id": G1.to_string(), "user_id": MIRA.to_string(),
            "emoji": {"id": null, "name": "🔥"}, "type": 0,
        }),
    )
}

fn fires(state: &State, message_id: u64) -> u32 {
    page(state)
        .items
        .iter()
        .find(|item| item.message.id.get() == message_id)
        .and_then(|item| item.message.reactions.clone())
        .and_then(|reactions| reactions.first().map(|reaction| reaction.normal))
        .unwrap_or(0)
}

// Pins 3, 2 and 1, pinned in that order at minutes 3, 2 and 1; Discord has more.
fn three_pins() -> State {
    let mut state = ready_state();
    load(
        &mut state,
        PinsLoad::First,
        &[pin(3, 3), pin(2, 2), pin(1, 1)],
        true,
    );
    state
}

#[test]
fn a_channel_whose_pins_were_never_asked_for_has_none_loaded() {
    let state = ready_state();

    assert_eq!(
        page(&state),
        PinsPage {
            items: Vec::new(),
            has_more: false,
            state: PinsState::NotLoaded,
        }
    );
}

#[test]
fn the_first_page_lists_the_pins_in_discords_order_and_says_whether_more_come() {
    let mut state = ready_state();

    let (ask, began) = begin(&mut state, PinsLoad::First);
    let ask = ask.unwrap();
    assert_eq!(ask.before, None);
    assert_eq!(began, [changed(GENERAL)]);
    assert_eq!(page(&state).state, PinsState::Loading);

    let landed = finish(&mut state, ask, &[pin(3, 3), pin(2, 2)], true);

    assert_eq!(landed, [changed(GENERAL)]);
    let page = page(&state);
    assert_eq!(page.state, PinsState::Loaded);
    assert!(page.has_more);
    assert_eq!(listed(&state), [3, 2]);
    assert_eq!(page.items[0].pinned_at.unix_millis(), at(3));
    assert!(page.items[0].message.pinned);
}

#[test]
fn a_failed_first_load_is_told_apart_from_an_empty_list() {
    let mut empty = ready_state();
    load(&mut empty, PinsLoad::First, &[], false);
    let mut failed = ready_state();
    let (ask, _) = begin(&mut failed, PinsLoad::First);
    let mut events = Vec::new();
    failed.fail_pins(ask.unwrap(), &mut events);

    assert_eq!(page(&empty).state, PinsState::Loaded);
    assert!(page(&empty).items.is_empty() && !page(&empty).has_more);
    assert_eq!(page(&failed).state, PinsState::Failed);
    assert!(page(&failed).items.is_empty());
    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        [changed(GENERAL)]
    );
}

#[test]
fn the_next_page_asks_before_the_last_pins_time_and_follows_it() {
    let mut state = three_pins();

    let (ask, _) = begin(&mut state, PinsLoad::More);
    let ask = ask.unwrap();
    assert_eq!(ask.before.map(Timestamp::unix_millis), Some(at(1)));
    finish(&mut state, ask, &[pin(9, 0)], false);

    assert_eq!(listed(&state), [3, 2, 1, 9]);
    assert!(!page(&state).has_more);
    assert_eq!(page(&state).state, PinsState::Loaded);
    assert!(
        begin(&mut state, PinsLoad::More).0.is_none(),
        "the last page"
    );
}

#[test]
fn nothing_is_asked_while_a_page_loads_or_once_the_list_is_current() {
    let mut state = ready_state();
    assert!(begin(&mut state, PinsLoad::More).0.is_none(), "no list yet");
    assert_eq!(page(&state), PinsPage::default());
    let (first, _) = begin(&mut state, PinsLoad::First);
    assert!(first.is_some());

    assert!(begin(&mut state, PinsLoad::First).0.is_none());
    assert!(begin(&mut state, PinsLoad::More).0.is_none());
    finish(&mut state, first.unwrap(), &[pin(3, 3)], true);
    assert!(begin(&mut state, PinsLoad::First).0.is_none(), "current");
    let unknown = state.begin_pins(id(999), PinsLoad::First, &mut Vec::new());
    assert!(unknown.is_none(), "a channel the store doesn't have");
}

#[test]
fn a_first_page_that_failed_is_asked_again_and_a_later_one_by_more() {
    let mut state = ready_state();
    let (ask, _) = begin(&mut state, PinsLoad::First);
    state.fail_pins(ask.unwrap(), &mut Vec::new());
    assert!(begin(&mut state, PinsLoad::More).0.is_none());
    let (again, _) = begin(&mut state, PinsLoad::First);
    finish(&mut state, again.unwrap(), &[pin(3, 3)], true);

    let (more, _) = begin(&mut state, PinsLoad::More);
    state.fail_pins(more.unwrap(), &mut Vec::new());

    assert_eq!(page(&state).state, PinsState::Failed);
    assert_eq!(listed(&state), [3], "the loaded ones stay");
    assert!(page(&state).has_more);
    assert!(begin(&mut state, PinsLoad::First).0.is_none());
    assert!(begin(&mut state, PinsLoad::More).0.is_some());
}

#[test]
fn a_page_given_up_before_its_answer_leaves_the_list_as_it_was() {
    let mut state = three_pins();
    let (more, _) = begin(&mut state, PinsLoad::More);
    let mut events = Vec::new();

    state.abandon_pins(more.unwrap(), &mut events);

    assert_eq!(page(&state).state, PinsState::Loaded);
    assert_eq!(listed(&state), [3, 2, 1]);
    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        [changed(GENERAL)]
    );
    let mut fresh = ready_state();
    let (first, _) = begin(&mut fresh, PinsLoad::First);
    fresh.abandon_pins(first.unwrap(), &mut Vec::new());
    assert_eq!(page(&fresh).state, PinsState::NotLoaded);
}

#[test]
fn a_pin_the_gateway_never_placed_in_the_list_has_it_asked_again() {
    let mut state = three_pins();
    last_pin(&mut state, 3);
    assert!(begin(&mut state, PinsLoad::First).0.is_none(), "known");

    last_pin(&mut state, 4);
    let (ask, _) = begin(&mut state, PinsLoad::First);
    finish(&mut state, ask.unwrap(), &[pin(4, 4), pin(3, 3)], true);

    assert_eq!(listed(&state), [4, 3, 2, 1]);
    assert!(begin(&mut state, PinsLoad::First).0.is_none());
}

// Discord's newest pin time stays when every pin goes: an empty list has seen it.
#[test]
fn an_empty_list_has_seen_the_channels_last_pin() {
    let mut state = ready_state();
    last_pin(&mut state, 2);

    load(&mut state, PinsLoad::First, &[], false);

    assert!(begin(&mut state, PinsLoad::First).0.is_none());
}

#[test]
fn a_new_session_keeps_the_pins_shown_and_asks_for_them_again() {
    let mut state = three_pins();
    let (late, _) = begin(&mut state, PinsLoad::More);

    let events = apply(&mut state, "READY", ready_value());
    let dropped = finish(&mut state, late.unwrap(), &[pin(9, 0)], false);

    assert!(events.contains(&changed(GENERAL)), "{events:?}");
    assert_eq!(page(&state).state, PinsState::NotLoaded);
    assert_eq!(listed(&state), [3, 2, 1], "shown until the reload");
    assert!(dropped.is_empty(), "asked before the READY: {dropped:?}");
    assert!(begin(&mut state, PinsLoad::First).0.is_some());
}

#[test]
fn a_reload_puts_each_page_in_place_of_the_pins_its_times_cover() {
    let mut state = ready_state();
    load(
        &mut state,
        PinsLoad::First,
        &[pin(5, 5), pin(4, 4), pin(3, 3), pin(2, 2), pin(1, 1)],
        false,
    );
    apply(&mut state, "READY", ready_value());

    load(
        &mut state,
        PinsLoad::First,
        &[pin(6, 6), pin(5, 5), pin(3, 3)],
        true,
    );
    assert_eq!(listed(&state), [6, 5, 3, 2, 1], "4 unpinned meanwhile");
    assert_eq!(page(&state).state, PinsState::Loaded);
    assert!(page(&state).has_more);

    let (more, _) = begin(&mut state, PinsLoad::More);
    let more = more.unwrap();
    assert_eq!(more.before.map(Timestamp::unix_millis), Some(at(3)));
    finish(&mut state, more, &[pin(2, 2)], false);
    assert_eq!(listed(&state), [6, 5, 3, 2], "1 unpinned meanwhile");
    assert!(!page(&state).has_more);
}

#[test]
fn a_page_never_lists_a_pin_twice() {
    let mut state = three_pins();

    load(&mut state, PinsLoad::More, &[pin(1, 1), pin(0, 0)], false);

    assert_eq!(listed(&state), [3, 2, 1, 0]);
}

// Discord said more and sent none: the list can't go on from there.
#[test]
fn an_empty_page_that_says_more_ends_the_list() {
    let mut state = three_pins();

    load(&mut state, PinsLoad::More, &[], true);

    assert!(!page(&state).has_more);
    assert!(begin(&mut state, PinsLoad::More).0.is_none());
}

// Its cursor couldn't move past pins it left out.
#[test]
fn a_page_that_leaves_out_every_pin_ends_the_list() {
    let mut state = three_pins();
    let elsewhere = json!({"pinned_at": iso(at(0)), "message": pinned(9, VOICE)});

    load(&mut state, PinsLoad::More, &[elsewhere], true);

    assert_eq!(listed(&state), [3, 2, 1]);
    assert!(!page(&state).has_more);
    assert!(begin(&mut state, PinsLoad::More).0.is_none());
}

// No pin of this connection is left to go on from.
#[test]
fn more_asks_for_the_first_page_once_every_pin_of_this_connection_went() {
    let mut state = three_pins();
    apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": ["1", "2", "3"], "channel_id": GENERAL.to_string()}),
    );
    assert!(listed(&state).is_empty() && page(&state).has_more);

    let (more, _) = begin(&mut state, PinsLoad::More);
    let more = more.expect("nothing asked");
    assert_eq!(more.before, None);
    finish(&mut state, more, &[pin(0, 0)], false);

    assert_eq!(listed(&state), [0]);
    assert!(!page(&state).has_more);
}

#[test]
fn a_pin_without_a_time_takes_its_messages() {
    let mut state = ready_state();
    let mut timeless = pin(3, 3);
    timeless["pinned_at"] = Value::Null;

    load(&mut state, PinsLoad::First, &[timeless], false);

    let item = &page(&state).items[0];
    assert_eq!(item.pinned_at, item.message.timestamp);
}

#[test]
fn an_edit_a_reaction_and_an_unpin_reach_a_pinned_message() {
    let mut state = three_pins();

    let edited = update(&mut state, 2, json!({"content": "edited"}));
    let reacted = react(&mut state, 2);
    assert_eq!(text_of(&state, 2), "edited");
    assert_eq!(fires(&state, 2), 1);
    assert_eq!(edited, [changed(GENERAL)]);
    assert_eq!(reacted, [changed(GENERAL)]);

    let unpinned = update(&mut state, 2, json!({"pinned": false}));
    assert_eq!(unpinned, [changed(GENERAL)]);
    assert_eq!(listed(&state), [3, 1]);
    assert!(update(&mut state, 77, json!({"content": "x"})).is_empty());
}

#[test]
fn a_delete_and_a_bulk_delete_take_pins_away() {
    let mut state = three_pins();

    let deleted = apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "3", "channel_id": GENERAL.to_string()}),
    );
    let bulk = apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": ["1", "77"], "channel_id": GENERAL.to_string()}),
    );

    assert_eq!(listed(&state), [2]);
    assert_eq!(deleted, [changed(GENERAL)]);
    assert_eq!(bulk, [changed(GENERAL)]);
}

#[test]
fn a_new_pin_of_a_message_the_window_shows_goes_to_the_front() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    let events = whole(&mut state, 10, true, "hi");

    assert_eq!(listed(&state), [10, 3, 2, 1]);
    assert!(events.contains(&changed(GENERAL)), "{events:?}");
    let front = &page(&state).items[0];
    assert!(front.message.pinned);
    assert!(
        front.pinned_at.unix_millis() >= at(3),
        "never behind the list"
    );
    let (more, _) = begin(&mut state, PinsLoad::More);
    assert_eq!(
        more.unwrap().before.map(Timestamp::unix_millis),
        Some(at(1)),
        "the pages go on after the last one"
    );
}

// This device's clock may be behind Discord's.
#[test]
fn a_new_pin_is_never_placed_behind_the_newest_one() {
    let mut state = ready_state();
    let future = at(60 * 24 * 365 * 100);
    let ahead = json!({"pinned_at": iso(future), "message": pinned(3, GENERAL)});
    load(&mut state, PinsLoad::First, &[ahead], false);
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    whole(&mut state, 10, true, "hi");

    let page = page(&state);
    assert_eq!(listed(&state), [10, 3]);
    assert!(page.items[0].pinned_at >= page.items[1].pinned_at);
}

#[test]
fn a_deleted_pin_leaves_the_pages_going_on_after_the_last_one() {
    let mut state = three_pins();
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "3", "channel_id": GENERAL.to_string()}),
    );

    let (more, _) = begin(&mut state, PinsLoad::More);

    assert_eq!(
        more.unwrap().before.map(Timestamp::unix_millis),
        Some(at(1))
    );
}

#[test]
fn a_reload_for_a_newer_pin_keeps_the_pages_after_it() {
    let mut state = three_pins();
    last_pin(&mut state, 4);

    load(&mut state, PinsLoad::First, &[pin(4, 4), pin(3, 3)], true);

    assert_eq!(listed(&state), [4, 3, 2, 1]);
    let (more, _) = begin(&mut state, PinsLoad::More);
    assert_eq!(
        more.unwrap().before.map(Timestamp::unix_millis),
        Some(at(1))
    );
}

#[test]
fn a_new_session_whose_first_page_failed_asks_it_again_also_by_more() {
    let mut state = three_pins();
    apply(&mut state, "READY", ready_value());
    let (ask, _) = begin(&mut state, PinsLoad::First);
    state.fail_pins(ask.unwrap(), &mut Vec::new());

    assert_eq!(page(&state).state, PinsState::Failed);
    assert_eq!(listed(&state), [3, 2, 1], "an earlier connection's, shown");
    let (more, _) = begin(&mut state, PinsLoad::More);
    let more = more.expect("nothing asked");
    assert_eq!(more.before, None);
    finish(&mut state, more, &[pin(3, 3), pin(2, 2)], false);
    assert_eq!(listed(&state), [3, 2], "1 unpinned meanwhile");
}

#[test]
fn a_page_leaves_out_a_message_of_another_channel_and_lists_its_own_as_pinned() {
    let mut state = ready_state();
    let mut unflagged = pin(3, 3);
    unflagged["message"]
        .as_object_mut()
        .unwrap()
        .remove("pinned");
    let elsewhere = json!({"pinned_at": iso(at(2)), "message": pinned(2, VOICE)});

    load(&mut state, PinsLoad::First, &[unflagged, elsewhere], false);

    assert_eq!(listed(&state), [3]);
    assert!(page(&state).items[0].message.pinned);
}

#[test]
fn a_pinned_message_a_reply_names_answers_it_once_a_page_brings_it() {
    let mut state = ready_state();
    state.view_channel(id(GENERAL), &mut Vec::new());
    let mut reply = message(20, GENERAL);
    reply["type"] = 19.into();
    reply["message_reference"] = json!({"message_id": "3", "channel_id": GENERAL.to_string()});
    apply(&mut state, "MESSAGE_CREATE", reply);
    assert_eq!(
        state.reply_target(id(GENERAL), id(3)),
        ReplyTarget::NotLoaded
    );

    load(&mut state, PinsLoad::First, &[pin(3, 3)], false);

    assert!(matches!(
        state.reply_target(id(GENERAL), id(3)),
        ReplyTarget::Loaded(_)
    ));
}

// A pin of an earlier connection may have missed reactions: the page's counts win.
#[test]
fn a_reload_after_a_new_session_takes_the_pages_counts() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", pinned(3, GENERAL));
    react(&mut state, 3);
    apply(&mut state, "READY", ready_value());

    load(&mut state, PinsLoad::First, &[pin(3, 3)], false);

    assert_eq!(fires(&state, 3), 0);
}

// A new session may have missed its unpin.
#[test]
fn a_pin_the_gateway_placed_before_a_new_session_waits_for_a_page() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    whole(&mut state, 10, true, "hi");
    apply(&mut state, "READY", ready_value());

    load(
        &mut state,
        PinsLoad::First,
        &[pin(3, 3), pin(2, 2), pin(1, 1)],
        false,
    );

    assert_eq!(listed(&state), [3, 2, 1]);
}

// Until a page of this connection lands, a pin not listed may be on a page not loaded yet.
#[test]
fn a_list_of_an_earlier_connection_isnt_complete() {
    let mut state = ready_state();
    load(&mut state, PinsLoad::First, &[pin(3, 3)], false);
    apply(&mut state, "READY", ready_value());
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", pinned(10, GENERAL));

    whole(&mut state, 10, true, "an edit of an old pin");

    assert_eq!(listed(&state), [3]);
}

#[test]
fn an_older_pin_time_names_no_pin() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    whole(&mut state, 10, true, "hi");

    last_pin(&mut state, 2);

    assert_ne!(page(&state).items[0].pinned_at.unix_millis(), at(2));
}

#[test]
fn a_pinned_message_of_a_page_not_loaded_yet_stays_out_until_its_page() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", pinned(10, GENERAL));
    let unknown = whole(&mut state, 11, true, "old");

    let edited = whole(&mut state, 10, true, "an edit of an old pin");

    assert_eq!(listed(&state), [3, 2, 1]);
    assert!(!edited.contains(&changed(GENERAL)), "{edited:?}");
    assert!(unknown.is_empty(), "no copy to show: {unknown:?}");
}

// Every pin is listed, so a pinned message that isn't is a new pin.
#[test]
fn a_complete_list_takes_a_pinned_message_it_lacks() {
    let mut state = ready_state();
    load(&mut state, PinsLoad::First, &[pin(3, 3)], false);
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", pinned(10, GENERAL));

    whole(&mut state, 10, true, "pinned");

    assert_eq!(listed(&state), [10, 3]);
}

#[test]
fn the_channels_pin_time_names_a_pin_the_gateway_placed_in_either_order() {
    for pin_update_first in [false, true] {
        let mut state = three_pins();
        state.view_channel(id(GENERAL), &mut Vec::new());
        apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

        let named = if pin_update_first {
            last_pin(&mut state, 9);
            whole(&mut state, 10, true, "hi")
        } else {
            whole(&mut state, 10, true, "hi");
            last_pin(&mut state, 9)
        };

        assert_eq!(
            page(&state).items[0].pinned_at.unix_millis(),
            at(9),
            "the update first: {pin_update_first}"
        );
        assert!(named.contains(&changed(GENERAL)), "{named:?}");
        assert!(
            begin(&mut state, PinsLoad::First).0.is_none(),
            "{pin_update_first}: placed, nothing to ask"
        );
    }
}

#[test]
fn changes_while_a_page_loads_are_put_over_it() {
    let mut state = ready_state();
    let (ask, _) = begin(&mut state, PinsLoad::First);

    update(&mut state, 3, json!({"content": "edited"}));
    react(&mut state, 3);
    update(&mut state, 2, json!({"pinned": false}));
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "1", "channel_id": GENERAL.to_string()}),
    );
    finish(
        &mut state,
        ask.unwrap(),
        &[pin(3, 3), pin(2, 2), pin(1, 1)],
        false,
    );

    assert_eq!(listed(&state), [3]);
    assert_eq!(text_of(&state, 3), "edited");
    assert_eq!(fires(&state, 3), 1);
}

#[test]
fn a_page_newer_than_an_edit_while_it_loaded_keeps_its_text() {
    let mut state = ready_state();
    let (ask, _) = begin(&mut state, PinsLoad::First);
    update(
        &mut state,
        3,
        json!({"content": "older", "edited_timestamp": iso(at(1))}),
    );
    let mut newer = pin(3, 3);
    newer["message"]["content"] = "newer".into();
    newer["message"]["edited_timestamp"] = iso(at(2)).into();

    finish(&mut state, ask.unwrap(), &[newer], false);

    assert_eq!(text_of(&state, 3), "newer");
}

// The current copy counted every reaction since; the page may or may not have them.
#[test]
fn a_reload_keeps_the_counts_of_a_pin_it_had_current() {
    let mut state = three_pins();
    last_pin(&mut state, 4);
    react(&mut state, 3);
    let (ask, _) = begin(&mut state, PinsLoad::First);
    react(&mut state, 3);

    finish(&mut state, ask.unwrap(), &[pin(4, 4), pin(3, 3)], true);

    assert_eq!(fires(&state, 3), 2);
}

#[test]
fn a_pin_the_gateway_placed_while_a_page_loads_stays_when_the_page_lacks_it() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    last_pin(&mut state, 4);
    let (ask, _) = begin(&mut state, PinsLoad::First);
    whole(&mut state, 10, true, "hi");

    finish(&mut state, ask.unwrap(), &[pin(4, 4), pin(3, 3)], true);

    assert_eq!(listed(&state), [10, 4, 3, 2, 1]);
    let (more, _) = begin(&mut state, PinsLoad::More);
    assert_eq!(
        more.unwrap().before.map(Timestamp::unix_millis),
        Some(at(1)),
        "the pages go on after the last one"
    );
}

// The page was asked before the unpin: the repin the gateway placed after it is the one that stays.
#[test]
fn an_unpin_and_a_repin_while_a_page_loads_leave_the_message_pinned() {
    let mut state = ready_state();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    load(&mut state, PinsLoad::First, &[pin(10, 4), pin(3, 3)], false);
    last_pin(&mut state, 5);
    let (ask, _) = begin(&mut state, PinsLoad::First);
    whole(&mut state, 10, false, "hi");
    whole(&mut state, 10, true, "hi");
    let repinned = pinned_at(&state, 10);

    finish(&mut state, ask.unwrap(), &[pin(10, 4), pin(3, 3)], false);

    assert_eq!(listed(&state), [10, 3]);
    assert_ne!(repinned, at(4));
    assert_eq!(
        pinned_at(&state, 10),
        repinned,
        "the repin's time, not the page's"
    );
}

// The page predates them; the replies' lines show the message as it is.
#[test]
fn a_reply_target_edited_or_deleted_while_a_page_loads_keeps_the_change() {
    let mut state = ready_state();
    state.view_channel(id(GENERAL), &mut Vec::new());
    for (reply_id, target) in [(20, "3"), (21, "2")] {
        let mut reply = message(reply_id, GENERAL);
        reply["type"] = 19.into();
        reply["message_reference"] =
            json!({"message_id": target, "channel_id": GENERAL.to_string()});
        apply(&mut state, "MESSAGE_CREATE", reply);
    }
    let (ask, _) = begin(&mut state, PinsLoad::First);
    update(&mut state, 3, json!({"content": "edited"}));
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "2", "channel_id": GENERAL.to_string()}),
    );

    finish(&mut state, ask.unwrap(), &[pin(3, 3), pin(2, 2)], false);

    match state.reply_target(id(GENERAL), id(3)) {
        ReplyTarget::Loaded(preview) => assert_eq!(&*preview.content, "edited"),
        other => panic!("{other:?}"),
    }
    assert_eq!(state.reply_target(id(GENERAL), id(2)), ReplyTarget::Deleted);
}

#[test]
fn at_most_ten_channels_keep_their_pins() {
    let mut state = ready_state();
    let channels: Vec<u64> = (0..11).map(|n| 300_000_000_000_000_700 + n).collect();
    for channel in &channels {
        apply(
            &mut state,
            "CHANNEL_CREATE",
            json!({"id": channel.to_string(), "type": 0, "guild_id": G1.to_string(),
                   "name": "c", "position": 9, "permission_overwrites": []}),
        );
    }
    for channel in &channels[..10] {
        let ask = state.begin_pins(id(*channel), PinsLoad::First, &mut Vec::new());
        state.finish_pins(
            ask.unwrap(),
            wire(&json!({"items": [], "has_more": false})),
            &mut Vec::new(),
        );
    }
    let mut events = Vec::new();

    state.begin_pins(id(channels[10]), PinsLoad::First, &mut events);

    let events: Vec<String> = events.iter().map(describe).collect();
    assert!(events.contains(&changed(channels[0])), "{events:?}");
    assert_eq!(state.pins(id(channels[0])).state, PinsState::NotLoaded);
    assert_eq!(state.pins(id(channels[1])).state, PinsState::Loaded);
}

#[test]
fn a_deleted_channel_or_a_left_guild_takes_its_pins() {
    let mut deleted = three_pins();
    apply(
        &mut deleted,
        "CHANNEL_DELETE",
        json!({"id": GENERAL.to_string(), "guild_id": G1.to_string(), "type": 0}),
    );
    let mut left = three_pins();
    apply(&mut left, "GUILD_DELETE", json!({"id": G1.to_string()}));

    assert!(page(&deleted).items.is_empty());
    assert_eq!(page(&deleted).state, PinsState::NotLoaded);
    assert!(page(&left).items.is_empty());
}

#[test]
fn a_pinned_reply_keeps_the_message_it_answers() {
    let mut state = ready_state();
    let mut reply = pin(5, 5);
    reply["message"]["type"] = 19.into();
    reply["message"]["message_reference"] =
        json!({"message_id": "4", "channel_id": GENERAL.to_string()});
    reply["message"]["referenced_message"] = message(4, GENERAL);

    load(&mut state, PinsLoad::First, &[reply], false);

    assert!(matches!(
        state.reply_target(id(GENERAL), id(4)),
        ReplyTarget::Loaded(_)
    ));
}

#[test]
fn a_pinned_message_outside_the_window_has_its_actions_and_reactions() {
    let mut state = three_pins();
    react(&mut state, 2);

    let now = state.now_millis();
    let actions = state.message_actions(id(GENERAL), id(2), now);

    assert!(actions.unpin && !actions.pin, "{actions:?}");
    assert_eq!(state.reactions(id(GENERAL), id(2)).len(), 1);
    assert_eq!(
        state.shown_message(id(GENERAL), id(2)),
        Some((id(GENERAL), id(2)))
    );
}

// As its actions allow, and without letting go of the reply held before.
#[test]
fn a_pinned_message_outside_the_window_takes_a_reply() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    let now = state.now_millis();
    assert!(state.hold_reply(id(GENERAL), id(10), now, &mut Vec::new()));
    assert!(state.message_actions(id(GENERAL), id(2), now).reply);

    assert!(state.hold_reply(id(GENERAL), id(2), now, &mut Vec::new()));
    assert_eq!(state.held_reply(id(GENERAL)), Some(id(2)));
}

#[test]
fn acking_pins_takes_the_dot_only_while_it_shows() {
    let mut state = ready_state();
    assert!(!state.ack_pins(id(GENERAL), &mut Vec::new()), "no pin");
    last_pin(&mut state, 3);
    let now = Timestamp::from_unix_millis(state.now_millis());
    assert!(state.pins_unread(id(GENERAL), now));
    let mut events = Vec::new();

    assert!(state.ack_pins(id(GENERAL), &mut events));

    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        [format!("PinsUnreadChanged({GENERAL})")]
    );
    assert!(!state.pins_unread(id(GENERAL), now));
    assert_eq!(
        state
            .entities
            .read_states
            .pins_acked(id(GENERAL))
            .map(Timestamp::unix_millis),
        Some(at(3))
    );
    assert!(!state.ack_pins(id(GENERAL), &mut Vec::new()), "seen");
}

// The list is Discord's newer word than a copy loaded before the pin.
#[test]
fn a_message_the_pins_list_has_is_unpinned_whatever_an_older_copy_says() {
    let mut state = ready_state();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(2, GENERAL));
    load(&mut state, PinsLoad::First, &[pin(2, 2)], false);

    let now = state.now_millis();
    let actions = state.message_actions(id(GENERAL), id(2), now);

    assert!(actions.unpin && !actions.pin, "{actions:?}");
}

// Its first page reconciles from the start; a later page would leave its head unasked.
#[test]
fn a_pin_placed_before_the_reload_of_a_new_session_doesnt_page_on() {
    let mut state = three_pins();
    apply(&mut state, "READY", ready_value());
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    whole(&mut state, 10, true, "hi");

    assert_eq!(listed(&state), [10, 3, 2, 1]);
    let (more, _) = begin(&mut state, PinsLoad::More);
    assert_eq!(more.expect("nothing asked").before, None);
}

#[test]
fn a_first_page_that_failed_after_a_new_pin_is_asked_again() {
    let mut state = three_pins();
    apply(&mut state, "READY", ready_value());
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    whole(&mut state, 10, true, "hi");
    last_pin(&mut state, 9);
    let (ask, _) = begin(&mut state, PinsLoad::First);
    state.fail_pins(ask.unwrap(), &mut Vec::new());

    assert_eq!(listed(&state), [10, 3, 2, 1]);
    assert!(begin(&mut state, PinsLoad::First).0.is_some());
}

#[test]
fn an_older_pin_edited_while_the_first_page_loads_waits_for_the_page() {
    let mut state = ready_state();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", pinned(10, GENERAL));
    let (ask, _) = begin(&mut state, PinsLoad::First);

    whole(&mut state, 10, true, "edited");

    assert!(listed(&state).is_empty(), "not before its page");
    finish(&mut state, ask.unwrap(), &[pin(10, 3)], false);
    assert_eq!(listed(&state), [10]);
    assert_eq!(text_of(&state, 10), "edited");
}

#[test]
fn an_empty_list_of_an_earlier_connection_isnt_complete() {
    let mut state = ready_state();
    load(&mut state, PinsLoad::First, &[], false);
    apply(&mut state, "READY", ready_value());
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", pinned(10, GENERAL));

    whole(&mut state, 10, true, "an edit of an old pin");

    assert!(listed(&state).is_empty());
}

#[test]
fn a_page_answered_once_is_never_answered_again() {
    let mut state = ready_state();
    let (ask, _) = begin(&mut state, PinsLoad::First);
    let ask = ask.unwrap();
    finish(&mut state, ask, &[pin(3, 3)], false);
    let mut events = Vec::new();

    state.fail_pins(ask, &mut events);
    state.abandon_pins(ask, &mut events);
    finish(&mut state, ask, &[], false);

    assert!(events.is_empty());
    assert_eq!(page(&state).state, PinsState::Loaded);
    assert_eq!(listed(&state), [3]);
}

// Each pin's CHANNEL_PINS_UPDATE follows its MESSAGE_UPDATE, in the order of the pins.
#[test]
fn the_channels_pin_times_name_the_gateways_pins_oldest_first() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    apply(&mut state, "MESSAGE_CREATE", message(11, GENERAL));
    whole(&mut state, 10, true, "first");
    whole(&mut state, 11, true, "second");

    last_pin(&mut state, 9);
    last_pin(&mut state, 10);

    let times: Vec<i64> = page(&state)
        .items
        .iter()
        .map(|item| item.pinned_at.unix_millis())
        .collect();
    assert_eq!(listed(&state), [11, 10, 3, 2, 1]);
    assert_eq!(times[..2], [at(10), at(9)]);
}

// The window's copy counted every reaction since; a page built after one already has it.
#[test]
fn a_first_page_takes_the_counts_of_the_windows_current_copy() {
    let mut state = ready_state();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", pinned(3, GENERAL));
    let (ask, _) = begin(&mut state, PinsLoad::First);
    react(&mut state, 3);
    let mut counted = pin(3, 3);
    counted["message"]["reactions"] = json!([{
        "emoji": {"id": null, "name": "🔥"}, "count": 1,
        "count_details": {"normal": 1, "burst": 0}, "me": false, "me_burst": false,
    }]);

    finish(&mut state, ask.unwrap(), &[counted], false);

    assert_eq!(fires(&state, 3), 1);
}

fn counted(message_id: u64, minute: i64) -> Value {
    let mut pin = pin(message_id, minute);
    pin["message"]["reactions"] = json!([{
        "emoji": {"id": null, "name": "🔥"}, "count": 1,
        "count_details": {"normal": 1, "burst": 0}, "me": false, "me_burst": false,
    }]);
    pin
}

fn unreact(state: &mut State, message_id: u64) -> Vec<String> {
    apply(
        state,
        "MESSAGE_REACTION_REMOVE",
        json!({
            "channel_id": GENERAL.to_string(), "message_id": message_id.to_string(),
            "guild_id": G1.to_string(), "user_id": MIRA.to_string(),
            "emoji": {"id": null, "name": "🧊"}, "type": 0,
        }),
    )
}

fn history(messages: &[Value]) -> Vec<model::Message> {
    messages
        .iter()
        .map(|message| serde_json::from_str(&message.to_string()).unwrap())
        .collect()
}

// Ten other channels' lists asked for after GENERAL's: GENERAL's is let go.
fn ten_more_lists(state: &mut State) {
    for n in 0..10 {
        let channel = 300_000_000_000_000_700 + n;
        apply(
            state,
            "CHANNEL_CREATE",
            json!({"id": channel.to_string(), "type": 0, "guild_id": G1.to_string(),
                   "name": "c", "position": 9, "permission_overwrites": []}),
        );
        let ask = state.begin_pins(id(channel), PinsLoad::First, &mut Vec::new());
        state.finish_pins(
            ask.unwrap(),
            wire(&json!({"items": [], "has_more": false})),
            &mut Vec::new(),
        );
    }
}

fn pinned_at(state: &State, message_id: u64) -> i64 {
    page(state)
        .items
        .iter()
        .find(|item| item.message.id.get() == message_id)
        .map(|item| item.pinned_at.unix_millis())
        .unwrap()
}

#[test]
fn an_answer_to_a_page_asked_before_a_new_session_never_lands_on_the_next() {
    let mut state = three_pins();
    let (old, _) = begin(&mut state, PinsLoad::More);
    let old = old.unwrap();
    apply(&mut state, "READY", ready_value());
    let (new, _) = begin(&mut state, PinsLoad::First);

    let landed = finish(&mut state, old, &[pin(0, 0)], false);
    state.fail_pins(old, &mut Vec::new());
    state.abandon_pins(old, &mut Vec::new());

    assert!(landed.is_empty(), "{landed:?}");
    assert_eq!(page(&state).state, PinsState::Loading);
    assert_eq!(listed(&state), [3, 2, 1]);
    finish(&mut state, new.unwrap(), &[pin(3, 3), pin(2, 2)], false);
    assert_eq!(listed(&state), [3, 2]);
    assert_eq!(page(&state).state, PinsState::Loaded);
}

#[test]
fn an_earlier_connections_pin_deleted_after_a_new_session_leaves_the_reload_as_it_was() {
    let mut state = three_pins();
    apply(&mut state, "READY", ready_value());

    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "3", "channel_id": GENERAL.to_string()}),
    );

    assert_eq!(listed(&state), [2, 1]);
    assert_eq!(page(&state).state, PinsState::NotLoaded);
    let (ask, _) = begin(&mut state, PinsLoad::First);
    assert_eq!(ask.expect("nothing asked").before, None);
}

// The pin counted the reaction live; the page didn't list it.
#[test]
fn a_reaction_while_a_later_page_loads_counts_once_on_a_pin_the_page_lacks() {
    let mut state = three_pins();
    let (more, _) = begin(&mut state, PinsLoad::More);
    react(&mut state, 3);

    finish(&mut state, more.unwrap(), &[pin(0, 0)], false);

    assert_eq!(fires(&state, 3), 1);
}

#[test]
fn a_reaction_missed_by_a_failed_page_isnt_put_over_the_next() {
    let mut state = three_pins();
    apply(&mut state, "READY", ready_value());
    let (ask, _) = begin(&mut state, PinsLoad::First);
    react(&mut state, 2);
    state.fail_pins(ask.unwrap(), &mut Vec::new());

    load(&mut state, PinsLoad::First, &[counted(2, 2)], false);

    assert_eq!(fires(&state, 2), 1);
}

#[test]
fn a_reaction_missed_before_a_new_session_isnt_put_over_its_first_page() {
    let mut state = three_pins();
    let (_more, _) = begin(&mut state, PinsLoad::More);
    react(&mut state, 2);
    apply(&mut state, "READY", ready_value());

    load(&mut state, PinsLoad::First, &[counted(2, 2)], false);

    assert_eq!(fires(&state, 2), 1);
}

// The last page's newest pin time is what the list has placed, not the channel's newest.
#[test]
fn a_later_page_hasnt_seen_a_newer_pin_the_gateway_never_brought() {
    let mut state = three_pins();
    last_pin(&mut state, 5);

    load(&mut state, PinsLoad::More, &[pin(0, 0)], false);

    assert!(begin(&mut state, PinsLoad::First).0.is_some());
}

#[test]
fn a_later_page_keeps_the_newest_pin_time_the_list_placed() {
    let mut state = three_pins();
    last_pin(&mut state, 3);

    load(&mut state, PinsLoad::More, &[pin(0, 0)], false);

    assert!(begin(&mut state, PinsLoad::First).0.is_none());
}

#[test]
fn a_channels_first_pin_is_asked_for_and_names_a_gateway_pin() {
    let mut missed = ready_state();
    load(&mut missed, PinsLoad::First, &[], false);
    last_pin(&mut missed, 5);
    assert!(
        begin(&mut missed, PinsLoad::First).0.is_some(),
        "never brought"
    );

    let mut placed = ready_state();
    load(&mut placed, PinsLoad::First, &[], false);
    placed.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut placed, "MESSAGE_CREATE", message(10, GENERAL));
    whole(&mut placed, 10, true, "hi");
    last_pin(&mut placed, 5);
    assert_eq!(pinned_at(&placed, 10), at(5));
}

#[test]
fn a_page_asked_from_the_start_that_failed_is_asked_again() {
    let mut state = three_pins();
    apply(
        &mut state,
        "MESSAGE_DELETE_BULK",
        json!({"ids": ["1", "2", "3"], "channel_id": GENERAL.to_string()}),
    );
    let (more, _) = begin(&mut state, PinsLoad::More);
    state.fail_pins(more.unwrap(), &mut Vec::new());

    let (again, _) = begin(&mut state, PinsLoad::First);

    assert_eq!(again.expect("nothing asked").before, None);
}

// A pin carried over a new session is no longer live: only this connection's pins are named.
#[test]
fn a_pin_time_names_this_connections_gateway_pin_not_an_earlier_ones() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    apply(&mut state, "MESSAGE_CREATE", message(11, GENERAL));
    whole(&mut state, 10, true, "first");
    apply(&mut state, "READY", ready_value());
    whole(&mut state, 11, true, "second");

    last_pin(&mut state, 9);

    assert_eq!(pinned_at(&state, 11), at(9));
    assert_ne!(pinned_at(&state, 10), at(9));
}

#[test]
fn an_update_or_a_reaction_that_changes_nothing_says_nothing() {
    let mut state = three_pins();

    let updated = update(&mut state, 3, json!({"pinned": true}));
    let unreacted = unreact(&mut state, 3);

    assert!(!updated.contains(&changed(GENERAL)), "{updated:?}");
    assert!(!unreacted.contains(&changed(GENERAL)), "{unreacted:?}");
}

#[test]
fn a_new_pin_of_a_message_held_below_the_window_goes_to_the_front() {
    let mut state = three_pins();
    state.view_channel(id(GENERAL), &mut Vec::new());
    state.windows.mark_stale(&mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    assert_eq!(state.windows.held(id(GENERAL)).len(), 1);

    whole(&mut state, 10, true, "hi");

    assert_eq!(listed(&state)[0], 10);
}

#[test]
fn a_new_pin_of_the_message_a_threads_starter_row_shows_goes_to_the_front() {
    let mut state = three_pins();
    state.view_channel(id(THREAD), &mut Vec::new());
    let mut starter = message(31, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({"message_id": "10", "channel_id": GENERAL.to_string()});
    starter["referenced_message"] = message(10, GENERAL);
    apply(&mut state, "MESSAGE_CREATE", starter);

    whole(&mut state, 10, true, "hi");

    assert_eq!(listed(&state)[0], 10);
}

// Held while the window loads its latest page, the copy counted the reaction too.
#[test]
fn a_first_page_takes_the_counts_of_a_held_copy() {
    let mut state = ready_state();
    state.view_channel(id(GENERAL), &mut Vec::new());
    let general = id(GENERAL);
    let mut events = Vec::new();
    let around = state
        .begin_load(general, LoadKind::Around(id(20)), 1, &mut events)
        .unwrap();
    state.finish_load(
        around,
        history(&[message(10, GENERAL), message(20, GENERAL)]),
        false,
        &mut events,
    );
    let _latest = state
        .begin_load(general, LoadKind::Latest, 2, &mut events)
        .unwrap();
    apply(&mut state, "MESSAGE_CREATE", pinned(30, GENERAL));
    assert_eq!(state.windows.held(general).len(), 1);
    let (ask, _) = begin(&mut state, PinsLoad::First);
    react(&mut state, 30);

    finish(&mut state, ask.unwrap(), &[counted(30, 3)], false);

    assert_eq!(fires(&state, 30), 1);
}

#[test]
fn a_list_let_go_loosens_the_targets_only_its_pinned_replies_named() {
    let mut state = ready_state();
    let mut reply = pin(5, 5);
    reply["message"]["type"] = 19.into();
    reply["message"]["message_reference"] =
        json!({"message_id": "4", "channel_id": GENERAL.to_string()});
    reply["message"]["referenced_message"] = message(4, GENERAL);
    load(&mut state, PinsLoad::First, &[reply], false);

    ten_more_lists(&mut state);

    assert_eq!(page(&state).state, PinsState::NotLoaded);
    assert_eq!(
        state.reply_target(id(GENERAL), id(4)),
        ReplyTarget::NotLoaded
    );
}

// The page covers pin times down to its last one: a pin there that it lacks was unpinned.
#[test]
fn a_reload_drops_an_unpinned_pin_at_the_pages_last_time() {
    let mut state = three_pins();
    apply(&mut state, "READY", ready_value());

    load(&mut state, PinsLoad::First, &[pin(3, 3), pin(4, 2)], true);

    assert_eq!(listed(&state), [3, 4, 1]);
}

#[test]
fn a_page_listing_a_pin_twice_lists_it_once() {
    let mut state = ready_state();

    load(&mut state, PinsLoad::First, &[pin(5, 5), pin(5, 5)], false);

    assert_eq!(listed(&state), [5]);
}

fn reply_pin(message_id: u64, target: u64, minute: i64) -> Value {
    let mut reply = pin(message_id, minute);
    reply["message"]["type"] = 19.into();
    reply["message"]["message_reference"] =
        json!({"message_id": target.to_string(), "channel_id": GENERAL.to_string()});
    reply["message"]["referenced_message"] = message(target, GENERAL);
    reply
}

#[test]
fn a_page_of_pinned_replies_keeps_the_channels_bound_on_reply_targets() {
    let mut state = ready_state();
    let general = id(GENERAL);
    state.view_channel(general, &mut Vec::new());
    let replies: Vec<Value> = (0..100_u64)
        .map(|index| {
            let mut reply = message(2000 + index, GENERAL);
            reply["type"] = 19.into();
            reply["message_reference"] = json!({
                "message_id": (1000 + index).to_string(), "channel_id": GENERAL.to_string(),
            });
            reply["referenced_message"] = message(1000 + index, GENERAL);
            reply
        })
        .collect();
    let mut events = Vec::new();
    let latest = state
        .begin_load(general, LoadKind::Latest, 1, &mut events)
        .unwrap();
    state.finish_load(latest, history(&replies), true, &mut events);
    let around = state
        .begin_load(general, LoadKind::Around(id(5000)), 2, &mut events)
        .unwrap();
    state.finish_load(
        around,
        history(&[message(5000, GENERAL)]),
        false,
        &mut events,
    );
    assert_eq!(state.windows.targets_len(general), 100);

    let pins: Vec<Value> = (0..5).map(|n| reply_pin(6000 + n, 7000 + n, 5)).collect();
    load(&mut state, PinsLoad::First, &pins, false);

    assert_eq!(state.windows.targets_len(general), 100);
    assert!(matches!(
        state.reply_target(general, id(7004)),
        ReplyTarget::Loaded(_)
    ));
}
