mod crypto;
mod protocol;

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use self::crypto::{RemoteAuthKey, nonce_proof};
use self::protocol::{ClientPacket, ServerPacket, parse_user};
use super::{CaptchaChallenge, LoginError, LoginSuccess};
use crate::heartbeat::{Beat, Heartbeat};
use crate::model::{Snowflake, UserMarker};
use crate::rest::{CaptchaSolution, RequestExtras, RestError};
use crate::ws::{self, WsStream};
use crate::{DiscordClient, Secret, Token, backoff, random};

/// The account that scanned the QR code, for a "check your phone" screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedUser {
    pub id: Snowflake<UserMarker>,
    /// `"0"` for users on the new username system.
    pub discriminator: String,
    /// Avatar hash, `None` for the default avatar.
    pub avatar: Option<String>,
    pub username: String,
}

/// What a QR code login reports, in order.
#[derive(Debug)]
pub enum QrEvent {
    /// Show this URL as a QR code, replacing any earlier one. Codes expire after a few
    /// minutes and are replaced on their own.
    Code {
        url: String,
    },
    /// The code was scanned; the user confirms on their phone.
    Scanned(ScannedUser),
    /// The user cancelled on their phone; a new code follows.
    CancelledOnPhone,
    /// Show the challenge, then call [`QrLogin::solve_captcha`].
    Captcha(CaptchaChallenge),
    Done(LoginSuccess),
}

/// A QR code login running in the background. Read its events with [`QrLogin::next`];
/// dropping it cancels the login.
pub struct QrLogin {
    shared: Arc<Shared>,
    solutions: mpsc::Sender<String>,
    cancel: CancellationToken,
}

// Events wait here instead of in a channel so the session never waits for the UI, and a
// new code can replace one the UI hasn't shown yet.
#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    notify: Notify,
    awaiting_captcha: AtomicBool,
}

// `finished` shares the lock with the events, so a reader never sees the end before the
// last event.
#[derive(Default)]
struct Queue {
    events: VecDeque<Result<QrEvent, LoginError>>,
    finished: bool,
}

enum Taken {
    Event(Result<QrEvent, LoginError>),
    Finished,
    Empty,
}

// Ends the login when the task stops, also by panicking, so `next()` never waits forever.
struct Ending(Arc<Shared>);

impl Drop for Ending {
    fn drop(&mut self) {
        let mut queue = self.0.lock();
        if !queue.finished {
            let err = LoginError::RemoteAuth("the QR code login stopped unexpectedly".to_owned());
            queue.events.push_back(Err(err));
            queue.finished = true;
        }
        drop(queue);
        self.0.notify.notify_waiters();
    }
}

const QR_PREFIX: &str = "https://discord.com/ra/";
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FAILURES: u32 = 3;
const RETRY_BASE: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);
// A session shorter than this restarts with backoff; a code normally lives a few minutes.
const HEALTHY_SESSION: Duration = Duration::from_secs(30);
// Remote auth packets are under 1 KiB.
const MAX_PACKET: usize = 16 * 1024;
const MAX_QUEUED: usize = 8;

impl QrLogin {
    pub(crate) fn start(client: DiscordClient) -> Result<Self, LoginError> {
        Self::start_with(client, HELLO_TIMEOUT)
    }

