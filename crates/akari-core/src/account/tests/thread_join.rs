use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;
use crate::ThreadLocation;
use crate::state::ThreadInfo;

// From thread_create.json, under #general; THREAD_CREATE without the member.
const UNJOINED: u64 = 300_000_000_000_000_021;
const UNJOINED_MEMBER: &str = "/api/v9/channels/300000000000000021/thread-members/@me";
// READY's joined thread under #general.
const JOINED: u64 = 300_000_000_000_000_020;
const JOINED_MEMBER: &str = "/api/v9/channels/300000000000000020/thread-members/@me";
const GUILD: u64 = 200_000_000_000_000_001;

struct Online {
    account: Account,
    subscription: Subscription,
    server: MockServer,
    connection: FakeConnection,
    _fake: FakeGateway,
}

async fn online_with_an_unjoined_thread(archived: bool) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    let mut thread: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/thread_create.json")).unwrap();
    thread.as_object_mut().unwrap().remove("member");
    thread["thread_metadata"]["archived"] = archived.into();
    connection
        .send(json!({"op": 0, "s": 2, "t": "THREAD_CREATE", "d": thread}))
        .await;
    let mut online = Online {
        account,
        subscription,
        server,
        connection,
        _fake: fake,
    };
    online.event("ChannelAdded").await;
    online
}

impl Online {
    async fn event(&mut self, prefix: &str) {
        while let Some(event) = next(&self.subscription).await {
            if event.starts_with(prefix) {
                return;
            }
        }
        panic!("the subscription ended before {prefix}");
    }

    async fn answer(&self, verb: &str, route: &str, status: u16) {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status))
            .mount(&self.server)
            .await;
    }

    async fn requests(&self) -> Vec<Request> {
        self.server.received_requests().await.unwrap()
    }

    fn joined(&self, thread: u64) -> bool {
        self.account
            .store()
            .channel(Snowflake::new(thread))
            .and_then(|channel| channel.thread.as_deref().map(ThreadInfo::joined))
            .expect("a known thread")
    }

    fn joined_at(&self, thread: u64) -> Option<Timestamp> {
        self.account
            .store()
            .channel(Snowflake::new(thread))
            .and_then(|channel| channel.thread.as_deref()?.member?.joined_at)
    }
}

#[tokio::test]
async fn joining_posts_with_its_location_and_marks_the_thread_joined() {
    let online = online_with_an_unjoined_thread(false).await;
    online.answer("POST", UNJOINED_MEMBER, 204).await;

    online
        .account
        .join_thread(Snowflake::new(UNJOINED), ThreadLocation::ToolbarOverflow)
        .await
        .unwrap();

    assert!(online.joined(UNJOINED));
    let requests = online.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.as_str(), "POST");
    assert_eq!(requests[0].url.query(), Some("location=Toolbar%20Overflow"));
    assert!(requests[0].body.is_empty());
}

#[tokio::test]
async fn a_thread_joined_here_goes_to_the_top_at_once() {
    let online = online_with_an_unjoined_thread(false).await;
    online.answer("POST", UNJOINED_MEMBER, 204).await;

    online
        .account
        .join_thread(Snowflake::new(UNJOINED), ThreadLocation::SidebarOverflow)
        .await
        .unwrap();

    // The list orders by it, the last joined first.
    assert!(online.joined_at(UNJOINED) > online.joined_at(JOINED));
}

#[tokio::test]
async fn leaving_deletes_with_its_location_and_unjoins() {
    let online = online_with_an_unjoined_thread(false).await;
    online.answer("DELETE", JOINED_MEMBER, 204).await;

    online
        .account
        .leave_thread(Snowflake::new(JOINED), ThreadLocation::ContextMenu)
        .await
        .unwrap();

    assert!(!online.joined(JOINED));
    let requests = online.requests().await;
    assert_eq!(requests[0].method.as_str(), "DELETE");
    assert_eq!(requests[0].url.query(), Some("location=Context%20Menu"));
    assert!(requests[0].body.is_empty());
}

