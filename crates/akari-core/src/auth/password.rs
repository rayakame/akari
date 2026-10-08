use std::fmt;
use std::future::Future;

use percent_encoding::percent_decode_str;
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{Mutex, MutexGuard};
use tokio_util::sync::CancellationToken;

use super::{CaptchaChallenge, LoginError, LoginSuccess};
use crate::model::{Snowflake, UserMarker};
use crate::rest::{CaptchaSolution, RequestExtras, RestError};
use crate::{DiscordClient, Secret, Token};

/// What an email/password login needs next.
#[derive(Debug)]
pub enum LoginStep {
    Done(LoginSuccess),
    /// Show the challenge, then call [`PasswordLogin::solve_captcha`].
    Captcha(CaptchaChallenge),
    /// Ask for a code, then call [`PasswordLogin::submit_mfa`].
    Mfa(MfaChallenge),
    /// Discord wants this login location confirmed: by the link in an email
    /// ([`PasswordLogin::confirm_new_location`]) or by an SMS code
    /// ([`PasswordLogin::verify_phone`]).
    NewLocation(NewLocation),
}

/// The account has two-factor authentication enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MfaChallenge {
    pub methods: Vec<MfaMethod>,
    /// `PublicKeyCredentialRequestOptions` as a JSON string, when WebAuthn is available.
    pub webauthn_options: Option<String>,
    /// The masked phone number, once [`PasswordLogin::send_mfa_sms`] sent a code.
    pub sms_sent_to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfaMethod {
    /// An authenticator app code. Discord also accepts backup codes here.
    Totp,
    /// A code sent by [`PasswordLogin::send_mfa_sms`].
    Sms,
    Backup,
    /// `code` is the JSON of `PublicKeyCredential.toJSON()`.
    WebAuthn,
}

impl MfaMethod {
    fn path(self) -> &'static str {
        match self {
            Self::Totp => "auth/mfa/totp",
            Self::Sms => "auth/mfa/sms",
            Self::Backup => "auth/mfa/backup",
            Self::WebAuthn => "auth/mfa/webauthn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewLocation {
    Email,
    Phone,
}

/// An email/password login, one step at a time: each method returns the next [`LoginStep`].
/// A step that fails with a recoverable error stays pending for a retry; only one step runs
/// at a time.
pub struct PasswordLogin {
    client: DiscordClient,
    flow: Mutex<Flow>,
    cancel: CancellationToken,
}

#[derive(Default)]
struct Flow {
    credentials: Option<Credentials>,
    step: Step,
}

struct Credentials {
    login: Secret,
    password: Secret,
}

#[derive(Default)]
enum Step {
    #[default]
    Idle,
    Captcha {
        retry: Request,
        challenge: CaptchaChallenge,
    },
    Mfa(Mfa),
    NewLocation(NewLocation),
    Done,
}

#[derive(Clone)]
struct Mfa {
    ticket: Secret,
    login_instance_id: Option<String>,
    user_id: Snowflake<UserMarker>,
    challenge: MfaChallenge,
}

#[derive(Clone)]
enum Request {
    Login,
    Mfa {
        mfa: Mfa,
        method: MfaMethod,
        code: Secret,
    },
    SendSms(Mfa),
    AuthorizeIp {
        token: Secret,
        kind: NewLocation,
    },
    VerifyPhone(Secret),
}

#[derive(Deserialize)]
struct LoginResponse {
    user_id: Option<Snowflake<UserMarker>>,
    token: Option<String>,
    #[serde(default)]
    mfa: bool,
    ticket: Option<String>,
    login_instance_id: Option<String>,
    #[serde(default)]
    totp: bool,
    #[serde(default)]
    sms: bool,
    #[serde(default)]
    backup: bool,
    webauthn: Option<String>,
    #[serde(default)]
    required_actions: Vec<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct SmsResponse {
    phone: String,
}

impl PasswordLogin {
    pub(crate) fn new(client: DiscordClient) -> Self {
        Self {
            client,
            flow: Mutex::new(Flow::default()),
            cancel: CancellationToken::new(),
        }
    }