    fn start_with(client: DiscordClient, hello_timeout: Duration) -> Result<Self, LoginError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| LoginError::NoRuntime)?;
        let shared = Arc::new(Shared::default());
        let cancel = CancellationToken::new();
        let (solutions, receiver) = mpsc::channel(1);
        let task = Task {
            client,
            shared: shared.clone(),
            cancel: cancel.clone(),
            solutions: receiver,
            hello_timeout,
        };
        runtime.spawn(task.run());
        Ok(Self {
            shared,
            solutions,
            cancel,
        })
    }

    /// The next event. After [`QrEvent::Done`] or an error the login is over, and further
    /// calls return [`LoginError::NoPendingStep`].
    pub async fn next(&self) -> Result<QrEvent, LoginError> {
        loop {
            if self.cancel.is_cancelled() {
                return Err(LoginError::Cancelled);
            }
            let notified = self.shared.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            match self.shared.take() {
                Taken::Event(event) => return event,
                Taken::Finished => return Err(LoginError::NoPendingStep),
                Taken::Empty => {}
            }
            tokio::select! {
                () = self.cancel.cancelled() => return Err(LoginError::Cancelled),
                () = notified => {}
            }
        }
    }

    /// Answers a [`QrEvent::Captcha`]; the login continues with the next event.
    pub async fn solve_captcha(&self, solution: String) -> Result<(), LoginError> {
        if self.cancel.is_cancelled() {
            return Err(LoginError::Cancelled);
        }
        if !self.shared.awaiting_captcha.swap(false, Ordering::AcqRel) {
            return Err(LoginError::NoPendingStep);
        }
        self.solutions
            .send(solution)
            .await
            .map_err(|_| LoginError::Cancelled)
    }

    /// Closes the connection and ends the login.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for QrLogin {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl fmt::Debug for QrLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QrLogin")
            .field("finished", &self.shared.lock().finished)
            .field("cancelled", &self.cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, event: Result<QrEvent, LoginError>) {
        let mut queue = self.lock();
        if matches!(event, Ok(QrEvent::Code { .. })) {
            queue
                .events
                .retain(|queued| !matches!(queued, Ok(QrEvent::Code { .. })));
        }
        // The end of the login is always pushed last, so dropping the oldest never loses it.
        while queue.events.len() >= MAX_QUEUED {
            queue.events.pop_front();
        }
        queue.events.push_back(event);
        drop(queue);
        self.notify.notify_waiters();
    }

    fn take(&self) -> Taken {
        let mut queue = self.lock();
        match queue.events.pop_front() {
            Some(event) => Taken::Event(event),
            None if queue.finished => Taken::Finished,
            None => Taken::Empty,
        }
    }

    fn finish(&self) {
        self.lock().finished = true;
        self.notify.notify_waiters();
    }
}

struct Task {
    client: DiscordClient,
    shared: Arc<Shared>,
    cancel: CancellationToken,
    solutions: mpsc::Receiver<String>,
    hello_timeout: Duration,
}

enum End {
    Done,
    Cancelled,
    // A normal user action: the next code shows at once and doesn't count as a quick restart.
    CancelledOnPhone,
    Fatal(LoginError),
    // `failure` is set when the session never showed a code; several in a row end the login.
    Restart { failure: Option<LoginError> },
}

enum Outcome {
    End(End),
    Exchange(Secret),
}

struct Session {
    key: RemoteAuthKey,
    // Until hello arrives there is no heartbeat, so this bounds a silent connection.
    hello_deadline: Instant,
    heartbeat: Option<Heartbeat>,
    code_shown: bool,
    user: Option<ScannedUser>,
}

#[derive(Deserialize)]
struct ExchangeResponse {
    encrypted_token: String,
}

impl Task {
    async fn run(mut self) {
        let _ending = Ending(self.shared.clone());
        let mut failures = 0;
        let mut quick_restarts = 0;
        loop {
            let started = Instant::now();
            let failure = match self.session().await {
                End::Done | End::Cancelled => break,
                End::CancelledOnPhone => {
                    failures = 0;
                    continue;
                }
                End::Fatal(err) => {
                    self.shared.push(Err(err));
                    break;
                }
                End::Restart { failure } => failure,
            };
            match failure {
                Some(err) => {
                    failures += 1;
                    if failures >= MAX_FAILURES {
                        self.shared.push(Err(err));
                        break;
                    }
                }
                None => failures = 0,
            }
            if started.elapsed() >= HEALTHY_SESSION {
                quick_restarts = 0;
                continue;
            }
            let delay = backoff::delay(quick_restarts, RETRY_BASE, RETRY_MAX, random::unit());
            quick_restarts += 1;
            tokio::select! {
                () = self.cancel.cancelled() => break,
                () = tokio::time::sleep(delay) => {}
            }
        }
        self.shared.finish();
    }

