use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::gateway::WAIT;
use super::support::{
    MemoryStore, USER, block_on, local_client, poll_once, refused_connection, rest_endpoints,
    rest_server, unreachable_client,
};
use crate::errors::{LoginError, NetworkErrorKind};
use crate::login::{LoginStep, MfaMethod};

fn mfa_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "user_id": USER.get().to_string(),
        "mfa": true,
        "sms": true,
        "ticket": "ticket.1",
        "login_instance_id": "instance-1",
        "backup": true,
        "totp": true,
        "webauthn": null,
    }))
}

#[tokio::test]
async fn a_password_login_with_mfa_ends_with_an_opaque_token() {
    let server = rest_server().await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/login"))
        .respond_with(mfa_response())
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/mfa/totp"))
        .and(body_json(json!({
            "ticket": "ticket.1",
            "login_instance_id": "instance-1",
            "code": "123456",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token": "token.mfa"})))
        .expect(1)
        .mount(&server)
        .await;
    let store = Arc::new(MemoryStore::default());
    let client = local_client(rest_endpoints(&server), store.clone());
    let login = client.password_login();

    let step = login
        .submit("me@example.com".to_owned(), "hunter2".to_owned())
        .await
        .unwrap();
    let LoginStep::Mfa { challenge } = step else {
        panic!("expected the MFA step");
    };
    assert!(challenge.methods.contains(&MfaMethod::Totp));
    let step = login
        .submit_mfa(MfaMethod::Totp, "123456".to_owned())
        .await
        .unwrap();
    let LoginStep::Done { success } = step else {
        panic!("expected the login to finish");
    };
    assert_eq!(success.user_id, USER);
    assert!(!success.password_update_required);

    client
        .save_token(success.user_id, success.token)
        .await
        .unwrap();
    assert_eq!(store.token(USER).as_deref(), Some("token.mfa"));
}

#[tokio::test]
async fn invalid_credentials_keep_discords_message() {
    let server = rest_server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 50035,
            "message": "Invalid Form Body",
            "errors": {
                "login": {"_errors": [{"code": "INVALID_LOGIN", "message": "Login or password is invalid."}]},
            },
        })))
        .mount(&server)
        .await;
    let client = local_client(rest_endpoints(&server), Arc::new(MemoryStore::default()));

    let err = client
        .password_login()
        .submit("me@example.com".to_owned(), "wrong".to_owned())
        .await
        .map(|_| ())
        .unwrap_err();

    assert_eq!(
        err,
        LoginError::InvalidCredentials {
            message: "Login or password is invalid.".to_owned()
        }
    );
}

#[test]
fn a_login_without_network_reports_the_kind() {
    let client = unreachable_client(Arc::new(MemoryStore::default()));

    let err = block_on(
        client
            .password_login()
            .submit("me@example.com".to_owned(), "hunter2".to_owned()),
    )
    .map(|_| ())
    .unwrap_err();

    assert_eq!(
        err,
        LoginError::Network {
            kind: NetworkErrorKind::Connect
        }
    );
}

#[tokio::test]
async fn login_errors_map_one_to_one() {
    use akari_core::auth::LoginError as Core;

    let cases = [
        (
            Core::InvalidCredentials {
                message: "m".to_owned(),
            },
            LoginError::InvalidCredentials {
                message: "m".to_owned(),
            },
        ),
        (Core::InvalidMfaCode, LoginError::InvalidMfaCode),
        (Core::Expired, LoginError::Expired),
        (Core::AccountDisabled, LoginError::AccountDisabled),
        (
            Core::AccountScheduledForDeletion,
            LoginError::AccountScheduledForDeletion,
        ),
        (Core::AccountSuspended, LoginError::AccountSuspended),
        (
            Core::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
                global: true,
            },
            LoginError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
                global: true,
            },
        ),
        (Core::Blocked, LoginError::Blocked),
        (Core::SmsUnavailable, LoginError::SmsUnavailable),
        (
            Core::InvalidVerificationLink,
            LoginError::InvalidVerificationLink,
        ),
        (
            Core::Discord {
                code: 40002,
                message: "m".to_owned(),
            },
            LoginError::Discord {
                code: 40002,
                message: "m".to_owned(),
            },
        ),
        (
            Core::RemoteAuth("m".to_owned()),
            LoginError::RemoteAuth {
                message: "m".to_owned(),
            },
        ),
        (Core::UnexpectedResponse, LoginError::UnexpectedResponse),
        (Core::Cancelled, LoginError::Cancelled),
        (Core::NoPendingStep, LoginError::NoPendingStep),
        (Core::Busy, LoginError::Busy),
        (
            Core::Network(refused_connection().await),
            LoginError::Network {
                kind: NetworkErrorKind::Connect,
            },
        ),
        (Core::NoRuntime, LoginError::UnexpectedResponse),
    ];

    for (core, expected) in cases {
        assert_eq!(LoginError::from(core), expected);
    }
}

#[tokio::test]
async fn cancel_ends_a_running_step() {
    let server = rest_server().await;
    Mock::given(path("/api/v9/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let client = local_client(rest_endpoints(&server), Arc::new(MemoryStore::default()));
    let login = client.password_login();
    let mut submit = pin!(login.submit("me@example.com".to_owned(), "hunter2".to_owned()));

    assert!(poll_once(submit.as_mut()).is_pending());
    tokio::time::timeout(WAIT, async {
        while !login_requested(&server).await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the login request never reached the server");
    login.cancel();

    let result = tokio::time::timeout(WAIT, submit)
        .await
        .expect("cancel didn't end the step");
    assert_eq!(result.map(|_| ()), Err(LoginError::Cancelled));
}

async fn login_requested(server: &wiremock::MockServer) -> bool {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .any(|request| request.url.path() == "/api/v9/auth/login")
}

#[test]
fn qr_login_starts_outside_a_runtime_and_cancel_ends_next() {
    let client = unreachable_client(Arc::new(MemoryStore::default()));

    let qr = std::thread::spawn(move || {
        assert!(tokio::runtime::Handle::try_current().is_err());
        client.qr_login()
    })
    .join()
    .unwrap()
    .unwrap();
    let mut next = pin!(qr.next());

    assert!(poll_once(next.as_mut()).is_pending());
    qr.cancel();

    assert!(matches!(
        poll_once(next.as_mut()),
        Poll::Ready(Err(LoginError::Cancelled))
    ));
}
