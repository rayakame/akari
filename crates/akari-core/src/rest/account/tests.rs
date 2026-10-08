use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;
use crate::gateway::fake::client_with;
use crate::model::Snowflake;

const TOKEN: &str = "account-rest-test-token.secret";
const CHANNEL: u64 = 300_000_000_000_000_002;
const MESSAGES: &str = "/api/v9/channels/300000000000000002/messages";

fn page() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/messages_page.json")).unwrap()
}

fn created() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/message_create.json")).unwrap()
}

async fn rest(server: &MockServer) -> AccountRest {
    let client = client_with(
        "ws://127.0.0.1:9/".to_owned(),
        format!("{}/api/v9/", server.uri()),
    );
    AccountRest::new(
        client,
        Token::new(TOKEN.to_owned()),
        Duration::from_millis(10),
    )
}

fn channel() -> ChannelId {
    Snowflake::new(CHANNEL)
}

fn body(nonce: &str) -> CreateMessage {
    CreateMessage::new("hello".to_owned(), nonce.to_owned())
}

async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

fn rate_limited(retry_after: Option<f64>) -> ResponseTemplate {
    let mut body = json!({"message": "You are being rate limited.", "global": false});
    if let Some(retry_after) = retry_after {
        body["retry_after"] = retry_after.into();
    }
    ResponseTemplate::new(429).set_body_json(body)
}

fn chain(err: &dyn std::error::Error) -> String {
    let mut text = format!("{err} {err:?}");
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(&format!(" {cause} {cause:?}"));
        source = cause.source();
    }
    text
}

#[tokio::test]
async fn requests_carry_the_token_and_client_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(MESSAGES))
        .and(header("authorization", TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 50)
        .await
        .unwrap();

    let sent = &requests(&server).await[0];
    assert!(sent.headers.get("x-super-properties").is_some());
}

#[tokio::test]
async fn list_queries_use_one_cursor() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let rest = rest(&server).await;
    let id = Snowflake::new(400_000_000_000_000_010);

    for query in [
        Query::Latest,
        Query::Before(id),
        Query::After(id),
        Query::Around(id),
    ] {
        rest.list_messages(channel(), query, 25).await.unwrap();
    }

    let queries: Vec<String> = requests(&server)
        .await
        .iter()
        .map(|request| request.url.query().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        queries,
        [
            "limit=25",
            "limit=25&before=400000000000000010",
            "limit=25&after=400000000000000010",
            "limit=25&around=400000000000000010",
        ]
    );
}

#[tokio::test]
async fn a_page_comes_back_oldest_first() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .and(query_param("limit", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;

    let messages = rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap();

    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.get())
            .collect::<Vec<_>>(),
        [
            400_000_000_000_000_010,
            400_000_000_000_000_011,
            400_000_000_000_000_012
        ]
    );
}

#[tokio::test]
async fn a_broken_message_in_a_page_is_skipped() {
    let server = MockServer::start().await;
    let mut broken = page();
    broken[1]["author"] = json!({"username": "no id"});
    Mock::given(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(broken))
        .mount(&server)
        .await;

    let messages = rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap();

    assert_eq!(messages.len(), 2);
}

#[tokio::test]
async fn a_429_is_retried_after_its_retry_after() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(rate_limited(Some(0.05)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;

    let messages = rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap();

    assert_eq!(messages.len(), 3);
    assert_eq!(requests(&server).await.len(), 2);
}

#[tokio::test]
async fn a_429_without_a_delay_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(rate_limited(None))
        .mount(&server)
        .await;

    let err = rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap_err();

    assert!(
        matches!(err, RequestError::RateLimited { retry_after: None }),
        "{err:?}"
    );
    assert_eq!(requests(&server).await.len(), 1);
}

#[tokio::test]
async fn retries_stop_after_three() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(rate_limited(Some(0.01)))
        .mount(&server)
        .await;

    let err = rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            RequestError::RateLimited {
                retry_after: Some(_)
            }
        ),
        "{err:?}"
    );
    assert_eq!(requests(&server).await.len(), 4);
}

