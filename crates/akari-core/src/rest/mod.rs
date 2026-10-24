mod account;
mod clock;
mod connections;
mod error;
mod ratelimit;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{RequestBuilder, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use account::RequestError;
pub(crate) use account::{
    AccountRest, CreateMessage, CreateThread, Failure, Page, Query, last_viewed, uri_component,
};
#[cfg(test)]
pub(crate) use error::FieldError;
pub(crate) use error::{ApiError, RestError};

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

use connections::{Connections, TimedConnectLayer};

use crate::error::TransportError;
use crate::properties::ClientProperties;
use crate::{Secret, Token};

#[derive(Debug, Clone)]
pub(crate) struct RestClient {
    http: reqwest::Client,
    base: Url,
    connections: Arc<Connections>,
    warming: Arc<watch::Sender<bool>>,
}

#[derive(Default)]
pub(crate) struct RequestExtras<'a> {
    pub(crate) authorization: Option<&'a Token>,
    pub(crate) fingerprint: Option<&'a str>,
    pub(crate) captcha: Option<&'a CaptchaSolution>,
}

// Sensitive values print as "Sensitive" in reqwest's Debug output.
fn sensitive(value: &str) -> Result<HeaderValue, RestError> {
    let mut value = HeaderValue::from_str(value).map_err(|_| RestError::InvalidRequest)?;
    value.set_sensitive(true);
    Ok(value)
}

// Login responses are a few KiB; anything near this is not Discord.
const MAX_BODY: usize = 4 * 1024 * 1024;
// A connect usually takes well under this; past it a connection of its own is the better bet.
const WARM_UP_WAIT: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub(crate) struct CaptchaSolution {
    pub(crate) key: Secret,
    pub(crate) rqtoken: Option<String>,
    pub(crate) session_id: Option<String>,
}

impl fmt::Debug for RequestExtras<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestExtras")
            .field("authorization", &self.authorization.is_some())
            .field("fingerprint", &self.fingerprint.is_some())
            .field("captcha", &self.captcha.is_some())
            .finish()
    }
}

impl fmt::Debug for CaptchaSolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptchaSolution(<redacted>)")
    }
}

// A warm-up future dropped mid-request must not leave every later request waiting for it.
struct Warming<'a>(&'a watch::Sender<bool>);

impl Drop for Warming<'_> {
    fn drop(&mut self) {
        self.0.send_replace(false);
    }
}

pub(crate) struct RawResponse {
    pub(crate) status: u16,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
    pub(crate) timing: ResponseTiming,
}

pub(crate) struct ResponseTiming {
    pub(crate) headers: Duration,
    pub(crate) body: Duration,
    pub(crate) headers_at: Instant,
    pub(crate) new_connection: bool,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BuildError {
    #[error("invalid {0} header value")]
    InvalidHeader(&'static str),
    #[error(transparent)]
    Transport(TransportError),
}

impl RestClient {
    pub(crate) fn new(
        tls: &rustls::ClientConfig,
        base: Url,
        properties: &ClientProperties,
        https_only: bool,
    ) -> Result<Self, BuildError> {
        let mut tls = tls.clone();
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let mut headers = HeaderMap::new();
        let mut insert = |name: &'static str, value: &str| {
            let value =
                HeaderValue::from_str(value).map_err(|_| BuildError::InvalidHeader(name))?;
            headers.insert(HeaderName::from_static(name), value);
            Ok(())
        };
        insert("user-agent", &properties.browser_user_agent)?;
        insert("x-super-properties", &properties.super_properties())?;
        insert("x-discord-locale", &properties.system_locale)?;
        // Chromium's value; reqwest's own would name the same encodings in another order.
        insert("accept-encoding", "gzip, deflate, br, zstd")?;

        let connections = Arc::new(Connections::default());
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .connector_layer(TimedConnectLayer(connections.clone()))
            .default_headers(headers)
            .https_only(https_only)
            // A redirect would carry the Authorization header to wherever Discord points.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            // A channel switch after a short break shouldn't pay a new handshake.
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|err| BuildError::Transport(TransportError::from_reqwest(err)))?;
        Ok(Self {
            http,
            base,
            connections,
            warming: Arc::new(watch::Sender::new(false)),
        })
    }

