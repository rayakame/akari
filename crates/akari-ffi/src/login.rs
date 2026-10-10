use std::sync::Arc;

use akari_core::Secret;
use akari_core::auth;
use akari_core::model::UserId;
use tokio::runtime::Runtime;

use crate::client::Token;
use crate::errors::LoginError;
use crate::runtime::run;

/// An email/password login, one step at a time. Only one step runs at a time.
#[derive(uniffi::Object)]
pub struct PasswordLogin {
    core: Arc<auth::PasswordLogin>,
    runtime: &'static Runtime,
}

impl PasswordLogin {
    pub(crate) fn new(core: auth::PasswordLogin, runtime: &'static Runtime) -> Arc<Self> {
        Arc::new(Self {
            core: Arc::new(core),
            runtime,
        })
    }

    async fn step<F>(
        &self,
        step: impl FnOnce(Arc<auth::PasswordLogin>) -> F + Send + 'static,
    ) -> Result<LoginStep, LoginError>
    where
        F: Future<Output = Result<auth::LoginStep, auth::LoginError>> + Send + 'static,
    {
        let core = self.core.clone();
        run(self.runtime, async move { step(core).await })
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[uniffi::export]
impl PasswordLogin {
    /// Starts or restarts the login. `login` is an email address or an E.164 phone number.
    pub async fn submit(&self, login: String, password: String) -> Result<LoginStep, LoginError> {
        let (login, password) = (Secret::new(login), Secret::new(password));
        self.step(|core| async move { core.submit(login.expose(), password).await })
            .await
    }

    /// Retries the request that asked for the CAPTCHA, with its solution.
    pub async fn solve_captcha(&self, solution: String) -> Result<LoginStep, LoginError> {
        self.step(|core| async move { core.solve_captcha(solution).await })
            .await
    }

    /// Texts a code to the account's phone; returns the MFA step with `sms_sent_to` set.
    pub async fn send_mfa_sms(&self) -> Result<LoginStep, LoginError> {
        self.step(|core| async move { core.send_mfa_sms().await })
            .await
    }

    pub async fn submit_mfa(
        &self,
        method: MfaMethod,
        code: String,
    ) -> Result<LoginStep, LoginError> {
        let code = Secret::new(code);
        self.step(move |core| async move { core.submit_mfa(method.into(), code).await })
            .await
    }

    /// The address Discord's email link opened, or the bare token.
    pub async fn confirm_new_location(
        &self,
        link_or_token: String,
    ) -> Result<LoginStep, LoginError> {
        let link_or_token = Secret::new(link_or_token);
        self.step(|core| async move { core.confirm_new_location(link_or_token.expose()).await })
            .await
    }

    /// The code Discord texted to the phone number used as the login.
    pub async fn verify_phone(&self, code: String) -> Result<LoginStep, LoginError> {
        let code = Secret::new(code);
        self.step(|core| async move { core.verify_phone(code).await })
            .await
    }

    /// Ends the flow; a running step fails with `Cancelled`.
    pub fn cancel(&self) {
        self.core.cancel();
    }
}

/// What an email/password login needs next.
#[derive(uniffi::Enum)]
pub enum LoginStep {
    Done {
        success: LoginSuccess,
    },
    /// Show the challenge, then call `solve_captcha`.
    Captcha {
        challenge: CaptchaChallenge,
    },
    /// Ask for a code, then call `submit_mfa`.
    Mfa {
        challenge: MfaChallenge,
    },
    /// Discord wants this login location confirmed by the email link
    /// (`confirm_new_location`) or by an SMS code (`verify_phone`).
    NewLocation {
        via: NewLocation,
    },
}

impl From<auth::LoginStep> for LoginStep {
    fn from(step: auth::LoginStep) -> Self {
        match step {
            auth::LoginStep::Done(success) => Self::Done {
                success: success.into(),
            },
            auth::LoginStep::Captcha(challenge) => Self::Captcha {
                challenge: challenge.into(),
            },
            auth::LoginStep::Mfa(challenge) => Self::Mfa {
                challenge: challenge.into(),
            },
            auth::LoginStep::NewLocation(via) => Self::NewLocation { via: via.into() },
        }
    }
}

/// A finished login. Store the token with `DiscordClient::save_token`.
#[derive(uniffi::Record)]
pub struct LoginSuccess {
    pub user_id: UserId,
    pub token: Arc<Token>,
    /// Discord wants the user to change their password to meet its current rules.
    pub password_update_required: bool,
}

impl From<auth::LoginSuccess> for LoginSuccess {
    fn from(success: auth::LoginSuccess) -> Self {
        Self {
            user_id: success.user_id,
            token: Token::new(success.token),
            password_update_required: success.password_update_required,
        }
    }
}

/// The account has two-factor authentication enabled.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MfaChallenge {
    pub methods: Vec<MfaMethod>,
    /// `PublicKeyCredentialRequestOptions` as JSON, when WebAuthn is available.
    pub webauthn_options: Option<String>,
    /// The masked phone number, once `send_mfa_sms` sent a code.
    pub sms_sent_to: Option<String>,
}

impl From<auth::MfaChallenge> for MfaChallenge {
    fn from(challenge: auth::MfaChallenge) -> Self {
        Self {
            methods: challenge.methods.into_iter().map(Into::into).collect(),
            webauthn_options: challenge.webauthn_options,
            sms_sent_to: challenge.sms_sent_to,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MfaMethod {
    /// An authenticator app code. Discord also accepts backup codes here.
    Totp,
    /// A code sent by `send_mfa_sms`.
    Sms,
    Backup,
    /// The code is the JSON of `PublicKeyCredential.toJSON()`.
    WebAuthn,
}

impl From<auth::MfaMethod> for MfaMethod {
    fn from(method: auth::MfaMethod) -> Self {
        match method {
            auth::MfaMethod::Totp => Self::Totp,
            auth::MfaMethod::Sms => Self::Sms,
            auth::MfaMethod::Backup => Self::Backup,
            auth::MfaMethod::WebAuthn => Self::WebAuthn,
        }
    }
}

impl From<MfaMethod> for auth::MfaMethod {
    fn from(method: MfaMethod) -> Self {
        match method {
            MfaMethod::Totp => Self::Totp,
            MfaMethod::Sms => Self::Sms,
            MfaMethod::Backup => Self::Backup,
            MfaMethod::WebAuthn => Self::WebAuthn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NewLocation {
    Email,
    Phone,
}

impl From<auth::NewLocation> for NewLocation {
    fn from(via: auth::NewLocation) -> Self {
        match via {
            auth::NewLocation::Email => Self::Email,
            auth::NewLocation::Phone => Self::Phone,
        }
    }
}

/// Discord wants a CAPTCHA solved before it accepts the request. Render the challenge
/// (hCaptcha for login) with `sitekey` and, when present, `rqdata`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CaptchaChallenge {
    /// `hcaptcha`, or `recaptcha_enterprise` for some endpoints.
    pub service: String,
    pub sitekey: Option<String>,
    /// Must be passed to the challenge if present, or the solution is rejected.
    pub rqdata: Option<String>,
    pub rqtoken: Option<String>,
    pub session_id: Option<String>,
    pub should_serve_invisible: bool,
}

impl From<auth::CaptchaChallenge> for CaptchaChallenge {
    fn from(challenge: auth::CaptchaChallenge) -> Self {
        Self {
            service: challenge.service,
            sitekey: challenge.sitekey,
            rqdata: challenge.rqdata,
            rqtoken: challenge.rqtoken,
            session_id: challenge.session_id,
            should_serve_invisible: challenge.should_serve_invisible,
        }
    }
}

/// A QR code login running in the background. Releasing it cancels the login.
#[derive(uniffi::Object)]
pub struct QrLogin {
    core: Arc<auth::QrLogin>,
}

impl QrLogin {
    pub(crate) fn new(core: auth::QrLogin) -> Arc<Self> {
        Arc::new(Self {
            core: Arc::new(core),
        })
    }
}

#[uniffi::export]
impl QrLogin {
    /// The next event; waits while there is none. After `Done` or an error the login is
    /// over. Swift cancellation doesn't stop the wait; `cancel()` does.
    pub async fn next(&self) -> Result<QrEvent, LoginError> {
        // Polled directly: a detached task would take an event and lose it.
        self.core.next().await.map(Into::into).map_err(Into::into)
    }

    /// Answers a `Captcha` event; the login continues with the next event.
    pub async fn solve_captcha(&self, solution: String) -> Result<(), LoginError> {
        self.core.solve_captcha(solution).await.map_err(Into::into)
    }

    /// Ends the login; a waiting `next()` fails with `Cancelled`.
    pub fn cancel(&self) {
        self.core.cancel();
    }
}

/// What a QR code login reports, in order.
#[derive(uniffi::Enum)]
pub enum QrEvent {
    /// Show this URL as a QR code, replacing any earlier one.
    Code {
        url: String,
    },
    /// The code was scanned; the user confirms on their phone.
    Scanned {
        user: ScannedUser,
    },
    /// The user cancelled on their phone; a new code follows.
    CancelledOnPhone,
    /// Show the challenge, then call `solve_captcha`.
    Captcha {
        challenge: CaptchaChallenge,
    },
    Done {
        success: LoginSuccess,
    },
}

impl From<auth::QrEvent> for QrEvent {
    fn from(event: auth::QrEvent) -> Self {
        match event {
            auth::QrEvent::Code { url } => Self::Code { url },
            auth::QrEvent::Scanned(user) => Self::Scanned { user: user.into() },
            auth::QrEvent::CancelledOnPhone => Self::CancelledOnPhone,
            auth::QrEvent::Captcha(challenge) => Self::Captcha {
                challenge: challenge.into(),
            },
            auth::QrEvent::Done(success) => Self::Done {
                success: success.into(),
            },
        }
    }
}

/// The account that scanned the QR code, for a "check your phone" screen.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ScannedUser {
    pub id: UserId,
    pub username: String,
    /// `"0"` for users on the new username system.
    pub discriminator: String,
    /// Avatar hash, `None` for the default avatar.
    pub avatar: Option<String>,
}

impl From<auth::ScannedUser> for ScannedUser {
    fn from(user: auth::ScannedUser) -> Self {
        Self {
            id: user.id,
            username: user.username,
            discriminator: user.discriminator,
            avatar: user.avatar,
        }
    }
}
