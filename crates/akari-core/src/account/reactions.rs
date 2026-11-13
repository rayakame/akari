use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::oneshot;

use super::{Account, Shared};
use crate::RequestError;
use crate::model::{ChannelId, MessageId};
use crate::state::ReactionEmoji;

const TOO_MANY_REACTIONS: u32 = 30010;
const REACTION_BLOCKED: u32 = 90001;
const UNKNOWN_EMOJI: u32 = 10014;
const MISSING_ACCESS: u32 = 50001;
const MISSING_PERMISSIONS: u32 = 50013;

/// Where the user reacted, which Discord's own client tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReactionLocation {
    /// The reactions window's ✕, and the official client's default.
    Message,
    /// A reaction under a message, or the add button at the row's end.
    InlineButton,
    /// A quick reaction in the hover bar.
    HoverBar,
    /// A quick reaction in the message's menu.
    ContextMenu,
    /// The emoji picker.
    Picker,
    /// The bar under a forum post's first message.
    ForumToolbar,
}

impl ReactionLocation {
    fn query(self) -> &'static str {
        match self {
            Self::Message => "Message",
            Self::InlineButton => "Message%20Inline%20Button",
            Self::HoverBar => "Message%20Hover%20Bar",
            Self::ContextMenu => "Message%20Context%20Menu",
            Self::Picker => "Message%20Reaction%20Picker",
            Self::ForumToolbar => "Forum%20Toolbar",
        }
    }
}

/// Why a reaction wasn't added or removed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReactionError {
    /// The message has as many different reactions as it may (30010): the official client's
    /// "We appreciate the enthusiasm but..." alert.
    #[error("the message has too many reactions")]
    TooMany,
    /// The message's author blocked the user (90001); the official client shakes its window.
    #[error("the message's author blocked the user")]
    Blocked,
    /// Discord refused for a missing permission or access, or a timeout (any other 403).
    #[error("reacting isn't allowed here")]
    Forbidden,
    /// Discord doesn't know the emoji (10014), or the user may not use it.
    #[error("Discord doesn't know the emoji")]
    UnknownEmoji,
    /// Still rate limited after the one retry.
    #[error("rate limited by Discord")]
    RateLimited { retry_after: Option<Duration> },
    /// Anything else, also a refusal before asking (`InvalidRequest`).
    #[error(transparent)]
    Request(RequestError),
}

impl From<RequestError> for ReactionError {
    fn from(err: RequestError) -> Self {
        match err {
            RequestError::Discord {
                code: TOO_MANY_REACTIONS,
                ..
            } => Self::TooMany,
            RequestError::Discord {
                code: REACTION_BLOCKED,
                ..
            } => Self::Blocked,
            RequestError::Discord {
                code: UNKNOWN_EMOJI,
                ..
            } => Self::UnknownEmoji,
            RequestError::Discord {
                code: MISSING_ACCESS | MISSING_PERMISSIONS,
                ..
            }
            | RequestError::Discord { status: 403, .. } => Self::Forbidden,
            RequestError::RateLimited { retry_after } => Self::RateLimited { retry_after },
            err => Self::Request(err),
        }
    }
}

type Answer = oneshot::Sender<Result<(), ReactionError>>;

// What the user last wanted for one emoji on one message, while requests for it run.
struct Wish {
    channel: ChannelId,
    message: MessageId,
    emoji: ReactionEmoji,
    want: bool,
    // The removal takes the user's super reaction.
    burst: bool,
    // What Discord has, as far as its answers said.
    discord: bool,
    location: ReactionLocation,
    answer: Option<Answer>,
}

impl Wish {
    fn is(&self, channel: ChannelId, message: MessageId, emoji: &ReactionEmoji) -> bool {
        self.channel == channel && self.message == message && self.emoji.same(emoji)
    }

    fn answer(&mut self, outcome: Result<(), ReactionError>) {
        if let Some(answer) = self.answer.take() {
            let _ = answer.send(outcome);
        }
    }
}

// One request at a time per message and emoji; the latest wish is the one that counts.
#[derive(Default)]
pub(super) struct Wishes(Mutex<Vec<Wish>>);

