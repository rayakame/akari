// Account will be the first caller outside tests.
#[cfg_attr(not(test), allow(dead_code))]
mod account;
mod error;
mod ratelimit;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{RequestBuilder, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use account::RequestError;
#[cfg(test)]
pub(crate) use error::FieldError;
pub(crate) use error::{ApiError, RestError};

use std::fmt;

use crate::error::TransportError;
use crate::properties::ClientProperties;
use crate::{Secret, Token};

#[derive(Debug, Clone)]
pub(crate) struct RestClient {
    http: reqwest::Client,
    base: Url,
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

pub(crate) struct RawResponse {
    pub(crate) status: u16,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
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

        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .default_headers(headers)
            .https_only(https_only)
            // A redirect would carry the Authorization header to wherever Discord points.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|err| BuildError::Transport(TransportError::from_reqwest(err)))?;
        Ok(Self { http, base })
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
        } = self.send_raw(request).await?;
        if (200..300).contains(&status) {
            Ok(body)
        } else {
            Err(RestError::from_response(status, &headers, &body))
        }
    }

    pub(crate) async fn send_raw(&self, request: RequestBuilder) -> Result<RawResponse, RestError> {
        let transport = |err| RestError::Transport(TransportError::from_reqwest(err));
        let mut response = request.send().await.map_err(transport)?;
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
        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests;
