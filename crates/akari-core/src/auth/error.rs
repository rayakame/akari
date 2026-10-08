use std::time::Duration;

use crate::error::TransportError;
use crate::rest::{ApiError, RestError};
use crate::token_store::TokenStoreError;

/// Why a login step failed. Messages are safe to show and never contain secrets.
#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    /// Wrong login or password; `message` is Discord's, for display.
    #[error("{message}")]
    InvalidCredentials { message: String },
    /// The step stays pending, so the user can try another code.
    #[error("invalid two-factor code")]
    InvalidMfaCode,
    /// The MFA ticket ran out; start the login again.
    #[error("the login attempt expired, start again")]
    Expired,
    #[error("this account is disabled")]
    AccountDisabled,
    #[error("this account is scheduled for deletion")]
    AccountScheduledForDeletion,
    #[error("this account is suspended")]
    AccountSuspended,
    /// `retry_after` is `None` when Discord didn't say how long to wait.
    #[error("rate limited by Discord")]
    RateLimited {
        retry_after: Option<Duration>,
        global: bool,
    },
    #[error("blocked by Discord's anti-abuse systems")]
    Blocked,
    #[error("Discord couldn't send an SMS for this account")]
    SmsUnavailable,
    #[error("not a login verification link or token")]
    InvalidVerificationLink,
    #[error("Discord error {code}: {message}")]
    Discord { code: u32, message: String },
    #[error("network error")]
    Network(#[source] TransportError),
    #[error("QR code login failed: {0}")]
    RemoteAuth(String),
    #[error("unexpected response from Discord")]
    UnexpectedResponse,
    #[error("the login was cancelled")]
    Cancelled,
    #[error("no login step is waiting for this")]
    NoPendingStep,
    #[error("another login step is still running")]
    Busy,
    /// QR code login runs in the background and needs a Tokio runtime to start in.
    #[error("QR code login needs a Tokio runtime")]
    NoRuntime,
}

impl LoginError {
    pub(crate) fn from_rest(err: RestError) -> Self {
        match err {
            RestError::Transport(err) => Self::Network(err),
            RestError::RateLimited {
                retry_after,
                global,
            } => Self::RateLimited {
                retry_after,
                global,
            },
            RestError::Suspended => Self::AccountSuspended,
            RestError::Api(api) => Self::from_api(api),
            RestError::UnexpectedStatus { status: 403 } => Self::Blocked,
            RestError::Captcha(_)
            | RestError::UnexpectedStatus { .. }
            | RestError::InvalidBody
            | RestError::InvalidRequest
            | RestError::TooLarge => Self::UnexpectedResponse,
        }
    }

    fn from_api(api: ApiError) -> Self {
        match api.code {
            20011 => Self::AccountScheduledForDeletion,
            20013 => Self::AccountDisabled,
            60006 | 60009 => Self::Expired,
            60008 => Self::InvalidMfaCode,
            60010 | 70003 => Self::SmsUnavailable,
            40333 => Self::Blocked,
            10008 if api.status == 403 => Self::Blocked,
            50035 => {
                let credentials = api
                    .field_errors
                    .iter()
                    .find(|error| error.path == "login" || error.path == "password");
                match (credentials, api.field_errors.first()) {
                    (Some(error), _) => Self::InvalidCredentials {
                        message: error.message.clone(),
                    },
                    (None, Some(error)) => Self::Discord {
                        code: api.code,
                        message: error.message.clone(),
                    },
                    (None, None) => Self::Discord {
                        code: api.code,
                        message: api.message,
                    },
                }
            }
            code => Self::Discord {
                code,
                message: api.message,
            },
        }
    }
}

