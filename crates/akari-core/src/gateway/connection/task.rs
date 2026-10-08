use std::sync::Arc;
#[cfg(feature = "capture")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep_until};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{self, Message};

use super::backlog::Backlog;
use super::{ConnectionEvent, DisconnectReason, GatewayError, Mode, Outgoing, SendError};
use crate::error::{TransportError, TransportErrorKind};
use crate::gateway::decompress::{DecompressError, ZstdStream};
use crate::gateway::outgoing;
use crate::gateway::payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
use crate::gateway::session::{
    AfterClose, Connection, Handshake, Retry, Session, Tick, Timing, after_close,
    invalid_session_floor,
};
use crate::ws::{self, WsStream};
use crate::{DiscordClient, Token, random};

// Applies compressed and decompressed; READY of a big account is a few MiB.
pub(super) const MAX_MESSAGE: usize = 64 * 1024 * 1024;
// 1000 and 1001 end the session on Discord's side; any other code keeps it resumable.
const NORMAL: u16 = 1000;
const RESUMABLE: u16 = 4000;

pub(super) struct Task {
    pub(super) client: DiscordClient,
    pub(super) token: Token,
    pub(super) timing: Timing,
    pub(super) mode: watch::Receiver<Mode>,
    pub(super) events: mpsc::UnboundedSender<Result<ConnectionEvent, GatewayError>>,
    pub(super) buffered: Arc<AtomicUsize>,
    pub(super) backlog: Backlog,
    pub(super) outgoing: mpsc::Receiver<Outgoing>,
    #[cfg(feature = "capture")]
    pub(super) capture: Arc<AtomicBool>,
    pub(super) session: Session,
    pub(super) retry: Retry,
    #[cfg(test)]
    pub(super) writes: super::Writes,
}

enum End {
    Closed,
    Idle,
    Reconnect {
        resume: bool,
        reason: DisconnectReason,
        floor: Duration,
        healthy: bool,
    },
    Fatal(GatewayError),
}

// None: Discord already closed the socket.
type Exit = (End, Option<u16>);

fn lost(reason: DisconnectReason, healthy: bool) -> End {
    End::Reconnect {
        resume: true,
        reason,
        floor: Duration::ZERO,
        healthy,
    }
}

fn broken(message: &'static str) -> DisconnectReason {
    DisconnectReason::Transport(TransportError::new(TransportErrorKind::Connect, message))
}

impl Task {
    pub(super) async fn run(mut self) {
        let mut backoff = None;
        let mut floor = None;
        while self.idle(backoff.take(), floor.take()).await {
            match self.connection().await {
                End::Closed => break,
                End::Idle => {}
                End::Fatal(err) => {
                    tracing::warn!(error = %err, "the gateway connection ended");
                    self.send(Err(err));
                    break;
                }
                End::Reconnect { .. } if *self.mode.borrow() == Mode::Closed => break,
                End::Reconnect {
                    resume,
                    reason,
                    floor: floor_wait,
                    healthy,
                } => {
                    if !resume {
                        self.session.forget();
                    }
                    let now = Instant::now();
                    let delay = self
                        .retry
                        .delay(healthy, &self.timing, random::unit())
                        .max(floor_wait);
                    let resume = self.session.can_resume();
                    tracing::info!(?reason, resume, ?delay, "reconnecting to the gateway");
                    self.emit(ConnectionEvent::Reconnecting {
                        resume,
                        delay,
                        reason,
                    });
                    backoff = Some(now + delay);
                    floor = (!floor_wait.is_zero()).then(|| now + floor_wait);
                }
            }
        }
    }

    async fn idle(&mut self, mut backoff: Option<Instant>, floor: Option<Instant>) -> bool {
        loop {
            let until = match *self.mode.borrow_and_update() {
                Mode::Closed => return false,
                // connect() after disconnect() skips any backoff still pending, but op 9's
                // wait still applies.
                Mode::Idle => {
                    backoff = None;
                    None
                }
                Mode::Connected => {
                    let until = backoff.max(floor);
                    if until.is_none_or(|until| Instant::now() >= until) {
                        return true;
                    }
                    until
                }
            };
            let sleep = sleep_until(until.unwrap_or_else(Instant::now));
            tokio::select! {
                changed = self.mode.changed() => {
                    if changed.is_err() {
                        return false;
                    }
                }
                () = sleep, if until.is_some() => {}
                Some(outgoing) = self.outgoing.recv() => outgoing.refuse(SendError::NotConnected),
            }
        }
    }

