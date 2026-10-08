use std::time::Duration;

use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::auth::CaptchaChallenge;
use crate::error::TransportError;

#[derive(Debug)]
pub(crate) enum RestError {
    Transport(TransportError),
    // No retry_after means Discord or Cloudflare didn't say; don't retry automatically then.
    RateLimited {
        retry_after: Option<Duration>,
        global: bool,
    },
    Captcha(Box<CaptchaChallenge>),
    // The suspended_user_token Discord sends is a credential, so it's dropped.
    Suspended,
    Api(ApiError),
    UnexpectedStatus {
        status: u16,
    },
    // Carries no serde message: it can quote the body, which may hold a token.
    InvalidBody,
    InvalidRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiError {
    pub(crate) status: u16,
    pub(crate) code: u32,
    pub(crate) message: String,
    pub(crate) field_errors: Vec<FieldError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldError {
    pub(crate) path: String,
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    code: Option<u32>,
    message: Option<String>,
    errors: Option<serde_json::Value>,
    retry_after: Option<f64>,
    global: Option<bool>,
    captcha_key: Option<IgnoredAny>,
    captcha_service: Option<String>,
    captcha_sitekey: Option<String>,
    captcha_rqdata: Option<String>,
    captcha_rqtoken: Option<String>,
    captcha_session_id: Option<String>,
    #[serde(default)]
    should_serve_invisible: bool,
    suspended_user_token: Option<IgnoredAny>,
}

impl RestError {
    // For logs: names the error without anything from the response body.
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::Transport(err) => format!("network error: {err}"),
            Self::RateLimited { .. } => "rate limited".to_owned(),
            Self::Captcha(_) => "captcha required".to_owned(),
            Self::Suspended => "account suspended".to_owned(),
            Self::Api(api) => format!("Discord error {}", api.code),
            Self::UnexpectedStatus { status } => format!("unexpected status {status}"),
            Self::InvalidBody => "unexpected response body".to_owned(),
            Self::InvalidRequest => "invalid request".to_owned(),
        }
    }

    pub(super) fn from_response(status: u16, headers: &HeaderMap, body: &[u8]) -> Self {
        let parsed: Option<ErrorBody> = serde_json::from_slice(body).ok();
        if status == 429 {
            return rate_limited(headers, parsed.as_ref());
        }
        let Some(body) = parsed else {
            return Self::UnexpectedStatus { status };
        };
        if status == 400 && body.captcha_key.is_some() {
            return Self::Captcha(Box::new(CaptchaChallenge {
                service: body.captcha_service.unwrap_or_default(),
                sitekey: body.captcha_sitekey,
                rqdata: body.captcha_rqdata,
                rqtoken: body.captcha_rqtoken,
                session_id: body.captcha_session_id,
                should_serve_invisible: body.should_serve_invisible,
            }));
        }
        if body.suspended_user_token.is_some() {
            return Self::Suspended;
        }
        match (body.code, body.message) {
            (Some(code), Some(message)) => {
                let mut field_errors = Vec::new();
                if let Some(errors) = &body.errors {
                    flatten(errors, String::new(), &mut field_errors);
                }
                Self::Api(ApiError {
                    status,
                    code,
                    message,
                    field_errors,
                })
            }
            _ => Self::UnexpectedStatus { status },
        }
    }
}

fn rate_limited(headers: &HeaderMap, body: Option<&ErrorBody>) -> RestError {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let retry_after = body
        .and_then(|body| body.retry_after)
        .or_else(|| header("retry-after").and_then(|value| value.parse().ok()))
        .and_then(|seconds: f64| Duration::try_from_secs_f64(seconds).ok());
    let global = body
        .and_then(|body| body.global)
        .unwrap_or_else(|| header("x-ratelimit-global") == Some("true"));
    RestError::RateLimited {
        retry_after,
        global,
    }
}

fn flatten(value: &serde_json::Value, path: String, out: &mut Vec<FieldError>) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (key, child) in object {
        if key == "_errors" {
            for error in child.as_array().into_iter().flatten() {
                let text = |field| {
                    error
                        .get(field)
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                out.push(FieldError {
                    path: path.clone(),
                    code: text("code"),
                    message: text("message"),
                });
            }
        } else if path.is_empty() {
            flatten(child, key.clone(), out);
        } else {
            flatten(child, format!("{path}.{key}"), out);
        }
    }
}