impl Shared {
    async fn wish(
        self: &Arc<Self>,
        channel: ChannelId,
        row: MessageId,
        emoji: &ReactionEmoji,
        want: bool,
        location: ReactionLocation,
    ) -> Result<(), ReactionError> {
        self.rest.usable()?;
        let (answer, outcome) = oneshot::channel();
        {
            let mut wishes = self.wishes.0.lock().unwrap_or_else(PoisonError::into_inner);
            let wanted = self
                .store
                .want_reaction(channel, row, emoji, want)
                .ok_or(RequestError::InvalidRequest)?;
            let known = wishes
                .iter_mut()
                .find(|wish| wish.is(wanted.channel, wanted.message, &wanted.emoji));
            match known {
                Some(wish) => {
                    // The earlier call's wish is no longer the user's: nothing failed for it.
                    wish.answer(Ok(()));
                    wish.want = want;
                    wish.location = location;
                    if wanted.changed && !want {
                        wish.burst = wanted.burst;
                    }
                    wish.answer = Some(answer);
                }
                None if !wanted.changed => return Ok(()),
                None => {
                    wishes.push(Wish {
                        channel: wanted.channel,
                        message: wanted.message,
                        emoji: wanted.emoji.clone(),
                        want,
                        burst: wanted.burst,
                        discord: !want,
                        location,
                        answer: Some(answer),
                    });
                    let shared = self.clone();
                    self.runtime.spawn(async move {
                        shared
                            .grant(wanted.channel, wanted.message, wanted.emoji)
                            .await;
                    });
                }
            }
        }
        outcome.await.unwrap_or(Err(RequestError::Closed.into()))
    }

    // Asks Discord until it has what the user last wanted, or refuses it.
    async fn grant(&self, channel: ChannelId, message: MessageId, emoji: ReactionEmoji) {
        loop {
            let (want, burst, location) = {
                let mut wishes = self.wishes.0.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(index) = wishes
                    .iter()
                    .position(|wish| wish.is(channel, message, &emoji))
                else {
                    return;
                };
                if wishes[index].want == wishes[index].discord {
                    let mut wish = wishes.swap_remove(index);
                    self.store.settle_reaction(channel, message, &emoji, None);
                    wish.answer(Ok(()));
                    return;
                }
                let wish = &wishes[index];
                (wish.want, wish.burst, wish.location)
            };
            let asked = self
                .ask_reaction(channel, message, &emoji, want, burst, location)
                .await;
            tracing::debug!(
                channel_id = channel.get(),
                message_id = message.get(),
                add = want,
                outcome = asked.as_ref().map_or_else(RequestError::kind, |()| "done"),
                "reaction"
            );
            let mut wishes = self.wishes.0.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(index) = wishes
                .iter()
                .position(|wish| wish.is(channel, message, &emoji))
            else {
                return;
            };
            match asked {
                Ok(()) => {
                    wishes[index].discord = want;
                    self.store.confirm_reaction(channel, message, &emoji);
                }
                Err(err) => {
                    self.failed(&err);
                    // A wish made since is for what Discord kept: the next round finds it so.
                    if wishes[index].want == want {
                        let mut wish = wishes.swap_remove(index);
                        let restore = Some((wish.discord, wish.burst));
                        self.store
                            .settle_reaction(channel, message, &emoji, restore);
                        wish.answer(Err(err.into()));
                        return;
                    }
                }
            }
        }
    }

    async fn ask_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        want: bool,
        burst: bool,
        location: ReactionLocation,
    ) -> Result<(), RequestError> {
        self.unarchive_for_reaction(channel).await?;
        if want {
            self.rest
                .add_reaction(channel, message, emoji, location.query())
                .await
        } else {
            self.rest
                .remove_reaction(channel, message, emoji, burst, None, location.query())
                .await
        }
    }

    // The official client opens an archived thread before every reaction request.
    async fn unarchive_for_reaction(&self, channel: ChannelId) -> Result<(), RequestError> {
        let archived = self
            .store
            .channel(channel)
            .and_then(|channel| channel.thread.as_deref().map(|thread| thread.archived))
            .unwrap_or(false);
        if archived {
            self.rest.unarchive_thread(channel).await?;
        }
        Ok(())
    }
}

impl Account {
    /// Adds the user's reaction with `emoji` to the message the row `message` of `channel`
    /// shows. The store has it at once on every copy of the message
    /// (`MessageReactionsChanged`), and Discord's echo changes nothing; a refusal takes it out
    /// again, unless the gateway said otherwise meanwhile, and is the error. One request runs
    /// per message and emoji: a call made meanwhile changes what the next one asks for, so
    /// Discord ends with what the user wanted last, and answers the call before it with `Ok`.
    /// Never a super reaction; `Ok` without a request when the reaction is the user's already.
    /// `InvalidRequest`, without a request, where
    /// [`MessageActions`](crate::state::MessageActions) says no: `react` for an emoji the
    /// message doesn't have, `toggle_reaction` for one it has.
    pub async fn react(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        location: ReactionLocation,
    ) -> Result<(), ReactionError> {
        self.shared
            .wish(channel, message, emoji, true, location)
            .await
    }

    /// Takes the user's reaction with `emoji` back, as [`Account::react`] adds it: at once in
    /// the store, put back on a refusal. A reaction that is the user's only as a super
    /// reaction is removed as one. Needs `toggle_reaction`.
    pub async fn unreact(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        location: ReactionLocation,
    ) -> Result<(), ReactionError> {
        self.shared
            .wish(channel, message, emoji, false, location)
            .await
    }
}