    async fn session(&mut self) -> End {
        let key = match tokio::task::spawn_blocking(RemoteAuthKey::generate).await {
            Ok(Ok(key)) => key,
            _ => return End::Fatal(LoginError::RemoteAuth("couldn't create a key".to_owned())),
        };
        let endpoints = self.client.endpoints();
        let headers = [
            ("origin", endpoints.origin.as_str()),
            (
                "user-agent",
                self.client.properties().browser_user_agent.as_str(),
            ),
        ];
        let connect = ws::connect(
            &endpoints.remote_auth,
            &headers,
            self.client.tls(),
            MAX_PACKET,
        );
        let mut socket = tokio::select! {
            () = self.cancel.cancelled() => return End::Cancelled,
            result = connect => match result {
                Ok(socket) => socket,
                Err(err) => return End::Restart { failure: Some(LoginError::Network(err)) },
            },
        };

        let mut session = Session {
            key,
            hello_deadline: Instant::now() + self.hello_timeout,
            heartbeat: None,
            code_shown: false,
            user: None,
        };
        let outcome = self.drive(&mut socket, &mut session).await;
        let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
        match outcome {
            Outcome::End(end) => end,
            Outcome::Exchange(ticket) => self.exchange(&session, &ticket).await,
        }
    }

    async fn drive(&mut self, socket: &mut WsStream, session: &mut Session) -> Outcome {
        loop {
            let deadline = match &session.heartbeat {
                Some(heartbeat) => heartbeat.deadline(),
                None => session.hello_deadline,
            };
            let message = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Outcome::End(End::Cancelled),
                () = tokio::time::sleep_until(deadline) => {
                    let Some(heartbeat) = &mut session.heartbeat else {
                        return session.lost("the gateway didn't say hello");
                    };
                    let beat = heartbeat.poll(Instant::now());
                    match beat {
                        Beat::Send if send(socket, &ClientPacket::Heartbeat).await => {}
                        Beat::NotYet => {}
                        Beat::Send | Beat::Zombie => return session.lost("the connection stopped answering"),
                    }
                    continue;
                }
                message = socket.next() => message,
            };
            let text = match message {
                Some(Ok(Message::Text(text))) => text,
                Some(Ok(Message::Close(frame))) => {
                    return session.closed(frame.map(|frame| u16::from(frame.code)));
                }
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => return session.closed(None),
            };
            // serde's message could quote the packet, so it isn't logged.
            let Ok(packet) = serde_json::from_str::<ServerPacket>(&text) else {
                tracing::warn!("ignoring an unreadable remote auth packet");
                continue;
            };
            match packet {
                ServerPacket::Hello { heartbeat_interval } => {
                    session.heartbeat = Some(Heartbeat::start(
                        Duration::from_millis(heartbeat_interval),
                        Instant::now(),
                        random::unit(),
                    ));
                    let key = session.key.encoded_public_key();
                    let init = ClientPacket::Init {
                        encoded_public_key: &key,
                    };
                    if !send(socket, &init).await {
                        return session.lost("the connection broke");
                    }
                }
                ServerPacket::NonceProof { encrypted_nonce } => {
                    let Ok(nonce) = session.key.decrypt(&encrypted_nonce) else {
                        return session.lost("couldn't decrypt the nonce");
                    };
                    let proof = nonce_proof(&nonce);
                    if !send(socket, &ClientPacket::NonceProof { nonce: &proof }).await {
                        return session.lost("the connection broke");
                    }
                }
                ServerPacket::PendingRemoteInit { fingerprint } => {
                    if fingerprint != session.key.fingerprint() {
                        return session.lost("the gateway answered with a different key");
                    }
                    session.code_shown = true;
                    self.shared.push(Ok(QrEvent::Code {
                        url: format!("{QR_PREFIX}{fingerprint}"),
                    }));
                }
                ServerPacket::PendingTicket {
                    encrypted_user_payload,
                } => {
                    let user = session
                        .key
                        .decrypt(&encrypted_user_payload)
                        .ok()
                        .and_then(|payload| String::from_utf8(payload.to_vec()).ok())
                        .and_then(|payload| parse_user(&payload));
                    match user {
                        Some(user) => {
                            self.shared.push(Ok(QrEvent::Scanned(user.clone())));
                            session.user = Some(user);
                        }
                        None => return session.lost("the gateway sent an unreadable user"),
                    }
                }
                ServerPacket::PendingLogin { ticket } => return Outcome::Exchange(ticket),
                ServerPacket::Cancel => {
                    self.shared.push(Ok(QrEvent::CancelledOnPhone));
                    return Outcome::End(End::CancelledOnPhone);
                }
                ServerPacket::HeartbeatAck => {
                    if let Some(heartbeat) = &mut session.heartbeat {
                        heartbeat.acked();
                    }
                }
                ServerPacket::Unknown => {}
            }
        }
    }

    async fn exchange(&mut self, session: &Session, ticket: &Secret) -> End {
        let Some(user) = &session.user else {
            return End::Fatal(LoginError::UnexpectedResponse);
        };
        let body = json!({"ticket": ticket.expose()});
        let mut solution: Option<CaptchaSolution> = None;
        loop {
            let fingerprint = tokio::select! {
                () = self.cancel.cancelled() => return End::Cancelled,
                fingerprint = self.client.fingerprint() => fingerprint,
            };
            let extras = RequestExtras {
                fingerprint,
                captcha: solution.as_ref(),
                ..RequestExtras::default()
            };
            let request = self.client.rest().post_json::<_, ExchangeResponse>(
                "users/@me/remote-auth/login",
                &body,
                &extras,
            );
            let response = tokio::select! {
                () = self.cancel.cancelled() => return End::Cancelled,
                response = request => response,
            };
            match response {
                Ok(response) => {
                    let token = session
                        .key
                        .decrypt(&response.encrypted_token)
                        .ok()
                        .and_then(|token| String::from_utf8(token.to_vec()).ok());
                    let Some(token) = token else {
                        return End::Fatal(LoginError::UnexpectedResponse);
                    };
                    self.shared.push(Ok(QrEvent::Done(LoginSuccess {
                        user_id: user.id,
                        token: Token::new(token),
                        password_update_required: false,
                    })));
                    return End::Done;
                }
                Err(RestError::Captcha(challenge)) => {
                    let challenge = *challenge;
                    self.shared.awaiting_captcha.store(true, Ordering::Release);
                    self.shared.push(Ok(QrEvent::Captcha(challenge.clone())));
                    let key = tokio::select! {
                        () = self.cancel.cancelled() => return End::Cancelled,
                        key = self.solutions.recv() => match key {
                            Some(key) => key,
                            None => return End::Cancelled,
                        },
                    };
                    solution = Some(CaptchaSolution {
                        key: Secret::new(key),
                        rqtoken: challenge.rqtoken,
                        session_id: challenge.session_id,
                    });
                }
                Err(err) => return End::Fatal(LoginError::from_rest(err)),
            }
        }
    }
}

