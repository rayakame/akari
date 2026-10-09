use std::collections::HashMap;

use serde_json::{Value, json};

use super::*;
use crate::gateway::MessageUpdate;
use crate::model::{Snowflake, Timestamp};
use crate::state::types::Delivery;

const CH: u64 = 1;
const LIMITS: WindowLimits = WindowLimits {
    channels: 2,
    messages: 3,
};

fn channel(id: u64) -> ChannelId {
    Snowflake::new(id)
}

fn author(id: u64) -> Value {
    json!({"id": id.to_string(), "username": format!("user{id}"), "avatar": null})
}

fn wire_by(id: u64, author: Value) -> model::Message {
    let message = json!({
        "id": id.to_string(),
        "channel_id": CH.to_string(),
        "type": 0,
        "content": format!("message {id}"),
        "timestamp": "2026-10-08T12:00:00+00:00",
        "author": author,
    });
    serde_json::from_str(&message.to_string()).unwrap()
}

fn wire(id: u64) -> model::Message {
    wire_by(id, author(7))
}

fn stored(id: u64) -> Arc<Message> {
    Arc::new(Message::from_wire(wire(id), &mut |user| {
        Arc::new(User::from_wire(user))
    }))
}

fn edited(id: u64) -> Arc<Message> {
    let mut message = (*stored(id)).clone();
    message.content = format!("edited {id}").into();
    Arc::new(message)
}

fn update(id: u64, content: &str) -> MessageUpdate {
    serde_json::from_str(
        &json!({"id": id.to_string(), "channel_id": CH.to_string(), "content": content})
            .to_string(),
    )
    .unwrap()
}

fn no_users() -> HashMap<UserId, Arc<User>> {
    HashMap::new()
}

fn describe(events: &[StoreEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| match event {
            StoreEvent::MessageInserted(message) => format!("Inserted({})", message.id.get()),
            StoreEvent::MessageUpdated(message) => format!("Updated({})", message.id.get()),
            StoreEvent::MessageDeleted { message_id, .. } => {
                format!("Deleted({})", message_id.get())
            }
            StoreEvent::MessagesLoaded { first, last, .. } => {
                format!("Loaded({}..{})", first.get(), last.get())
            }
            StoreEvent::MessagesTrimmed { first, last, .. } => {
                format!("Trimmed({}..{})", first.get(), last.get())
            }
            StoreEvent::MessagesStale { channel_id } => format!("Stale({})", channel_id.get()),
            StoreEvent::MessagesCleared { channel_id } => format!("Cleared({})", channel_id.get()),
            StoreEvent::MessageReplaced {
                pending_id,
                message,
                ..
            } => format!("Replaced({} -> {})", pending_id.get(), message.id.get()),
            other => format!("{other:?}"),
        })
        .collect()
}

struct Harness {
    windows: Windows,
    users: HashMap<UserId, Arc<User>>,
}

impl Harness {
    fn new(limits: WindowLimits) -> Self {
        Self {
            windows: Windows::new(limits),
            users: no_users(),
        }
    }

    fn viewing(limits: WindowLimits) -> Self {
        let mut harness = Self::new(limits);
        harness.view(CH);
        harness
    }

    fn view(&mut self, id: u64) -> Vec<String> {
        let mut events = Vec::new();
        self.windows.view(channel(id), &mut events);
        describe(&events)
    }

    fn live(&mut self, message: model::Message) -> Vec<String> {
        let mut events = Vec::new();
        self.windows.live(message, &self.users, &mut events);
        describe(&events)
    }

    fn batch(&mut self, ids: &[Arc<Message>], end: End, reached_end: bool) -> Vec<String> {
        let mut events = Vec::new();
        self.windows
            .insert_batch(channel(CH), ids.to_vec(), end, reached_end, &mut events);
        describe(&events)
    }

    fn update(&mut self, update: MessageUpdate) -> Vec<String> {
        let mut events = Vec::new();
        self.windows.update(update, &self.users, &mut events);
        describe(&events)
    }

    fn delete(&mut self, id: u64) -> Vec<String> {
        let mut events = Vec::new();
        self.windows
            .delete(channel(CH), Snowflake::new(id), &mut events);
        describe(&events)
    }

    fn window(&self) -> MessageWindow {
        self.windows.snapshot(channel(CH)).unwrap()
    }

    fn ids(&self) -> Vec<u64> {
        self.window()
            .messages
            .iter()
            .map(|message| message.id.get())
            .collect()
    }
}

#[test]
fn messages_for_unviewed_channels_are_not_stored() {
    let mut harness = Harness::new(LIMITS);

    assert!(harness.live(wire(10)).is_empty());
    assert!(harness.windows.snapshot(channel(CH)).is_none());
}

