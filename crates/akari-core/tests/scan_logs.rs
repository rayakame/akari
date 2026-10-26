// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use std::sync::Arc;

use akari_core::model::Snowflake;
use akari_core::{DiscordClient, Endpoints, Token};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::time::{Instant, sleep};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::gateway::{Logs, WAIT, accept, handshake, send};

const DM: u64 = 300_000_000_000_000_010;
const STRANGER: &str = "100000000000000002";

// A refused scan names its error's kind, never what Discord sent (a captcha's fields here).
#[tokio::test]
async fn a_refused_scan_logs_its_kind_not_discords_answer() {
    let logs = Logs::capture();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(format!("/api/v9/channels/{DM}/explicit-media")))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "captcha_key": ["captcha-required"],
            "captcha_sitekey": "leak-sitekey",
            "captcha_service": "hcaptcha",
            "captcha_rqdata": "leak-rqdata",
            "captcha_rqtoken": "leak-rqtoken",
        })))
        .mount(&server)
        .await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|err| panic!("bind failed: {err}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|err| panic!("no address: {err}"));
    let endpoints = Endpoints {
        gateway: format!("ws://{address}/"),
        api: format!("{}/api/v9/", server.uri()),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    let client =
        DiscordClient::with_endpoints(support::properties(), Arc::new(support::NoStore), endpoints)
            .unwrap_or_else(|err| panic!("client setup failed: {err}"));
    let account = client
        .account(
            Token::new("scan-log-test-token.secret".to_owned()),
            Snowflake::new(1),
        )
        .unwrap_or_else(|err| panic!("{err}"));
    account.connect().unwrap_or_else(|err| panic!("{err}"));
    let mut ws = accept(&listener).await;
    assert_eq!(handshake(&mut ws).await["op"], 2);
    let mut ready: Value = serde_json::from_str(include_str!("fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    ready["d"]["resume_gateway_url"] = format!("ws://{address}/resume").into();
    send(&mut ws, ready).await;
    account.view_channel(Snowflake::new(DM));
    let mut message: Value = serde_json::from_str(include_str!("fixtures/message_create.json"))
        .unwrap_or_else(|err| panic!("{err}"));
    message["channel_id"] = DM.to_string().into();
    message["author"]["id"] = STRANGER.into();
    if let Some(fields) = message.as_object_mut() {
        fields.remove("guild_id");
        fields.remove("member");
    }
    message["attachments"] = json!([{
        "id": "600000000000000001", "filename": "photo.png", "size": 1000,
        "url": "https://cdn.discordapp.com/attachments/1/2/photo.png",
        "proxy_url": "https://media.discordapp.net/attachments/1/2/photo.png",
        "width": 640, "height": 480,
    }]);
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": message}),
    )
    .await;

    let deadline = Instant::now() + WAIT;
    let line = loop {
        let text = logs.text();
        if let Some(line) = text
            .lines()
            .find(|line| line.contains("explicit media scan not asked"))
        {
            break line.to_owned();
        }
        assert!(
            Instant::now() < deadline,
            "no failed scan was logged: {text}"
        );
        sleep(std::time::Duration::from_millis(20)).await;
    };
    account.close();

    assert!(line.contains("captcha_required"), "{line}");
    let text = logs.text();
    assert!(!text.contains("leak-"), "{text}");
}
