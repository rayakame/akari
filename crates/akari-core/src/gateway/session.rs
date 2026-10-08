use std::time::Duration;

use reqwest::Url;
use tokio::time::Instant;

use super::hello::Hello;
use super::limiter::CommandLimiter;
use super::ready::Ready;
use crate::backoff;
use crate::heartbeat::{Beat, Heartbeat};

const API_VERSION: &str = "9";

#[derive(Debug, Clone)]
pub(crate) struct Timing {
    pub(crate) hello_timeout: Duration,
    pub(crate) close_timeout: Duration,
    pub(crate) retry_base: Duration,
    pub(crate) retry_max: Duration,
    pub(crate) invalid_session_min: Duration,
    pub(crate) invalid_session_max: Duration,
    pub(crate) healthy: Duration,
    pub(crate) rate_window: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            hello_timeout: Duration::from_secs(20),
            close_timeout: Duration::from_secs(2),
            retry_base: Duration::from_secs(1),
            retry_max: Duration::from_secs(60),
            invalid_session_min: Duration::from_secs(1),
            invalid_session_max: Duration::from_secs(5),
            healthy: Duration::from_secs(30),
            rate_window: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Session {
    resume: Option<Resume>,
    seq: Option<u64>,
}

#[derive(Debug)]
struct Resume {
    session_id: String,
    // None when READY's resume_gateway_url was unusable.
    url: Option<Url>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Handshake<'a> {
    Identify,
    Resume { session_id: &'a str, seq: u64 },
}

impl Session {
    pub(crate) fn connect_url(&self, gateway: &Url) -> Url {
        let resume_url = match self.handshake() {
            Handshake::Resume { .. } => self.resume.as_ref().and_then(|resume| resume.url.as_ref()),
            Handshake::Identify => None,
        };
        let mut url = resume_url.unwrap_or(gateway).clone();
        url.query_pairs_mut()
            .clear()
            .append_pair("v", API_VERSION)
            .append_pair("encoding", "json")
            .append_pair("compress", "zstd-stream");
        url
    }

    pub(crate) fn handshake(&self) -> Handshake<'_> {
        match (&self.resume, self.seq) {
            (Some(resume), Some(seq)) => Handshake::Resume {
                session_id: &resume.session_id,
                seq,
            },
            _ => Handshake::Identify,
        }
    }

    pub(crate) fn can_resume(&self) -> bool {
        matches!(self.handshake(), Handshake::Resume { .. })
    }

    pub(crate) fn seq(&self) -> Option<u64> {
        self.seq
    }

    pub(crate) fn dispatched(&mut self, seq: u64) {
        self.seq = Some(seq);
    }

    pub(crate) fn ready(&mut self, ready: &Ready, allow_plaintext: bool) {
        let url = resume_url(&ready.resume_gateway_url, allow_plaintext);
        if url.is_none() {
            tracing::warn!(
                "resume_gateway_url isn't a wss:// URL; resuming through the gateway URL"
            );
        }
        self.resume = Some(Resume {
            session_id: ready.session_id.clone(),
            url,
        });
    }