#[tokio::test]
async fn each_location_has_the_official_query() {
    let online = online_with_an_unjoined_thread(false).await;
    online.answer("POST", UNJOINED_MEMBER, 204).await;
    online.answer("DELETE", UNJOINED_MEMBER, 204).await;
    let thread = Snowflake::new(UNJOINED);

    for location in [
        ThreadLocation::SidebarOverflow,
        ThreadLocation::ToolbarOverflow,
        ThreadLocation::ContextMenu,
    ] {
        online.account.join_thread(thread, location).await.unwrap();
        online.account.leave_thread(thread, location).await.unwrap();
    }

    let queries: Vec<String> = online
        .requests()
        .await
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.query().unwrap_or("")))
        .collect();
    assert_eq!(
        queries,
        [
            "POST location=Sidebar%20Overflow",
            "DELETE location=Sidebar%20Overflow",
            "POST location=Toolbar%20Overflow",
            "DELETE location=Toolbar%20Overflow",
            "POST location=Context%20Menu",
            "DELETE location=Context%20Menu",
        ]
    );
}

#[tokio::test]
async fn a_failed_join_changes_nothing() {
    let online = online_with_an_unjoined_thread(false).await;
    Mock::given(method("POST"))
        .and(path(UNJOINED_MEMBER))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code": 50001, "message": "Missing Access"})),
        )
        .mount(&online.server)
        .await;

    let joined = online
        .account
        .join_thread(Snowflake::new(UNJOINED), ThreadLocation::ToolbarOverflow)
        .await;

    assert!(matches!(
        joined,
        Err(RequestError::Discord { status: 403, .. })
    ));
    assert!(!online.joined(UNJOINED));
}

#[tokio::test]
async fn archived_threads_refuse_both_locally() {
    let online = online_with_an_unjoined_thread(true).await;
    let thread = Snowflake::new(UNJOINED);

    let joined = online
        .account
        .join_thread(thread, ThreadLocation::ToolbarOverflow)
        .await;
    let left = online
        .account
        .leave_thread(thread, ThreadLocation::ToolbarOverflow)
        .await;

    assert!(matches!(joined, Err(RequestError::InvalidRequest)));
    assert!(matches!(left, Err(RequestError::InvalidRequest)));
    assert!(online.requests().await.is_empty());
}

#[tokio::test]
async fn a_thread_the_store_doesnt_have_is_refused_locally() {
    let online = online_with_an_unjoined_thread(false).await;

    let joined = online
        .account
        .join_thread(
            Snowflake::new(UNJOINED + 50),
            ThreadLocation::ToolbarOverflow,
        )
        .await;

    assert!(matches!(joined, Err(RequestError::InvalidRequest)));
    assert!(online.requests().await.is_empty());
}

#[tokio::test]
async fn the_members_event_after_a_join_brings_discords_join_time() {
    let mut online = online_with_an_unjoined_thread(false).await;
    online.answer("POST", UNJOINED_MEMBER, 204).await;
    online
        .account
        .join_thread(Snowflake::new(UNJOINED), ThreadLocation::ToolbarOverflow)
        .await
        .unwrap();

    online
        .connection
        .send(json!({"op": 0, "s": 3, "t": "THREAD_MEMBERS_UPDATE", "d": {
            "id": UNJOINED.to_string(), "guild_id": GUILD.to_string(), "member_count": 2,
            "added_members": [{
                "id": UNJOINED.to_string(), "user_id": ME,
                "join_timestamp": "2026-10-21T10:00:00.000000+00:00", "flags": 1,
                "muted": false, "mute_config": null
            }]
        }}))
        .await;
    sync(&mut online.connection, &online.subscription, 4).await;

    assert!(online.joined(UNJOINED));
    assert_eq!(
        online.joined_at(UNJOINED),
        Some(Timestamp::from_unix_millis(1_792_576_800_000))
    );
}

#[tokio::test]
async fn joining_a_joined_thread_keeps_its_membership() {
    let online = online_with_an_unjoined_thread(false).await;
    online.answer("POST", JOINED_MEMBER, 204).await;
    let before = online.account.store().channel(Snowflake::new(JOINED));

    online
        .account
        .join_thread(Snowflake::new(JOINED), ThreadLocation::ToolbarOverflow)
        .await
        .unwrap();

    assert_eq!(
        online.account.store().channel(Snowflake::new(JOINED)),
        before
    );
}
