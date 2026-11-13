use super::*;

const FAR: u64 = 5000;

fn general() -> ChannelId {
    Snowflake::new(GENERAL)
}

fn hold(state: &mut State, target: u64) -> (bool, Vec<String>) {
    let now = state.now_millis();
    let mut events = Vec::new();
    let held = state.hold_reply(general(), Snowflake::new(target), now, &mut events);
    (held, events.iter().map(describe).collect())
}

fn held(state: &State) -> Option<u64> {
    state.held_reply(general()).map(Snowflake::get)
}

// A jump that replaces GENERAL's window with messages far from TARGET.
fn jump_away(state: &mut State, placing: u64) {
    let mut events = Vec::new();
    let ticket = state
        .begin_load(
            general(),
            LoadKind::Around(Snowflake::new(FAR)),
            placing,
            &mut events,
        )
        .unwrap();
    let around = [message(FAR, GENERAL), message(FAR + 1, GENERAL)];
    state.finish_load(ticket, page(&around), false, &mut events);
}

// GENERAL viewed with TARGET loaded and held, then left behind by a jump.
fn holding_out_of_sight() -> State {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[original(GENERAL, "the original")],
    );
    assert!(hold(&mut state, TARGET).0);
    jump_away(&mut state, 2);
    assert!(state.message(general(), Snowflake::new(TARGET)).is_none());
    state
}

#[test]
fn holding_loads_the_target_and_names_it_for_the_channel() {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[original(GENERAL, "the original")],
    );
    assert_eq!(held(&state), None);

    let (taken, events) = hold(&mut state, TARGET);

    assert!(taken);
    assert_eq!(events, [changed(GENERAL, TARGET)]);
    assert_eq!(held(&state), Some(TARGET));
    assert_eq!(target(&state, GENERAL, TARGET), "the original");
}

#[test]
fn a_held_target_that_left_the_window_takes_the_reply() {
    for mention in [true, false] {
        let mut state = holding_out_of_sight();
        assert_eq!(target(&state, GENERAL, TARGET), "the original");

        let (queued, events) = queue_reply(&mut state, 9000, TARGET, mention);

        assert!(queued);
        assert_eq!(events, ["MessageInserted(9000)"], "the target is as it was");
        let outgoing = state.pending(general(), Snowflake::new(9000)).unwrap();
        assert_eq!(outgoing.mention, mention);
        let row = outgoing.message;
        assert_eq!(row.kind, MessageType::Reply);
        assert_eq!(row.replied_to(), Some((general(), Snowflake::new(TARGET))));
        let mentioned: Vec<u64> = row.mentions.iter().map(|user| user.id.get()).collect();
        assert_eq!(mentioned, if mention { vec![MIRA] } else { vec![] });
        assert_eq!(target(&state, GENERAL, TARGET), "the original");
        assert_eq!(held(&state), None, "the row names the target now");
    }
}

#[test]
fn a_target_that_left_the_window_unheld_is_refused() {
    let mut state = holding_out_of_sight();
    state.release_reply(general());

    let (queued, events) = queue_reply(&mut state, 9000, TARGET, true);

    assert!(!queued);
    assert!(events.is_empty());
}

#[test]
fn only_the_held_target_is_taken_from_outside_the_window() {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[
            original(GENERAL, "the original"),
            reply_to(20, GENERAL, 7, Some(message(7, GENERAL))),
        ],
    );
    assert!(hold(&mut state, TARGET).0);
    jump_away(&mut state, 2);
    assert_ne!(target(&state, GENERAL, 7), "not loaded");

    assert!(!queue_reply(&mut state, 9000, 7, true).0);
    assert_eq!(held(&state), Some(TARGET), "a refused reply ends no hold");
}

// Back at the present, far from TARGET, where live messages are rows again.
fn jump_to_present(state: &mut State, placing: u64) {
    let mut events = Vec::new();
    let ticket = state
        .begin_load(general(), LoadKind::Latest, placing, &mut events)
        .unwrap();
    state.finish_load(
        ticket,
        page(&[message(FAR + 5, GENERAL)]),
        false,
        &mut events,
    );
    assert!(state.messages(general()).unwrap().latest);
}

#[test]
fn a_held_target_stays_past_the_bound_until_it_is_released() {
    let mut state = holding_out_of_sight();
    jump_to_present(&mut state, 3);

    churn(&mut state);
    assert_eq!(state.windows.targets_len(general()), 100);
    assert_eq!(target(&state, GENERAL, TARGET), "the original");

    state.release_reply(general());
    assert_eq!(held(&state), None);
    apply(&mut state, "MESSAGE_CREATE", with_own_target(9500));
    assert_eq!(state.windows.targets_len(general()), 100);
    assert_eq!(target(&state, GENERAL, TARGET), "not loaded");
}

#[test]
fn a_held_target_outlives_its_evicted_window() {
    let mut state = holding_out_of_sight();
    for other in 1..=10_u64 {
        state.view_channel(Snowflake::new(900 + other), &mut Vec::new());
    }
    assert!(state.messages(general()).is_none(), "evicted");
    assert_eq!(target(&state, GENERAL, TARGET), "the original");
    assert_eq!(held(&state), Some(TARGET));

    state.view_channel(general(), &mut Vec::new());
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[message(FAR, GENERAL)],
    );

    assert!(queue_reply(&mut state, 9000, TARGET, true).0);
}