#[test]
fn viewing_opens_an_empty_live_window() {
    let mut harness = Harness::new(LIMITS);

    assert!(harness.view(CH).is_empty());

    let window = harness.window();
    assert!(window.messages.is_empty());
    assert!(window.latest && !window.oldest && !window.stale);
}

#[test]
fn live_messages_are_appended_in_id_order() {
    let mut harness = Harness::viewing(LIMITS);

    let events: Vec<_> = [10, 12, 11]
        .into_iter()
        .flat_map(|id| harness.live(wire(id)))
        .collect();

    assert_eq!(events, ["Inserted(10)", "Inserted(12)", "Inserted(11)"]);
    assert_eq!(harness.ids(), [10, 11, 12]);
}

#[test]
fn a_repeated_message_create_replaces_the_message() {
    let mut harness = Harness::viewing(LIMITS);
    let mut again = wire(10);
    again.content = "again".to_owned();

    let first = harness.live(wire(10));
    let changed = harness.live(again.clone());
    let identical = harness.live(again);

    assert_eq!(first, ["Inserted(10)"]);
    assert_eq!(changed, ["Updated(10)"]);
    assert!(identical.is_empty(), "{identical:?}");
    assert_eq!(harness.ids(), [10]);
    assert_eq!(&*harness.window().messages[0].content, "again");
}

#[test]
fn a_live_append_past_the_limit_trims_the_oldest() {
    let mut harness = Harness::viewing(LIMITS);
    for id in 10..13 {
        harness.live(wire(id));
    }

    let events = harness.live(wire(13));

    assert_eq!(events, ["Inserted(13)", "Trimmed(11..13)"]);
    assert_eq!(harness.ids(), [11, 12, 13]);
    let window = harness.window();
    assert!(window.latest && !window.oldest);
}

#[test]
fn an_older_batch_past_the_limit_trims_the_newest_and_detaches() {
    let mut harness = Harness::viewing(LIMITS);
    for id in 10..13 {
        harness.live(wire(id));
    }

    let events = harness.batch(&[stored(7), stored(8)], End::Older, false);
    let late = harness.live(wire(13));

    assert_eq!(events, ["Loaded(7..8)", "Trimmed(7..10)"]);
    assert_eq!(harness.ids(), [7, 8, 10]);
    assert!(!harness.window().latest);
    assert!(late.is_empty(), "{late:?}");
    assert_eq!(harness.ids(), [7, 8, 10]);
}

#[test]
fn a_newer_batch_past_the_limit_trims_the_oldest() {
    let mut harness = Harness::viewing(LIMITS);
    for id in 10..13 {
        harness.live(wire(id));
    }
    harness.batch(&[stored(7), stored(8)], End::Older, false);

    let events = harness.batch(&[stored(11), stored(12)], End::Newer, true);
    let live = harness.live(wire(13));

    assert_eq!(events, ["Loaded(11..12)", "Trimmed(10..12)"]);
    let window = harness.window();
    assert!(window.latest && !window.oldest);
    assert_eq!(live, ["Inserted(13)", "Trimmed(11..13)"]);
    assert_eq!(harness.ids(), [11, 12, 13]);
}

#[test]
fn a_batch_that_reaches_the_start_sets_oldest() {
    let mut harness = Harness::viewing(LIMITS);

    harness.batch(&[stored(1), stored(2)], End::Older, true);

    let window = harness.window();
    assert!(window.oldest && window.latest);
}

#[test]
fn a_batch_overlapping_the_window_updates_changed_messages_only() {
    let mut harness = Harness::viewing(WindowLimits {
        channels: 2,
        messages: 5,
    });
    harness.live(wire(10));
    harness.live(wire(11));

    let events = harness.batch(
        &[stored(8), stored(9), edited(10), stored(11)],
        End::Older,
        false,
    );

    assert_eq!(events, ["Updated(10)", "Loaded(8..9)"]);
    assert_eq!(harness.ids(), [8, 9, 10, 11]);
    assert_eq!(&*harness.window().messages[2].content, "edited 10");
}

#[test]
fn a_batch_into_an_empty_window_becomes_the_window() {
    let mut harness = Harness::viewing(LIMITS);

    let events = harness.batch(&[stored(5), stored(6), stored(7)], End::Older, false);

    assert_eq!(events, ["Loaded(5..7)"]);
    assert_eq!(harness.ids(), [5, 6, 7]);
    assert!(harness.window().latest);
}

#[test]
fn viewing_past_the_channel_limit_clears_the_least_recently_viewed() {
    let mut harness = Harness::new(LIMITS);
    harness.view(1);
    harness.view(2);

    let events = harness.view(3);

    assert_eq!(events, ["Cleared(1)"]);
    assert!(harness.windows.snapshot(channel(1)).is_none());
    assert!(harness.windows.snapshot(channel(2)).is_some());
}