    /// Starts or restarts the login. `login` is an email address or an E.164 phone number.
    pub async fn submit(&self, login: &str, password: Secret) -> Result<LoginStep, LoginError> {
        let mut flow = self.lock()?;
        if matches!(flow.step, Step::Done) {
            return Err(LoginError::NoPendingStep);
        }
        flow.credentials = Some(Credentials {
            login: Secret::new(login.to_owned()),
            password,
        });
        flow.step = Step::Idle;
        self.run(&mut flow, Request::Login, None).await
    }

    /// Retries the request that asked for the CAPTCHA, with its solution.
    pub async fn solve_captcha(&self, solution: String) -> Result<LoginStep, LoginError> {
        let mut flow = self.lock()?;
        // The step stays in the flow until the request finishes, so dropping this future
        // can't lose the MFA ticket or a verification token.
        let Step::Captcha { retry, challenge } = &flow.step else {
            return Err(LoginError::NoPendingStep);
        };
        let retry = retry.clone();
        let solution = CaptchaSolution {
            key: Secret::new(solution),
            rqtoken: challenge.rqtoken.clone(),
            session_id: challenge.session_id.clone(),
        };
        self.run(&mut flow, retry, Some(solution)).await
    }

    /// Texts a code to the account's phone. Returns the MFA step again, with
    /// [`MfaChallenge::sms_sent_to`] set.
    pub async fn send_mfa_sms(&self) -> Result<LoginStep, LoginError> {
        let mut flow = self.lock()?;
        let mfa = Self::pending_mfa(&flow)?;
        self.run(&mut flow, Request::SendSms(mfa), None).await
    }

    pub async fn submit_mfa(
        &self,
        method: MfaMethod,
        code: Secret,
    ) -> Result<LoginStep, LoginError> {
        let mut flow = self.lock()?;
        let mfa = Self::pending_mfa(&flow)?;
        self.run(&mut flow, Request::Mfa { mfa, method, code }, None)
            .await
    }

    /// Confirms the login location with the link from Discord's email (the address it opens,
    /// containing `#token=…`) or with the bare token, then logs in again.
    pub async fn confirm_new_location(&self, link_or_token: &str) -> Result<LoginStep, LoginError> {
        let mut flow = self.lock()?;
        let Step::NewLocation(kind) = flow.step else {
            return Err(LoginError::NoPendingStep);
        };
        let token = verification_token(link_or_token).ok_or(LoginError::InvalidVerificationLink)?;
        self.run(
            &mut flow,
            Request::AuthorizeIp {
                token: Secret::new(token),
                kind,
            },
            None,
        )
        .await
    }

    /// Confirms the login location with the code Discord texted to the phone number used as
    /// the login, then logs in again.
    pub async fn verify_phone(&self, code: Secret) -> Result<LoginStep, LoginError> {
        let mut flow = self.lock()?;
        if !matches!(flow.step, Step::NewLocation(NewLocation::Phone)) {
            return Err(LoginError::NoPendingStep);
        }
        self.run(&mut flow, Request::VerifyPhone(code), None).await
    }

