use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::Method as HttpMethod;
use serde::Serialize;

use super::clock::ServerClock;
use super::error::{ApiError, RestError};
use super::ratelimit::{Method, Permit, RateLimiter, ResponseLimits, RouteKey};
use super::{RawResponse, RequestExtras};
use crate::auth::CaptchaChallenge;
use crate::error::TransportError;
use crate::lenient::parse_list;
use crate::model::{self, ChannelId, DISCORD_EPOCH, MessageId};
use crate::state::{AckBody, limit_in};
use crate::{DiscordClient, LoadReport, Token};

const CHANNEL: &str = "channels/{}";
const MESSAGES: &str = "channels/{}/messages";
const ACK: &str = "channels/{}/messages/{}/ack";
const ACK_BULK: &str = "read-states/ack-bulk";
const THREAD_MEMBER: &str = "channels/{}/thread-members/@me";
const THREAD_SEARCH: &str = "channels/{}/threads/search";
const THREADS: &str = "channels/{}/threads";
const MESSAGE_THREADS: &str = "channels/{}/messages/{}/threads";
const SLOWMODE: u32 = 20016;
const DAY_MILLIS: u64 = 86_400_000;
const INVALID_FORM_BODY: u32 = 50035;
const RATE_LIMIT_RETRIES: u32 = 3;
// Longer waits, such as slowmode, fail so the UI can say so instead of showing a pending message.
const MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(10);

/// A request on behalf of an account that Discord didn't fulfil.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RequestError {
    /// The token is no longer valid; log in again. Later requests fail at once.
    #[error("Discord no longer accepts the token")]
    Unauthorized,
    #[error("rate limited by Discord")]
    RateLimited { retry_after: Option<Duration> },
    /// Discord wants a captcha solved, which Akari doesn't support yet.
    #[error("Discord wants a captcha solved")]
    CaptchaRequired(Box<CaptchaChallenge>),
    /// e.g. 50013 Missing Permissions, or 50035 for a message that is too long.
    #[error("Discord error {code}: {message}")]
    Discord {
        status: u16,
        code: u32,
        message: String,
    },
    /// A 5xx: Discord couldn't answer. Trying again later may work.
    #[error("Discord server error {status}")]
    ServerError { status: u16 },
    #[error("network error")]
    Network(#[source] TransportError),
    #[error("unexpected response from Discord")]
    UnexpectedResponse,
    /// Longer than `Store::message_length_limit()`, or refused by Discord as too long;
    /// `limit` is Discord's limit then.
    #[error("the message is longer than {limit} characters")]
    TooLong { limit: usize },
    /// Empty content, a send before the account is online, a retry of a message that isn't
    /// failed, or a token that isn't a valid header value.
    #[error("invalid request")]
    InvalidRequest,
    /// The account is closed.
    #[error("the account is closed")]
    Closed,
}

impl RequestError {
    pub(crate) fn is_transient(&self) -> bool {
        matches!(self, Self::Network(_) | Self::ServerError { .. })
    }

    // For logs: never Discord's message, which can quote what was sent.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::RateLimited { .. } => "rate_limited",
            Self::CaptchaRequired(_) => "captcha_required",
            Self::Discord { .. } => "discord",
            Self::ServerError { .. } => "server_error",
            Self::Network(_) => "network",
            Self::UnexpectedResponse => "unexpected_response",
            Self::TooLong { .. } => "too_long",
            Self::InvalidRequest => "invalid_request",
            Self::Closed => "closed",
        }
    }
}

// A request that failed, or that the rate limiter held back without asking Discord.
#[derive(Debug)]
pub(crate) enum Failure {
    Held(Duration),
    Request(RequestError),
    // Only for `Waiting::Create`: Discord's slowmode answer, and its other refusals as sent.
    Slowmode(Option<Duration>),
    Api(ApiError),
}

impl From<RequestError> for Failure {
    fn from(err: RequestError) -> Self {
        Self::Request(err)
    }
}

impl From<RestError> for Failure {
    fn from(err: RestError) -> Self {
        Self::Request(err.into())
    }
}

impl From<Failure> for RequestError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Held(wait) => Self::RateLimited {
                retry_after: Some(wait),
            },
            Failure::Request(err) => err,
            Failure::Slowmode(retry_after) => Self::RateLimited { retry_after },
            Failure::Api(api) => RestError::Api(api).into(),
        }
    }
}