#[test]
fn viewing_again_moves_a_channel_to_the_front() {
    let mut harness = Harness::new(LIMITS);
    harness.view(1);
    harness.view(2);
    harness.view(1);

    let events = harness.view(3);

    assert_eq!(events, ["Cleared(2)"]);
}

#[test]
fn message_update_patches_only_stored_messages() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));

    let stored = harness.update(update(10, "fixed"));
    let unknown = harness.update(update(99, "fixed"));
    let mut elsewhere = update(10, "fixed");
    elsewhere.channel_id = channel(2);
    let unviewed = harness.update(elsewhere);

    assert_eq!(stored, ["Updated(10)"]);
    assert_eq!(&*harness.window().messages[0].content, "fixed");
    assert!(unknown.is_empty() && unviewed.is_empty());
}

#[test]
fn an_update_that_changes_nothing_emits_nothing() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));

    assert!(harness.update(update(10, "message 10")).is_empty());
}

#[test]
fn message_delete_removes_stored_messages() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));
    harness.live(wire(11));

    let deleted = harness.delete(10);
    let unknown = harness.delete(99);

    assert_eq!(deleted, ["Deleted(10)"]);
    assert!(unknown.is_empty());
    assert_eq!(harness.ids(), [11]);
}

#[test]
fn a_stale_window_holds_live_messages_back() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));
    let mut events = Vec::new();
    harness.windows.mark_stale(&mut events);

    let held: Vec<_> = (11..15).flat_map(|id| harness.live(wire(id))).collect();
    let held_update = harness.update(update(13, "edited while stale"));
    let held_delete = harness.delete(12);
    let visible_update = harness.update(update(10, "edited"));

    assert_eq!(describe(&events), ["Stale(1)"]);
    assert!(held.is_empty() && held_update.is_empty() && held_delete.is_empty());
    assert_eq!(harness.ids(), [10]);
    assert!(harness.window().stale);
    let still_held = harness.windows.held(channel(CH));
    assert_eq!(
        still_held
            .iter()
            .map(|message| message.id.get())
            .collect::<Vec<_>>(),
        [13, 14]
    );
    assert_eq!(&*still_held[0].content, "edited while stale");
    assert_eq!(visible_update, ["Updated(10)"]);
}

#[test]
fn a_dropped_channel_loses_its_window_silently() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));

    harness.windows.drop_channel(channel(CH));

    assert!(harness.windows.snapshot(channel(CH)).is_none());
    assert!(harness.view(2).is_empty());
    assert!(harness.view(3).is_empty());
}

#[test]
fn equal_authors_share_one_allocation() {
    let mut harness = Harness::viewing(WindowLimits {
        channels: 2,
        messages: 10,
    });
    let recipient = Arc::new(User::from_wire(serde_json::from_value(author(8)).unwrap()));
    harness.users.insert(recipient.id, recipient.clone());
    let mut new_avatar = author(7);
    new_avatar["avatar"] = "0123456789abcdef0123456789abcdef".into();

    harness.live(wire(10));
    harness.live(wire(11));
    harness.live(wire_by(12, new_avatar));
    harness.live(wire_by(13, author(8)));

    let messages = harness.window().messages;
    assert!(Arc::ptr_eq(&messages[0].author, &messages[1].author));
    assert!(!Arc::ptr_eq(&messages[1].author, &messages[2].author));
    assert!(Arc::ptr_eq(&messages[3].author, &recipient));
}

#[test]
fn a_stale_window_updates_a_visible_message_instead_of_holding_it() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));
    harness.windows.mark_stale(&mut Vec::new());
    let mut again = wire(10);
    again.content = "again".to_owned();

    let events = harness.live(again);

    assert_eq!(events, ["Updated(10)"]);
    assert_eq!(&*harness.window().messages[0].content, "again");
    assert!(harness.windows.held(channel(CH)).is_empty());
}

const LOADS: WindowLimits = WindowLimits {
    channels: 2,
    messages: 5,
};

impl Harness {
    fn begin(&mut self, kind: LoadKind) -> Option<LoadTicket> {
        let mut events = Vec::new();
        let ticket = self.windows.begin_load(channel(CH), kind, &mut events);
        assert!(describe(&events).is_empty(), "{:?}", describe(&events));
        ticket
    }

    fn finish(&mut self, ticket: LoadTicket, ids: &[u64], limit: usize) -> Vec<String> {
        let mut events = Vec::new();
        let page = ids.iter().map(|id| wire(*id)).collect();
        self.windows
            .finish_load(ticket, page, &self.users, ids.len() < limit, &mut events);
        describe(&events)
    }

