mod support;

use std::time::{Duration, Instant};

use akari_core::auth::{LoginError, QrEvent};
use akari_core::model::Snowflake;
use serde_json::json;
use support::remote_auth::RemoteAuthServer;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER: &str = "100000000000000001:0:0:tester";

async fn mount_exchange(rest: &MockServer, ticket: &str, encrypted_token: String) {
    Mock::given(method("POST"))
        .and(path("/api/v9/users/@me/remote-auth/login"))
        .and(header("x-fingerprint", "fp.1"))
        .and(body_json(json!({"ticket": ticket})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"encrypted_token": encrypted_token})),
        )
        .mount(rest)
        .await;
}

#[track_caller]
fn code_url(event: Result<QrEvent, LoginError>) -> String {
    match event {
        Ok(QrEvent::Code { url }) => url,
        other => panic!("expected a QR code, got {other:?}"),
    }
}

#[tokio::test]
async fn scanning_and_confirming_returns_the_token() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let client = support::client(&rest, &gateway);
    let qr = client.qr_login().unwrap();

    let mut session = gateway.accept().await;
    assert_eq!(session.header("origin"), Some("https://discord.com"));
    assert_eq!(
        session.header("user-agent"),
        Some(client.properties().browser_user_agent.as_str())
    );
    let fingerprint = session.handshake(30_000).await;
    assert_eq!(
        code_url(qr.next().await),
        format!("https://discord.com/ra/{fingerprint}")
    );

    session.scan(USER).await;
    let Ok(QrEvent::Scanned(user)) = qr.next().await else {
        panic!("expected the scanning user");
    };
    assert_eq!(user.id, Snowflake::new(100_000_000_000_000_001));
    assert_eq!(user.username, "tester");
    assert_eq!(user.avatar, None);

    mount_exchange(&rest, "ticket-1", session.encrypt(b"token.qr")).await;
    session.finish("ticket-1").await;
    let Ok(QrEvent::Done(success)) = qr.next().await else {
        panic!("expected the login to finish");
    };
    assert_eq!(success.token.expose(), "token.qr");
    assert_eq!(success.user_id, user.id);
    assert!(matches!(qr.next().await, Err(LoginError::NoPendingStep)));
}

#[tokio::test]
async fn cancelling_on_the_phone_shows_a_new_code() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();

    let mut first = gateway.accept().await;
    let first_code = format!("https://discord.com/ra/{}", first.handshake(30_000).await);
    assert_eq!(code_url(qr.next().await), first_code);
    first.scan(USER).await;
    assert!(matches!(qr.next().await, Ok(QrEvent::Scanned(_))));
    first.send(json!({"op": "cancel"})).await;
    first.close(1000).await;

    assert!(matches!(qr.next().await, Ok(QrEvent::CancelledOnPhone)));
    let mut second = gateway.accept().await;
    let second_code = format!("https://discord.com/ra/{}", second.handshake(30_000).await);
    assert_ne!(second_code, first_code);
    assert_eq!(code_url(qr.next().await), second_code);
}

#[tokio::test]
async fn an_expired_session_restarts_with_a_new_code() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();

    let mut first = gateway.accept().await;
    let first_code = code_url({
        first.handshake(30_000).await;
        qr.next().await
    });
    first.close(4003).await;

    let mut second = gateway.accept().await;
    let fingerprint = second.handshake(30_000).await;
    let second_code = code_url(qr.next().await);
    assert_eq!(second_code, format!("https://discord.com/ra/{fingerprint}"));
    assert_ne!(second_code, first_code);
}

#[tokio::test]
async fn repeated_handshake_failures_end_the_login() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();
    let started = Instant::now();

    for code in [4002, 4001, 4002] {
        let mut session = gateway.accept().await;
        session.hello(30_000).await;
        assert_eq!(
            session.recv().await.map(|init| init["op"].clone()),
            Some(json!("init"))
        );
        session.close(code).await;
    }

    let result = qr.next().await;
    assert!(
        matches!(result, Err(LoginError::RemoteAuth(_))),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[tokio::test]
async fn a_wrong_fingerprint_restarts_the_session() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();

    let mut forged = gateway.accept().await;
    forged.handshake_until_fingerprint(30_000).await;
    forged
        .send(json!({"op": "pending_remote_init", "fingerprint": "not-our-key"}))
        .await;
    assert!(forged.closed_by_client().await);

    let mut honest = gateway.accept().await;
    let fingerprint = honest.handshake(30_000).await;
    assert_eq!(
        code_url(qr.next().await),
        format!("https://discord.com/ra/{fingerprint}")
    );
}

#[tokio::test]
async fn missing_heartbeat_acks_restart_the_session() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();

    let mut silent = gateway.accept().await;
    silent.ack = false;
    silent.handshake(100).await;
    code_url(qr.next().await);
    assert!(silent.closed_by_client().await);
    assert!(silent.heartbeats >= 1);

    let mut next = gateway.accept().await;
    let fingerprint = next.handshake(30_000).await;
    assert_eq!(
        code_url(qr.next().await),
        format!("https://discord.com/ra/{fingerprint}")
    );
}