    /// Stops a running step and ends the flow; every later call returns
    /// [`LoginError::Cancelled`].
    pub fn cancel(&self) {
        self.cancel.cancel();
        if let Ok(mut flow) = self.flow.try_lock() {
            *flow = Flow::default();
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Flow>, LoginError> {
        if self.cancel.is_cancelled() {
            // cancel() can't clear a flow a running step holds; the next call catches up.
            if let Ok(mut flow) = self.flow.try_lock() {
                *flow = Flow::default();
            }
            return Err(LoginError::Cancelled);
        }
        self.flow.try_lock().map_err(|_| LoginError::Busy)
    }

    // A captcha on an MFA request still leaves the ticket usable for another code.
    fn pending_mfa(flow: &Flow) -> Result<Mfa, LoginError> {
        match &flow.step {
            Step::Mfa(mfa)
            | Step::Captcha {
                retry: Request::Mfa { mfa, .. } | Request::SendSms(mfa),
                ..
            } => Ok(mfa.clone()),
            _ => Err(LoginError::NoPendingStep),
        }
    }

    async fn run(
        &self,
        flow: &mut Flow,
        request: Request,
        captcha: Option<CaptchaSolution>,
    ) -> Result<LoginStep, LoginError> {
        let result = self.run_chain(flow, request, captcha).await;
        if self.cancel.is_cancelled() {
            *flow = Flow::default();
        }
        result
    }

    // AuthorizeIp and VerifyPhone continue with further requests, so one call can run a
    // chain; the CAPTCHA solution belongs to the first request only.
    async fn run_chain(
        &self,
        flow: &mut Flow,
        mut request: Request,
        mut captcha: Option<CaptchaSolution>,
    ) -> Result<LoginStep, LoginError> {
        loop {
            let fingerprint = self.guard(self.client.fingerprint()).await?;
            let solution = captcha.take();
            let extras = RequestExtras {
                fingerprint,
                captcha: solution.as_ref(),
                ..RequestExtras::default()
            };
            let rest = self.client.rest();
            request = match request {
                Request::Login => {
                    let credentials = flow.credentials.as_ref().ok_or(LoginError::NoPendingStep)?;
                    let body = json!({
                        "login": credentials.login.expose(),
                        "password": credentials.password.expose(),
                        "undelete": false,
                    });
                    let response = self
                        .guard(rest.post_json("auth/login", &body, &extras))
                        .await?;
                    return self.after_login(flow, response);
                }
                Request::Mfa { mfa, method, code } => {
                    let mut body = json!({"ticket": mfa.ticket.expose(), "code": code.expose()});
                    if let Some(instance) = &mfa.login_instance_id {
                        body["login_instance_id"] = json!(instance);
                    }
                    let response = self
                        .guard(rest.post_json::<_, TokenResponse>(method.path(), &body, &extras))
                        .await?;
                    return match response {
                        Ok(response) => Ok(Self::done(flow, mfa.user_id, response.token, false)),
                        Err(RestError::Captcha(challenge)) => Ok(Self::captcha(
                            flow,
                            Request::Mfa { mfa, method, code },
                            *challenge,
                        )),
                        Err(err) => Err(Self::back_to_mfa(flow, mfa, err)),
                    };
                }
                Request::SendSms(mut mfa) => {
                    let body = json!({"ticket": mfa.ticket.expose()});
                    let response = self
                        .guard(rest.post_json::<_, SmsResponse>(
                            "auth/mfa/sms/send",
                            &body,
                            &extras,
                        ))
                        .await?;
                    return match response {
                        Ok(response) => {
                            mfa.challenge.sms_sent_to = Some(response.phone);
                            let challenge = mfa.challenge.clone();
                            flow.step = Step::Mfa(mfa);
                            Ok(LoginStep::Mfa(challenge))
                        }
                        Err(RestError::Captcha(challenge)) => {
                            Ok(Self::captcha(flow, Request::SendSms(mfa), *challenge))
                        }
                        Err(err) => Err(Self::back_to_mfa(flow, mfa, err)),
                    };
                }
                Request::AuthorizeIp { token, kind } => {
                    let body = json!({"token": token.expose()});
                    match self
                        .guard(rest.post("auth/authorize-ip", &body, &extras))
                        .await?
                    {
                        Ok(()) => Request::Login,
                        Err(RestError::Captcha(challenge)) => {
                            let retry = Request::AuthorizeIp { token, kind };
                            return Ok(Self::captcha(flow, retry, *challenge));
                        }
                        Err(err) => {
                            flow.step = Step::NewLocation(kind);
                            return Err(LoginError::from_rest(err));
                        }
                    }
                }
                Request::VerifyPhone(code) => {
                    let credentials = flow.credentials.as_ref().ok_or(LoginError::NoPendingStep)?;
                    let body = json!({"phone": credentials.login.expose(), "code": code.expose()});
                    let response = self
                        .guard(rest.post_json::<_, TokenResponse>(
                            "phone-verifications/verify",
                            &body,
                            &extras,
                        ))
                        .await?;
                    match response {
                        Ok(response) => Request::AuthorizeIp {
                            token: Secret::new(response.token),
                            kind: NewLocation::Phone,
                        },
                        Err(RestError::Captcha(challenge)) => {
                            return Ok(Self::captcha(flow, Request::VerifyPhone(code), *challenge));
                        }
                        Err(err) => {
                            flow.step = Step::NewLocation(NewLocation::Phone);
                            return Err(LoginError::from_rest(err));
                        }
                    }
                }
            };
        }
    }

    fn after_login(
        &self,
        flow: &mut Flow,
        response: Result<LoginResponse, RestError>,
    ) -> Result<LoginStep, LoginError> {
        let response = match response {
            Ok(response) => response,
            Err(RestError::Captcha(challenge)) => {
                return Ok(Self::captcha(flow, Request::Login, *challenge));
            }
            Err(RestError::Api(api))
                if api.field_errors.iter().any(|error| {
                    error.path == "login" && error.code == "ACCOUNT_LOGIN_VERIFICATION_EMAIL"
                }) =>
            {
                flow.step = Step::NewLocation(NewLocation::Email);
                return Ok(LoginStep::NewLocation(NewLocation::Email));
            }
            // Phone logins are E.164 numbers; for an email login, 70007 isn't about this login.
            Err(RestError::Api(api))
                if api.code == 70007
                    && flow
                        .credentials
                        .as_ref()
                        .is_some_and(|credentials| credentials.login.expose().starts_with('+')) =>
            {
                flow.step = Step::NewLocation(NewLocation::Phone);
                return Ok(LoginStep::NewLocation(NewLocation::Phone));
            }
            Err(err) => {
                flow.step = Step::Idle;
                return Err(LoginError::from_rest(err));
            }
        };
        let Some(user_id) = response.user_id else {
            return Err(LoginError::UnexpectedResponse);
        };
        if let Some(token) = response.token {
            let update = response
                .required_actions
                .iter()
                .any(|action| action == "update_password");
            return Ok(Self::done(flow, user_id, token, update));
        }
        let Some(ticket) = response.ticket.filter(|_| response.mfa) else {
            return Err(LoginError::UnexpectedResponse);
        };
        let methods = [
            (response.totp, MfaMethod::Totp),
            (response.sms, MfaMethod::Sms),
            (response.backup, MfaMethod::Backup),
            (response.webauthn.is_some(), MfaMethod::WebAuthn),
        ]
        .into_iter()
        .filter_map(|(available, method)| available.then_some(method))
        .collect();
        let challenge = MfaChallenge {
            methods,
            webauthn_options: response.webauthn,
            sms_sent_to: None,
        };
        flow.step = Step::Mfa(Mfa {
            ticket: Secret::new(ticket),
            login_instance_id: response.login_instance_id,
            user_id,
            challenge: challenge.clone(),
        });
        Ok(LoginStep::Mfa(challenge))
    }

    fn done(
        flow: &mut Flow,
        user_id: Snowflake<UserMarker>,
        token: String,
        password_update_required: bool,
    ) -> LoginStep {
        flow.credentials = None;
        flow.step = Step::Done;
        LoginStep::Done(LoginSuccess {
            user_id,
            token: Token::new(token),
            password_update_required,
        })
    }

    fn captcha(flow: &mut Flow, retry: Request, challenge: CaptchaChallenge) -> LoginStep {
        flow.step = Step::Captcha {
            retry,
            challenge: challenge.clone(),
        };
        LoginStep::Captcha(challenge)
    }

    fn back_to_mfa(flow: &mut Flow, mfa: Mfa, err: RestError) -> LoginError {
        let err = LoginError::from_rest(err);
        flow.step = match err {
            LoginError::Expired => Step::Idle,
            _ => Step::Mfa(mfa),
        };
        err
    }

    async fn guard<T>(&self, request: impl Future<Output = T>) -> Result<T, LoginError> {
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(LoginError::Cancelled),
            result = request => Ok(result),
        }
    }
}

impl fmt::Debug for PasswordLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let step = match self.flow.try_lock() {
            Ok(flow) => match flow.step {
                Step::Idle => "idle",
                Step::Captcha { .. } => "captcha",
                Step::Mfa(_) => "mfa",
                Step::NewLocation(_) => "new location",
                Step::Done => "done",
            },
            Err(_) => "running",
        };
        f.debug_struct("PasswordLogin")
            .field("step", &step)
            .field("cancelled", &self.cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

fn verification_token(input: &str) -> Option<String> {
    let input = input.trim();
    let link = Url::parse(input).ok().or_else(|| {
        // A pasted address may lack its scheme; a bare token has no slash.
        if input.contains('/') {
            Url::parse(&format!("https://{input}")).ok()
        } else {
            None
        }
    });
    let token = match link {
        Some(url) => url
            .fragment()
            .and_then(|fragment| {
                fragment
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("token="))
                    .and_then(|token| percent_decode_str(token).decode_utf8().ok())
                    .map(|token| token.into_owned())
            })
            .or_else(|| {
                url.query_pairs()
                    .find(|(key, _)| key == "token")
                    .map(|(_, value)| value.into_owned())
            })?,
        None => input.to_owned(),
    };
    let plausible = !token.is_empty() && token.chars().all(|c| c.is_ascii_graphic());
    plausible.then_some(token)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
    use crate::{Endpoints, TokenStore, TokenStoreError};

    struct NoStore;

    impl TokenStore for NoStore {
        fn load(&self, _: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
            Ok(None)
        }
        fn save(&self, _: Snowflake<UserMarker>, _: &Token) -> Result<(), TokenStoreError> {
            Ok(())
        }
        fn delete(&self, _: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
            Ok(())
        }
    }

    async fn slow_login() -> (MockServer, PasswordLogin) {
        let server = MockServer::start().await;
        Mock::given(path("/api/v9/experiments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fingerprint": "f"})))
            .mount(&server)
            .await;
        Mock::given(path("/api/v9/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
            .mount(&server)
            .await;
        let host = HostInfo {
            os: DesktopOs::Linux,
            os_version: "6.8.0".to_owned(),
            arch: Arch::X64,
            system_locale: "en-US".to_owned(),
        };
        let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::Linux));
        let endpoints = Endpoints {
            api: format!("{}/api/v9/", server.uri()),
            allow_plaintext: true,
            ..Endpoints::default()
        };
        let client =
            DiscordClient::with_endpoints(properties, Arc::new(NoStore), endpoints).unwrap();
        (server, client.password_login())
    }