    fn edited_finish(
        &mut self,
        ticket: LoadTicket,
        page: Vec<model::Message>,
        limit: usize,
    ) -> Vec<String> {
        let mut events = Vec::new();
        let reached_end = page.len() < limit;
        self.windows
            .finish_load(ticket, page, &self.users, reached_end, &mut events);
        describe(&events)
    }

    fn detached(limits: WindowLimits, ids: &[u64]) -> Self {
        let mut harness = Self::viewing(limits);
        let around = ids[ids.len() / 2];
        let ticket = harness
            .begin(LoadKind::Around(Snowflake::new(around)))
            .unwrap();
        harness.finish(ticket, ids, 100);
        assert!(!harness.window().latest);
        harness
    }
}

fn edited_wire(id: u64) -> model::Message {
    let mut message = wire(id);
    message.content = format!("edited {id}");
    message
}

#[test]
fn older_and_newer_continue_from_the_ends() {
    let mut live = Harness::viewing(LOADS);
    live.live(wire(10));
    live.live(wire(11));
    let older = live.begin(LoadKind::Older).unwrap();
    let mut detached = Harness::detached(LOADS, &[19, 20, 21]);
    let newer = detached.begin(LoadKind::Newer).unwrap();

    assert_eq!(older.cursor, Cursor::Before(Snowflake::new(10)));
    assert_eq!(live.finish(older, &[7, 8, 9], 3), ["Loaded(7..9)"]);
    assert_eq!(live.ids(), [7, 8, 9, 10, 11]);
    assert_eq!(newer.cursor, Cursor::After(Snowflake::new(21)));
    assert_eq!(detached.finish(newer, &[22, 23], 3), ["Loaded(22..23)"]);
    assert!(detached.window().latest);
}

#[test]
fn older_on_an_empty_window_loads_the_latest() {
    let mut harness = Harness::viewing(LOADS);

    let ticket = harness.begin(LoadKind::Older).unwrap();

    assert_eq!(ticket.kind, LoadKind::Latest);
    assert_eq!(ticket.cursor, Cursor::Latest);
}

#[test]
fn latest_on_a_live_window_merges() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.live(wire(11));

    let ticket = harness.begin(LoadKind::Latest).unwrap();
    let events = harness.finish(ticket, &[9, 10, 11], 3);

    assert_eq!(events, ["Loaded(9..9)"]);
    assert_eq!(harness.ids(), [9, 10, 11]);
    assert!(harness.window().latest);
}

#[test]
fn latest_on_a_detached_window_jumps_to_the_present() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);

    let ticket = harness.begin(LoadKind::Latest).unwrap();
    let held = harness.live(wire(30));
    let events = harness.finish(ticket, &[28, 29], 5);

    assert!(held.is_empty(), "{held:?}");
    assert_eq!(events, ["Cleared(1)", "Loaded(28..30)"]);
    assert_eq!(harness.ids(), [28, 29, 30]);
    let window = harness.window();
    assert!(window.latest && window.oldest && !window.stale);
}

#[test]
fn refresh_reconciles_a_stale_window() {
    let mut harness = Harness::viewing(LOADS);
    for id in 10..13 {
        harness.live(wire(id));
    }
    harness.windows.mark_stale(&mut Vec::new());
    harness.live(wire(14));

    let ticket = harness.begin(LoadKind::Refresh).unwrap();
    let events = harness.edited_finish(ticket, vec![edited_wire(11), wire(13)], 5);

    assert_eq!(
        events,
        [
            "Deleted(12)",
            "Updated(11)",
            "Loaded(13..13)",
            "Loaded(14..14)"
        ]
    );
    assert_eq!(harness.ids(), [10, 11, 13, 14]);
    let window = harness.window();
    assert!(!window.stale && window.latest);
    assert!(harness.windows.held(channel(CH)).is_empty());
}

#[test]
fn reconcile_touches_only_the_pages_range() {
    let mut harness = Harness::viewing(LOADS);
    for id in 5..10 {
        harness.live(wire(id));
    }
    harness.windows.mark_stale(&mut Vec::new());

    let ticket = harness.begin(LoadKind::Refresh).unwrap();
    let events = harness.finish(ticket, &[8, 10], 5);

    assert_eq!(events, ["Deleted(9)", "Loaded(10..10)"]);
    assert_eq!(harness.ids(), [5, 6, 7, 8, 10]);
}