#[tokio::test]
async fn a_captcha_on_the_ticket_exchange_is_solved_and_retried() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();
    let mut session = gateway.accept().await;
    session.handshake(30_000).await;
    code_url(qr.next().await);
    session.scan(USER).await;
    assert!(matches!(qr.next().await, Ok(QrEvent::Scanned(_))));

    Mock::given(path("/api/v9/users/@me/remote-auth/login"))
        .and(header("x-captcha-key", "solved"))
        .and(header("x-captcha-rqtoken", "rq-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"encrypted_token": session.encrypt(b"token.after-captcha")})),
        )
        .expect(1)
        .mount(&rest)
        .await;
    Mock::given(path("/api/v9/users/@me/remote-auth/login"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "captcha_key": ["captcha-required"],
            "captcha_sitekey": "site-key",
            "captcha_service": "hcaptcha",
            "captcha_rqtoken": "rq-token",
        })))
        .expect(1)
        .mount(&rest)
        .await;
    session.finish("ticket-1").await;

    let Ok(QrEvent::Captcha(challenge)) = qr.next().await else {
        panic!("expected a captcha");
    };
    assert_eq!(challenge.sitekey.as_deref(), Some("site-key"));
    assert!(matches!(
        qr.solve_captcha("solved".to_owned()).await,
        Ok(())
    ));
    let Ok(QrEvent::Done(success)) = qr.next().await else {
        panic!("expected the login to finish");
    };
    assert_eq!(success.token.expose(), "token.after-captcha");
}

#[tokio::test]
async fn solving_a_captcha_nobody_asked_for_is_rejected() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();
    let mut session = gateway.accept().await;
    session.handshake(30_000).await;
    code_url(qr.next().await);

    assert!(matches!(
        qr.solve_captcha("x".to_owned()).await,
        Err(LoginError::NoPendingStep)
    ));
}

#[tokio::test]
async fn cancel_closes_the_connection() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();
    let mut session = gateway.accept().await;
    session.handshake(30_000).await;
    code_url(qr.next().await);

    qr.cancel();

    assert!(matches!(qr.next().await, Err(LoginError::Cancelled)));
    assert!(session.closed_by_client().await);
}

#[tokio::test]
async fn dropping_the_login_closes_the_connection() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();
    let mut session = gateway.accept().await;
    session.handshake(30_000).await;

    drop(qr);

    assert!(session.closed_by_client().await);
}

#[tokio::test]
async fn an_unread_login_keeps_heartbeating_and_keeps_only_the_newest_code() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();

    let mut fingerprint = String::new();
    for round in 0..3 {
        let mut session = gateway.accept().await;
        fingerprint = session.handshake(50).await;
        assert!(
            session.pump(Duration::from_millis(300)).await,
            "round {round}"
        );
        assert!(
            session.heartbeats >= 3,
            "round {round}: {} heartbeats",
            session.heartbeats
        );
        if round < 2 {
            session.close(4003).await;
        }
    }

    assert_eq!(
        code_url(qr.next().await),
        format!("https://discord.com/ra/{fingerprint}")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), qr.next())
            .await
            .is_err(),
        "older codes were still queued"
    );
}

#[tokio::test]
async fn timeouts_before_any_code_count_as_failures() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();

    for _ in 0..3 {
        let mut session = gateway.accept().await;
        session.hello(30_000).await;
        assert!(session.recv().await.is_some());
        session.close(4003).await;
    }

    let result = tokio::time::timeout(Duration::from_secs(5), qr.next()).await;
    assert!(
        matches!(result, Ok(Err(LoginError::RemoteAuth(_)))),
        "{result:?}"
    );
}

#[test]
fn starting_outside_a_tokio_runtime_is_an_error() {
    let client = akari_core::DiscordClient::new(
        support::properties(),
        std::sync::Arc::new(support::NoStore),
    )
    .unwrap();

    assert!(matches!(client.qr_login(), Err(LoginError::NoRuntime)));
}

#[tokio::test]
async fn oversized_packets_end_the_session() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let _qr = support::client(&rest, &gateway).qr_login().unwrap();

    let mut session = gateway.accept().await;
    session.handshake(30_000).await;
    session
        .send(json!({"op": "something_new", "padding": "x".repeat(64 * 1024)}))
        .await;

    assert!(session.closed_by_client().await);
    gateway.accept().await;
}

#[tokio::test]
async fn a_gateway_that_drops_every_code_at_once_is_backed_off() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let _qr = support::client(&rest, &gateway).qr_login().unwrap();

    let started = Instant::now();
    let mut sessions = 0;
    while started.elapsed() < Duration::from_secs(2) {
        let Ok(mut session) =
            tokio::time::timeout(Duration::from_secs(2) - started.elapsed(), gateway.accept())
                .await
        else {
            break;
        };
        sessions += 1;
        session.handshake(30_000).await;
        session.close(4003).await;
    }

    assert!(sessions <= 4, "{sessions} sessions in 2 s");
}

#[tokio::test]
async fn an_unreadable_ticket_payload_restarts_the_session() {
    let rest = support::rest_server().await;
    let mut gateway = RemoteAuthServer::start().await;
    let qr = support::client(&rest, &gateway).qr_login().unwrap();
    let mut first = gateway.accept().await;
    first.handshake(30_000).await;
    code_url(qr.next().await);

    first.scan("not a user payload").await;

    assert!(first.closed_by_client().await);
    let mut second = gateway.accept().await;
    let fingerprint = second.handshake(30_000).await;
    assert_eq!(
        code_url(qr.next().await),
        format!("https://discord.com/ra/{fingerprint}")
    );
}
