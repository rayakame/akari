use std::collections::HashMap;

use serde_json::{Value, json};

use super::*;
use crate::gateway::MessageUpdate;
use crate::model::Snowflake;

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
    let second = harness.live(again);

    assert_eq!(first, ["Inserted(10)"]);
    assert_eq!(second, ["Inserted(10)"]);
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