impl From<RestError> for RequestError {
    fn from(err: RestError) -> Self {
        match err {
            RestError::Transport(err) => Self::Network(err),
            RestError::RateLimited { retry_after, .. } => Self::RateLimited { retry_after },
            RestError::Captcha(challenge) => Self::CaptchaRequired(challenge),
            RestError::UnexpectedStatus { status } if status >= 500 => Self::ServerError { status },
            RestError::Api(api) if api.status >= 500 => Self::ServerError { status: api.status },
            RestError::Api(api) if api.code == INVALID_FORM_BODY => {
                match api
                    .field_errors
                    .iter()
                    .find(|error| error.path == "content" && error.code == "BASE_TYPE_MAX_LENGTH")
                {
                    Some(error) => Self::TooLong {
                        limit: limit_in(&error.message).unwrap_or(0),
                    },
                    None => Self::Discord {
                        status: api.status,
                        code: api.code,
                        message: api.message,
                    },
                }
            }
            RestError::Api(api) => Self::Discord {
                status: api.status,
                code: api.code,
                message: api.message,
            },
            RestError::InvalidRequest => Self::InvalidRequest,
            RestError::Suspended
            | RestError::UnexpectedStatus { .. }
            | RestError::InvalidBody
            | RestError::TooLarge => Self::UnexpectedResponse,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Query {
    Latest,
    Before(MessageId),
    After(MessageId),
    Around(MessageId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CreateMessage {
    content: String,
    nonce: String,
    tts: bool,
    flags: u64,
}

impl CreateMessage {
    pub(crate) fn new(content: String, nonce: String) -> Self {
        Self {
            content,
            nonce,
            tts: false,
            flags: 0,
        }
    }
}

// The official client's body, in its order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CreateThread {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) kind: u16,
    pub(crate) auto_archive_duration: u32,
    pub(crate) location: &'static str,
}

#[derive(Serialize)]
struct Ack {
    // Not the account's token: an old ack token, which the official client sends as null.
    token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_viewed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flags: Option<u8>,
}

const IS_THREAD: u8 = 1 << 1;

#[derive(Serialize)]
struct BulkAck {
    read_states: Vec<BulkAckEntry>,
}

#[derive(Serialize)]
struct BulkAckEntry {
    channel_id: String,
    message_id: String,
    read_state_type: u8,
}

// Acks are retried by their sender, which mustn't be held up by one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    // Waits out rate limits up to MAX_RATE_LIMIT_WAIT, and before retrying a 429, 502 or 504.
    Allowed,
    Never,
    // As `Allowed`, but slowmode isn't waited out: the UI shows its countdown instead.
    Create,
}

#[derive(Debug)]
pub(crate) struct Page {
    pub(crate) messages: Vec<model::Message>,
    // Skipped messages count too: a short page means the end is reached.
    pub(crate) received: usize,
    pub(crate) report: LoadReport,
}

struct Exchange {
    body: Vec<u8>,
    report: LoadReport,
}

pub(crate) struct AccountRest {
    client: DiscordClient,
    token: Token,
    limiter: RateLimiter,
    clock: ServerClock,
    retry_delay: Duration,
    unauthorized: AtomicBool,
    closed: AtomicBool,
}

impl AccountRest {
    pub(crate) fn new(client: DiscordClient, token: Token, retry_delay: Duration) -> Self {
        Self {
            client,
            token,
            limiter: RateLimiter::new(MAX_RATE_LIMIT_WAIT),
            clock: ServerClock::default(),
            retry_delay,
            unauthorized: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub(crate) fn is_unauthorized(&self) -> bool {
        self.unauthorized.load(Ordering::Acquire)
    }

    pub(crate) fn usable(&self) -> Result<(), RequestError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(RequestError::Closed);
        }
        if self.is_unauthorized() {
            return Err(RequestError::Unauthorized);
        }
        Ok(())
    }

    // Oldest first; a message that doesn't decode is skipped.
    pub(crate) async fn list_messages(
        &self,
        channel: ChannelId,
        query: Query,
        limit: u8,
    ) -> Result<Page, RequestError> {
        let cursor = match query {
            Query::Latest => String::new(),
            Query::Before(id) => format!("&before={}", id.get()),
            Query::After(id) => format!("&after={}", id.get()),
            Query::Around(id) => format!("&around={}", id.get()),
        };
        let path = format!("channels/{}/messages?limit={limit}{cursor}", channel.get());
        let route = RouteKey::new(Method::Get, MESSAGES, channel.get());
        let Exchange { body, mut report } = self
            .send(route, Waiting::Allowed, || {
                self.request(HttpMethod::GET, &path)
            })
            .await
            .map_err(RequestError::from)?;
        let decoding = Instant::now();
        let (mut messages, received): (Vec<model::Message>, usize) =
            parse_list(&body).map_err(|_| RequestError::UnexpectedResponse)?;
        messages.sort_by_key(|message| message.id);
        report.decode = decoding.elapsed();
        report.bytes = u32::try_from(body.len()).unwrap_or(u32::MAX);
        report.received = u16::try_from(received).unwrap_or(u16::MAX);
        Ok(Page {
            messages,
            received,
            report,
        })
    }

    pub(crate) async fn get_channel(
        &self,
        channel: ChannelId,
    ) -> Result<model::Channel, RequestError> {
        let path = format!("channels/{}", channel.get());
        let route = RouteKey::new(Method::Get, CHANNEL, channel.get());
        let Exchange { body, .. } = self
            .send(route, Waiting::Allowed, || {
                self.request(HttpMethod::GET, &path)
            })
            .await
            .map_err(RequestError::from)?;
        serde_json::from_slice(&body).map_err(|_| RequestError::UnexpectedResponse)
    }

    // Retries send the same body, so the nonce stays the same.
    pub(crate) async fn create_message(
        &self,
        channel: ChannelId,
        message: &CreateMessage,
    ) -> Result<model::Message, RequestError> {
        let path = format!("channels/{}/messages", channel.get());
        let route = RouteKey::new(Method::Post, MESSAGES, channel.get());
        let Exchange { body, .. } = self
            .send(route, Waiting::Allowed, || {
                Ok(self.request(HttpMethod::POST, &path)?.json(message))
            })
            .await
            .map_err(RequestError::from)?;
        serde_json::from_slice(&body).map_err(|_| RequestError::UnexpectedResponse)
    }

    // Joins or leaves; `location` is already a query value. Answered with 204.
    pub(crate) async fn thread_member(
        &self,
        thread: ChannelId,
        join: bool,
        location: &str,
    ) -> Result<(), RequestError> {
        let path = format!(
            "channels/{}/thread-members/@me?location={location}",
            thread.get()
        );
        let (method, verb) = if join {
            (Method::Post, HttpMethod::POST)
        } else {
            (Method::Delete, HttpMethod::DELETE)
        };
        let route = RouteKey::new(method, THREAD_MEMBER, thread.get());
        self.send(route, Waiting::Allowed, || {
            self.request(verb.clone(), &path)
        })
        .await
        .map_err(RequestError::from)?;
        Ok(())
    }

    pub(crate) async fn create_thread(
        &self,
        parent: ChannelId,
        message: Option<MessageId>,
        body: &CreateThread,
    ) -> Result<model::Channel, Failure> {
        let (path, route) = match message {
            Some(message) => (
                format!(
                    "channels/{}/messages/{}/threads",
                    parent.get(),
                    message.get()
                ),
                RouteKey::new(Method::Post, MESSAGE_THREADS, parent.get()),
            ),
            None => (
                format!("channels/{}/threads", parent.get()),
                RouteKey::new(Method::Post, THREADS, parent.get()),
            ),
        };
        let Exchange { body, .. } = self
            .send(route, Waiting::Create, || {
                Ok(self.request(HttpMethod::POST, &path)?.json(body))
            })
            .await?;
        serde_json::from_slice(&body).map_err(|_| RequestError::UnexpectedResponse.into())
    }

    // `query` is already encoded. A 202 while Discord indexes the channel decodes without
    // threads.
    pub(crate) async fn search_threads(
        &self,
        channel: ChannelId,
        query: &str,
    ) -> Result<model::ThreadSearchResults, RequestError> {
        let path = format!("channels/{}/threads/search?{query}", channel.get());
        let route = RouteKey::new(Method::Get, THREAD_SEARCH, channel.get());
        let Exchange { body, .. } = self
            .send(route, Waiting::Allowed, || {
                self.request(HttpMethod::GET, &path)
            })
            .await
            .map_err(RequestError::from)?;
        serde_json::from_slice(&body).map_err(|_| RequestError::UnexpectedResponse)
    }

    pub(crate) async fn ack(
        &self,
        channel: ChannelId,
        message: MessageId,
        last_viewed: u32,
        kind: AckBody,
    ) -> Result<(), Failure> {
        let path = format!("channels/{}/messages/{}/ack", channel.get(), message.get());
        let route = RouteKey::new(Method::Post, ACK, channel.get());
        let body = match kind {
            AckBody::Channel => Ack {
                token: None,
                last_viewed: Some(last_viewed),
                flags: None,
            },
            AckBody::Thread { joined } => Ack {
                token: None,
                last_viewed: joined.then_some(last_viewed),
                flags: Some(IS_THREAD),
            },
        };
        self.send(route, Waiting::Never, || {
            Ok(self.request(HttpMethod::POST, &path)?.json(&body))
        })
        .await?;
        Ok(())
    }

    pub(crate) async fn ack_bulk(&self, entries: &[(ChannelId, MessageId)]) -> Result<(), Failure> {
        let body = BulkAck {
            read_states: entries
                .iter()
                .map(|(channel, message)| BulkAckEntry {
                    channel_id: channel.get().to_string(),
                    message_id: message.get().to_string(),
                    read_state_type: 0,
                })
                .collect(),
        };
        let route = RouteKey::new(Method::Post, ACK_BULK, 0);
        self.send(route, Waiting::Never, || {
            Ok(self.request(HttpMethod::POST, ACK_BULK)?.json(&body))
        })
        .await?;
        Ok(())
    }

    pub(crate) fn server_now_millis(&self) -> Option<i64> {
        self.clock.now_millis()
    }

    fn request(
        &self,
        method: HttpMethod,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, RequestError> {
        let extras = RequestExtras {
            authorization: Some(&self.token),
            ..RequestExtras::default()
        };
        Ok(self.client.rest().request(method, path, &extras)?)
    }

    async fn send(
        &self,
        route: RouteKey,
        waiting: Waiting,
        build: impl Fn() -> Result<reqwest::RequestBuilder, RequestError>,
    ) -> Result<Exchange, Failure> {
        let waits = waiting != Waiting::Never;
        let creates = waiting == Waiting::Create;
        let mut rate_limited = 0;
        let mut server_errors = 0;
        let mut permit: Option<Permit<'_>> = None;
        let mut report = LoadReport::default();
        let waiting = Instant::now();
        self.client.rest().after_warm_up().await;
        let behind_warm_up = waiting.elapsed();
        loop {
            self.usable()?;
            let request = build()?;
            let permit = match &permit {
                Some(held) => {
                    report.limited += held.renew().await.map_err(Failure::Held)?;
                    held
                }
                None => {
                    let acquired = if waits {
                        self.limiter.acquire(route.clone()).await
                    } else {
                        self.limiter.acquire_now(route.clone()).await
                    }
                    .map_err(Failure::Held)?;
                    report.queued = behind_warm_up + acquired.queued;
                    report.limited = acquired.limited;
                    permit.insert(acquired)
                }
            };
            // The wait for the route can be long; the account may have closed meanwhile.
            self.usable()?;
            let RawResponse {
                status,
                headers,
                body,
                timing,
            } = self.client.rest().send_raw(request).await?;
            self.clock.observe(&headers, timing.headers_at);
            let limits = ResponseLimits::from_response(status, &headers, &body);
            permit.finish(&limits, timing.headers_at);
            report.headers += timing.headers;
            report.body += timing.body;
            report.new_connection |= timing.new_connection;
            report.remaining = limits.remaining;
            report.reset_after = limits.reset_after;
            match status {
                200..=299 => return Ok(Exchange { body, report }),
                401 => {
                    // 401s count toward Cloudflare's ban on invalid requests.
                    self.unauthorized.store(true, Ordering::Release);
                    return Err(RequestError::Unauthorized.into());
                }
                429 if creates && limits.code == Some(SLOWMODE) => {
                    return Err(Failure::Slowmode(limits.retry_after));
                }
                429 if waits
                    && limits.retry_after.is_some()
                    && rate_limited < RATE_LIMIT_RETRIES =>
                {
                    rate_limited += 1;
                    report.retries += 1;
                }
                429 => {
                    return Err(RequestError::RateLimited {
                        retry_after: limits.retry_after,
                    }
                    .into());
                }
                502 | 504 if waits && server_errors == 0 => {
                    server_errors += 1;
                    report.retries += 1;
                    tokio::time::sleep(self.retry_delay).await;
                }
                _ => {
                    return Err(match RestError::from_response(status, &headers, &body) {
                        RestError::Api(api) if creates && api.status < 500 => Failure::Api(api),
                        err => err.into(),
                    });
                }
            }
        }
    }
}

// As JavaScript's encodeURIComponent, which the official client's requests use.
pub(crate) fn uri_component(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

// An ack's `last_viewed`: days since the Discord epoch, rounded up like the official client.
pub(crate) fn last_viewed(unix_millis: i64) -> u32 {
    let since_epoch = u64::try_from(unix_millis.saturating_sub(DISCORD_EPOCH)).unwrap_or(0);
    u32::try_from(since_epoch.div_ceil(DAY_MILLIS)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