    async fn connection(&mut self) -> End {
        let Ok(mut zstd) = ZstdStream::new(MAX_MESSAGE) else {
            return lost(DisconnectReason::Decompress, false);
        };
        // Scoped so the connect future and its borrow of the client end here.
        let mut socket = {
            let url = self.session.connect_url(self.client.gateway_url());
            let headers = [
                (
                    "user-agent",
                    self.client.properties().browser_user_agent.as_str(),
                ),
                ("origin", self.client.endpoints().origin.as_str()),
            ];
            let connect = ws::connect(url.as_str(), &headers, self.client.tls(), MAX_MESSAGE);
            tokio::pin!(connect);
            loop {
                tokio::select! {
                    result = &mut connect => match result {
                        Ok(socket) => break socket,
                        Err(err) => {
                            self.session.unreachable();
                            return lost(DisconnectReason::Transport(err), false);
                        }
                    },
                    changed = self.mode.changed() => {
                        let mode = if changed.is_err() {
                            Mode::Closed
                        } else {
                            *self.mode.borrow_and_update()
                        };
                        match mode {
                            Mode::Connected => {}
                            Mode::Idle => return End::Idle,
                            Mode::Closed => return End::Closed,
                        }
                    }
                    Some(outgoing) = self.outgoing.recv() => outgoing.refuse(SendError::NotConnected),
                }
            }
        };
        let mut link = Connection::new(Instant::now(), &self.timing);
        let (end, code) = self.drive(&mut socket, &mut zstd, &mut link).await;
        if !link.got_hello() && matches!(end, End::Reconnect { .. }) {
            self.session.unreachable();
        }
        self.close(&mut socket, code).await;
        end
    }

    async fn drive(
        &mut self,
        socket: &mut WsStream,
        zstd: &mut ZstdStream,
        link: &mut Connection,
    ) -> Exit {
        loop {
            let now = Instant::now();
            let ready = link.is_ready();
            let budget = ready && link.limiter.allows_command(now);
            let refill = (ready && !budget).then(|| link.limiter.next_free());
            tokio::select! {
                biased;
                changed = self.mode.changed() => {
                    let mode = if changed.is_err() {
                        Mode::Closed
                    } else {
                        *self.mode.borrow_and_update()
                    };
                    match mode {
                        Mode::Connected => {}
                        Mode::Idle => return (End::Idle, Some(RESUMABLE)),
                        Mode::Closed => return (End::Closed, Some(NORMAL)),
                    }
                }
                () = sleep_until(link.deadline()) => match link.tick(Instant::now()) {
                    Tick::NotYet => {}
                    Tick::Heartbeat => {
                        let beat = outgoing::heartbeat(self.session.seq());
                        if let Err(exit) = self.write(socket, link, beat).await {
                            return exit;
                        }
                    }
                    Tick::Zombie => {
                        return (self.lost(link, DisconnectReason::Zombie), Some(RESUMABLE));
                    }
                    Tick::NoHello => {
                        return (self.lost(link, DisconnectReason::NoHello), Some(RESUMABLE));
                    }
                },
                message = socket.next() => {
                    if let Some(exit) = self.receive(socket, zstd, link, message).await {
                        return exit;
                    }
                }
                Some(outgoing) = self.outgoing.recv(), if !ready || budget => {
                    if !ready {
                        outgoing.refuse(SendError::NotConnected);
                        continue;
                    }
                    let Outgoing { payload, sent } = outgoing;
                    if let Err(exit) = self.write(socket, link, payload).await {
                        let _ = sent.send(Err(SendError::NotConnected));
                        return exit;
                    }
                    let _ = sent.send(Ok(()));
                }
                () = sleep_until(refill.unwrap_or(now)), if refill.is_some() => {}
            }
        }
    }

