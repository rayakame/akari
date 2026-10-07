//! Logging in: email and password, or a QR code scanned by the mobile app.

mod captcha;
mod error;
mod password;
mod remote;

pub use captcha::CaptchaChallenge;
pub use error::{LoginError, LogoutError};
pub use password::{LoginStep, MfaChallenge, MfaMethod, NewLocation, PasswordLogin};
pub use remote::{QrEvent, QrLogin, ScannedUser};

use crate::Token;
use crate::model::{Snowflake, UserMarker};

/// A finished login. Store the token with [`crate::DiscordClient::save_token`].
#[derive(Debug, Clone)]
pub struct LoginSuccess {
    pub user_id: Snowflake<UserMarker>,
    pub token: Token,
    /// Discord wants the user to change their password to meet its current rules.
    pub password_update_required: bool,
}
