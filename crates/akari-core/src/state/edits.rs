use std::time::Duration;

use super::types::Message;
use crate::RequestError;
use crate::gateway::MessageUpdate;
use crate::model::{self, ChannelId, MessageId, Timestamp};

pub(crate) const FAILED_KEPT: usize = 16;

/// Why Discord didn't take an edit: see [`Account::edit_message`](crate::Account::edit_message).
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum EditError {
    /// AutoMod refused the new text (200000, 200001, 200002); `reason` is Discord's message.
    #[error("AutoMod refused the message")]
    MessageBlocked { reason: String },
    /// A published announcement may be edited three times an hour (20022).
    #[error("the message was edited as often as Discord allows for now")]
    TooManyEdits { retry_after: Option<Duration> },
    /// Discord refused for a missing permission or access, or for someone else's message.
    #[error("editing this message isn't allowed")]
    Forbidden,
    /// Discord doesn't know the message (10008): it was deleted.
    #[error("the message was deleted")]
    Deleted,
    /// Still rate limited after the one retry.
    #[error("rate limited by Discord")]
    RateLimited { retry_after: Option<Duration> },
    /// Anything else, also a refusal before asking (`InvalidRequest`, `TooLong`).
    #[error(transparent)]
    Request(RequestError),
}

/// The user's own edit of a message, while Discord hasn't taken it: see
/// [`Store::message_edit`](super::Store::message_edit).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MessageEdit {
    /// The message shows the saved text, which Discord hasn't confirmed yet.
    Pending,
    /// Discord refused the edit: the message shows its text as Discord has it again, and
    /// `content` is the text the user saved, to edit again or dismiss.
    Failed { content: Box<str>, error: EditError },
}

#[derive(Debug, Clone)]
struct Asked {
    content: Box<str>,
    // The gateway showed it: Discord has it, whatever the answer says.
    seen: bool,
    // The gateway told of the message's text since it was asked.
    heard: bool,
}

#[derive(Debug, Clone)]
struct OwnEdit {
    channel: ChannelId,
    message: MessageId,
    // The text the user saved last.
    want: Box<str>,
    // What Discord has by the answers to the requests: asked for again while `want` differs.
    sent: Box<str>,
    // Discord's text and edit time as last heard, from the gateway or an answer.
    discord: Box<str>,
    edited: Option<Timestamp>,
    // The newest word was an answer: the copies take its edit at the end.
    answer: Option<Box<MessageUpdate>>,
    asked: Option<Asked>,
    // The gateway showed `want`: the copies show what it says from then on.
    shown: bool,
    failed: Option<EditError>,
}

impl OwnEdit {
    fn is(&self, channel: ChannelId, message: MessageId) -> bool {
        self.channel == channel && self.message == message
    }

    fn shows_saved(&self) -> bool {
        self.failed.is_none() && !self.shown
    }

    fn settled(self) -> Shown {
        match self.answer {
            Some(answer) => Shown::Answer(answer),
            None => Shown::Content(self.discord),
        }
    }
}

pub(crate) struct Saved {
    // `false`: nothing to ask, the message shows the text already.
    pub(crate) changed: bool,
    pub(crate) show: Option<Box<str>>,
}

pub(crate) enum Shown {
    Answer(Box<MessageUpdate>),
    Content(Box<str>),
}

pub(crate) enum Ask {
    Send(Box<str>),
    // Discord has the saved text already.
    Settled(Shown),
    Nothing,
}

pub(crate) enum EditOutcome {
    // A newer save goes next.
    Again,
    Done(Shown),
    // Discord's text, and the message whose refusal went for the limit.
    Refused(EditError, Box<str>, Option<(ChannelId, MessageId)>),
    // The message was deleted meanwhile.
    Gone,
}

#[derive(Debug, Default)]
pub(crate) struct Edits {
    edits: Vec<OwnEdit>,
    deleting: Vec<(ChannelId, MessageId)>,
}

// The state's comparisons in tests; a refusal's error has no equality of its own.
#[cfg(test)]
impl PartialEq for Edits {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

impl Edits {
    fn find(&mut self, channel: ChannelId, message: MessageId) -> Option<&mut OwnEdit> {
        self.edits.iter_mut().find(|own| own.is(channel, message))
    }

