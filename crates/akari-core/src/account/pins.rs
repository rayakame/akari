use std::sync::Arc;
use std::time::Duration;

use super::{ACK_RETRY, Account, Shared};
use crate::RequestError;
use crate::model::{ChannelId, MessageId};
use crate::rest::Failure;
use crate::state::{PINS_PAGE, PinsAsk, PinsLoad, Store};

const TOO_MANY_PINS: u32 = 30003;
const UNKNOWN_MESSAGE: u32 = 10008;
const MISSING_ACCESS: u32 = 50001;
const MISSING_PERMISSIONS: u32 = 50013;
const NOT_PINNABLE_HERE: u32 = 50019;
const SYSTEM_MESSAGE: u32 = 50021;
const ARCHIVED_THREAD: u32 = 50083;
// The official client's read state retries: three tries, 2 s and then 4 s apart.
const ACK_TRIES: u32 = 3;

/// The most pins a channel has: the number the official client's alert names.
pub const PIN_LIMIT: u32 = 250;

/// Why a pin or an unpin didn't happen. The official client has an alert for each when pinning,
/// and one for every failure of an unpin.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PinError {
    /// The channel has as many pins as it may (30003): "The Pin Broke".
    #[error("the channel has {limit} pins already")]
    TooMany { limit: u32 },
    /// The thread is archived and couldn't be opened first (50083).
    #[error("the thread is archived")]
    Archived,
    /// System messages can't be pinned (50021).
    #[error("system messages can't be pinned")]
    SystemMessage,
    /// Messages can't be pinned in this channel (50019).
    #[error("messages can't be pinned in this channel")]
    WrongChannel,
    /// A missing permission or access (50001, 50013, any other 403).
    #[error("pinning isn't allowed here")]
    Forbidden,
    /// The message is gone (10008).
    #[error("the message is gone")]
    Gone,
    /// Anything else, also a refusal before asking (`InvalidRequest`); a `Discord` error has the
    /// message the official client's alert shows.
    #[error(transparent)]
    Request(RequestError),
}

impl From<RequestError> for PinError {
    fn from(err: RequestError) -> Self {
        match err {
            RequestError::Discord {
                code: TOO_MANY_PINS,
                ..
            } => Self::TooMany { limit: PIN_LIMIT },
            RequestError::Discord {
                code: ARCHIVED_THREAD,
                ..
            } => Self::Archived,
            RequestError::Discord {
                code: SYSTEM_MESSAGE,
                ..
            } => Self::SystemMessage,
            RequestError::Discord {
                code: NOT_PINNABLE_HERE,
                ..
            } => Self::WrongChannel,
            RequestError::Discord {
                code: UNKNOWN_MESSAGE,
                ..
            } => Self::Gone,
            RequestError::Discord {
                code: MISSING_ACCESS | MISSING_PERMISSIONS,
                ..
            }
            | RequestError::Discord { status: 403, .. } => Self::Forbidden,
            err => Self::Request(err),
        }
    }
}

// A page dropped before its answer, e.g. by a cancelled task, can be asked for again.
struct PendingPins<'a> {
    store: &'a Store,
    ask: Option<PinsAsk>,
}

impl Drop for PendingPins<'_> {
    fn drop(&mut self) {
        if let Some(ask) = self.ask.take() {
            self.store.abandon_pins(ask);
        }
    }
}

impl Shared {
    async fn load_pins(&self, channel: ChannelId, load: PinsLoad) -> Result<(), RequestError> {
        self.rest.usable()?;
        let Some(ask) = self.store.begin_pins(channel, load) else {
            return Ok(());
        };
        let mut pending = PendingPins {
            store: &self.store,
            ask: Some(ask),
        };
        let page = self.rest.list_pins(channel, ask.before, PINS_PAGE).await;
        let page = match page {
            Ok(page) => page,
            Err(err) => {
                tracing::debug!(
                    target: "akari_core::load",
                    channel_id = channel.get(),
                    first = ask.before.is_none(),
                    outcome = err.kind(),
                    "pins failed"
                );
                self.failed(&err);
                pending.ask = None;
                self.store.fail_pins(ask);
                return Err(err);
            }
        };
        let messages: Vec<_> = page.items.iter().map(|pin| pin.message.clone()).collect();
        self.prepare_page(&messages, &mut (Duration::ZERO, 0)).await;
        tracing::debug!(
            target: "akari_core::load",
            channel_id = channel.get(),
            first = ask.before.is_none(),
            pins = page.items.len(),
            more = page.has_more,
            "pins loaded"
        );
        pending.ask = None;
        self.store.finish_pins(ask, page);
        Ok(())
    }

