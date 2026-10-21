use std::collections::HashMap;
use std::sync::Arc;

use super::State;
use crate::gateway::MessageUpdate;
use crate::model::{self, ChannelId, MessageId, MessageType, UserId};

// The official client's count stops here and shows "100k+".
const MAX_COUNT: u32 = 100_000;
use crate::state::events::StoreEvent;
use crate::state::types::{Message, User};

impl State {
    pub(crate) fn latest_message(&self, thread: ChannelId) -> Option<Arc<Message>> {
        self.latest.get(&thread).cloned()
    }

    pub(super) fn note_live_latest(&mut self, message: &model::Message) -> bool {
        Self::counts(message) && self.note_latest(message)
    }

    // Only newer: a sync can bring an older message than a live one that came first.
    pub(super) fn note_latest(&mut self, message: &model::Message) -> bool {
        if message.kind == MessageType::ThreadStarterMessage {
            return false;
        }
        let thread = message.channel_id;
        let is_thread = self
            .entities
            .channels
            .get(&thread)
            .is_some_and(|stored| stored.is_thread());
        if !is_thread
            || self
                .latest
                .get(&thread)
                .is_some_and(|known| known.id >= message.id)
        {
            return false;
        }
        let users = &self.entities.users;
        let latest = Message::from_wire(message.clone(), &mut |user| known_user(user, users));
        self.latest.insert(thread, Arc::new(latest));
        true
    }

    pub(super) fn update_latest(&mut self, update: &MessageUpdate) -> bool {
        let Some(known) = self
            .latest
            .get(&update.channel_id)
            .filter(|known| known.id == update.id)
        else {
            return false;
        };
        let users = &self.entities.users;
        let next = known.patch(update.clone(), &mut |user| known_user(user, users));
        if next == **known {
            return false;
        }
        self.latest.insert(update.channel_id, Arc::new(next));
        true
    }

    // The newest page of a thread says what its newest message is, or that it has none.
    pub(super) fn note_newest_page(&mut self, thread: ChannelId, page: &[model::Message]) -> bool {
        let is_thread = self
            .entities
            .channels
            .get(&thread)
            .is_some_and(|stored| stored.is_thread());
        if !is_thread {
            return false;
        }
        let Some(newest) = page.iter().max_by_key(|message| message.id) else {
            let had = self.latest.remove(&thread).is_some();
            return self.recount(thread, |_| 0) | had;
        };
        if newest.kind == MessageType::ThreadStarterMessage {
            return false;
        }
        self.latest.remove(&thread);
        self.note_latest(newest)
    }

    // A thread payload naming a newer message than the one kept: it was missed.
    pub(super) fn drop_stale_latest(&mut self, thread: ChannelId) -> bool {
        let newest = self
            .entities
            .channels
            .get(&thread)
            .and_then(|stored| stored.last_message_id);
        let stale = self
            .latest
            .get(&thread)
            .is_some_and(|known| newest.is_some_and(|newest| newest.get() > known.id.get()));
        if stale {
            self.latest.remove(&thread);
        }
        stale
    }

    pub(super) fn delete_latest(&mut self, thread: ChannelId, id: MessageId) -> bool {
        let latest = self.latest.get(&thread).is_some_and(|known| known.id == id);
        if latest {
            self.latest.remove(&thread);
        }
        latest
    }

    // A thread a message brings, as THREAD_CREATE would; some payloads leave its guild out.
    pub(super) fn put_thread(&mut self, mut thread: model::Channel, events: &mut Vec<StoreEvent>) {
        if thread.guild_id.is_none() {
            thread.guild_id = thread
                .parent_id
                .and_then(|parent| self.entities.channels.get(&parent))
                .and_then(|parent| parent.guild_id);
        }
        self.put_channel(thread, events);
    }

    pub(super) fn put_threads_of(
        &mut self,
        messages: &[model::Message],
        events: &mut Vec<StoreEvent>,
    ) {
        for thread in messages
            .iter()
            .filter_map(|message| message.thread.as_deref())
        {
            self.put_thread(thread.clone(), events);
        }
    }

    // As the official client counts: a starter, and a forum post's first message (which has the
    // post's ID), aren't counted.
    fn counts(message: &model::Message) -> bool {
        message.kind != MessageType::ThreadStarterMessage
            && message.id.get() != message.channel_id.get()
    }

    // Kept like the newest message: no event for the thread; its box's row is reread instead.
    pub(super) fn count_created(&mut self, message: &model::Message) -> bool {
        Self::counts(message)
            && self.recount(message.channel_id, |count| (count + 1).min(MAX_COUNT))
    }

    pub(super) fn count_deleted(&mut self, thread: ChannelId, ids: &[MessageId]) -> bool {
        let removed = ids.iter().filter(|id| id.get() != thread.get()).count();
        let removed = u32::try_from(removed).unwrap_or(u32::MAX);
        removed > 0 && self.recount(thread, |count| count.saturating_sub(removed))
    }

    fn recount(&mut self, thread: ChannelId, change: impl FnOnce(u32) -> u32) -> bool {
        let Some(stored) = self.entities.channels.get_mut(&thread) else {
            return false;
        };
        let Some(info) = stored.thread.as_deref() else {
            return false;
        };
        let next = change(info.message_count.unwrap_or(0));
        if info.message_count == Some(next) {
            return false;
        }
        if let Some(info) = Arc::make_mut(stored).thread.as_deref_mut() {
            info.message_count = Some(next);
        }
        true
    }

    // The parent's row that shows the thread's box: the message the thread started from, or the
    // line about a thread started without one.
    pub(super) fn reread_boxes(&self, thread: ChannelId, events: &mut Vec<StoreEvent>) {
        let Some(parent) = self
            .entities
            .channels
            .get(&thread)
            .and_then(|stored| stored.parent_id)
        else {
            return;
        };
        if let Some(row) = self.windows.box_row(parent, thread) {
            events.push(StoreEvent::MessageUpdated(row));
        }
    }
}

fn known_user(user: model::User, users: &HashMap<UserId, Arc<User>>) -> Arc<User> {
    let user = User::from_wire(user);
    match users.get(&user.id) {
        Some(known) if **known == user => known.clone(),
        _ => Arc::new(user),
    }
}
