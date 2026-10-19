use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::state::StarterMessage;

// READY's joined thread under #general, started from #general's message of the same ID.
const THREAD: u64 = 300_000_000_000_000_020;
const THREAD_MESSAGES: &str = "/api/v9/channels/300000000000000020/messages";
const STARTER: u64 = 400_000_000_000_000_031;
const REPLY: u64 = 400_000_000_000_000_032;
const NEIGHBOR: u64 = 300_000_000_000_000_019;

fn thread() -> ChannelId {
    Snowflake::new(THREAD)
}

fn plain(id: u64, channel: u64, content: &str) -> Value {
    let mut message = message(0, id)["d"].clone();
    message["channel_id"] = channel.to_string().into();
    message["content"] = content.into();
    message.as_object_mut().unwrap().remove("nonce");
    message
}

fn original() -> Value {
    plain(THREAD, GENERAL, "the original")
}

// The thread's first message. `referenced`: `None` when Discord leaves the original out,
// `Some(null)` when it was deleted.
fn starter(referenced: Option<Value>) -> Value {
    let mut message = plain(STARTER, THREAD, "");
    message["type"] = 21.into();
    message["message_reference"] = json!({
        "message_id": THREAD.to_string(),
        "channel_id": GENERAL.to_string(),
        "guild_id": "200000000000000001",
    });
    if let Some(referenced) = referenced {
        message["referenced_message"] = referenced;
    }
    message
}

fn reply() -> Value {
    plain(REPLY, THREAD, "a reply")
}

async fn mock_thread_page(server: &MockServer, newest_first: Vec<Value>) {
    Mock::given(path(THREAD_MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(newest_first)))
        .mount(server)
        .await;
}

async fn mock_fetch(server: &MockServer, answer: ResponseTemplate) {
    Mock::given(path(MESSAGES))
        .and(query_param("limit", "1"))
        .and(query_param("around", THREAD.to_string()))
        .respond_with(answer)
        .mount(server)
        .await;
}

fn fetches(requests: &[wiremock::Request]) -> usize {
    requests
        .iter()
        .filter(|request| request.url.path() == MESSAGES)
        .count()
}

struct Online {
    account: Account,
    subscription: Subscription,
    server: MockServer,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

async fn online_with_thread(newest_first: Vec<Value>) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    mock_thread_page(&server, newest_first).await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    Online {
        account,
        subscription,
        server,
        _connection: connection,
        _fake: fake,
    }
}

async fn open_thread(online: &Online) {
    online
        .account
        .load_messages(thread(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();
}

fn starter_of(online: &Online) -> Option<StarterMessage> {
    online.account.store().starter_message(thread())
}

async fn settled(online: &Online) {
    events_until(&online.subscription, &format!("MessageUpdated({STARTER})")).await;
}

#[tokio::test]
async fn a_starter_message_resolves_to_the_parent_message() {
    let online = online_with_thread(vec![reply(), starter(Some(original()))]).await;

    open_thread(&online).await;

    let Some(StarterMessage::Loaded(message)) = starter_of(&online) else {
        panic!(
            "expected the original message, got {:?}",
            starter_of(&online)
        );
    };
    assert_eq!(message.id.get(), THREAD);
    assert_eq!(message.channel_id, general());
    assert_eq!(&*message.content, "the original");
    let requests = online.server.received_requests().await.unwrap();
    assert_eq!(fetches(&requests), 0);
}

#[tokio::test]
async fn a_deleted_starter_is_unavailable() {
    let online = online_with_thread(vec![reply(), starter(Some(Value::Null))]).await;

    open_thread(&online).await;

    assert!(matches!(
        starter_of(&online),
        Some(StarterMessage::Unavailable)
    ));
}

#[tokio::test]
async fn a_thread_without_a_starter_has_none() {
    let online = online_with_thread(vec![reply()]).await;

    open_thread(&online).await;

    assert!(starter_of(&online).is_none());
}

#[tokio::test]
async fn an_absent_starter_is_fetched_from_the_parent() {
    let online = online_with_thread(vec![reply(), starter(None)]).await;
    mock_fetch(
        &online.server,
        ResponseTemplate::new(200)
            .set_body_json(json!([original()]))
            .set_delay(Duration::from_millis(200)),
    )
    .await;

    open_thread(&online).await;
    assert!(matches!(starter_of(&online), Some(StarterMessage::Loading)));
    settled(&online).await;

    let Some(StarterMessage::Loaded(message)) = starter_of(&online) else {
        panic!(
            "expected the fetched message, got {:?}",
            starter_of(&online)
        );
    };
    assert_eq!(&*message.content, "the original");
    assert_eq!(message.channel_id, general());
}

#[tokio::test]
async fn a_starter_the_fetch_cant_find_is_unavailable() {
    let online = online_with_thread(vec![reply(), starter(None)]).await;
    mock_fetch(
        &online.server,
        ResponseTemplate::new(200).set_body_json(json!([plain(NEIGHBOR, GENERAL, "before")])),
    )
    .await;

    open_thread(&online).await;
    settled(&online).await;

    assert!(matches!(
        starter_of(&online),
        Some(StarterMessage::Unavailable)
    ));
}

#[tokio::test]
async fn a_starter_the_user_cant_read_is_unavailable() {
    let online = online_with_thread(vec![reply(), starter(None)]).await;
    mock_fetch(
        &online.server,
        ResponseTemplate::new(403)
            .set_body_json(json!({"code": 50001, "message": "Missing Access"})),
    )
    .await;

    open_thread(&online).await;
    settled(&online).await;

    assert!(matches!(
        starter_of(&online),
        Some(StarterMessage::Unavailable)
    ));
}

#[tokio::test]
async fn the_starter_is_fetched_once_per_session() {
    let online = online_with_thread(vec![reply(), starter(None)]).await;
    mock_fetch(
        &online.server,
        ResponseTemplate::new(200).set_body_json(json!([original()])),
    )
    .await;

    open_thread(&online).await;
    settled(&online).await;
    open_thread(&online).await;
    open_thread(&online).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let requests = online.server.received_requests().await.unwrap();
    assert_eq!(fetches(&requests), 1);
    assert!(matches!(
        starter_of(&online),
        Some(StarterMessage::Loaded(_))
    ));
}

#[tokio::test]
async fn a_failed_starter_fetch_is_tried_again_with_the_next_page() {
    let online = online_with_thread(vec![reply(), starter(None)]).await;
    Mock::given(path(MESSAGES))
        .and(query_param("around", THREAD.to_string()))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&online.server)
        .await;
    mock_fetch(
        &online.server,
        ResponseTemplate::new(200).set_body_json(json!([original()])),
    )
    .await;

    open_thread(&online).await;
    settled(&online).await;
    assert!(matches!(
        starter_of(&online),
        Some(StarterMessage::Unavailable)
    ));
    open_thread(&online).await;
    settled(&online).await;

    assert!(matches!(
        starter_of(&online),
        Some(StarterMessage::Loaded(_))
    ));
}
