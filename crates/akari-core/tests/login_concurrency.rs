mod support;

use std::time::Duration;

use akari_core::Secret;
use akari_core::auth::{LoginError, LoginStep, MfaMethod, QrEvent};
use serde_json::json;
use support::remote_auth::RemoteAuthServer;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER_ID: &str = "100000000000000001";

async fn mount_password_login(rest: &MockServer, delay: Duration) {
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "user_id": USER_ID,
                    "mfa": true,
                    "totp": true,
                    "ticket": "ticket.1",
                    "login_instance_id": "instance-1",
                }))
                .set_delay(delay),
        )
        .mount(rest)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/mfa/totp"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token": "token.password"})))
        .mount(rest)
        .await;
}

#[tokio::test]
async fn password_login_finishes_while_the_qr_code_waits() {
    let rest = support::rest_server().await;
    mount_password_login(&rest, Duration::ZERO).await;
    let mut gateway = RemoteAuthServer::start().await;
    let client = support::client(&rest, &gateway);
    let qr = client.qr_login().unwrap();
    let mut session = gateway.accept().await;
    session.handshake(30_000).await;
    assert!(matches!(qr.next().await, Ok(QrEvent::Code { .. })));

    let password = client.password_login();
    let step = password
        .submit("me@example.com", Secret::new("pw".to_owned()))
        .await
        .unwrap();
    assert!(matches!(step, LoginStep::Mfa(_)), "{step:?}");
    let step = password
        .submit_mfa(MfaMethod::Totp, Secret::new("123456".to_owned()))
        .await
        .unwrap();
    assert!(matches!(step, LoginStep::Done(ref s) if s.token.expose() == "token.password"));

    qr.cancel();
    assert!(matches!(qr.next().await, Err(LoginError::Cancelled)));
    assert!(session.closed_by_client().await);
}

#[tokio::test]
async fn qr_login_finishes_while_a_password_login_is_cancelled() {
    let rest = support::rest_server().await;
    mount_password_login(&rest, Duration::from_secs(10)).await;
    let mut gateway = RemoteAuthServer::start().await;
    let client = support::client(&rest, &gateway);
    let qr = client.qr_login().unwrap();
    let password = client.password_login();

    let qr_side = async {
        let mut session = gateway.accept().await;
        session.handshake(30_000).await;
        assert!(matches!(qr.next().await, Ok(QrEvent::Code { .. })));
        session.scan(&format!("{USER_ID}:0:0:tester")).await;
        assert!(matches!(qr.next().await, Ok(QrEvent::Scanned(_))));
        Mock::given(path("/api/v9/users/@me/remote-auth/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"encrypted_token": session.encrypt(b"token.qr")})),
            )
            .mount(&rest)
            .await;
        session.finish("ticket-1").await;
        let done = qr.next().await;
        password.cancel();
        done
    };
    let (submitted, done) = tokio::join!(
        password.submit("me@example.com", Secret::new("pw".to_owned())),
        qr_side
    );

    assert!(
        matches!(submitted, Err(LoginError::Cancelled)),
        "{submitted:?}"
    );
    assert!(matches!(done, Ok(QrEvent::Done(ref s)) if s.token.expose() == "token.qr"));
}

#[tokio::test]
async fn a_new_login_works_after_one_was_cancelled_mid_fingerprint() {
    let rest = MockServer::start().await;
    Mock::given(path("/api/v9/experiments"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"fingerprint": "fp.slow"}))
                .set_delay(Duration::from_secs(10)),
        )
        .up_to_n_times(1)
        .mount(&rest)
        .await;
    Mock::given(path("/api/v9/experiments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fingerprint": "fp.2"})))
        .mount(&rest)
        .await;
    mount_password_login(&rest, Duration::ZERO).await;
    let gateway = RemoteAuthServer::start().await;
    let client = support::client(&rest, &gateway);

    let first = client.password_login();
    let (result, ()) = tokio::join!(
        first.submit("me@example.com", Secret::new("pw".to_owned())),
        async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            first.cancel();
        }
    );
    assert!(matches!(result, Err(LoginError::Cancelled)), "{result:?}");

    let step = client
        .password_login()
        .submit("me@example.com", Secret::new("pw".to_owned()))
        .await
        .unwrap();
    assert!(matches!(step, LoginStep::Mfa(_)), "{step:?}");
    let requests = rest.received_requests().await.unwrap();
    let login = requests
        .iter()
        .find(|request| request.url.path() == "/api/v9/auth/login")
        .unwrap();
    assert_eq!(login.headers["x-fingerprint"], "fp.2");
}