    pub(crate) fn forget(&mut self) {
        self.resume = None;
        self.seq = None;
    }
}

// The resume request carries the token, so it never goes over plaintext.
fn resume_url(raw: &str, allow_plaintext: bool) -> Option<Url> {
    let url = Url::parse(raw).ok()?;
    match url.scheme() {
        "wss" => Some(url),
        "ws" if allow_plaintext => Some(url),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tick {
    NotYet,
    Heartbeat,
    Zombie,
    NoHello,
}

pub(crate) struct Connection {
    hello_deadline: Instant,
    heartbeat: Option<Heartbeat>,
    handshake_sent: bool,
    ready_since: Option<Instant>,
    pub(crate) limiter: CommandLimiter,
}

impl Connection {
    pub(crate) fn new(now: Instant, timing: &Timing) -> Self {
        Self {
            hello_deadline: now + timing.hello_timeout,
            heartbeat: None,
            handshake_sent: false,
            ready_since: None,
            limiter: CommandLimiter::new(timing.rate_window),
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.heartbeat
            .as_ref()
            .map_or(self.hello_deadline, Heartbeat::deadline)
    }

    pub(crate) fn tick(&mut self, now: Instant) -> Tick {
        match &mut self.heartbeat {
            Some(heartbeat) => match heartbeat.poll(now) {
                Beat::NotYet => Tick::NotYet,
                Beat::Send => Tick::Heartbeat,
                Beat::Zombie => Tick::Zombie,
            },
            None if now >= self.hello_deadline => Tick::NoHello,
            None => Tick::NotYet,
        }
    }

    // True only for the first Hello: a second Identify gets the connection closed (4005).
    pub(crate) fn hello(&mut self, hello: &Hello, now: Instant, jitter: f64) -> bool {
        let interval = Duration::from_millis(hello.heartbeat_interval);
        self.heartbeat = Some(Heartbeat::start(interval, now, jitter));
        self.limiter.reserve_for(interval);
        !std::mem::replace(&mut self.handshake_sent, true)
    }

    pub(crate) fn acked(&mut self) {
        if let Some(heartbeat) = &mut self.heartbeat {
            heartbeat.acked();
        }
    }

    pub(crate) fn ready(&mut self, now: Instant) {
        self.ready_since.get_or_insert(now);
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.ready_since.is_some()
    }

    pub(crate) fn healthy(&self, now: Instant, timing: &Timing) -> bool {
        self.ready_since
            .is_some_and(|since| now.saturating_duration_since(since) >= timing.healthy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AfterClose {
    Resume,
    Identify,
    AuthenticationFailed,
    Rejected { code: u16 },
}

pub(crate) fn after_close(code: Option<u16>) -> AfterClose {
    match code {
        Some(4004) => AfterClose::AuthenticationFailed,
        Some(code @ 4010..=4016) => AfterClose::Rejected { code },
        Some(4003 | 4007 | 4009) => AfterClose::Identify,
        // A session may still exist; if not, Discord answers the resume with op 9.
        _ => AfterClose::Resume,
    }
}

#[derive(Debug, Default)]
pub(crate) struct Retry {
    attempt: u32,
}

impl Retry {
    pub(crate) fn delay(&mut self, healthy: bool, timing: &Timing, jitter: f64) -> Duration {
        if healthy {
            self.attempt = 0;
        }
        let delay = backoff::delay(self.attempt, timing.retry_base, timing.retry_max, jitter);
        self.attempt = self.attempt.saturating_add(1);
        delay
    }
}

pub(crate) fn invalid_session_floor(timing: &Timing, jitter: f64) -> Duration {
    let spread = timing
        .invalid_session_max
        .saturating_sub(timing.invalid_session_min);
    timing.invalid_session_min + spread.mul_f64(jitter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{DispatchEvent, GatewayEvent, decode};

    fn gateway() -> Url {
        Url::parse("wss://gateway.discord.gg/").unwrap()
    }

    fn fixture_ready() -> Ready {
        match decode(include_bytes!("../../tests/fixtures/ready.json")).unwrap() {
            GatewayEvent::Dispatch {
                event: DispatchEvent::Ready(ready),
                ..
            } => *ready,
            other => panic!("not READY: {other:?}"),
        }
    }

    fn query(url: &Url) -> Vec<(String, String)> {
        url.query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    fn hello(interval: u64) -> Hello {
        Hello {
            heartbeat_interval: interval,
            trace: Vec::new(),
        }
    }

    #[test]
    fn a_new_session_identifies_on_the_gateway_url() {
        let session = Session::default();

        let url = session.connect_url(&gateway());

        assert_eq!(url.host_str(), Some("gateway.discord.gg"));
        assert_eq!(
            query(&url),
            [
                ("v".to_owned(), "9".to_owned()),
                ("encoding".to_owned(), "json".to_owned()),
                ("compress".to_owned(), "zstd-stream".to_owned()),
            ]
        );
        assert_eq!(session.handshake(), Handshake::Identify);
    }

    #[test]
    fn ready_makes_the_session_resumable_on_the_resume_url() {
        let mut session = Session::default();
        session.dispatched(1);
        session.ready(&fixture_ready(), false);
        session.dispatched(5);

        let url = session.connect_url(&gateway());

        assert_eq!(url.host_str(), Some("gateway-us-east1-b.discord.gg"));
        assert_eq!(query(&url).len(), 3);
        assert_eq!(
            session.handshake(),
            Handshake::Resume {
                session_id: "0123456789abcdef0123456789abcdef",
                seq: 5
            }
        );
    }

    #[test]
    fn a_resume_url_without_tls_is_ignored() {
        for bad in [
            "ws://gateway.example/",
            "https://gateway.example/",
            "not a url",
        ] {
            let mut ready = fixture_ready();
            ready.resume_gateway_url = bad.to_owned();
            let mut session = Session::default();
            session.dispatched(1);
            session.ready(&ready, false);

            let url = session.connect_url(&gateway());

            assert_eq!(url.host_str(), Some("gateway.discord.gg"), "{bad}");
            assert!(session.can_resume());
        }
    }

    #[test]
    fn plaintext_resume_urls_need_the_test_opt_in() {
        let mut ready = fixture_ready();
        ready.resume_gateway_url = "ws://127.0.0.1:9/resume".to_owned();
        let mut session = Session::default();
        session.dispatched(1);

        session.ready(&ready, true);

        assert_eq!(
            session.connect_url(&gateway()).host_str(),
            Some("127.0.0.1")
        );
    }

    #[test]
    fn forgetting_starts_over() {
        let mut session = Session::default();
        session.dispatched(3);
        session.ready(&fixture_ready(), false);

        session.forget();

        assert_eq!(session.handshake(), Handshake::Identify);
        assert_eq!(session.seq(), None);
        assert_eq!(
            session.connect_url(&gateway()).host_str(),
            Some("gateway.discord.gg")
        );
    }

    #[test]
    fn hello_starts_heartbeats_and_allows_one_handshake() {
        let timing = Timing::default();
        let start = Instant::now();
        let mut connection = Connection::new(start, &timing);

        assert!(connection.hello(&hello(40_000), start, 0.0));
        assert_eq!(connection.tick(start), Tick::Heartbeat);
        assert!(!connection.hello(&hello(40_000), start, 0.0));
    }

    #[test]
    fn a_missing_hello_times_out() {
        let timing = Timing::default();
        let start = Instant::now();
        let mut connection = Connection::new(start, &timing);

        assert_eq!(connection.tick(start), Tick::NotYet);
        assert_eq!(connection.deadline(), start + timing.hello_timeout);
        assert_eq!(connection.tick(start + timing.hello_timeout), Tick::NoHello);
    }

    #[test]
    fn an_unacknowledged_heartbeat_is_a_zombie() {
        let timing = Timing::default();
        let start = Instant::now();
        let mut connection = Connection::new(start, &timing);
        connection.hello(&hello(1000), start, 0.0);

        assert_eq!(connection.tick(start), Tick::Heartbeat);
        assert_eq!(
            connection.tick(start + Duration::from_secs(1)),
            Tick::Zombie
        );
    }

    #[test]
    fn close_codes_decide_what_comes_next() {
        assert_eq!(after_close(Some(4004)), AfterClose::AuthenticationFailed);
        for code in 4010..=4016 {
            assert_eq!(after_close(Some(code)), AfterClose::Rejected { code });
        }
        for code in [4003, 4007, 4009] {
            assert_eq!(after_close(Some(code)), AfterClose::Identify, "{code}");
        }
        for code in [
            Some(1000),
            Some(1001),
            Some(4000),
            Some(4001),
            Some(4002),
            Some(4005),
            Some(4008),
            Some(4999),
            None,
        ] {
            assert_eq!(after_close(code), AfterClose::Resume, "{code:?}");
        }
    }

    #[test]
    fn retries_back_off_until_a_healthy_connection() {
        let timing = Timing::default();
        let mut retry = Retry::default();

        assert_eq!(retry.delay(false, &timing, 0.0), Duration::ZERO);
        assert_eq!(retry.delay(false, &timing, 1.0), Duration::from_secs(1));
        assert_eq!(retry.delay(false, &timing, 1.0), Duration::from_secs(2));
        assert_eq!(retry.delay(true, &timing, 1.0), Duration::ZERO);
    }

    #[test]
    fn a_connection_is_healthy_after_staying_ready() {
        let timing = Timing::default();
        let start = Instant::now();
        let mut connection = Connection::new(start, &timing);
        assert!(!connection.healthy(start + timing.healthy, &timing));

        connection.ready(start);

        assert!(connection.is_ready());
        assert!(!connection.healthy(start + Duration::from_secs(1), &timing));
        assert!(connection.healthy(start + timing.healthy, &timing));
    }

    #[test]
    fn invalid_sessions_wait_one_to_five_seconds() {
        let timing = Timing::default();

        assert_eq!(invalid_session_floor(&timing, 0.0), Duration::from_secs(1));
        assert_eq!(invalid_session_floor(&timing, 1.0), Duration::from_secs(5));
    }
}