#[test]
fn refresh_without_overlap_keeps_the_position() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.live(wire(11));
    harness.windows.mark_stale(&mut Vec::new());
    harness.live(wire(20));

    let ticket = harness.begin(LoadKind::Refresh).unwrap();
    let events = harness.finish(ticket, &[30, 31, 32], 3);

    assert_eq!(events, ["Stale(1)"]);
    assert_eq!(harness.ids(), [10, 11]);
    let window = harness.window();
    assert!(window.stale && !window.latest);
    assert!(harness.windows.held(channel(CH)).is_empty());
    assert!(harness.live(wire(33)).is_empty());
    assert!(harness.windows.held(channel(CH)).is_empty());
}

#[test]
fn held_messages_join_after_a_catch_up() {
    let mut harness = Harness::detached(
        WindowLimits {
            channels: 2,
            messages: 10,
        },
        &[19, 20, 21],
    );

    let ticket = harness.begin(LoadKind::Newer).unwrap();
    let held = harness.live(wire(30));
    let events = harness.finish(ticket, &[22, 23], 5);

    assert!(held.is_empty(), "{held:?}");
    assert_eq!(events, ["Loaded(22..23)", "Loaded(30..30)"]);
    assert_eq!(harness.ids(), [19, 20, 21, 22, 23, 30]);
    assert!(harness.window().latest);
    assert_eq!(harness.live(wire(31)), ["Inserted(31)"]);
}

#[test]
fn a_catch_up_that_doesnt_reach_the_present_drops_held_messages() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);

    let ticket = harness.begin(LoadKind::Newer).unwrap();
    harness.live(wire(30));
    harness.finish(ticket, &[22, 23], 2);

    assert!(!harness.window().latest);
    assert!(harness.windows.held(channel(CH)).is_empty());
    assert!(harness.live(wire(31)).is_empty());
}

#[test]
fn a_load_for_a_replaced_window_is_dropped() {
    let mut evicted = Harness::viewing(LOADS);
    evicted.live(wire(10));
    let stale_ticket = evicted.begin(LoadKind::Older).unwrap();
    evicted.view(2);
    evicted.view(3);
    evicted.view(CH);

    let mut jumped = Harness::viewing(LOADS);
    jumped.live(wire(10));
    let old_ticket = jumped.begin(LoadKind::Older).unwrap();
    let around = jumped.begin(LoadKind::Around(Snowflake::new(50))).unwrap();
    jumped.finish(around, &[49, 50, 51], 3);

    let mut deleted = Harness::viewing(LOADS);
    deleted.live(wire(10));
    let gone_ticket = deleted.begin(LoadKind::Older).unwrap();
    deleted.windows.drop_channel(channel(CH));

    assert!(evicted.finish(stale_ticket, &[7, 8, 9], 3).is_empty());
    assert!(evicted.ids().is_empty());
    assert!(jumped.finish(old_ticket, &[7, 8, 9], 3).is_empty());
    assert_eq!(jumped.ids(), [49, 50, 51]);
    assert!(deleted.finish(gone_ticket, &[7, 8, 9], 3).is_empty());
    assert!(deleted.windows.snapshot(channel(CH)).is_none());
}

#[test]
fn around_replaces_the_window() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.live(wire(11));

    let ticket = harness.begin(LoadKind::Around(Snowflake::new(5))).unwrap();
    let events = harness.finish(ticket, &[4, 5, 6], 3);

    assert_eq!(ticket.cursor, Cursor::Around(Snowflake::new(5)));
    assert_eq!(events, ["Cleared(1)", "Loaded(4..6)"]);
    assert_eq!(harness.ids(), [4, 5, 6]);
    assert!(!harness.window().latest);
}

#[test]
fn aborting_a_load_stops_holding() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);

    let ticket = harness.begin(LoadKind::Newer).unwrap();
    harness.windows.abort_load(ticket, &mut Vec::new());
    harness.live(wire(30));

    assert!(harness.windows.held(channel(CH)).is_empty());
}

#[test]
fn loads_with_nothing_to_do_are_skipped() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));

    assert!(harness.begin(LoadKind::Newer).is_none());
    assert!(harness.begin(LoadKind::Refresh).is_none());
    let mut unviewed = Harness::new(LOADS);
    assert!(unviewed.begin(LoadKind::Older).is_none());
}

fn pending(id: u64, content: &str) -> Arc<Message> {
    let mut message = (*stored(id)).clone();
    message.content = content.into();
    message.delivery = Delivery::Pending;
    Arc::new(message)
}

fn echo(id: u64, nonce: u64) -> model::Message {
    let mut message = wire(id);
    message.nonce = Some(model::Nonce::Text(nonce.to_string()));
    message
}

impl Harness {
    fn queue(&mut self, message: Arc<Message>) -> Vec<String> {
        let mut events = Vec::new();
        self.windows.queue(channel(CH), message, &mut events);
        describe(&events)
    }

