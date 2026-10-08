use std::sync::Arc;
use std::time::{Duration, Instant};

use akari_core::auth::{LoginError, LoginStep, MfaChallenge, MfaMethod, NewLocation};
use akari_core::model::Snowflake;
use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
use akari_core::{DiscordClient, Endpoints, Secret, Token, TokenStore, TokenStoreError};
use serde_json::{Value, json};
use wiremock::matchers::{body_json, header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER_ID: &str = "100000000000000001";
const PASSWORD: &str = "hunter2-secret";

struct NoStore;

impl TokenStore for NoStore {
    fn load(
        &self,
        _: Snowflake<akari_core::model::UserMarker>,
    ) -> Result<Option<Token>, TokenStoreError> {
        Ok(None)
    }
    fn save(
        &self,
        _: Snowflake<akari_core::model::UserMarker>,
        _: &Token,
    ) -> Result<(), TokenStoreError> {
        Ok(())
    }
    fn delete(&self, _: Snowflake<akari_core::model::UserMarker>) -> Result<(), TokenStoreError> {
        Ok(())
    }
}

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v9/experiments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fingerprint": "fp.1"})))
        .mount(&server)
        .await;
    server
}

fn client(server: &MockServer) -> DiscordClient {
    let host = HostInfo {
        os: DesktopOs::MacOs,
        os_version: "25.0.0".to_owned(),
        arch: Arch::Arm64,
        system_locale: "en-US".to_owned(),
    };
    let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs));
    let endpoints = Endpoints {
        api: format!("{}/api/v9/", server.uri()),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    DiscordClient::with_endpoints(properties, Arc::new(NoStore), endpoints)
        .unwrap_or_else(|err| panic!("client setup failed: {err}"))
}

fn password() -> Secret {
    Secret::new(PASSWORD.to_owned())
}

fn login_body(login: &str) -> Value {
    json!({"login": login, "password": PASSWORD, "undelete": false})
}

fn token_response(token: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "user_id": USER_ID,
        "token": token,
        "user_settings": {"locale": "en-US", "theme": "dark"},
    }))
}

fn mfa_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "user_id": USER_ID,
        "mfa": true,
        "sms": true,
        "ticket": "ticket.1",
        "login_instance_id": "instance-1",
        "backup": true,
        "totp": true,
        "webauthn": null,
    }))
}

fn captcha_response() -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(json!({
        "captcha_key": ["captcha-required"],
        "captcha_sitekey": "site-key",
        "captcha_service": "hcaptcha",
        "captcha_rqdata": "rq-data",
        "captcha_rqtoken": "rq-token",
        "captcha_session_id": "session-1",
    }))
}

async fn mount_mfa_login(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/login"))
        .respond_with(mfa_response())
        .mount(server)
        .await;
}

#[track_caller]
fn expect_done(step: LoginStep, token: &str) {
    let LoginStep::Done(success) = step else {
        panic!("expected Done, got {step:?}");
    };
    assert_eq!(success.user_id, Snowflake::new(100_000_000_000_000_001));
    assert_eq!(success.token.expose(), token);
}

#[track_caller]
fn expect_mfa(step: LoginStep) -> MfaChallenge {
    match step {
        LoginStep::Mfa(challenge) => challenge,
        other => panic!("expected Mfa, got {other:?}"),
    }
}

#[tokio::test]
async fn correct_credentials_return_the_token() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/login"))
        .and(header("x-fingerprint", "fp.1"))
        .and(body_json(login_body("me@example.com")))
        .respond_with(token_response("token.ok"))
        .expect(1)
        .mount(&server)
        .await;

    let step = client(&server)
        .password_login()
        .submit("me@example.com", password())
        .await
        .unwrap();

    let LoginStep::Done(success) = &step else {
        panic!("expected Done, got {step:?}");
    };
    assert!(!success.password_update_required);
    expect_done(step, "token.ok");
}

#[tokio::test]
async fn required_password_update_is_flagged() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": USER_ID,
            "token": "token.ok",
            "required_actions": ["update_password"],
        })))
        .mount(&server)
        .await;

    let step = client(&server)
        .password_login()
        .submit("me@example.com", password())
        .await
        .unwrap();

    assert!(matches!(step, LoginStep::Done(ref success) if success.password_update_required));
}

