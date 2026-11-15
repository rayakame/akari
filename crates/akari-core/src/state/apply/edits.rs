use std::sync::Arc;

use super::State;
use crate::gateway::MessageUpdate;
use crate::model::{self, ChannelId, MessageId, MessageType};
use crate::state::edits::{Ask, EditError, EditOutcome, MessageEdit, Shown, content_of};
use crate::state::events::StoreEvent;
use crate::state::replies::ReplyTarget;
use crate::state::types::{Delivery, Message};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WantedEdit {
    pub(crate) channel: ChannelId,
    pub(crate) message: MessageId,
    // `false`: the message shows the text already; nothing to ask unless a request runs.
    pub(crate) changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditAsk {
    pub(crate) content: Box<str>,
    pub(crate) quiet_reply: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeleteStart {
    Request {
        channel: ChannelId,
        message: MessageId,
    },
    Discarded,
}

impl State {
    pub(crate) fn message_edit(
        &self,
        channel: ChannelId,
        message: MessageId,
    ) -> Option<MessageEdit> {
        self.edits.edit(channel, message)
    }

    pub(crate) fn deleting(&self, channel: ChannelId, message: MessageId) -> bool {
        self.edits.deleting(channel, message)
    }

    pub(crate) fn newest_editable_message(
        &self,
        channel: ChannelId,
        now_millis: i64,
    ) -> Option<MessageId> {
        let me = self.me()?;
        self.windows
            .messages(channel)
            .iter()
            .rev()
            .filter(|message| message.author.id == me)
            .find(|message| self.message_actions_of(message, now_millis).edit)
            .map(|message| message.id)
    }

    // `None` where the message's actions don't offer it, or for blank text on its own.
    pub(crate) fn want_edit(
        &mut self,
        channel: ChannelId,
        row: MessageId,
        content: &str,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<WantedEdit> {
        if !self.message_actions(channel, row, now_millis).edit {
            return None;
        }
        let shown = self
            .windows
            .shown_by_row(channel, row)
            .or_else(|| self.acted_on(channel, row))?;
        if content.trim().is_empty() && shown.attachments.is_empty() && shown.stickers.is_empty() {
            return None;
        }
        let (channel, message) = (shown.channel_id, shown.id);
        let failed = matches!(
            self.edits.edit(channel, message),
            Some(MessageEdit::Failed { .. })
        );
        let saved = self.edits.save(&shown, content);
        if let Some(text) = &saved.show {
            self.show_content(channel, message, text, events);
        }
        if saved.show.is_some() || failed {
            self.announce(channel, message, events);
        }
        Some(WantedEdit {
            channel,
            message,
            changed: saved.changed,
        })
    }

    // `None` when Discord has the saved text already, which ends the edit.
    pub(crate) fn ask_edit(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        events: &mut Vec<StoreEvent>,
    ) -> Option<EditAsk> {
        let shown = self.edits.edit(channel, message).is_some();
        match self.edits.ask(channel, message) {
            Ask::Send(content) => Some(EditAsk {
                content,
                quiet_reply: self.quiet_reply(channel, message),
            }),
            Ask::Settled(settled) => {
                self.show_settled(channel, message, &settled, events);
                if shown {
                    self.announce(channel, message, events);
                }
                None
            }
            Ask::Nothing => None,
        }
    }

    fn show_settled(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        settled: &Shown,
        events: &mut Vec<StoreEvent>,
    ) {
        match settled {
            Shown::Answer(update) => self.update_message((**update).clone(), events),
            Shown::Content(text) => self.show_content(channel, message, text, events),
        }
    }

    // The official client's rule (module 138399): a reply whose loaded target's author it
    // doesn't mention was sent without the ping.
    fn quiet_reply(&self, channel: ChannelId, message: MessageId) -> bool {
        let Some(shown) = self.windows.shown(channel, message) else {
            return false;
        };
        let Some((target_channel, target)) = shown.replied_to() else {
            return false;
        };
        shown.kind == MessageType::Reply
            && match self.windows.reply_target(target_channel, target) {
                ReplyTarget::Loaded(preview) => !shown.mentions_user(preview.author.id),
                ReplyTarget::Deleted | ReplyTarget::NotLoaded => false,
            }
    }

    pub(crate) fn edit_answered(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        outcome: Result<Option<model::Message>, EditError>,
        events: &mut Vec<StoreEvent>,
    ) -> EditOutcome {
        let shown = self.edits.edit(channel, message).is_some();
        let answered = self.edits.answered(channel, message, outcome);
        match &answered {
            EditOutcome::Done(settled) => self.show_settled(channel, message, settled, events),
            EditOutcome::Refused(_, text) => self.show_content(channel, message, text, events),
            EditOutcome::Again | EditOutcome::Gone => return answered,
        }
        if shown {
            self.announce(channel, message, events);
        }
        answered
    }

    pub(crate) fn dismiss_edit(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        if self.edits.dismiss(channel, message) {
            self.announce(channel, message, events);
        }
    }

    // `None` where the message's actions don't offer it.
    pub(crate) fn begin_delete(
        &mut self,
        channel: ChannelId,
        row: MessageId,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<DeleteStart> {
        if !self.message_actions(channel, row, now_millis).delete {
            return None;
        }
        let failed = self
            .acted_on(channel, row)
            .is_some_and(|message| message.delivery == Delivery::Failed);
        if failed {
            self.windows.discard(channel, row, events);
            return Some(DeleteStart::Discarded);
        }
        let (channel, message) = self.shown_message(channel, row)?;
        self.edits.begin_delete(channel, message);
        self.announce(channel, message, events);
        Some(DeleteStart::Request { channel, message })
    }

    pub(crate) fn delete_answered(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        deleted: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        if !self.edits.end_delete(channel, message) {
            return;
        }
        if deleted {
            // The thread's count waits for the gateway's MESSAGE_DELETE, so it counts once.
            self.remove_messages(channel, &[message], false, events);
        } else {
            self.announce(channel, message, events);
        }
    }

    fn show_content(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        content: &str,
        events: &mut Vec<StoreEvent>,
    ) {
        self.update_message(content_of(channel, message, content), events);
    }

    pub(super) fn update_message(&mut self, update: MessageUpdate, events: &mut Vec<StoreEvent>) {
        if self.update_latest(&update) {
            self.reread_boxes(update.channel_id, events);
        }
        self.update_first_message(&update, events);
        self.windows.update(update, &self.entities.users, events);
    }

    pub(super) fn remove_messages(
        &mut self,
        channel: ChannelId,
        ids: &[MessageId],
        counted: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        let mut boxes = counted && self.count_deleted(channel, ids);
        for id in ids {
            boxes |= self.delete_latest(channel, *id);
        }
        if boxes {
            self.reread_boxes(channel, events);
        }
        for id in ids {
            self.delete_first_message(channel, *id, events);
            self.windows.delete(channel, *id, events);
            self.edits.forget(channel, *id);
        }
    }

    // Only the message's edit or delete changed, which no copy shows.
    fn announce(&self, channel: ChannelId, message: MessageId, events: &mut Vec<StoreEvent>) {
        let rows = self
            .windows
            .message(channel, message)
            .into_iter()
            .chain(self.windows.starter_rows_showing(channel, message));
        for row in rows {
            let told = events.iter().any(|event| {
                matches!(event, StoreEvent::MessageUpdated(updated)
                    if updated.id == row.id && updated.channel_id == row.channel_id)
            });
            if !told {
                events.push(StoreEvent::MessageUpdated(row));
            }
        }
    }

    // A save Discord hasn't confirmed may still be refused.
    pub(super) fn with_discord_text(&self, message: &Arc<Message>) -> Arc<Message> {
        match self.edits.discord_text(message.channel_id, message.id) {
            Some(text) if *message.content != *text => {
                let mut cached = (**message).clone();
                cached.content = text.into();
                Arc::new(cached)
            }
            _ => message.clone(),
        }
    }
}