    #[tokio::test]
    async fn cancel_forgets_credentials_even_when_the_step_was_dropped() {
        let (_server, login) = slow_login().await;
        {
            let submit = login.submit("me@example.com", Secret::new("hunter2".to_owned()));
            tokio::pin!(submit);
            let running = tokio::time::timeout(Duration::from_millis(100), &mut submit).await;
            assert!(running.is_err());
            // The running step holds the lock, so cancel() can't clear the flow itself.
            login.cancel();
        }

        assert!(matches!(
            login
                .submit("me@example.com", Secret::new("x".to_owned()))
                .await,
            Err(LoginError::Cancelled)
        ));
        let flow = login.flow.try_lock().unwrap();
        assert!(flow.credentials.is_none());
        assert!(matches!(flow.step, Step::Idle));
    }

    #[test]
    fn verification_links_may_omit_the_scheme_and_be_percent_encoded() {
        assert_eq!(
            verification_token("discord.com/authorize-ip#token=abc.def").as_deref(),
            Some("abc.def")
        );
        assert_eq!(
            verification_token("https://discord.com/authorize-ip#token=a%2Eb%3Dc").as_deref(),
            Some("a.b=c")
        );
        assert_eq!(
            verification_token("discord.com/authorize-ip?token=x%2Dy").as_deref(),
            Some("x-y")
        );
        assert_eq!(verification_token("discord.com/authorize-ip"), None);
    }

    #[test]
    fn verification_tokens_come_from_links_or_stand_alone() {
        assert_eq!(
            verification_token("https://discord.com/authorize-ip#token=abc.def").as_deref(),
            Some("abc.def")
        );
        assert_eq!(
            verification_token("https://discord.com/authorize-ip?token=abc").as_deref(),
            Some("abc")
        );
        assert_eq!(
            verification_token("  abc.def\n").as_deref(),
            Some("abc.def")
        );
        assert_eq!(verification_token("https://discord.com/"), None);
        assert_eq!(verification_token("two words"), None);
        assert_eq!(verification_token(""), None);
    }
}