    fn confirm(&mut self, pending_id: u64, message: model::Message) -> Vec<String> {
        let mut events = Vec::new();
        self.windows.confirm(
            channel(CH),
            Snowflake::new(pending_id),
            message,
            &self.users,
            &mut events,
        );
        describe(&events)
    }

    fn outbox(&self) -> Vec<(u64, Delivery)> {
        self.window()
            .pending
            .iter()
            .map(|message| (message.id.get(), message.delivery))
            .collect()
    }
}

#[test]
fn queued_messages_wait_in_the_outbox() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));

    let events = harness.queue(pending(500, "hi"));

    assert_eq!(events, ["Inserted(500)"]);
    assert_eq!(harness.ids(), [10]);
    assert_eq!(harness.outbox(), [(500, Delivery::Pending)]);
}

#[test]
fn an_echo_replaces_its_pending_message() {
    let mut harness = Harness::viewing(LIMITS);
    harness.queue(pending(500, "hi"));

    let events = harness.live(echo(20, 500));
    let response = harness.confirm(500, echo(20, 500));

    assert_eq!(events, ["Replaced(500 -> 20)"]);
    assert!(response.is_empty(), "{response:?}");
    assert_eq!(harness.ids(), [20]);
    assert!(harness.outbox().is_empty());
}

#[test]
fn a_response_replaces_its_pending_message_and_the_echo_changes_nothing() {
    let mut harness = Harness::viewing(LIMITS);
    harness.queue(pending(500, "hi"));

    let response = harness.confirm(500, echo(20, 500));
    let late_echo = harness.live(echo(20, 500));

    assert_eq!(response, ["Replaced(500 -> 20)"]);
    assert!(late_echo.is_empty(), "{late_echo:?}");
    assert_eq!(harness.ids(), [20]);
}

#[test]
fn a_failed_message_can_be_retried_or_discarded() {
    let mut harness = Harness::viewing(LIMITS);
    harness.queue(pending(500, "hi"));
    harness.queue(pending(501, "again"));
    let mut events = Vec::new();

    harness
        .windows
        .fail(channel(CH), Snowflake::new(500), &mut events);
    harness
        .windows
        .fail(channel(CH), Snowflake::new(501), &mut events);
    let retried = harness
        .windows
        .retry(channel(CH), Snowflake::new(500), &mut events);
    let discarded = harness
        .windows
        .discard(channel(CH), Snowflake::new(501), &mut events);

    assert_eq!(
        describe(&events),
        [
            "Updated(500)",
            "Updated(501)",
            "Updated(500)",
            "Deleted(501)"
        ]
    );
    assert_eq!(
        retried.map(|message| message.delivery),
        Some(Delivery::Pending)
    );
    assert!(discarded);
    assert_eq!(harness.outbox(), [(500, Delivery::Pending)]);
    let mut none = Vec::new();
    assert!(
        harness
            .windows
            .retry(channel(CH), Snowflake::new(500), &mut none)
            .is_none()
    );
    assert!(
        !harness
            .windows
            .discard(channel(CH), Snowflake::new(500), &mut none)
    );
}

#[test]
fn an_echo_for_a_failed_message_still_replaces_it() {
    let mut harness = Harness::viewing(LIMITS);
    harness.queue(pending(500, "hi"));
    harness
        .windows
        .fail(channel(CH), Snowflake::new(500), &mut Vec::new());

    let events = harness.live(echo(20, 500));

    assert_eq!(events, ["Replaced(500 -> 20)"]);
    assert!(harness.outbox().is_empty());
}

#[test]
fn trimming_never_drops_pending_messages() {
    let mut harness = Harness::viewing(LIMITS);
    for id in 10..13 {
        harness.live(wire(id));
    }
    harness.queue(pending(500, "hi"));

    harness.batch(&[stored(7), stored(8)], End::Older, false);
    harness.live(wire(13));

    assert!(!harness.window().latest);
    assert_eq!(harness.outbox(), [(500, Delivery::Pending)]);
}

#[test]
fn a_detached_window_drops_the_confirmed_message_but_reports_the_replacement() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);
    harness.queue(pending(500, "hi"));

    let events = harness.live(echo(40, 500));

    assert_eq!(events, ["Replaced(500 -> 40)"]);
    assert_eq!(harness.ids(), [19, 20, 21]);
    assert!(harness.outbox().is_empty());
}

#[test]
fn reconcile_keeps_the_outbox() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.queue(pending(500, "hi"));
    harness.windows.mark_stale(&mut Vec::new());

    let ticket = harness.begin(LoadKind::Refresh).unwrap();
    harness.finish(ticket, &[11], 5);

    assert_eq!(harness.outbox(), [(500, Delivery::Pending)]);
}