impl Session {
    fn lost(&self, reason: &str) -> Outcome {
        let failure = (!self.code_shown).then(|| LoginError::RemoteAuth(reason.to_owned()));
        Outcome::End(End::Restart { failure })
    }

    fn closed(&self, code: Option<u16>) -> Outcome {
        // A timeout (4003) after the code was shown is the normal end of a code's life.
        match code {
            Some(code) => self.lost(&format!("the gateway closed the connection ({code})")),
            None => self.lost("the connection broke"),
        }
    }
}

async fn send(socket: &mut WsStream, packet: &ClientPacket<'_>) -> bool {
    match serde_json::to_string(packet) {
        Ok(text) => socket.send(Message::text(text)).await.is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login() -> QrLogin {
        let (solutions, _) = mpsc::channel(1);
        QrLogin {
            shared: Arc::new(Shared::default()),
            solutions,
            cancel: CancellationToken::new(),
        }
    }

    struct NoStore;

    impl crate::TokenStore for NoStore {
        fn load(&self, _: Snowflake<UserMarker>) -> Result<Option<Token>, crate::TokenStoreError> {
            Ok(None)
        }
        fn save(&self, _: Snowflake<UserMarker>, _: &Token) -> Result<(), crate::TokenStoreError> {
            Ok(())
        }
        fn delete(&self, _: Snowflake<UserMarker>) -> Result<(), crate::TokenStoreError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_gateway_that_never_says_hello_fails_the_session() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (connections, mut accepted) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                if let Ok(socket) = tokio_tungstenite::accept_async(stream).await {
                    let _ = connections.send(socket);
                }
            }
        });
        let host = crate::properties::HostInfo {
            os: crate::properties::DesktopOs::Linux,
            os_version: "6.8.0".to_owned(),
            arch: crate::properties::Arch::X64,
            system_locale: "en-US".to_owned(),
        };
        let properties = crate::properties::ClientProperties::desktop(
            &host,
            &crate::properties::ClientBuild::current(crate::properties::DesktopOs::Linux),
        );
        let endpoints = crate::Endpoints {
            remote_auth: format!("ws://{address}/?v=2"),
            allow_plaintext: true,
            ..crate::Endpoints::default()
        };
        let client =
            DiscordClient::with_endpoints(properties, Arc::new(NoStore), endpoints).unwrap();

        let qr = QrLogin::start_with(client, Duration::from_millis(50)).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), qr.next()).await;

        assert!(
            matches!(result, Ok(Err(LoginError::RemoteAuth(_)))),
            "{result:?}"
        );
        let mut silent = Vec::new();
        while let Ok(socket) = accepted.try_recv() {
            silent.push(socket);
        }
        assert_eq!(silent.len(), MAX_FAILURES as usize);
    }

    #[tokio::test]
    async fn an_unread_queue_stays_small_and_keeps_the_newest_events() {
        let qr = login();
        for _ in 0..1000 {
            qr.shared.push(Ok(QrEvent::CancelledOnPhone));
        }
        qr.shared.push(Err(LoginError::Expired));
        qr.shared.finish();

        assert!(qr.shared.lock().events.len() <= MAX_QUEUED);
        let end = loop {
            if let Err(err) = qr.next().await {
                break err;
            }
        };
        assert!(matches!(end, LoginError::Expired), "{end:?}");
    }

    #[tokio::test]
    async fn events_queued_before_the_end_are_still_delivered() {
        let qr = login();
        qr.shared.push(Err(LoginError::Expired));
        qr.shared.finish();

        assert!(matches!(qr.next().await, Err(LoginError::Expired)));
        assert!(matches!(qr.next().await, Err(LoginError::NoPendingStep)));
    }

    #[tokio::test]
    async fn a_task_that_panics_still_ends_the_login() {
        let qr = login();
        let shared = qr.shared.clone();

        let task = tokio::spawn(async move {
            let _ending = Ending(shared);
            panic!("the QR task broke");
        });

        assert!(task.await.is_err());
        assert!(matches!(qr.next().await, Err(LoginError::RemoteAuth(_))));
        assert!(matches!(qr.next().await, Err(LoginError::NoPendingStep)));
    }

    #[tokio::test]
    async fn a_task_that_ends_normally_adds_no_error() {
        let qr = login();
        let shared = qr.shared.clone();

        {
            let _ending = Ending(shared.clone());
            shared.push(Err(LoginError::Expired));
            shared.finish();
        }

        assert!(matches!(qr.next().await, Err(LoginError::Expired)));
        assert!(matches!(qr.next().await, Err(LoginError::NoPendingStep)));
    }
}