    async fn receive(
        &mut self,
        socket: &mut WsStream,
        zstd: &mut ZstdStream,
        link: &mut Connection,
        message: Option<Result<Message, tungstenite::Error>>,
    ) -> Option<Exit> {
        let event = match message {
            Some(Ok(Message::Binary(data))) => {
                match zstd.decompress(&data, |json| self.decode(json)) {
                    Ok(event) => event,
                    // Fatal: a resume would replay the same message.
                    Err(DecompressError::TooLarge { limit }) => {
                        let end = End::Fatal(GatewayError::MessageTooLarge { limit });
                        return Some((end, Some(NORMAL)));
                    }
                    Err(DecompressError::Zstd(err)) => {
                        tracing::warn!(error = %err, "the gateway's zstd stream broke");
                        return Some((
                            self.lost(link, DisconnectReason::Decompress),
                            Some(RESUMABLE),
                        ));
                    }
                }
            }
            // Without transport compression, or if Discord ignores the request for it.
            Some(Ok(Message::Text(text))) => self.decode(text.as_bytes()),
            Some(Ok(Message::Close(frame))) => {
                let code = frame.map(|frame| u16::from(frame.code));
                return Some((self.closed(link, code), None));
            }
            Some(Ok(_)) => return None,
            Some(Err(tungstenite::Error::Capacity(_))) => {
                let end = End::Fatal(GatewayError::MessageTooLarge { limit: MAX_MESSAGE });
                return Some((end, Some(NORMAL)));
            }
            Some(Err(err)) => {
                let reason = DisconnectReason::Transport(TransportError::from_tungstenite(err));
                return Some((self.lost(link, reason), None));
            }
            None => return Some((self.lost(link, broken("the connection closed")), None)),
        };
        match event {
            Ok(event) => self.handle(socket, link, event).await,
            Err(err) => {
                let Some((seq, event)) = err.dispatch() else {
                    // serde's message can quote the payload, so it isn't logged.
                    tracing::warn!("skipping an unreadable gateway message");
                    return None;
                };
                self.session.dispatched(seq);
                if event != "READY" {
                    tracing::warn!(event, seq, "skipping a dispatch that failed to decode");
                    return None;
                }
                Some((End::Fatal(GatewayError::InvalidReady(err)), Some(NORMAL)))
            }
        }
    }

    async fn handle(
        &mut self,
        socket: &mut WsStream,
        link: &mut Connection,
        event: GatewayEvent,
    ) -> Option<Exit> {
        match event {
            GatewayEvent::Hello(hello) => {
                self.session.reached();
                if link.hello(&hello, Instant::now(), random::unit()) {
                    let handshake = match self.session.handshake() {
                        Handshake::Identify => {
                            outgoing::identify(&self.token, self.client.properties())
                        }
                        Handshake::Resume { session_id, seq } => {
                            outgoing::resume(&self.token, session_id, seq)
                        }
                    };
                    return self.write(socket, link, handshake).await.err();
                }
            }
            GatewayEvent::HeartbeatAck => link.acked(),
            GatewayEvent::Heartbeat => {
                let beat = outgoing::heartbeat(self.session.seq());
                return self.write(socket, link, beat).await.err();
            }
            GatewayEvent::Dispatch { seq, event } => {
                self.session.dispatched(seq);
                match &event {
                    DispatchEvent::Ready(ready) => {
                        self.session.ready(ready, self.client.allows_plaintext());
                        link.ready(Instant::now());
                    }
                    DispatchEvent::Resumed => link.ready(Instant::now()),
                    DispatchEvent::Other(_) => {}
                }
                self.emit(ConnectionEvent::Dispatch(event));
            }
            GatewayEvent::Reconnect => {
                return Some((
                    self.lost(link, DisconnectReason::Requested),
                    Some(RESUMABLE),
                ));
            }
            GatewayEvent::InvalidSession { resumable } => {
                let end = End::Reconnect {
                    resume: self.session.resume_after_invalid_session(resumable),
                    reason: DisconnectReason::InvalidSession,
                    floor: invalid_session_floor(&self.timing, random::unit()),
                    healthy: link.healthy(Instant::now(), &self.timing),
                };
                return Some((end, Some(RESUMABLE)));
            }
            GatewayEvent::Unknown { op } => {
                tracing::debug!(op, "ignoring an unknown gateway opcode");
            }
        }
        None
    }