#[tokio::test]
async fn captcha_is_solved_by_resending_the_login() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .and(header("x-captcha-key", "solved"))
        .and(header("x-captcha-rqtoken", "rq-token"))
        .and(header("x-captcha-session-id", "session-1"))
        .and(body_json(login_body("me@example.com")))
        .respond_with(token_response("token.after-captcha"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(captcha_response())
        .expect(1)
        .mount(&server)
        .await;
    let login = client(&server).password_login();

    let step = login.submit("me@example.com", password()).await.unwrap();
    let LoginStep::Captcha(challenge) = step else {
        panic!("expected a captcha, got {step:?}");
    };
    assert_eq!(challenge.sitekey.as_deref(), Some("site-key"));
    assert_eq!(challenge.rqdata.as_deref(), Some("rq-data"));

    let step = login.solve_captcha("solved".to_owned()).await.unwrap();
    expect_done(step, "token.after-captcha");
}

#[tokio::test]
async fn totp_finishes_an_mfa_login() {
    let server = server().await;
    mount_mfa_login(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/mfa/totp"))
        .and(header("x-fingerprint", "fp.1"))
        .and(body_json(json!({
            "ticket": "ticket.1",
            "login_instance_id": "instance-1",
            "code": "123456",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token": "token.mfa"})))
        .expect(1)
        .mount(&server)
        .await;
    let login = client(&server).password_login();

    let challenge = expect_mfa(login.submit("me@example.com", password()).await.unwrap());
    assert_eq!(
        challenge.methods,
        [MfaMethod::Totp, MfaMethod::Sms, MfaMethod::Backup]
    );
    assert_eq!(challenge.webauthn_options, None);
    assert_eq!(challenge.sms_sent_to, None);

    let step = login
        .submit_mfa(MfaMethod::Totp, Secret::new("123456".to_owned()))
        .await
        .unwrap();
    expect_done(step, "token.mfa");
}

#[tokio::test]
async fn sms_mfa_sends_the_code_first() {
    let server = server().await;
    mount_mfa_login(&server).await;
    Mock::given(path("/api/v9/auth/mfa/sms/send"))
        .and(body_json(json!({"ticket": "ticket.1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"phone": "+*******0085"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/mfa/sms"))
        .and(body_json(json!({
            "ticket": "ticket.1",
            "login_instance_id": "instance-1",
            "code": "654321",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token": "token.sms"})))
        .expect(1)
        .mount(&server)
        .await;
    let login = client(&server).password_login();
    login.submit("me@example.com", password()).await.unwrap();

    let challenge = expect_mfa(login.send_mfa_sms().await.unwrap());
    assert_eq!(challenge.sms_sent_to.as_deref(), Some("+*******0085"));

    let step = login
        .submit_mfa(MfaMethod::Sms, Secret::new("654321".to_owned()))
        .await
        .unwrap();
    expect_done(step, "token.sms");
}

#[tokio::test]
async fn webauthn_options_are_passed_through() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": USER_ID,
            "mfa": true,
            "ticket": "ticket.1",
            "webauthn": "{\"publicKey\":{\"challenge\":\"abc\"}}",
        })))
        .mount(&server)
        .await;

    let challenge = expect_mfa(
        client(&server)
            .password_login()
            .submit("me@example.com", password())
            .await
            .unwrap(),
    );

    assert_eq!(challenge.methods, [MfaMethod::WebAuthn]);
    assert_eq!(
        challenge.webauthn_options.as_deref(),
        Some("{\"publicKey\":{\"challenge\":\"abc\"}}")
    );
}

#[tokio::test]
async fn captcha_on_the_mfa_endpoint_replays_the_code() {
    let server = server().await;
    mount_mfa_login(&server).await;
    Mock::given(path("/api/v9/auth/mfa/totp"))
        .and(header_exists("x-captcha-key"))
        .and(body_json(json!({
            "ticket": "ticket.1",
            "login_instance_id": "instance-1",
            "code": "123456",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token": "token.mfa"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/mfa/totp"))
        .respond_with(captcha_response())
        .expect(1)
        .mount(&server)
        .await;
    let login = client(&server).password_login();
    login.submit("me@example.com", password()).await.unwrap();

    let step = login
        .submit_mfa(MfaMethod::Totp, Secret::new("123456".to_owned()))
        .await
        .unwrap();
    assert!(matches!(step, LoginStep::Captcha(_)), "{step:?}");

    expect_done(
        login.solve_captcha("solved".to_owned()).await.unwrap(),
        "token.mfa",
    );
}

#[tokio::test]
async fn wrong_mfa_code_keeps_the_step_pending() {
    let server = server().await;
    mount_mfa_login(&server).await;
    Mock::given(path("/api/v9/auth/mfa/totp"))
        .and(body_json(json!({
            "ticket": "ticket.1",
            "login_instance_id": "instance-1",
            "code": "000000",
        })))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "message": "Invalid two-factor code",
            "code": 60008,
        })))
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/mfa/totp"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token": "token.mfa"})))
        .mount(&server)
        .await;
    let login = client(&server).password_login();
    login.submit("me@example.com", password()).await.unwrap();

    let err = login
        .submit_mfa(MfaMethod::Totp, Secret::new("000000".to_owned()))
        .await
        .unwrap_err();
    assert!(matches!(err, LoginError::InvalidMfaCode), "{err:?}");

    let step = login
        .submit_mfa(MfaMethod::Totp, Secret::new("123456".to_owned()))
        .await
        .unwrap();
    expect_done(step, "token.mfa");
}

#[tokio::test]
async fn new_location_is_confirmed_with_the_email_link() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/authorize-ip"))
        .and(body_json(json!({"token": "ip.token"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 50035,
            "message": "Invalid Form Body",
            "errors": {"login": {"_errors": [{
                "code": "ACCOUNT_LOGIN_VERIFICATION_EMAIL",
                "message": "New login location detected, please check your e-mail.",
            }]}},
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/login"))
        .and(body_json(login_body("me@example.com")))
        .respond_with(token_response("token.new-location"))
        .expect(1)
        .mount(&server)
        .await;
    let login = client(&server).password_login();

    let step = login.submit("me@example.com", password()).await.unwrap();
    assert!(
        matches!(step, LoginStep::NewLocation(NewLocation::Email)),
        "{step:?}"
    );

    let step = login
        .confirm_new_location("https://discord.com/authorize-ip#token=ip.token")
        .await
        .unwrap();
    expect_done(step, "token.new-location");
}

#[tokio::test]
async fn new_location_accepts_a_bare_token_and_rejects_garbage() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 50035,
            "message": "Invalid Form Body",
            "errors": {"login": {"_errors": [{"code": "ACCOUNT_LOGIN_VERIFICATION_EMAIL", "message": "Check your e-mail."}]}},
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/authorize-ip"))
        .and(body_json(json!({"token": "bare.token"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(token_response("token.ok"))
        .mount(&server)
        .await;
    let login = client(&server).password_login();
    login.submit("me@example.com", password()).await.unwrap();

    for garbage in ["", "hello world", "https://discord.com/authorize-ip"] {
        let err = login.confirm_new_location(garbage).await.unwrap_err();
        assert!(
            matches!(err, LoginError::InvalidVerificationLink),
            "{garbage:?}: {err:?}"
        );
    }

    expect_done(
        login.confirm_new_location(" bare.token\n").await.unwrap(),
        "token.ok",
    );
}

#[tokio::test]
async fn new_location_by_phone_verifies_the_sms_code() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 70007,
            "message": "You need to verify your phone number in order to perform this action.",
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/phone-verifications/verify"))
        .and(body_json(
            json!({"phone": "+15555550100", "code": "112233"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token": "phone.token"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/authorize-ip"))
        .and(body_json(json!({"token": "phone.token"})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/login"))
        .and(body_json(login_body("+15555550100")))
        .respond_with(token_response("token.phone"))
        .expect(1)
        .mount(&server)
        .await;
    let login = client(&server).password_login();

    let step = login.submit("+15555550100", password()).await.unwrap();
    assert!(
        matches!(step, LoginStep::NewLocation(NewLocation::Phone)),
        "{step:?}"
    );

    let step = login
        .verify_phone(Secret::new("112233".to_owned()))
        .await
        .unwrap();
    expect_done(step, "token.phone");
}

#[tokio::test]
async fn wrong_password_shows_discords_message_and_allows_another_try() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .and(body_json(login_body("me@example.com")))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 50035,
            "message": "Invalid Form Body",
            "errors": {
                "login": {"_errors": [{"code": "INVALID_LOGIN", "message": "Login or password is invalid."}]},
                "password": {"_errors": [{"code": "INVALID_LOGIN", "message": "Login or password is invalid."}]},
            },
        })))
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(token_response("token.second-try"))
        .mount(&server)
        .await;
    let login = client(&server).password_login();

    let err = login
        .submit("me@example.com", password())
        .await
        .unwrap_err();
    assert!(
        matches!(err, LoginError::InvalidCredentials { ref message } if message == "Login or password is invalid."),
        "{err:?}"
    );

    let step = login.submit("me@example.org", password()).await.unwrap();
    expect_done(step, "token.second-try");
}

#[tokio::test]
async fn account_states_are_typed_errors() {
    for (code, check) in [
        (
            20013,
            (|err: &LoginError| matches!(err, LoginError::AccountDisabled))
                as fn(&LoginError) -> bool,
        ),
        (20011, |err| {
            matches!(err, LoginError::AccountScheduledForDeletion)
        }),
    ] {
        let server = server().await;
        Mock::given(path("/api/v9/auth/login"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"code": code, "message": "nope"})),
            )
            .mount(&server)
            .await;

        let err = client(&server)
            .password_login()
            .submit("me@example.com", password())
            .await
            .unwrap_err();

        assert!(check(&err), "{code}: {err:?}");
    }
}

#[tokio::test]
async fn rate_limits_are_reported_with_the_delay() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "message": "You are being rate limited.",
            "retry_after": 12.5,
            "global": false,
        })))
        .mount(&server)
        .await;

    let err = client(&server)
        .password_login()
        .submit("me@example.com", password())
        .await
        .unwrap_err();

    assert!(
        matches!(err, LoginError::RateLimited { retry_after: Some(d), global: false } if d == Duration::from_millis(12_500)),
        "{err:?}"
    );
}