#[test]
fn releasing_without_a_window_leaves_no_target_behind() {
    let mut state = holding_out_of_sight();
    for other in 1..=10_u64 {
        state.view_channel(Snowflake::new(900 + other), &mut Vec::new());
    }
    assert_eq!(state.windows.targets_len(general()), 1);

    state.release_reply(general());

    assert_eq!(held(&state), None);
    assert_eq!(state.windows.targets_len(general()), 0);
}

#[test]
fn an_edit_reaches_a_held_target_outside_the_window() {
    let mut state = holding_out_of_sight();

    let events = apply(
        &mut state,
        "MESSAGE_UPDATE",
        json!({
            "id": TARGET.to_string(), "channel_id": GENERAL.to_string(),
            "content": "reworded", "edited_timestamp": "2026-01-01T00:00:00+00:00"
        }),
    );

    assert_eq!(events, [changed(GENERAL, TARGET)]);
    assert_eq!(target(&state, GENERAL, TARGET), "reworded (edited)");
    assert!(queue_reply(&mut state, 9000, TARGET, true).0);
}

#[test]
fn a_held_target_deleted_outside_the_window_is_deleted_and_takes_no_reply() {
    let single = json!({"id": TARGET.to_string(), "channel_id": GENERAL.to_string()});
    let bulk = json!({"ids": [TARGET.to_string()], "channel_id": GENERAL.to_string()});
    for (name, data) in [("MESSAGE_DELETE", single), ("MESSAGE_DELETE_BULK", bulk)] {
        let mut state = holding_out_of_sight();

        let events = apply(&mut state, name, data);

        assert_eq!(events, [changed(GENERAL, TARGET)], "{name}");
        assert_eq!(target(&state, GENERAL, TARGET), "deleted");
        let (queued, events) = queue_reply(&mut state, 9000, TARGET, true);
        assert!(!queued, "{name}");
        assert!(events.is_empty());
    }
}

#[test]
fn another_hold_replaces_the_channels_first() {
    let mut state = viewing(GENERAL);
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[original(GENERAL, "the original"), message(6, GENERAL)],
    );
    assert!(hold(&mut state, TARGET).0);

    assert!(hold(&mut state, 6).0);

    assert_eq!(held(&state), Some(6));
    jump_away(&mut state, 2);
    assert!(!queue_reply(&mut state, 9000, TARGET, true).0);
    assert!(queue_reply(&mut state, 9001, 6, true).0);
}

#[test]
fn a_message_that_takes_no_reply_isnt_held_and_ends_the_channels_hold() {
    let mut state = viewing(GENERAL);
    let mut pin = message(6, GENERAL);
    pin["type"] = 6.into();
    load(
        &mut state,
        GENERAL,
        LoadKind::Latest,
        &[original(GENERAL, "the original"), pin],
    );
    assert!(hold(&mut state, TARGET).0);

    for refused in [6, 4242] {
        let (taken, events) = hold(&mut state, refused);
        assert!(!taken);
        assert!(events.is_empty());
        assert_eq!(held(&state), None);
    }
}

#[test]
fn a_hold_is_its_channels_alone() {
    let mut state = holding_out_of_sight();
    let voice = Snowflake::new(VOICE);
    state.view_channel(voice, &mut Vec::new());

    state.release_reply(voice);
    assert_eq!(held(&state), Some(TARGET));
    assert_eq!(state.held_reply(voice), None);
}

#[test]
fn the_channels_deletion_ends_the_hold() {
    let mut state = holding_out_of_sight();

    apply(
        &mut state,
        "CHANNEL_DELETE",
        json!({"id": GENERAL.to_string(), "guild_id": G1.to_string(), "type": 0}),
    );

    assert_eq!(held(&state), None);
    assert_eq!(state.windows.targets_len(general()), 0);
}

#[test]
fn leaving_the_guild_ends_the_hold() {
    let mut state = holding_out_of_sight();

    apply(&mut state, "GUILD_DELETE", json!({"id": G1.to_string()}));

    assert_eq!(held(&state), None);
    assert_eq!(state.windows.targets_len(general()), 0);
}

#[test]
fn a_new_session_without_the_channel_ends_the_hold_and_one_with_it_keeps_it() {
    let mut state = holding_out_of_sight();

    apply(&mut state, "READY", ready_value());
    assert_eq!(held(&state), Some(TARGET));
    assert_eq!(target(&state, GENERAL, TARGET), "the original");

    let mut next = ready_value();
    next["guilds"][0]["channels"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    next["guilds"][0]["threads"] = json!([]);
    apply(&mut state, "READY", next);
    assert_eq!(held(&state), None);
    assert_eq!(state.windows.targets_len(general()), 0);
}

#[test]
fn releasing_every_hold_leaves_none() {
    let mut state = holding_out_of_sight();

    state.release_replies();

    assert_eq!(held(&state), None);
    assert!(!queue_reply(&mut state, 9000, TARGET, true).0);
}

#[test]
fn a_reply_to_the_users_own_message_never_pings() {
    let mut state = viewing(GENERAL);
    let mut own = original(GENERAL, "mine");
    own["author"]["id"] = ME.to_string().into();
    load(&mut state, GENERAL, LoadKind::Latest, &[own]);

    assert!(queue_reply(&mut state, 9000, TARGET, true).0);

    let outgoing = state.pending(general(), Snowflake::new(9000)).unwrap();
    assert!(!outgoing.mention, "the request says so too");
    assert!(outgoing.message.mentions.is_empty());
    assert!(!state.reply_mentions_author(&outgoing.message));
}
