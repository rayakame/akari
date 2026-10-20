use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

// An archived thread under #general that READY didn't bring.
const ARCHIVED: u64 = 300_000_000_000_000_040;
const ARCHIVED_PATH: &str = "/api/v9/channels/300000000000000040";
// READY's joined thread under #general.
const KNOWN: u64 = 300_000_000_000_000_020;

fn archived_id() -> ChannelId {
    Snowflake::new(ARCHIVED)
}

fn archived(member: bool) -> Value {
    let mut thread = json!({
        "id": ARCHIVED.to_string(),
        "type": 11,
        "name": "old plans",
        "guild_id": "200000000000000001",
        "parent_id": GENERAL.to_string(),
        "owner_id": "100000000000000002",
        "last_message_id": "400000000000000041",
        "message_count": 3,
        "member_count": 2,
        "rate_limit_per_user": 0,
        "flags": 0,
        "thread_metadata": {
            "archived": true,
            "auto_archive_duration": 1440,
            "archive_timestamp": "2024-03-02T08:30:00.250000+00:00",
            "locked": false
        }
    });
    if member {
        thread["member"] = json!({
            "join_timestamp": "2024-03-01T08:30:00.250000+00:00",
            "flags": 1,
            "muted": false,
            "mute_config": null
        });
    }
    thread
}

struct Online {
    account: Account,
    subscription: Subscription,
    server: MockServer,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

async fn online_answering(answer: ResponseTemplate) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ARCHIVED_PATH))
        .respond_with(answer)
        .mount(&server)
        .await;
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

impl Online {
    async fn fetch(&self) -> Result<bool, RequestError> {
        self.account.fetch_thread(general(), archived_id()).await
    }

    async fn requests_to(&self, wanted: &str) -> usize {
        let requests = self.server.received_requests().await.unwrap();
        requests
            .iter()
            .filter(|request| request.url.path() == wanted)
            .count()
    }

    async fn event(&self, prefix: &str) {
        while let Some(event) = next(&self.subscription).await {
            if event.starts_with(prefix) {
                return;
            }
        }
        panic!("the subscription ended before {prefix}");
    }
}

fn refused(status: u16, code: u32) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({"code": code, "message": "refused"}))
}

#[tokio::test]
async fn an_unknown_thread_is_fetched_into_the_store_unjoined() {
    let online = online_answering(ResponseTemplate::new(200).set_body_json(archived(false))).await;

    assert!(online.fetch().await.unwrap());

    let thread = online.account.store().channel(archived_id()).unwrap();
    assert_eq!(thread.parent_id, Some(general()));
    let info = thread.thread.as_deref().unwrap();
    assert!(info.archived);
    assert!(info.member.is_none());
    assert!(!online.account.store().thread_unavailable(archived_id()));
    online.event("ChannelAdded").await;
}

// Get Channel includes the user's thread member when they joined it.
#[tokio::test]
async fn a_fetched_thread_keeps_the_users_member() {
    let online = online_answering(ResponseTemplate::new(200).set_body_json(archived(true))).await;

    assert!(online.fetch().await.unwrap());

    let thread = online.account.store().channel(archived_id()).unwrap();
    assert!(thread.thread.as_deref().unwrap().member.is_some());
}

#[tokio::test]
async fn a_thread_the_store_has_is_not_fetched() {
    let online = online_answering(ResponseTemplate::new(200).set_body_json(archived(false))).await;

    let known = online
        .account
        .fetch_thread(general(), Snowflake::new(KNOWN))
        .await
        .unwrap();

    assert!(known);
    assert_eq!(
        online
            .requests_to("/api/v9/channels/300000000000000020")
            .await,
        0
    );
}

#[tokio::test]
async fn a_thread_the_user_cant_see_is_unavailable() {
    let online = online_answering(refused(403, 50001)).await;

    assert!(!online.fetch().await.unwrap());

    assert!(online.account.store().thread_unavailable(archived_id()));
    assert!(online.account.store().channel(archived_id()).is_none());
    online.event("ThreadUnavailable").await;
}

#[tokio::test]
async fn a_deleted_thread_is_unavailable() {
    let online = online_answering(refused(404, 10003)).await;

    assert!(!online.fetch().await.unwrap());

    assert!(online.account.store().thread_unavailable(archived_id()));
}

#[tokio::test]
async fn an_unavailable_thread_is_not_fetched_again() {
    let online = online_answering(refused(404, 10003)).await;

    let _ = online.fetch().await;
    let again = online.fetch().await;

    assert!(!again.unwrap());
    assert_eq!(online.requests_to(ARCHIVED_PATH).await, 1);
}

#[tokio::test]
async fn a_server_error_leaves_the_thread_to_be_fetched_again() {
    let online = online_answering(ResponseTemplate::new(200).set_body_json(archived(false))).await;
    Mock::given(method("GET"))
        .and(path(ARCHIVED_PATH))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&online.server)
        .await;

    let first = online.fetch().await;
    assert!(matches!(first, Err(RequestError::ServerError { .. })));
    assert!(!online.account.store().thread_unavailable(archived_id()));
    assert!(online.fetch().await.unwrap());

    assert!(online.account.store().channel(archived_id()).is_some());
}

#[tokio::test]
async fn a_channel_that_isnt_a_thread_is_unavailable() {
    let mut channel = archived(false);
    channel["type"] = 0.into();
    channel.as_object_mut().unwrap().remove("thread_metadata");
    let online = online_answering(ResponseTemplate::new(200).set_body_json(channel)).await;

    assert!(!online.fetch().await.unwrap());

    assert!(online.account.store().channel(archived_id()).is_none());
    assert!(online.account.store().thread_unavailable(archived_id()));
}

// Nothing Discord refused: the guild may be down, or not restored yet.
#[tokio::test]
async fn a_thread_in_a_guild_the_store_lacks_stays_to_be_fetched() {
    let mut thread = archived(false);
    thread["guild_id"] = "200000000000000099".into();
    let online = online_answering(ResponseTemplate::new(200).set_body_json(thread)).await;

    assert!(!online.fetch().await.unwrap());

    assert!(!online.account.store().thread_unavailable(archived_id()));
}
