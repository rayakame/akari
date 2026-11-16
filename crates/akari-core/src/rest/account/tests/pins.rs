use std::sync::atomic::Ordering;

use wiremock::matchers::path_regex;

use super::reactions::{flaky, rest_at};
use super::*;

const MESSAGE: u64 = 400_000_000_000_000_003;
const PINS: &str = "/api/v9/channels/300000000000000002/messages/pins";
const ACK: &str = "/api/v9/channels/300000000000000002/pins/ack";
const PINNED_AT: &str = "2026-10-02T09:30:00.250999+00:00";

fn target() -> MessageId {
    Snowflake::new(MESSAGE)
}

fn pin(id: u64, pinned_at: Value) -> Value {
    let mut message = created();
    message["id"] = id.to_string().into();
    message["pinned"] = true.into();
    json!({"pinned_at": pinned_at, "message": message})
}

async fn answering(answer: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path_regex("/pins"))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

fn lines(requests: &[Request]) -> Vec<String> {
    requests
        .iter()
        .map(|request| {
            let query = request
                .url
                .query()
                .map(|query| format!("?{query}"))
                .unwrap_or_default();
            format!("{} {}{query}", request.method, request.url.path())
        })
        .collect()
}

fn listed(pins: Vec<Value>, more: bool) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"items": pins, "has_more": more}))
}

#[tokio::test]
async fn the_first_page_of_pins_asks_for_25_without_a_cursor() {
    let server = answering(listed(vec![pin(MESSAGE, PINNED_AT.into())], true)).await;

    let page = rest(&server)
        .await
        .list_pins(channel(), None, 25)
        .await
        .unwrap();

    let requests = requests(&server).await;
    assert_eq!(lines(&requests), [format!("GET {PINS}?limit=25")]);
    assert!(page.has_more);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].message.id, target());
    assert_eq!(
        page.items[0].pinned_at.map(Timestamp::unix_nanos),
        Some(1_790_933_400_250_999_000)
    );
}

#[tokio::test]
async fn a_later_page_asks_before_a_pin_time_written_as_the_official_client_writes_it() {
    let server = answering(listed(Vec::new(), false)).await;
    let before: Timestamp = serde_json::from_value(PINNED_AT.into()).unwrap();

    let page = rest(&server)
        .await
        .list_pins(channel(), Some(before), 25)
        .await
        .unwrap();

    assert_eq!(
        lines(&requests(&server).await),
        [format!(
            "GET {PINS}?limit=25&before=2026-10-02T09%3A30%3A00.250Z"
        )]
    );
    assert!(!page.has_more);
    assert!(page.items.is_empty());
}

// From the code read, not observed: the answer is read leniently.
#[tokio::test]
async fn a_pin_that_does_not_read_is_left_out_and_a_missing_time_or_more_is_none() {
    let broken = json!({"pinned_at": PINNED_AT, "message": {"id": "nope"}});
    let without_time = pin(MESSAGE, Value::Null);
    let odd_time = pin(MESSAGE + 1, "yesterday".into());
    let server = answering(ResponseTemplate::new(200).set_body_json(json!({
        "items": [broken, without_time, odd_time],
        "has_more": "maybe",
    })))
    .await;

    let page = rest(&server)
        .await
        .list_pins(channel(), None, 25)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].pinned_at, None);
    assert_eq!(page.items[1].pinned_at, None);
    assert!(!page.has_more);
}

#[tokio::test]
async fn an_answer_that_is_no_page_of_pins_is_unexpected() {
    let server = answering(ResponseTemplate::new(200).set_body_json(json!([created()]))).await;

    let failed = rest(&server).await.list_pins(channel(), None, 25).await;

    assert!(
        matches!(failed, Err(RequestError::UnexpectedResponse)),
        "{failed:?}"
    );
}

#[tokio::test]
async fn a_pin_puts_and_an_unpin_deletes_the_message_under_the_pins_without_a_body() {
    let server = answering(ResponseTemplate::new(204)).await;
    let rest = rest(&server).await;

    rest.pin_message(channel(), target()).await.unwrap();
    rest.unpin_message(channel(), target()).await.unwrap();

    let requests = requests(&server).await;
    assert_eq!(
        lines(&requests),
        [
            format!("PUT {PINS}/{MESSAGE}"),
            format!("DELETE {PINS}/{MESSAGE}"),
        ]
    );
    assert!(requests.iter().all(|request| request.body.is_empty()));
}

#[tokio::test]
async fn the_pins_ack_posts_to_the_channels_old_path_without_a_body() {
    let server = answering(ResponseTemplate::new(204)).await;

    rest(&server).await.ack_pins(channel()).await.unwrap();

    let requests = requests(&server).await;
    assert_eq!(lines(&requests), [format!("POST {ACK}")]);
    assert!(requests[0].body.is_empty());
}

const CALLS: [&str; 3] = ["list", "pin", "unpin"];

async fn call(rest: &AccountRest, which: &str) -> Result<(), RequestError> {
    match which {
        "list" => rest.list_pins(channel(), None, 25).await.map(|_| ()),
        "pin" => rest.pin_message(channel(), target()).await,
        _ => rest.unpin_message(channel(), target()).await,
    }
}

#[tokio::test]
async fn listing_pinning_and_unpinning_are_asked_once_more_after_a_429_and_never_a_third_time() {
    for which in CALLS {
        let limited = answering(rate_limited(Some(0.02))).await;

        let failed = call(&rest(&limited).await, which).await;

        assert!(
            matches!(failed, Err(RequestError::RateLimited { .. })),
            "{which}: {failed:?}"
        );
        assert_eq!(requests(&limited).await.len(), 2, "{which}");
    }
}

#[tokio::test]
async fn listing_pinning_and_unpinning_are_asked_once_more_after_a_502_504_or_a_broken_connection()
{
    for which in CALLS {
        for status in [502, 504] {
            let failing = answering(ResponseTemplate::new(status)).await;

            let failed = call(&rest(&failing).await, which).await;

            assert!(
                matches!(failed, Err(RequestError::ServerError { .. })),
                "{which} {status}: {failed:?}"
            );
            assert_eq!(requests(&failing).await.len(), 2, "{which} {status}");
        }
        let (api, asked) = flaky(1).await;
        let recovered = call(&rest_at(api), which).await;
        assert!(
            !matches!(recovered, Err(RequestError::Network(_))),
            "{which}: {recovered:?}"
        );
        assert_eq!(asked.load(Ordering::SeqCst), 2, "{which}");
    }
}

// The ack's sender retries it, and mustn't be held up meanwhile.
#[tokio::test]
async fn the_pins_ack_is_asked_once_whatever_the_answer() {
    for answer in [
        rate_limited(Some(0.02)),
        ResponseTemplate::new(502),
        ResponseTemplate::new(504),
    ] {
        let server = answering(answer).await;

        let failed = rest(&server).await.ack_pins(channel()).await;

        assert!(failed.is_err());
        assert_eq!(requests(&server).await.len(), 1);
    }
}

#[tokio::test]
async fn a_refused_pin_or_unpin_is_not_asked_again() {
    for which in ["pin", "unpin"] {
        let refused = answering(ResponseTemplate::new(400).set_body_json(
            json!({"message": "Maximum number of pins reached (250)", "code": 30003}),
        ))
        .await;

        let failed = call(&rest(&refused).await, which).await;

        assert!(
            matches!(failed, Err(RequestError::Discord { code: 30003, .. })),
            "{which}: {failed:?}"
        );
        assert_eq!(requests(&refused).await.len(), 1, "{which}");
    }
}
