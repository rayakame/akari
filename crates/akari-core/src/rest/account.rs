use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::Method as HttpMethod;
use serde::Serialize;

use super::error::RestError;
use super::ratelimit::{Method, Permit, RateLimiter, ResponseLimits, RouteKey};
use super::{RawResponse, RequestExtras};
use crate::auth::CaptchaChallenge;
use crate::error::TransportError;
use crate::lenient::parse_list;
use crate::model::{self, ChannelId, MessageId};
use crate::{DiscordClient, Token};

const MESSAGES: &str = "channels/{}/messages";
const RATE_LIMIT_RETRIES: u32 = 3;

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
    #[error("network error")]
    Network(#[source] TransportError),
    #[error("unexpected response from Discord")]
    UnexpectedResponse,
    /// Empty content, or a channel or message the store doesn't know.
    #[error("invalid request")]
    InvalidRequest,
    /// The account is closed.
    #[error("the account is closed")]
    Closed,
}

impl From<RestError> for RequestError {
    fn from(err: RestError) -> Self {
        match err {
            RestError::Transport(err) => Self::Network(err),
            RestError::RateLimited { retry_after, .. } => Self::RateLimited { retry_after },
            RestError::Captcha(challenge) => Self::CaptchaRequired(challenge),
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

pub(crate) struct AccountRest {
    client: DiscordClient,
    token: Token,
    limiter: RateLimiter,
    retry_delay: Duration,
    unauthorized: AtomicBool,
    closed: AtomicBool,
}

impl AccountRest {
    pub(crate) fn new(client: DiscordClient, token: Token, retry_delay: Duration) -> Self {
        Self {
            client,
            token,
            limiter: RateLimiter::default(),
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

    // Oldest first; a message that doesn't decode is skipped.
    pub(crate) async fn list_messages(
        &self,
        channel: ChannelId,
        query: Query,
        limit: u8,
    ) -> Result<Vec<model::Message>, RequestError> {
        let cursor = match query {
            Query::Latest => String::new(),
            Query::Before(id) => format!("&before={}", id.get()),
            Query::After(id) => format!("&after={}", id.get()),
            Query::Around(id) => format!("&around={}", id.get()),
        };
        let path = format!("channels/{}/messages?limit={limit}{cursor}", channel.get());
        let route = RouteKey::new(Method::Get, MESSAGES, channel.get());
        let body = self
            .send(route, || self.request(HttpMethod::GET, &path))
            .await?;
        let mut messages: Vec<model::Message> =
            parse_list(&body).map_err(|_| RequestError::UnexpectedResponse)?;
        messages.sort_by_key(|message| message.id);
        Ok(messages)
    }

    // Retries send the same body, so the nonce stays the same.
    pub(crate) async fn create_message(
        &self,
        channel: ChannelId,
        message: &CreateMessage,
    ) -> Result<model::Message, RequestError> {
        let path = format!("channels/{}/messages", channel.get());
        let route = RouteKey::new(Method::Post, MESSAGES, channel.get());
        let body = self
            .send(route, || {
                Ok(self.request(HttpMethod::POST, &path)?.json(message))
            })
            .await?;
        serde_json::from_slice(&body).map_err(|_| RequestError::UnexpectedResponse)
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
        build: impl Fn() -> Result<reqwest::RequestBuilder, RequestError>,
    ) -> Result<Vec<u8>, RequestError> {
        let mut rate_limited = 0;
        let mut server_errors = 0;
        let mut permit: Option<Permit<'_>> = None;
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Err(RequestError::Closed);
            }
            if self.is_unauthorized() {
                return Err(RequestError::Unauthorized);
            }
            let request = build()?;
            let permit = match &permit {
                Some(held) => {
                    held.renew().await;
                    held
                }
                None => permit.insert(self.limiter.acquire(route.clone()).await),
            };
            let RawResponse {
                status,
                headers,
                body,
            } = self.client.rest().send_raw(request).await?;
            let limits = ResponseLimits::from_response(status, &headers, &body);
            permit.finish(&limits);
            match status {
                200..=299 => return Ok(body),
                401 => {
                    // 401s count toward Cloudflare's ban on invalid requests.
                    self.unauthorized.store(true, Ordering::Release);
                    return Err(RequestError::Unauthorized);
                }
                429 if limits.retry_after.is_some() && rate_limited < RATE_LIMIT_RETRIES => {
                    rate_limited += 1;
                }
                429 => {
                    return Err(RequestError::RateLimited {
                        retry_after: limits.retry_after,
                    });
                }
                502 | 504 if server_errors == 0 => {
                    server_errors += 1;
                    tokio::time::sleep(self.retry_delay).await;
                }
                _ => return Err(RestError::from_response(status, &headers, &body).into()),
            }
        }
    }
}

#[cfg(test)]
mod tests;