/// Why logging out failed. The stored token is deleted even when Discord can't be reached,
/// so only [`LogoutError::Storage`] means it is still there.
#[derive(Debug, thiserror::Error)]
pub enum LogoutError {
    #[error("not logged in")]
    NotLoggedIn,
    #[error("couldn't remove the stored token")]
    Storage(#[source] TokenStoreError),
    #[error("network error")]
    Network(#[source] TransportError),
    #[error("rate limited by Discord")]
    RateLimited { retry_after: Option<Duration> },
    #[error("Discord error {code}: {message}")]
    Discord { code: u32, message: String },
    #[error("unexpected response from Discord")]
    UnexpectedResponse,
}

impl LogoutError {
    // None: Discord no longer accepts the token, so the session is gone anyway.
    pub(crate) fn from_rest(err: RestError) -> Option<Self> {
        Some(match err {
            RestError::Api(ApiError { status: 401, .. })
            | RestError::UnexpectedStatus { status: 401 }
            | RestError::InvalidRequest => return None,
            RestError::Transport(err) => Self::Network(err),
            RestError::RateLimited { retry_after, .. } => Self::RateLimited { retry_after },
            RestError::Api(api) => Self::Discord {
                code: api.code,
                message: api.message,
            },
            RestError::Captcha(_)
            | RestError::Suspended
            | RestError::UnexpectedStatus { .. }
            | RestError::InvalidBody
            | RestError::TooLarge => Self::UnexpectedResponse,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::rest::{ApiError, FieldError, RestError};

    fn api(status: u16, code: u32) -> RestError {
        RestError::Api(ApiError {
            status,
            code,
            message: format!("message {code}"),
            field_errors: Vec::new(),
        })
    }

    fn login(err: RestError) -> LoginError {
        LoginError::from_rest(err)
    }

    #[test]
    fn json_codes_map_to_typed_login_errors() {
        assert!(matches!(
            login(api(400, 20011)),
            LoginError::AccountScheduledForDeletion
        ));
        assert!(matches!(
            login(api(400, 20013)),
            LoginError::AccountDisabled
        ));
        assert!(matches!(login(api(400, 60008)), LoginError::InvalidMfaCode));
        assert!(matches!(login(api(400, 60006)), LoginError::Expired));
        assert!(matches!(login(api(400, 60009)), LoginError::Expired));
        assert!(matches!(login(api(400, 60010)), LoginError::SmsUnavailable));
        assert!(matches!(login(api(400, 70003)), LoginError::SmsUnavailable));
        assert!(matches!(login(api(403, 40333)), LoginError::Blocked));
        assert!(matches!(login(api(403, 10008)), LoginError::Blocked));
        assert!(matches!(
            login(api(404, 10008)),
            LoginError::Discord { code: 10008, .. }
        ));
        assert!(matches!(
            login(api(400, 12345)),
            LoginError::Discord { code: 12345, ref message } if message == "message 12345"
        ));
    }

    #[test]
    fn form_errors_on_credentials_are_invalid_credentials() {
        let err = login(RestError::Api(ApiError {
            status: 400,
            code: 50035,
            message: "Invalid Form Body".to_owned(),
            field_errors: vec![FieldError {
                path: "password".to_owned(),
                code: "INVALID_LOGIN".to_owned(),
                message: "Login or password is invalid.".to_owned(),
            }],
        }));

        assert!(matches!(
            err,
            LoginError::InvalidCredentials { ref message } if message == "Login or password is invalid."
        ));
    }

    #[test]
    fn other_form_errors_show_the_field_message() {
        let err = login(RestError::Api(ApiError {
            status: 400,
            code: 50035,
            message: "Invalid Form Body".to_owned(),
            field_errors: vec![FieldError {
                path: "gift_code_sku_id".to_owned(),
                code: "BASE_TYPE_BAD".to_owned(),
                message: "Bad value.".to_owned(),
            }],
        }));

        assert!(matches!(
            err,
            LoginError::Discord { code: 50035, ref message } if message == "Bad value."
        ));
    }

    #[test]
    fn transport_level_failures_map_to_login_errors() {
        assert!(matches!(
            login(RestError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
                global: true
            }),
            LoginError::RateLimited {
                retry_after: Some(_),
                global: true
            }
        ));
        assert!(matches!(
            login(RestError::Suspended),
            LoginError::AccountSuspended
        ));
        assert!(matches!(
            login(RestError::UnexpectedStatus { status: 403 }),
            LoginError::Blocked
        ));
        assert!(matches!(
            login(RestError::UnexpectedStatus { status: 503 }),
            LoginError::UnexpectedResponse
        ));
        assert!(matches!(
            login(RestError::InvalidBody),
            LoginError::UnexpectedResponse
        ));
    }

    #[test]
    fn logout_treats_a_dead_token_as_done() {
        assert!(LogoutError::from_rest(api(401, 0)).is_none());
        assert!(LogoutError::from_rest(RestError::UnexpectedStatus { status: 401 }).is_none());
        assert!(matches!(
            LogoutError::from_rest(api(500, 0)),
            Some(LogoutError::Discord { code: 0, .. })
        ));
        assert!(matches!(
            LogoutError::from_rest(RestError::RateLimited {
                retry_after: None,
                global: false
            }),
            Some(LogoutError::RateLimited { retry_after: None })
        ));
    }
}