#[test]
fn a_jump_keeps_the_outbox() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);
    harness.queue(pending(500, "hi"));

    let ticket = harness.begin(LoadKind::Latest).unwrap();
    harness.finish(ticket, &[30, 31], 5);

    assert_eq!(harness.outbox(), [(500, Delivery::Pending)]);
}

#[test]
fn a_window_with_pending_messages_isnt_evicted() {
    let mut harness = Harness::viewing(LIMITS);
    harness.queue(pending(500, "hi"));

    harness.view(2);
    let events = harness.view(3);

    assert_eq!(events, ["Cleared(2)"]);
    assert_eq!(harness.outbox(), [(500, Delivery::Pending)]);
}

#[test]
fn an_older_page_whose_end_was_trimmed_meanwhile_is_dropped() {
    let mut harness = Harness::viewing(LOADS);
    for id in 10..15 {
        harness.live(wire(id));
    }
    let ticket = harness.begin(LoadKind::Older).unwrap();
    harness.live(wire(15));

    let events = harness.finish(ticket, &[7, 8, 9], 3);

    assert!(events.is_empty(), "{events:?}");
    assert_eq!(harness.ids(), [11, 12, 13, 14, 15]);
}

#[test]
fn a_newer_page_whose_end_was_trimmed_meanwhile_is_dropped() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);
    let newer = harness.begin(LoadKind::Newer).unwrap();
    let older = harness.begin(LoadKind::Older).unwrap();
    harness.finish(older, &[16, 17, 18], 3);

    let events = harness.finish(newer, &[22, 23], 3);
    harness.live(wire(30));

    assert!(events.is_empty(), "{events:?}");
    assert_eq!(harness.ids(), [16, 17, 18, 19, 20]);
    assert!(!harness.window().latest);
    assert!(harness.windows.held(channel(CH)).is_empty());
}

#[test]
fn a_refresh_that_finishes_after_the_window_was_refreshed_changes_nothing() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.live(wire(11));
    harness.windows.mark_stale(&mut Vec::new());
    let refresh = harness.begin(LoadKind::Refresh).unwrap();
    let latest = harness.begin(LoadKind::Latest).unwrap();
    harness.finish(latest, &[10, 11, 12], 3);

    let events = harness.finish(refresh, &[10, 11, 12], 3);

    assert!(events.is_empty(), "{events:?}");
    let window = harness.window();
    assert!(window.latest && !window.stale);
    assert_eq!(harness.live(wire(13)), ["Inserted(13)"]);
}

#[test]
fn a_newer_load_that_reaches_the_present_ends_staleness() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);
    harness.windows.mark_stale(&mut Vec::new());
    let ticket = harness.begin(LoadKind::Newer).unwrap();

    harness.finish(ticket, &[22], 3);

    let window = harness.window();
    assert!(window.latest && !window.stale);
    assert_eq!(harness.live(wire(30)), ["Inserted(30)"]);
}

#[test]
fn a_stale_window_at_the_present_waits_for_its_refresh_instead_of_newer() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.windows.mark_stale(&mut Vec::new());

    assert!(harness.begin(LoadKind::Newer).is_none());
}

#[test]
fn a_failed_refresh_detaches_the_window() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.windows.mark_stale(&mut Vec::new());
    let ticket = harness.begin(LoadKind::Refresh).unwrap();
    harness.live(wire(11));

    harness.windows.abort_load(ticket, &mut Vec::new());

    let window = harness.window();
    assert!(window.stale && !window.latest);
    assert!(harness.windows.held(channel(CH)).is_empty());
    assert!(harness.windows.stale_channels().is_empty());
}

#[test]
fn refreshing_an_empty_stale_window_fills_it() {
    let mut harness = Harness::viewing(LOADS);
    harness.windows.mark_stale(&mut Vec::new());
    let ticket = harness.begin(LoadKind::Refresh).unwrap();

    let events = harness.finish(ticket, &[10, 11], 3);

    assert_eq!(events, ["Loaded(10..11)"]);
    let window = harness.window();
    assert!(window.latest && !window.stale && window.oldest);
}

#[test]
fn an_empty_refresh_page_keeps_the_window_live() {
    let mut harness = Harness::viewing(LOADS);
    harness.live(wire(10));
    harness.windows.mark_stale(&mut Vec::new());
    let ticket = harness.begin(LoadKind::Refresh).unwrap();

    harness.finish(ticket, &[], 3);

    let window = harness.window();
    assert!(window.latest && !window.stale);
    assert_eq!(harness.live(wire(12)), ["Inserted(12)"]);
}