    pub(crate) async fn get_json<R: DeserializeOwned>(
        &self,
        path: &str,
        extras: &RequestExtras<'_>,
    ) -> Result<R, RestError> {
        let body = self
            .execute(self.request(reqwest::Method::GET, path, extras)?)
            .await?;
        serde_json::from_slice(body.as_ref()).map_err(|_| RestError::InvalidBody)
    }

    pub(crate) async fn post_json<B: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extras: &RequestExtras<'_>,
    ) -> Result<R, RestError> {
        let request = self
            .request(reqwest::Method::POST, path, extras)?
            .json(body);
        let body = self.execute(request).await?;
        serde_json::from_slice(body.as_ref()).map_err(|_| RestError::InvalidBody)
    }

    pub(crate) async fn post<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
        extras: &RequestExtras<'_>,
    ) -> Result<(), RestError> {
        let request = self
            .request(reqwest::Method::POST, path, extras)?
            .json(body);
        self.execute(request).await.map(|_| ())
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        extras: &RequestExtras<'_>,
    ) -> Result<RequestBuilder, RestError> {
        let url = self
            .base
            .join(path)
            .map_err(|_| RestError::InvalidRequest)?;
        let mut request = self.http.request(method, url);
        if let Some(token) = extras.authorization {
            request = request.header(reqwest::header::AUTHORIZATION, sensitive(token.expose())?);
        }
        if let Some(fingerprint) = extras.fingerprint {
            request = request.header("x-fingerprint", fingerprint);
        }
        if let Some(captcha) = extras.captcha {
            request = request.header("x-captcha-key", sensitive(captcha.key.expose())?);
            if let Some(rqtoken) = &captcha.rqtoken {
                request = request.header("x-captcha-rqtoken", sensitive(rqtoken)?);
            }
            if let Some(session_id) = &captcha.session_id {
                request = request.header("x-captcha-session-id", sensitive(session_id)?);
            }
        }
        Ok(request)
    }

    async fn execute(&self, request: RequestBuilder) -> Result<impl AsRef<[u8]>, RestError> {
        let RawResponse {
            status,
            headers,
            body,
            ..
        } = self.send_raw(request).await?;
        if (200..300).contains(&status) {
            Ok(body)
        } else {
            Err(RestError::from_response(status, &headers, &body))
        }
    }

    // Unauthenticated and harmless; only its connection is wanted.
    pub(crate) async fn warm_up(&self) {
        if self.warming.send_replace(true) {
            return;
        }
        let _warming = Warming(&self.warming);
        let started = Instant::now();
        let warmed = match self.request(reqwest::Method::GET, "gateway", &RequestExtras::default())
        {
            Ok(request) => self.send_raw(request).await.ok(),
            Err(_) => None,
        };
        tracing::debug!(
            after_ms = crate::millis(started.elapsed()),
            new_connection = warmed
                .as_ref()
                .is_some_and(|response| response.timing.new_connection),
            ok = warmed.is_some(),
            "rest warmed up"
        );
    }

    // A request that starts while the warm-up still connects would open a second connection.
    pub(crate) async fn after_warm_up(&self) {
        let mut warming = self.warming.subscribe();
        let _ = tokio::time::timeout(WARM_UP_WAIT, warming.wait_for(|warming| !warming)).await;
    }

    pub(crate) async fn send_raw(&self, request: RequestBuilder) -> Result<RawResponse, RestError> {
        let transport = |err| RestError::Transport(TransportError::from_reqwest(err));
        let opened = self.connections.opened();
        let started = Instant::now();
        let mut response = request.send().await.map_err(transport)?;
        let headers_at = Instant::now();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        if response
            .content_length()
            .is_some_and(|len| len > MAX_BODY as u64)
        {
            return Err(RestError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(RestError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let timing = ResponseTiming {
            headers: headers_at - started,
            body: headers_at.elapsed(),
            headers_at,
            new_connection: self.connections.opened() > opened,
        };
        Ok(RawResponse {
            status,
            headers,
            body,
            timing,
        })
    }
}

#[cfg(test)]
mod tests;
