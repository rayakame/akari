use std::sync::Arc;

use super::State;
use crate::model::{self, ChannelId};
use crate::state::events::StoreEvent;

impl State {
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

    // Kept like the newest message: no event for the thread, only for the rows showing its box.
    pub(super) fn count_thread_messages(
        &mut self,
        thread: ChannelId,
        added: u32,
        removed: u32,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(stored) = self.entities.channels.get_mut(&thread) else {
            return;
        };
        let Some(count) = stored.thread.as_deref().and_then(|info| info.message_count) else {
            return;
        };
        let next = count.saturating_add(added).saturating_sub(removed);
        if next == count {
            return;
        }
        if let Some(info) = Arc::make_mut(stored).thread.as_deref_mut() {
            info.message_count = Some(next);
        }
        self.reread_boxes(thread, events);
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
