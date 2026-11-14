use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::oneshot;

use super::{Account, Shared};
use crate::RequestError;
use crate::model::{ChannelId, MessageId, UserId};
use crate::state::apply::ReactionAsk;
use crate::state::{ReactionEmoji, User};

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

// The caller waiting for one emoji on one message, while requests for it run.
struct Wish {
    channel: ChannelId,
    message: MessageId,
    emoji: ReactionEmoji,
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
                    wish.location = location;
                    wish.answer = Some(answer);
                }
                None if !wanted.changed => return Ok(()),
                None => {
                    wishes.push(Wish {
                        channel: wanted.channel,
                        message: wanted.message,
                        emoji: wanted.emoji.clone(),
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
            let (ask, location) = {
                let mut wishes = self.wishes.0.lock().unwrap_or_else(PoisonError::into_inner);
                let index = wishes
                    .iter()
                    .position(|wish| wish.is(channel, message, &emoji));
                let ask = self.store.ask_reaction(channel, message, &emoji);
                let (Some(ask), Some(index)) = (ask, index) else {
                    self.store.settle_reaction(channel, message, &emoji);
                    if let Some(index) = index {
                        wishes.swap_remove(index).answer(Ok(()));
                    }
                    return;
                };
                (ask, wishes[index].location)
            };
            let asked = self
                .ask_reaction(channel, message, &emoji, ask, location)
                .await;
            tracing::debug!(
                channel_id = channel.get(),
                message_id = message.get(),
                add = ask.add,
                outcome = asked.as_ref().map_or_else(RequestError::kind, |()| "done"),
                "reaction"
            );
            let mut wishes = self.wishes.0.lock().unwrap_or_else(PoisonError::into_inner);
            let refused = self
                .store
                .reaction_answered(channel, message, &emoji, asked.is_ok());
            let Err(err) = asked else {
                continue;
            };
            self.failed(&err);
            if !refused {
                // The wish changed while Discord refused the one before it.
                continue;
            }
            self.store.settle_reaction(channel, message, &emoji);
            if let Some(index) = wishes
                .iter()
                .position(|wish| wish.is(channel, message, &emoji))
            {
                wishes.swap_remove(index).answer(Err(err.into()));
            }
            return;
        }
    }

    async fn ask_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        ask: ReactionAsk,
        location: ReactionLocation,
    ) -> Result<(), RequestError> {
        self.unarchive_for_reaction(channel).await?;
        if ask.add {
            self.rest
                .add_reaction(channel, message, emoji, location.query())
                .await
        } else {
            self.rest
                .remove_reaction(channel, message, emoji, ask.burst, None, location.query())
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
    /// Adds the user's reaction with `emoji`, never a super reaction, to the message the row
    /// shows. The store has it at once (`MessageReactionsChanged`); an error means the store
    /// went back to what Discord has. A call made while one runs for the same emoji replaces it,
    /// and that one answers `Ok`. `InvalidRequest` where
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

    /// Takes the user's reaction with `emoji` back, as [`Account::react`] adds it; one that is
    /// theirs only as a super reaction is removed as one. Needs `toggle_reaction`.
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

/// Which of an emoji's two counts: plain reactions or super reactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionKind {
    Normal,
    Burst,
}

impl Shared {
    async fn moderate(
        &self,
        channel: ChannelId,
        row: MessageId,
        emoji: Option<&ReactionEmoji>,
        whose: Option<(UserId, ReactionKind)>,
    ) -> Result<(), ReactionError> {
        self.rest.usable()?;
        if !self.store.message_actions(channel, row).remove_reactions {
            return Err(RequestError::InvalidRequest.into());
        }
        let (shown, message) = self
            .store
            .shown_message(channel, row)
            .ok_or(RequestError::InvalidRequest)?;
        let emoji = emoji.map(|emoji| self.store.reaction_emoji(channel, row, emoji));
        let removed = async {
            self.unarchive_for_reaction(shown).await?;
            match (&emoji, whose) {
                (Some(emoji), Some((user, kind))) => {
                    self.rest
                        .remove_reaction(
                            shown,
                            message,
                            emoji,
                            kind == ReactionKind::Burst,
                            Some(user),
                            ReactionLocation::Message.query(),
                        )
                        .await
                }
                _ => {
                    self.rest
                        .remove_reactions(shown, message, emoji.as_ref())
                        .await
                }
            }
        }
        .await;
        if let Err(err) = &removed {
            self.failed(err);
        }
        Ok(removed?)
    }
}

impl Account {
    /// Removes `user`'s reaction with `emoji`, from the reactions window. The store changes
    /// when the gateway says so. The user's own goes as [`Account::unreact`] does; anyone
    /// else's needs [`MessageActions::remove_reactions`](crate::state::MessageActions), else
    /// `InvalidRequest` without a request.
    pub async fn remove_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        kind: ReactionKind,
        user: UserId,
    ) -> Result<(), ReactionError> {
        if self.shared.store.current_user().map(|me| me.user.id) == Some(user) {
            return self
                .shared
                .wish(channel, message, emoji, false, ReactionLocation::Message)
                .await;
        }
        self.shared
            .moderate(channel, message, Some(emoji), Some((user, kind)))
            .await
    }

    /// Removes every reaction of the message, or every one with `emoji`. The store changes
    /// when the gateway says so. `InvalidRequest`, without a request, without
    /// [`MessageActions::remove_reactions`](crate::state::MessageActions).
    pub async fn remove_reactions(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: Option<&ReactionEmoji>,
    ) -> Result<(), ReactionError> {
        self.shared.moderate(channel, message, emoji, None).await
    }
}

/// Discord's page of a reaction's users, and the most a request may ask for.
pub const REACTORS_PAGE: u8 = 100;

/// A page of the users who reacted with an emoji, in Discord's order.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ReactorsPage {
    pub users: Vec<Arc<User>>,
    /// Discord filled the page: ask on with the last user's ID as `after`.
    pub has_more: bool,
}

impl Account {
    /// The users who reacted with `emoji`, up to `limit` (1 to [`REACTORS_PAGE`]; the official
    /// client asks 100 for its list and 3 for a reaction's tooltip), those after the user
    /// `after`. Nothing is kept: each call asks Discord.
    pub async fn reactors(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: &ReactionEmoji,
        kind: ReactionKind,
        after: Option<UserId>,
        limit: u8,
    ) -> Result<ReactorsPage, RequestError> {
        let shared = &self.shared;
        shared.rest.usable()?;
        let limit = limit.clamp(1, REACTORS_PAGE);
        let named = shared.store.reaction_emoji(channel, message, emoji);
        let (channel, message) = shared
            .store
            .shown_message(channel, message)
            .unwrap_or((channel, message));
        let burst = kind == ReactionKind::Burst;
        let page = shared
            .rest
            .reactors(channel, message, &named, burst, after, limit)
            .await;
        if let Err(err) = &page {
            shared.failed(err);
        }
        let page = page?;
        Ok(ReactorsPage {
            has_more: page.received >= usize::from(limit),
            users: shared.store.known_users(page.users),
        })
    }
}