#[test]
fn a_failed_load_leaves_held_messages_to_the_one_still_running() {
    let mut harness = Harness::detached(LOADS, &[19, 20, 21]);
    let newer = harness.begin(LoadKind::Newer).unwrap();
    let latest = harness.begin(LoadKind::Latest).unwrap();

    harness.windows.abort_load(newer, &mut Vec::new());
    harness.live(wire(40));
    harness.finish(latest, &[30, 31], 3);

    assert_eq!(harness.ids(), [30, 31, 40]);
    assert!(harness.window().latest);
}

impl Harness {
    fn content(&self, id: u64) -> String {
        self.windows
            .message(channel(CH), Snowflake::new(id))
            .unwrap()
            .content
            .to_string()
    }
}

fn edited_at(id: u64, content: &str, unix_millis: i64) -> MessageUpdate {
    let mut update = update(id, content);
    update.edited_timestamp = Some(Some(Timestamp::from_unix_millis(unix_millis)));
    update
}

#[test]
fn a_delete_during_a_load_stays_deleted() {
    let mut harness = Harness::viewing(LOADS);
    let ticket = harness.begin(LoadKind::Latest).unwrap();
    harness.delete(12);

    let events = harness.finish(ticket, &[10, 11, 12], 3);

    assert_eq!(events, ["Loaded(10..11)"]);
    assert_eq!(harness.ids(), [10, 11]);
}

#[test]
fn a_delete_during_a_refresh_isnt_brought_back() {
    let mut harness = Harness::viewing(LOADS);
    for id in [10, 11, 12] {
        harness.live(wire(id));
    }
    harness.windows.mark_stale(&mut Vec::new());
    let ticket = harness.begin(LoadKind::Refresh).unwrap();
    assert_eq!(harness.delete(12), ["Deleted(12)"]);

    let events = harness.finish(ticket, &[10, 11, 12], 3);

    assert!(events.is_empty(), "{events:?}");
    assert_eq!(harness.ids(), [10, 11]);
}

#[test]
fn an_edit_during_a_load_is_applied_to_the_page() {
    let mut harness = Harness::viewing(LOADS);
    let ticket = harness.begin(LoadKind::Latest).unwrap();
    harness.update(update(11, "edited while loading"));

    harness.finish(ticket, &[10, 11, 12], 3);

    assert_eq!(harness.content(11), "edited while loading");
}

#[test]
fn an_edit_older_than_the_page_doesnt_revert_it() {
    let mut harness = Harness::viewing(LOADS);
    let ticket = harness.begin(LoadKind::Latest).unwrap();
    harness.update(edited_at(11, "first edit", 1_700_000_000_000));
    let mut newer = wire(11);
    newer.content = "second edit".to_owned();
    newer.edited_timestamp = Some(Timestamp::from_unix_millis(1_700_000_060_000));

    harness.edited_finish(ticket, vec![wire(10), newer], 3);

    assert_eq!(harness.content(11), "second edit");
}

#[test]
fn edits_during_one_load_arent_replayed_onto_the_next() {
    let mut harness = Harness::viewing(LOADS);
    let first = harness.begin(LoadKind::Latest).unwrap();
    harness.delete(12);
    harness.finish(first, &[10, 11], 3);

    let older = harness.begin(LoadKind::Older).unwrap();
    harness.finish(older, &[7, 8, 9], 3);
    let ticket = harness.begin(LoadKind::Around(Snowflake::new(12))).unwrap();
    harness.finish(ticket, &[11, 12, 13], 3);

    assert_eq!(harness.ids(), [11, 12, 13]);
}

#[test]
fn another_users_message_with_our_nonce_doesnt_replace_ours() {
    let mut harness = Harness::viewing(LIMITS);
    harness.queue(pending(500, "hi"));
    let mut theirs = wire_by(20, author(8));
    theirs.nonce = Some(model::Nonce::Text("500".to_owned()));

    let events = harness.live(theirs);

    assert_eq!(events, ["Inserted(20)"]);
    assert_eq!(harness.outbox(), [(500, Delivery::Pending)]);
}

#[test]
fn a_dropped_channel_keeps_its_unsent_messages() {
    let mut harness = Harness::viewing(LIMITS);
    harness.live(wire(10));
    harness.queue(pending(500, "failed"));
    harness
        .windows
        .fail(channel(CH), Snowflake::new(500), &mut Vec::new());
    harness.queue(pending(501, "in flight"));

    harness.windows.drop_channel(channel(CH));
    let confirmed = harness.confirm(501, echo(21, 501));

    let window = harness.window();
    assert!(window.messages.is_empty() && !window.latest);
    assert_eq!(confirmed, ["Replaced(501 -> 21)"]);
    assert_eq!(harness.outbox(), [(500, Delivery::Failed)]);
    assert!(
        harness
            .windows
            .retry(channel(CH), Snowflake::new(500), &mut Vec::new())
            .is_some()
    );
}
