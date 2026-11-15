use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::oneshot;

use super::{Account, Shared};
use crate::RequestError;
use crate::model::{ChannelId, MessageId};
use crate::rest::{EditMessage, Failure};
use crate::state::apply::DeleteStart;
use crate::state::{EditError, EditOutcome, Store, message_length};

const UNKNOWN_MESSAGE: u32 = 10008;
const AUTOMOD: [u32; 3] = [200_000, 200_001, 200_002];
const NOT_THE_AUTHOR: u32 = 50005;
const MISSING_ACCESS: u32 = 50001;
const MISSING_PERMISSIONS: u32 = 50013;

fn edit_error(failure: Failure, limit: usize) -> EditError {
    let err = match failure {
        Failure::EditLimit(retry_after) => return EditError::TooManyEdits { retry_after },
        failure => RequestError::from(failure),
    };
    match err {
        RequestError::Discord { code, message, .. } if AUTOMOD.contains(&code) => {
            EditError::MessageBlocked { reason: message }
        }
        RequestError::Discord {
            code: UNKNOWN_MESSAGE,
            ..
        } => EditError::Deleted,
        RequestError::Discord {
            code: NOT_THE_AUTHOR | MISSING_ACCESS | MISSING_PERMISSIONS,
            ..
        }
        | RequestError::Discord { status: 403, .. } => EditError::Forbidden,
        RequestError::RateLimited { retry_after } => EditError::RateLimited { retry_after },
        // The core can't count what Discord refused; the 0 asks for our own limit.
        RequestError::TooLong { limit: 0 } => EditError::Request(RequestError::TooLong { limit }),
        err => EditError::Request(err),
    }
}

type Answer = oneshot::Sender<Result<(), EditError>>;

struct Waiter {
    channel: ChannelId,
    message: MessageId,
    answer: Option<Answer>,
}

impl Waiter {
    fn answer(&mut self, outcome: Result<(), EditError>) {
        if let Some(answer) = self.answer.take() {
            let _ = answer.send(outcome);
        }
    }
}

// One request at a time per message; the latest save is the one that counts.
#[derive(Default)]
pub(super) struct Editing(Mutex<Vec<Waiter>>);

fn finish(
    waiters: &mut Vec<Waiter>,
    channel: ChannelId,
    message: MessageId,
    outcome: Result<(), EditError>,
) {
    if let Some(index) = waiters
        .iter()
        .position(|waiter| waiter.channel == channel && waiter.message == message)
    {
        waiters.swap_remove(index).answer(outcome);
    }
}

// A delete dropped before Discord answered, e.g. by a cancelled task, leaves the row as it was
// instead of dimmed for good.
struct Undimmed<'a> {
    store: &'a Store,
    message: Option<(ChannelId, MessageId)>,
}

impl Drop for Undimmed<'_> {
    fn drop(&mut self) {
        if let Some((channel, message)) = self.message.take() {
            self.store.delete_answered(channel, message, false);
        }
    }
}

impl Shared {
    async fn edit(
        self: &Arc<Self>,
        channel: ChannelId,
        row: MessageId,
        content: &str,
    ) -> Result<(), EditError> {
        self.rest.usable().map_err(EditError::Request)?;
        let limit = self.store.message_length_limit();
        if message_length(content) > limit {
            return Err(EditError::Request(RequestError::TooLong { limit }));
        }
        let (answer, outcome) = oneshot::channel();
        {
            let mut waiters = self
                .editing
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let wanted = self
                .store
                .want_edit(channel, row, content)
                .ok_or(EditError::Request(RequestError::InvalidRequest))?;
            let known = waiters.iter_mut().find(|waiter| {
                waiter.channel == wanted.channel && waiter.message == wanted.message
            });
            match known {
                Some(waiter) => {
                    // The earlier save is no longer the user's: nothing failed for it.
                    waiter.answer(Ok(()));
                    waiter.answer = Some(answer);
                }
                None if !wanted.changed => return Ok(()),
                None => {
                    waiters.push(Waiter {
                        channel: wanted.channel,
                        message: wanted.message,
                        answer: Some(answer),
                    });
                    let shared = self.clone();
                    self.runtime.spawn(async move {
                        shared.save(wanted.channel, wanted.message).await;
                    });
                }
            }
        }
        outcome
            .await
            .unwrap_or(Err(EditError::Request(RequestError::Closed)))
    }