    fn position(&self, channel: ChannelId, message: MessageId) -> Option<usize> {
        self.edits.iter().position(|own| own.is(channel, message))
    }

    pub(crate) fn edit(&self, channel: ChannelId, message: MessageId) -> Option<MessageEdit> {
        let own = self.edits.iter().find(|own| own.is(channel, message))?;
        match &own.failed {
            Some(error) => Some(MessageEdit::Failed {
                content: own.want.clone(),
                error: error.clone(),
            }),
            None => (!own.shown).then_some(MessageEdit::Pending),
        }
    }

    pub(crate) fn save(&mut self, shown: &Message, content: &str) -> Saved {
        let (channel, message) = (shown.channel_id, shown.id);
        let Some(index) = self.position(channel, message) else {
            if *shown.content == *content {
                return Saved {
                    changed: false,
                    show: None,
                };
            }
            self.edits.push(OwnEdit {
                channel,
                message,
                want: content.into(),
                sent: shown.content.clone(),
                discord: shown.content.clone(),
                edited: shown.edited_timestamp,
                answer: None,
                asked: None,
                shown: false,
                failed: None,
            });
            return Saved {
                changed: true,
                show: Some(content.into()),
            };
        };
        let own = &mut self.edits[index];
        if own.failed.is_some() && *own.discord == *content {
            self.edits.remove(index);
            return Saved {
                changed: false,
                show: None,
            };
        }
        let changed = own.failed.is_some() || *own.want != *content;
        if changed {
            own.failed = None;
            own.want = content.into();
            own.shown = false;
        }
        Saved {
            changed,
            show: changed.then(|| content.into()),
        }
    }

    pub(crate) fn ask(&mut self, channel: ChannelId, message: MessageId) -> Ask {
        let Some(index) = self.position(channel, message) else {
            return Ask::Nothing;
        };
        let own = &mut self.edits[index];
        if own.failed.is_some() {
            return Ask::Nothing;
        }
        // After a refusal `sent` is older than what Discord may have heard since.
        if own.want == own.sent && own.want == own.discord {
            return Ask::Settled(self.edits.remove(index).settled());
        }
        own.asked = Some(Asked {
            content: own.want.clone(),
            seen: false,
            heard: false,
        });
        Ask::Send(own.want.clone())
    }

    pub(crate) fn answered(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        outcome: Result<Option<model::Message>, EditError>,
    ) -> EditOutcome {
        let Some(index) = self.position(channel, message) else {
            return EditOutcome::Gone;
        };
        let own = &mut self.edits[index];
        let Some(asked) = own.asked.take() else {
            return EditOutcome::Again;
        };
        let outcome = match outcome {
            Err(_) if asked.seen => Ok(None),
            outcome => outcome,
        };
        match outcome {
            Ok(answer) => {
                match answer {
                    Some(answer) if answer.edited_timestamp >= own.edited => {
                        own.discord = answer.content.as_str().into();
                        own.edited = answer.edited_timestamp;
                        own.answer = Some(Box::new(edit_of(answer)));
                    }
                    Some(_) => {}
                    None if !asked.heard => {
                        own.discord.clone_from(&asked.content);
                        own.answer = None;
                    }
                    None => {}
                }
                own.sent = asked.content;
                if own.want != own.sent {
                    return EditOutcome::Again;
                }
                EditOutcome::Done(self.edits.remove(index).settled())
            }
            Err(error) if own.want == asked.content => {
                own.failed = Some(error.clone());
                let discord = own.discord.clone();
                let gone = self.keep_failed(channel, message);
                EditOutcome::Refused(error, discord, gone)
            }
            Err(_) => EditOutcome::Again,
        }
    }

    fn keep_failed(
        &mut self,
        channel: ChannelId,
        message: MessageId,
    ) -> Option<(ChannelId, MessageId)> {
        let failed = self.edits.iter().filter(|own| own.failed.is_some());
        if failed.count() <= FAILED_KEPT {
            return None;
        }
        let oldest = self
            .edits
            .iter()
            .position(|own| own.failed.is_some() && !own.is(channel, message))?;
        let gone = self.edits.remove(oldest);
        Some((gone.channel, gone.message))
    }

