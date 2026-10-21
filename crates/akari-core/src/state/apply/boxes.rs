use std::collections::HashMap;
use std::sync::Arc;

use super::State;
use crate::gateway::MessageUpdate;
use crate::model::{self, ChannelId, MessageId, UserId};
use crate::state::events::StoreEvent;
use crate::state::types::{Message, User};

impl State {
    pub(crate) fn latest_message(&self, thread: ChannelId) -> Option<Arc<Message>> {
        self.latest.get(&thread).cloned()
    }

    // Only newer: a sync can bring an older message than a live one that came first.
    pub(super) fn note_latest(&mut self, message: &model::Message) -> bool {
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

    // Kept like the newest message: no event for the thread; its box's rows are reread instead.
    pub(super) fn count_thread_messages(
        &mut self,
        thread: ChannelId,
        added: u32,
        removed: u32,
    ) -> bool {
        let Some(stored) = self.entities.channels.get_mut(&thread) else {
            return false;
        };
        let Some(count) = stored.thread.as_deref().and_then(|info| info.message_count) else {
            return false;
        };
        let next = count.saturating_add(added).saturating_sub(removed);
        if next == count {
            return false;
        }
        if let Some(info) = Arc::make_mut(stored).thread.as_deref_mut() {
            info.message_count = Some(next);
        }
        true
    }

    // The parent's rows that show the thread's box: the message it started from, and the line
    // that names it.
    pub(super) fn reread_boxes(&self, thread: ChannelId, events: &mut Vec<StoreEvent>) {
        let Some(parent) = self
            .entities
            .channels
            .get(&thread)
            .and_then(|stored| stored.parent_id)
        else {
            return;
        };
        for row in self.windows.box_rows(parent, thread) {
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
