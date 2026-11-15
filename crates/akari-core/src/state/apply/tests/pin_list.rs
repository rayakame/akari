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

// The whole message, as an edit or a pin brings it.
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
    assert!(begin(&mut state, PinsLoad::More).0.is_none());
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
fn a_new_session_asks_nothing_more_after_its_first_page_failed() {
    let mut state = three_pins();
    apply(&mut state, "READY", ready_value());
    let (ask, _) = begin(&mut state, PinsLoad::First);
    state.fail_pins(ask.unwrap(), &mut Vec::new());

    assert_eq!(page(&state).state, PinsState::Failed);
    assert_eq!(listed(&state), [3, 2, 1], "an earlier connection's, shown");
    assert!(begin(&mut state, PinsLoad::More).0.is_none());
    assert!(begin(&mut state, PinsLoad::First).0.is_some());
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

        if pin_update_first {
            last_pin(&mut state, 9);
            whole(&mut state, 10, true, "hi");
        } else {
            whole(&mut state, 10, true, "hi");
            last_pin(&mut state, 9);
        }

        assert_eq!(
            page(&state).items[0].pinned_at.unix_millis(),
            at(9),
            "the update first: {pin_update_first}"
        );
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