    // While the saved text is still to show, the copies keep it over Discord's.
    pub(crate) fn heard(&mut self, mut update: MessageUpdate) -> MessageUpdate {
        let Some(own) = self.find(update.channel_id, update.id) else {
            return update;
        };
        let Some(content) = update.content.as_deref() else {
            return update;
        };
        let edited = update.edited_timestamp.unwrap_or(own.edited);
        let older = own
            .answer
            .as_deref()
            .and_then(|answer| answer.edited_timestamp)
            .is_some_and(|answered| answered > edited);
        if !older {
            own.discord = content.into();
            own.edited = edited;
            own.answer = None;
            // An echo older than the last answer was sent before this request was asked.
            if let Some(asked) = &mut own.asked {
                asked.heard = true;
                asked.seen |= *asked.content == *content;
            }
        }
        own.shown |= *own.want == *content;
        if own.shows_saved() {
            update.content = Some(own.want.to_string());
        }
        update
    }

    pub(crate) fn dismiss(&mut self, channel: ChannelId, message: MessageId) -> bool {
        let failed = self
            .edits
            .iter()
            .position(|own| own.is(channel, message) && own.failed.is_some());
        if let Some(index) = failed {
            self.edits.remove(index);
        }
        failed.is_some()
    }

    pub(crate) fn deleting(&self, channel: ChannelId, message: MessageId) -> bool {
        self.deleting.contains(&(channel, message))
    }

    pub(crate) fn begin_delete(&mut self, channel: ChannelId, message: MessageId) {
        if !self.deleting(channel, message) {
            self.deleting.push((channel, message));
        }
    }

    // Whether the delete was still on its way: not when the gateway's came first.
    pub(crate) fn end_delete(&mut self, channel: ChannelId, message: MessageId) -> bool {
        let before = self.deleting.len();
        self.deleting
            .retain(|deleting| *deleting != (channel, message));
        self.deleting.len() != before
    }

    pub(crate) fn forget(&mut self, channel: ChannelId, message: MessageId) {
        self.edits.retain(|own| !own.is(channel, message));
        self.deleting
            .retain(|deleting| *deleting != (channel, message));
    }

    // What the disk cache writes instead of a save Discord hasn't confirmed.
    pub(crate) fn discord_text(&self, channel: ChannelId, message: MessageId) -> Option<&str> {
        self.showing_saved(channel, message)
            .map(|own| &*own.discord)
    }

    pub(crate) fn saved_text(&self, channel: ChannelId, message: MessageId) -> Option<&str> {
        self.showing_saved(channel, message).map(|own| &*own.want)
    }

    fn showing_saved(&self, channel: ChannelId, message: MessageId) -> Option<&OwnEdit> {
        self.edits
            .iter()
            .find(|own| own.is(channel, message) && own.shows_saved())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

// What an edit's answer changes on the copies; embeds, attachments and pins are the gateway's,
// which may have changed them since.
fn edit_of(message: model::Message) -> MessageUpdate {
    MessageUpdate {
        id: message.id,
        channel_id: message.channel_id,
        guild_id: None,
        kind: None,
        author: None,
        webhook_id: None,
        content: Some(message.content),
        edited_timestamp: Some(message.edited_timestamp),
        mention_everyone: Some(message.mention_everyone),
        mentions: Some(message.mentions),
        mention_roles: Some(message.mention_roles),
        attachments: None,
        embeds: None,
        sticker_items: None,
        pinned: None,
        flags: None,
        call: None,
        thread: None,
    }
}

pub(crate) fn content_of(channel: ChannelId, message: MessageId, content: &str) -> MessageUpdate {
    MessageUpdate {
        id: message,
        channel_id: channel,
        guild_id: None,
        kind: None,
        author: None,
        webhook_id: None,
        content: Some(content.to_owned()),
        edited_timestamp: None,
        mention_everyone: None,
        mentions: None,
        mention_roles: None,
        attachments: None,
        embeds: None,
        sticker_items: None,
        pinned: None,
        flags: None,
        call: None,
        thread: None,
    }
}