    async fn write(
        &self,
        socket: &mut WsStream,
        link: &mut Connection,
        payload: String,
    ) -> Result<(), Exit> {
        let now = Instant::now();
        link.limiter.record(now);
        #[cfg(test)]
        self.writes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((now, payload.clone()));
        socket.send(Message::text(payload)).await.map_err(|err| {
            let reason = DisconnectReason::Transport(TransportError::from_tungstenite(err));
            (self.lost(link, reason), None)
        })
    }

    fn closed(&self, link: &Connection, code: Option<u16>) -> End {
        let reason = match code {
            Some(code) => DisconnectReason::ClosedByDiscord { code },
            None => broken("closed without a code"),
        };
        match after_close(code) {
            AfterClose::Resume => self.lost(link, reason),
            AfterClose::Identify => End::Reconnect {
                resume: false,
                reason,
                floor: Duration::ZERO,
                healthy: link.healthy(Instant::now(), &self.timing),
            },
            AfterClose::AuthenticationFailed => End::Fatal(GatewayError::AuthenticationFailed),
            AfterClose::Rejected { code } => End::Fatal(GatewayError::Rejected { code }),
        }
    }

    fn lost(&self, link: &Connection, reason: DisconnectReason) -> End {
        lost(reason, link.healthy(Instant::now(), &self.timing))
    }

    // Messages read after this are dropped unseen, so their seq isn't counted and a resume
    // replays them.
    async fn close(&mut self, socket: &mut WsStream, code: Option<u16>) {
        let frame = code.map(|code| CloseFrame {
            code: CloseCode::from(code),
            reason: "".into(),
        });
        let handshake = tokio::time::timeout(self.timing.close_timeout, async {
            if socket.close(frame).await.is_ok() {
                while let Some(Ok(_)) = socket.next().await {}
            }
        });
        tokio::pin!(handshake);
        // A close() that ends a resumable close cuts it short; one already closing finishes.
        let watch_mode = *self.mode.borrow() != Mode::Closed;
        loop {
            tokio::select! {
                biased;
                _ = &mut handshake => return,
                changed = self.mode.changed(), if watch_mode => {
                    if changed.is_err() || *self.mode.borrow_and_update() == Mode::Closed {
                        return;
                    }
                }
                Some(outgoing) = self.outgoing.recv() => outgoing.refuse(SendError::NotConnected),
            }
        }
    }

    fn decode(&mut self, json: &[u8]) -> Result<GatewayEvent, DecodeError> {
        let event = decode(json);
        #[cfg(feature = "capture")]
        if is_ready(&event) && self.capture.swap(false, Ordering::AcqRel) {
            self.emit(ConnectionEvent::CapturedReady(json.to_vec()));
        }
        event
    }

    fn emit(&mut self, event: ConnectionEvent) {
        self.send(Ok(event));
    }

    // The connection must never wait for the reader, so the queue is unbounded.
    fn send(&mut self, event: Result<ConnectionEvent, GatewayError>) {
        let buffered = self.buffered.fetch_add(1, Ordering::Relaxed) + 1;
        if self.backlog.warns_at(buffered) {
            tracing::warn!(buffered, "gateway events are piling up unread");
        }
        let _ = self.events.send(event);
    }
}

// Also a READY that fails to decode: that's the one worth checking.
#[cfg(feature = "capture")]
fn is_ready(event: &Result<GatewayEvent, DecodeError>) -> bool {
    match event {
        Ok(GatewayEvent::Dispatch {
            event: DispatchEvent::Ready(_),
            ..
        }) => true,
        Ok(_) => false,
        Err(err) => err.dispatch().is_some_and(|(_, name)| name == "READY"),
    }
}
