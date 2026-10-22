use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::state::OlderThreads;

const SEARCH_PATH: &str = "/api/v9/channels/300000000000000002/threads/search";
const FIRST_PAGE: &str = "archived=true&sort_by=last_message_time&sort_order=desc&limit=25&tag_setting=match_some&offset=0";
const OLD_A: u64 = 300_000_000_000_000_041;
const OLD_B: u64 = 300_000_000_000_000_042;

fn thread(id: u64, archived: bool) -> Value {
    json!({
        "id": id.to_string(),
        "type": 11,
        "name": "old plans",
        "guild_id": "200000000000000001",
        "parent_id": GENERAL.to_string(),
        "owner_id": "100000000000000002",
        "last_message_id": null,
        "message_count": 0,
        "member_count": 1,
        "rate_limit_per_user": 0,
        "flags": 0,
        "thread_metadata": {
            "archived": archived,
            "auto_archive_duration": 1440,
            "archive_timestamp": "2024-03-02T08:30:00.250000+00:00",
            "locked": false
        }
    })
}

fn found(threads: Vec<Value>, has_more: bool) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "threads": threads, "members": [], "has_more": has_more, "total_results": 40
    }))
}

fn not_indexed() -> ResponseTemplate {
    ResponseTemplate::new(202).set_body_json(json!({
        "message": "Index not yet available. Try again later",
        "code": 110_000, "documents_indexed": 0, "retry_after": 2
    }))
}

struct Online {
    account: Account,
    server: MockServer,
    _subscription: Subscription,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

async fn online_answering(answers: Vec<ResponseTemplate>) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    for answer in answers {
        Mock::given(method("GET"))
            .and(path(SEARCH_PATH))
            .respond_with(answer)
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    Online {
        account,
        server,
        _subscription: subscription,
        _connection: connection,
        _fake: fake,
    }
}

impl Online {
    async fn queries(&self) -> Vec<String> {
        let requests = self.server.received_requests().await.unwrap();
        requests
            .iter()
            .filter(|request| request.url.path() == SEARCH_PATH)
            .map(|request| request.url.query().unwrap_or_default().to_owned())
            .collect()
    }

    fn older(&self) -> OlderThreads {
        self.account.store().thread_browser(general()).older
    }

    fn archived(&self) -> Vec<u64> {
        self.account
            .store()
            .thread_browser(general())
            .archived
            .iter()
            .map(|thread| thread.id.get())
            .collect()
    }
}

#[tokio::test]
async fn the_first_page_asks_as_the_official_client() {
    let online = online_answering(vec![found(vec![thread(OLD_A, true)], true)]).await;

    online.account.load_older_threads(general()).await.unwrap();

    assert_eq!(online.queries().await, vec![FIRST_PAGE.to_owned()]);
    assert_eq!(online.archived(), vec![OLD_A]);
    assert_eq!(online.older(), OlderThreads::More);
}

#[tokio::test]
async fn the_next_page_asks_from_offset_25() {
    let online = online_answering(vec![
        found(vec![thread(OLD_B, true)], true),
        found(vec![thread(OLD_A, true)], false),
    ])
    .await;

    online.account.load_older_threads(general()).await.unwrap();
    online.account.load_older_threads(general()).await.unwrap();

    let queries = online.queries().await;
    assert_eq!(queries.len(), 2);
    assert!(queries[1].ends_with("&offset=25"), "{queries:?}");
    assert_eq!(online.archived(), vec![OLD_B, OLD_A]);
    assert_eq!(online.older(), OlderThreads::All);
}

#[tokio::test]
async fn nothing_is_asked_while_a_page_loads_or_after_the_last() {
    let slow = found(vec![thread(OLD_A, true)], false).set_delay(Duration::from_millis(200));
    let online = online_answering(vec![slow]).await;

    let (first, second) = tokio::join!(
        online.account.load_older_threads(general()),
        online.account.load_older_threads(general())
    );
    first.unwrap();
    second.unwrap();
    online.account.load_older_threads(general()).await.unwrap();

    assert_eq!(online.queries().await.len(), 1);
}

#[tokio::test]
async fn a_failed_page_fails_the_section_for_the_connection() {
    let online = online_answering(vec![ResponseTemplate::new(500)]).await;

    assert!(online.account.load_older_threads(general()).await.is_err());
    online.account.load_older_threads(general()).await.unwrap();

    assert_eq!(online.queries().await.len(), 1);
    assert_eq!(online.older(), OlderThreads::Failed);
}

#[tokio::test]
async fn a_202_fails_the_section() {
    let online = online_answering(vec![not_indexed()]).await;

    online.account.load_older_threads(general()).await.unwrap();

    assert_eq!(online.older(), OlderThreads::Failed);
    assert!(online.archived().is_empty());
}

#[tokio::test]
async fn a_search_asks_with_the_encoded_name() {
    let online = online_answering(vec![found(Vec::new(), false)]).await;

    online
        .account
        .search_threads(general(), "t1 & ü/(x)")
        .await
        .unwrap();

    assert_eq!(
        online.queries().await,
        vec!["name=t1%20%26%20%C3%BC%2F(x)&tag_setting=match_some".to_owned()]
    );
}

#[tokio::test]
async fn a_search_returns_discords_order_and_keeps_the_threads() {
    let online = online_answering(vec![found(
        vec![thread(OLD_A, true), thread(OLD_B, false)],
        false,
    )])
    .await;

    let found = online.account.search_threads(general(), "plans").await;

    assert_eq!(
        found.unwrap(),
        Some(vec![Snowflake::new(OLD_A), Snowflake::new(OLD_B)])
    );
    assert!(
        online
            .account
            .store()
            .channel(Snowflake::new(OLD_B))
            .is_some()
    );
    assert_eq!(online.older(), OlderThreads::NotLoaded);
}

#[tokio::test]
async fn a_search_that_isnt_indexed_returns_none() {
    let online = online_answering(vec![not_indexed()]).await;

    let found = online.account.search_threads(general(), "plans").await;

    assert_eq!(found.unwrap(), None);
}

#[tokio::test]
async fn a_failed_search_is_an_error() {
    let online = online_answering(vec![ResponseTemplate::new(500)]).await;

    assert!(
        online
            .account
            .search_threads(general(), "plans")
            .await
            .is_err()
    );
}

// A Rust caller can drop the request, e.g. in a select! or a timeout.
#[tokio::test]
async fn a_dropped_page_request_can_ask_again() {
    let slow = found(Vec::new(), false).set_delay(Duration::from_millis(500));
    let online = online_answering(vec![slow, found(vec![thread(OLD_A, true)], false)]).await;

    let dropped = timeout(
        Duration::from_millis(50),
        online.account.load_older_threads(general()),
    )
    .await;
    assert!(
        dropped.is_err(),
        "the first request should still be waiting"
    );
    assert_eq!(online.older(), OlderThreads::NotLoaded);

    online.account.load_older_threads(general()).await.unwrap();

    assert_eq!(online.archived(), vec![OLD_A]);
}

// A host may ask on every scroll; a write would make the cache flusher snapshot for nothing.
#[tokio::test]
async fn asking_after_the_last_page_writes_nothing() {
    let online = online_answering(vec![found(vec![thread(OLD_A, true)], false)]).await;
    online.account.load_older_threads(general()).await.unwrap();
    let before = online.account.store().changes();

    online.account.load_older_threads(general()).await.unwrap();

    assert_eq!(online.account.store().changes(), before);
}