#[tokio::test]
async fn a_502_is_retried_once() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;
    let failing = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(ResponseTemplate::new(504))
        .mount(&failing)
        .await;

    let recovered = rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await;
    let failed = rest(&failing)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await;

    assert!(recovered.is_ok());
    assert!(failed.is_err());
    assert_eq!(requests(&failing).await.len(), 2);
}

#[tokio::test]
async fn a_send_retry_keeps_its_nonce() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(created()))
        .mount(&server)
        .await;

    let message = rest(&server)
        .await
        .create_message(channel(), &body("1213141516171819200"))
        .await
        .unwrap();

    assert_eq!(message.id.get(), 400_000_000_000_000_003);
    let bodies: Vec<Value> = requests(&server)
        .await
        .iter()
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
    assert_eq!(
        bodies[0],
        json!({"content": "hello", "nonce": "1213141516171819200", "tts": false, "flags": 0})
    );
}

#[tokio::test]
async fn a_401_stops_every_later_request() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"message": "401: Unauthorized", "code": 0})),
        )
        .mount(&server)
        .await;
    let rest = rest(&server).await;

    let first = rest
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap_err();
    let second = rest
        .create_message(channel(), &body("1"))
        .await
        .unwrap_err();

    assert!(matches!(first, RequestError::Unauthorized), "{first:?}");
    assert!(matches!(second, RequestError::Unauthorized), "{second:?}");
    assert_eq!(requests(&server).await.len(), 1);
    assert!(rest.is_unauthorized());
}

#[tokio::test]
async fn a_captcha_is_reported_not_solved() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "captcha_key": ["captcha-required"],
            "captcha_service": "hcaptcha",
            "captcha_sitekey": "site-key",
            "captcha_rqdata": "rq-data",
            "captcha_rqtoken": "rq-token"
        })))
        .mount(&server)
        .await;

    let err = rest(&server)
        .await
        .create_message(channel(), &body("1"))
        .await
        .unwrap_err();

    let RequestError::CaptchaRequired(challenge) = err else {
        panic!("expected a captcha, got {err:?}");
    };
    assert_eq!(challenge.sitekey.as_deref(), Some("site-key"));
    assert_eq!(challenge.rqdata.as_deref(), Some("rq-data"));
}

#[tokio::test]
async fn api_errors_keep_status_code_and_message() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"message": "Missing Permissions", "code": 50013})),
        )
        .mount(&server)
        .await;

    let err = rest(&server)
        .await
        .create_message(channel(), &body("1"))
        .await
        .unwrap_err();

    assert!(
        matches!(&err, RequestError::Discord { status: 403, code: 50013, message } if message == "Missing Permissions"),
        "{err:?}"
    );
}

#[tokio::test]
async fn requests_after_close_fail_with_closed() {
    let server = MockServer::start().await;
    let rest = rest(&server).await;

    rest.close();
    let err = rest
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap_err();

    assert!(matches!(err, RequestError::Closed), "{err:?}");
    assert!(requests(&server).await.is_empty());
}

#[tokio::test]
async fn errors_never_contain_the_token() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"message": "No", "code": 50001})),
        )
        .mount(&server)
        .await;
    let unreachable = AccountRest::new(
        client_with(
            "ws://127.0.0.1:9/".to_owned(),
            "http://127.0.0.1:9/api/v9/".to_owned(),
        ),
        Token::new(TOKEN.to_owned()),
        Duration::from_millis(10),
    );

    let api = rest(&server)
        .await
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap_err();
    let network = unreachable
        .list_messages(channel(), Query::Latest, 3)
        .await
        .unwrap_err();

    assert!(matches!(network, RequestError::Network(_)), "{network:?}");
    for err in [api, network] {
        assert!(!chain(&err).contains(TOKEN), "{}", chain(&err));
    }
}
