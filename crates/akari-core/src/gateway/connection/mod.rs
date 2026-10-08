mod backlog;
mod task;

#[cfg(test)]
mod fake;
#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;
#[cfg(feature = "capture")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::{Mutex, mpsc, oneshot, watch};

use self::backlog::Backlog;
use self::task::Task;
use super::outgoing::{GatewayCommand, MAX_PAYLOAD};
use super::payload::{DecodeError, DispatchEvent};
use super::session::{Retry, Session, Timing};
use crate::error::TransportError;
use crate::{DiscordClient, Token};

/// A gateway connection running in the background. Dropping it ends the session like
/// [`Gateway::close`].
pub struct Gateway {
    control: watch::Sender<Mode>,
    events: Mutex<mpsc::UnboundedReceiver<Result<ConnectionEvent, GatewayError>>>,
    buffered: Arc<AtomicUsize>,
    outgoing: mpsc::Sender<Outgoing>,
    #[cfg(feature = "capture")]
    capture: Arc<AtomicBool>,
}

const COMMAND_QUEUE: usize = 16;

pub(crate) struct Outgoing {
    pub(crate) payload: String,
    pub(crate) sent: oneshot::Sender<Result<(), SendError>>,
}

impl Outgoing {
    pub(crate) fn refuse(self, err: SendError) {
        let _ = self.sent.send(Err(err));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Idle,
    Connected,
    Closed,
}

/// What a [`Gateway`] reports, in order.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConnectionEvent {
    /// A dispatch in Discord's order. `Ready` starts a new session, which makes anything an
    /// earlier session delivered stale; `Resumed` ends the replay after a reconnect.
    Dispatch(DispatchEvent),
    /// The connection dropped. The gateway reconnects after `delay`, resuming if `resume`.
    Reconnecting {
        resume: bool,
        delay: Duration,
        reason: DisconnectReason,
    },
    /// READY as received, decompressed, before it is decoded. Holds personal data and
    /// secrets such as `analytics_token`.
    #[cfg(feature = "capture")]
    CapturedReady(Vec<u8>),
}

/// Why a connection dropped.
#[derive(Debug)]
#[non_exhaustive]
pub enum DisconnectReason {
    /// Connecting failed, or the connection broke without a close frame.
    Transport(TransportError),
    ClosedByDiscord {
        code: u16,
    },
    /// Opcode 7.
    Requested,
    /// Opcode 9.
    InvalidSession,
    /// A heartbeat went unacknowledged.
    Zombie,
    /// Discord didn't send Hello in time.
    NoHello,
    /// The zstd stream was corrupt.
    Decompress,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GatewayError {
    /// Close code 4004: the token is invalid and the user has to log in again.
    #[error("Discord rejected the token")]
    AuthenticationFailed,
    /// A close code that rules out reconnecting (4010–4016).
    #[error("Discord refused the connection ({code})")]
    Rejected { code: u16 },
    #[error("a gateway message is larger than {limit} bytes")]
    MessageTooLarge { limit: usize },
    #[error("READY couldn't be decoded")]
    InvalidReady(#[source] DecodeError),
    #[error("the gateway connection is closed")]
    Closed,
    #[error("a gateway connection needs a Tokio runtime")]
    NoRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SendError {
    /// No ready session: before READY or RESUMED, while reconnecting, or after
    /// `disconnect()`.
    #[error("the gateway isn't connected")]
    NotConnected,
    /// Larger than the 15 KiB Discord accepts.
    #[error("the command is too large to send")]
    TooLarge,
    #[error("the gateway connection is closed")]
    Closed,
}

impl Gateway {
    pub(crate) fn start(
        client: DiscordClient,
        token: Token,
        timing: Timing,
    ) -> Result<Self, GatewayError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| GatewayError::NoRuntime)?;
        let (control, mode) = watch::channel(Mode::Idle);
        let (events, receiver) = mpsc::unbounded_channel();
        let buffered = Arc::new(AtomicUsize::new(0));
        let (outgoing, commands) = mpsc::channel(COMMAND_QUEUE);
        #[cfg(feature = "capture")]
        let capture = Arc::new(AtomicBool::new(false));
        let task = Task {
            client,
            token,
            timing,
            mode,
            events,
            buffered: buffered.clone(),
            backlog: Backlog::default(),
            outgoing: commands,
            #[cfg(feature = "capture")]
            capture: capture.clone(),
            session: Session::default(),
            retry: Retry::default(),
        };
        runtime.spawn(task.run());
        Ok(Self {
            control,
            events: Mutex::new(receiver),
            buffered,
            outgoing,
            #[cfg(feature = "capture")]
            capture,
        })
    }

    /// Connects, or reconnects after [`Gateway::disconnect`] and resumes the kept session.
    /// Does nothing while connected. Fails with [`GatewayError::Closed`] after `close()` or
    /// a fatal error.
    pub fn connect(&self) -> Result<(), GatewayError> {
        if self.control.is_closed() {
            return Err(GatewayError::Closed);
        }
        let mut result = Ok(());
        self.control.send_if_modified(|mode| match mode {
            Mode::Closed => {
                result = Err(GatewayError::Closed);
                false
            }
            Mode::Connected => false,
            Mode::Idle => {
                *mode = Mode::Connected;
                true
            }
        });
        result
    }

    /// Closes the connection with a resumable code (4000) and keeps the session, so the
    /// next `connect()` resumes it.
    pub fn disconnect(&self) {
        self.control.send_if_modified(|mode| {
            let connected = *mode == Mode::Connected;
            if connected {
                *mode = Mode::Idle;
            }
            connected
        });
    }

    /// Ends the session (close code 1000). `next()` returns [`GatewayError::Closed`] once
    /// the socket is closed.
    pub fn close(&self) {
        self.control.send_if_modified(|mode| {
            let open = *mode != Mode::Closed;
            *mode = Mode::Closed;
            open
        });
    }

    /// The next event. Unread events are buffered; the connection never waits for the
    /// reader. After the gateway ends, returns its fatal error once, then `Closed`.
    pub async fn next(&self) -> Result<ConnectionEvent, GatewayError> {
        match self.events.lock().await.recv().await {
            Some(event) => {
                self.buffered.fetch_sub(1, Ordering::Relaxed);
                event
            }
            None => Err(GatewayError::Closed),
        }
    }

    /// Writes `command` to the current connection within the rate limit and resolves once
    /// it is written.
    pub async fn send(&self, command: GatewayCommand) -> Result<(), SendError> {
        self.send_payload(command.to_payload()).await
    }

    /// Delivers the next READY's raw JSON as [`ConnectionEvent::CapturedReady`].
    #[cfg(feature = "capture")]
    pub fn capture_next_ready(&self) {
        self.capture.store(true, Ordering::Release);
    }

    pub(crate) async fn send_payload(&self, payload: String) -> Result<(), SendError> {
        if payload.len() > MAX_PAYLOAD {
            return Err(SendError::TooLarge);
        }
        let (sent, result) = oneshot::channel();
        self.outgoing
            .send(Outgoing { payload, sent })
            .await
            .map_err(|_| SendError::Closed)?;
        result.await.unwrap_or(Err(SendError::Closed))
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.close();
    }
}

impl fmt::Debug for Gateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gateway")
            .field("mode", &*self.control.borrow())
            .finish_non_exhaustive()
    }
}