    async fn change_pin(
        &self,
        channel: ChannelId,
        row: MessageId,
        pin: bool,
    ) -> Result<(), PinError> {
        self.rest.usable()?;
        let actions = self.store.message_actions(channel, row);
        let allowed = if pin { actions.pin } else { actions.unpin };
        let shown = self.store.shown_message(channel, row).filter(|_| allowed);
        let Some((channel, message)) = shown else {
            return Err(RequestError::InvalidRequest.into());
        };
        let changed = async {
            self.open_archived(channel).await?;
            if pin {
                self.rest.pin_message(channel, message).await
            } else {
                self.rest.unpin_message(channel, message).await
            }
        }
        .await;
        let changed = match changed {
            // Gone already: the pin went with it.
            Err(RequestError::Discord {
                code: UNKNOWN_MESSAGE,
                ..
            }) if !pin => Ok(()),
            changed => changed,
        };
        tracing::debug!(
            channel_id = channel.get(),
            message_id = message.get(),
            pin,
            outcome = changed
                .as_ref()
                .map_or_else(RequestError::kind, |()| "done"),
            "pin"
        );
        if let Err(err) = &changed {
            self.failed(err);
        }
        Ok(changed?)
    }

    async fn send_pins_ack(&self, channel: ChannelId) {
        let mut wait = ACK_RETRY;
        for attempt in 1..=ACK_TRIES {
            let retry_after = match self.rest.ack_pins(channel).await {
                Ok(()) => {
                    tracing::debug!(target: "akari_core::acks", channel_id = channel.get(), "pins acked");
                    return;
                }
                Err(Failure::Held(held)) => Some(held),
                Err(failure) => {
                    let err = RequestError::from(failure);
                    self.failed(&err);
                    match err {
                        RequestError::RateLimited { retry_after } => retry_after,
                        err if err.is_transient() => None,
                        err => {
                            tracing::debug!(
                                target: "akari_core::acks",
                                channel_id = channel.get(),
                                outcome = err.kind(),
                                "pins ack refused"
                            );
                            return;
                        }
                    }
                }
            };
            if attempt == ACK_TRIES {
                break;
            }
            tokio::time::sleep(wait.max(retry_after.unwrap_or_default())).await;
            wait *= 2;
        }
        tracing::warn!(
            target: "akari_core::acks",
            channel_id = channel.get(),
            "pins ack given up"
        );
    }
}

impl Account {
    /// Asks for the first page of the channel's pins ([`Store::pins`], 25 a page) when its list
    /// isn't loaded in this connection, its first page failed, or Discord named a pin newer
    /// than the list has; otherwise nothing, as while a page loads or for a current list. Call
    /// it whenever the list shows and reads `NotLoaded`: on opening, and after a new session.
    /// A failure leaves the list `Failed` and is returned.
    pub async fn load_pins(&self, channel: ChannelId) -> Result<(), RequestError> {
        self.shared.load_pins(channel, PinsLoad::First).await
    }

    /// Asks for the page after the loaded pins while [`PinsPage::has_more`](crate::state::PinsPage)
    /// says so, also after a failed one; nothing otherwise.
    pub async fn load_more_pins(&self, channel: ChannelId) -> Result<(), RequestError> {
        self.shared.load_pins(channel, PinsLoad::More).await
    }

    /// Pins the message the row shows (a thread's starter row pins the parent's message),
    /// opening an archived thread first. Nothing changes in the store until the gateway says so.
    /// `Request(InvalidRequest)`, without a request, where
    /// [`MessageActions::pin`](crate::state::MessageActions) is false.
    pub async fn pin(&self, channel: ChannelId, message: MessageId) -> Result<(), PinError> {
        self.shared.change_pin(channel, message, true).await
    }

    /// Unpins the message, as [`Account::pin`] pins it; also one only the pins list has.
    /// A message that is gone counts as unpinned. Needs `MessageActions::unpin`.
    pub async fn unpin(&self, channel: ChannelId, message: MessageId) -> Result<(), PinError> {
        self.shared.change_pin(channel, message, false).await
    }

    /// The user sees the channel's pins: while [`Store::pins_unread`] says so, the dot goes at
    /// once and Discord is told, tried up to three times; nothing otherwise. Call it when the
    /// pins show, and again on `PinsUnreadChanged` while they do.
    pub fn ack_pins(&self, channel: ChannelId) {
        if !self.shared.store.ack_pins(channel) {
            return;
        }
        let shared = Arc::clone(&self.shared);
        self.shared.runtime.spawn(async move {
            shared.send_pins_ack(channel).await;
        });
    }
}