    // Asks Discord until it has the text the user saved last, or refuses it.
    async fn save(&self, channel: ChannelId, message: MessageId) {
        loop {
            let ask = {
                // So a save can't come between the store's answer and its caller's.
                let mut waiters = self
                    .editing
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let Some(ask) = self.store.ask_edit(channel, message) else {
                    finish(&mut waiters, channel, message, Ok(()));
                    return;
                };
                ask
            };
            let body = EditMessage::new(ask.content.into(), ask.quiet_reply);
            let sent = async {
                self.open_archived(channel).await?;
                self.rest.edit_message(channel, message, &body).await
            }
            .await;
            let limit = self.store.message_length_limit();
            let outcome = sent.map_err(|failure| edit_error(failure, limit));
            tracing::debug!(
                channel_id = channel.get(),
                message_id = message.get(),
                outcome = match &outcome {
                    Ok(_) => "done",
                    Err(EditError::Request(err)) => err.kind(),
                    Err(_) => "refused",
                },
                "edit"
            );
            if let Err(EditError::Request(err)) = &outcome {
                self.failed(err);
            }
            let asked = outcome.as_ref().map(|_| ()).map_err(Clone::clone);
            let mut waiters = self
                .editing
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let answer = match self.store.edit_answered(channel, message, outcome) {
                EditOutcome::Again => continue,
                EditOutcome::Done(_) => Ok(()),
                EditOutcome::Refused(err, _) => Err(err),
                // Deleted meanwhile: the caller hears what Discord said.
                EditOutcome::Gone => asked,
            };
            finish(&mut waiters, channel, message, answer);
            return;
        }
    }

    async fn delete(&self, channel: ChannelId, row: MessageId) -> Result<(), RequestError> {
        self.rest.usable()?;
        let start = self
            .store
            .begin_delete(channel, row)
            .ok_or(RequestError::InvalidRequest)?;
        let DeleteStart::Request { channel, message } = start else {
            return Ok(());
        };
        let mut undimmed = Undimmed {
            store: &self.store,
            message: Some((channel, message)),
        };
        let deleted = async {
            self.open_archived(channel).await?;
            self.rest.delete_message(channel, message).await
        }
        .await;
        let deleted = match deleted {
            // Gone already: what the user wanted.
            Err(RequestError::Discord {
                code: UNKNOWN_MESSAGE,
                ..
            }) => Ok(()),
            deleted => deleted,
        };
        tracing::debug!(
            channel_id = channel.get(),
            message_id = message.get(),
            outcome = deleted
                .as_ref()
                .map_or_else(RequestError::kind, |()| "done"),
            "delete"
        );
        undimmed.message = None;
        self.store
            .delete_answered(channel, message, deleted.is_ok());
        if let Err(err) = &deleted {
            self.failed(err);
        }
        deleted
    }
}

impl Account {
    /// Saves `content` as the new text of the message the row shows (a thread's starter row
    /// edits the parent's message). Every copy of the message shows it at once and
    /// [`Store::message_edit`] reads `Pending` until Discord's answer or the gateway's update,
    /// whichever comes first; a refusal puts Discord's text back and keeps `Failed` with the
    /// saved text and the reason, which the error repeats. A save while one runs replaces it,
    /// and that call answers `Ok`. Text the message shows already sends nothing. Refused
    /// before asking (`EditError::Request`): `InvalidRequest` where
    /// [`MessageActions::edit`](crate::state::MessageActions) says no, or for blank text on a
    /// message without attachments or stickers (offer to delete it instead); `TooLong` past
    /// [`Store::message_length_limit`].
    pub async fn edit_message(
        &self,
        channel: ChannelId,
        message: MessageId,
        content: &str,
    ) -> Result<(), EditError> {
        self.shared.edit(channel, message, content).await
    }

    /// Deletes the message the row shows. A failed send goes at once, without a request. Any
    /// other is [`Store::deleting`] (the row dims) until Discord answers, then gone from every
    /// copy; a refusal leaves it as it was and is returned. `InvalidRequest`, without a
    /// request, where [`MessageActions::delete`](crate::state::MessageActions) says no.
    pub async fn delete_message(
        &self,
        channel: ChannelId,
        message: MessageId,
    ) -> Result<(), RequestError> {
        self.shared.delete(channel, message).await
    }

    /// Forgets a refused edit of the message (by its own IDs): the editor took its text back,
    /// or the user gave it up. Nothing for an edit on its way.
    pub fn dismiss_edit(&self, channel: ChannelId, message: MessageId) {
        self.shared.store.dismiss_edit(channel, message);
    }
}