#[tokio::test]
async fn steps_out_of_order_are_rejected() {
    let server = server().await;
    let login = client(&server).password_login();

    assert!(matches!(
        login
            .submit_mfa(MfaMethod::Totp, Secret::new("1".to_owned()))
            .await,
        Err(LoginError::NoPendingStep)
    ));
    assert!(matches!(
        login.solve_captcha("x".to_owned()).await,
        Err(LoginError::NoPendingStep)
    ));
    assert!(matches!(
        login.confirm_new_location("abc").await,
        Err(LoginError::NoPendingStep)
    ));
    assert!(matches!(
        login.send_mfa_sms().await,
        Err(LoginError::NoPendingStep)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_second_call_while_one_runs_is_busy() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(token_response("token.ok").set_delay(Duration::from_millis(300)))
        .mount(&server)
        .await;
    let login = client(&server).password_login();

    let (first, second) = tokio::join!(login.submit("me@example.com", password()), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        login.submit("me@example.com", password()).await
    });

    expect_done(first.unwrap(), "token.ok");
    assert!(matches!(second, Err(LoginError::Busy)), "{second:?}");
}

#[tokio::test]
async fn cancel_stops_a_running_request_and_the_flow() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(token_response("token.ok").set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let login = client(&server).password_login();
    let started = Instant::now();

    let (result, ()) = tokio::join!(login.submit("me@example.com", password()), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        login.cancel();
    });

    assert!(matches!(result, Err(LoginError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(matches!(
        login.submit("me@example.com", password()).await,
        Err(LoginError::Cancelled)
    ));
}

#[tokio::test]
async fn finished_flows_reject_further_steps() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(token_response("token.ok"))
        .mount(&server)
        .await;
    let login = client(&server).password_login();
    login.submit("me@example.com", password()).await.unwrap();

    assert!(matches!(
        login.submit("me@example.com", password()).await,
        Err(LoginError::NoPendingStep)
    ));
}

#[tokio::test]
async fn debug_output_never_contains_secrets() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(mfa_response())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/auth/mfa/totp"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"token": "token.very-secret"})),
        )
        .mount(&server)
        .await;
    let login = client(&server).password_login();

    let mfa = login.submit("me@example.com", password()).await.unwrap();
    let flow = format!("{login:?}");
    let done = login
        .submit_mfa(MfaMethod::Totp, Secret::new("123456".to_owned()))
        .await
        .unwrap();

    for output in [
        format!("{mfa:?}"),
        flow,
        format!("{done:?}"),
        format!("{login:?}"),
    ] {
        for secret in [PASSWORD, "token.very-secret", "ticket.1", "123456"] {
            assert!(!output.contains(secret), "{output} contains {secret}");
        }
    }
}

#[tokio::test]
async fn phone_verification_on_an_email_login_is_an_error_not_a_step() {
    let server = server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 70007,
            "message": "You need to verify your phone number in order to perform this action.",
        })))
        .mount(&server)
        .await;

    let result = client(&server)
        .password_login()
        .submit("me@example.com", password())
        .await;

    assert!(
        matches!(result, Err(LoginError::Discord { code: 70007, .. })),
        "{result:?}"
    );
}
