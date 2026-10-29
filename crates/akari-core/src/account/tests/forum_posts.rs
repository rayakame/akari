use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::model::TagId;
use crate::state::{ForumSort, ForumView, OlderThreads, TagMatching};

// The forum's page (task 168): its archived posts page by page per tag filter (G-2), its search
// (G-3), as the official client asks for them.

const FORUM: u64 = 300_000_000_000_000_050;
const SEARCH_PATH: &str = "/api/v9/channels/300000000000000050/threads/search";
const BUG: u64 = 500_000_000_000_000_001;
const IDEA: u64 = 500_000_000_000_000_002;
const OLD_A: u64 = 300_000_000_000_000_061;
const OLD_B: u64 = 300_000_000_000_000_062;

fn forum() -> ChannelId {
    Snowflake::new(FORUM)
}

fn forum_create(seq: u64) -> Value {
    json!({"op": 0, "s": seq, "t": "CHANNEL_CREATE", "d": {
        "id": FORUM.to_string(), "type": 15, "guild_id": "200000000000000001", "name": "help",
        "position": 3, "flags": 0, "permission_overwrites": [],
        "available_tags": [
            {"id": BUG.to_string(), "name": "Bug", "moderated": false},
            {"id": IDEA.to_string(), "name": "Idea", "moderated": false},
        ],
    }})
}

fn post(id: u64, last_message: u64, tags: &[u64]) -> Value {
    json!({
        "id": id.to_string(),
        "type": 11,
        "name": "a question",
        "guild_id": "200000000000000001",
        "parent_id": FORUM.to_string(),
        "owner_id": "100000000000000002",
        "last_message_id": last_message.to_string(),
        "message_count": 1,
        "member_count": 1,
        "rate_limit_per_user": 0,
        "flags": 0,
        "applied_tags": tags.iter().map(u64::to_string).collect::<Vec<_>>(),
        "thread_metadata": {
            "archived": true,
            "auto_archive_duration": 1440,
            "archive_timestamp": "2024-03-02T08:30:00.250000+00:00",
            "locked": false
        }
    })
}

fn found(posts: Vec<Value>, has_more: bool) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "threads": posts, "members": [], "has_more": has_more, "total_results": 40
    }))
}

fn view(tags: &[u64], matching: TagMatching) -> ForumView {
    ForumView::new(
        ForumSort::LatestActivity,
        tags.iter().copied().map(TagId::new).collect(),
        matching,
    )
}

struct Online {
    account: Account,
    server: MockServer,
    _subscription: Subscription,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

async fn online_with_forum(answers: Vec<ResponseTemplate>) -> Online {
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
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    connection.send(forum_create(2)).await;
    for _ in 0..200 {
        if account.store().channel(forum()).is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        account.store().channel(forum()).is_some(),
        "the forum arrived"
    );
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

    fn archived(&self, view: &ForumView) -> Vec<u64> {
        self.account
            .store()
            .forum_posts(forum(), view)
            .archived
            .iter()
            .map(|post| post.id.get())
            .collect()
    }

    fn older(&self, view: &ForumView) -> OlderThreads {
        self.account.store().forum_posts(forum(), view).older
    }
}

// Module 472456: the last message first whatever the sort, 25 a page, the tags comma-joined in
// one value, the matching always sent.
#[tokio::test]
async fn a_first_page_asks_as_the_official_client() {
    let online = online_with_forum(vec![
        found(
            vec![post(OLD_A, 400_000_000_000_000_900, &[BUG, IDEA])],
            true,
        ),
        found(Vec::new(), false),
    ])
    .await;
    let tagged = view(&[IDEA, BUG], TagMatching::All);

    online
        .account
        .load_older_posts(forum(), &tagged)
        .await
        .unwrap();
    online
        .account
        .load_older_posts(forum(), &view(&[], TagMatching::All))
        .await
        .unwrap();

    assert_eq!(
        online.queries().await,
        vec![
            format!(
                "archived=true&sort_by=last_message_time&sort_order=desc&limit=25&tag={BUG}%2C{IDEA}&tag_setting=match_all&offset=0"
            ),
            "archived=true&sort_by=last_message_time&sort_order=desc&limit=25&tag_setting=match_some&offset=0"
                .to_owned(),
        ]
    );
    assert_eq!(online.archived(&tagged), vec![OLD_A]);
    assert_eq!(online.older(&tagged), OlderThreads::More);
}

// Each filter has its pages: a second filter starts at 0, the first goes on from 25.
#[tokio::test]
async fn each_filter_pages_on_its_own() {
    let online = online_with_forum(vec![
        found(vec![post(OLD_B, 400_000_000_000_000_950, &[BUG])], true),
        found(vec![post(OLD_A, 400_000_000_000_000_900, &[IDEA])], true),
        found(Vec::new(), false),
    ])
    .await;
    let bugs = view(&[BUG], TagMatching::Some);
    let everything = view(&[], TagMatching::Some);

    online
        .account
        .load_older_posts(forum(), &bugs)
        .await
        .unwrap();
    online
        .account
        .load_older_posts(forum(), &everything)
        .await
        .unwrap();
    online
        .account
        .load_older_posts(forum(), &bugs)
        .await
        .unwrap();

    let queries = online.queries().await;
    assert!(
        queries[0].ends_with("offset=0") && queries[1].ends_with("offset=0"),
        "{queries:?}"
    );
    assert!(
        queries[2].contains(&format!("tag={BUG}")) && queries[2].ends_with("offset=25"),
        "{queries:?}"
    );
    assert_eq!(online.older(&bugs), OlderThreads::All);
    assert_eq!(online.older(&everything), OlderThreads::More);
    // Every archived post the store holds passes through the filter, down to the last page's.
    assert_eq!(online.archived(&bugs), vec![OLD_B]);
    assert_eq!(online.archived(&everything), vec![OLD_B, OLD_A]);
}

#[tokio::test]
async fn a_failed_page_fails_its_filter() {
    let online = online_with_forum(vec![ResponseTemplate::new(500)]).await;
    let bugs = view(&[BUG], TagMatching::Some);

    assert!(
        online
            .account
            .load_older_posts(forum(), &bugs)
            .await
            .is_err()
    );
    online
        .account
        .load_older_posts(forum(), &bugs)
        .await
        .unwrap();

    assert_eq!(online.queries().await.len(), 1);
    assert_eq!(online.older(&bugs), OlderThreads::Failed);
}

// Module 919577: the name, the tags and the matching; no archived, sort or paging; Discord's order.
#[tokio::test]
async fn a_search_asks_with_the_name_and_the_tags() {
    let online = online_with_forum(vec![found(
        vec![
            post(OLD_A, 400_000_000_000_000_900, &[BUG]),
            post(OLD_B, 400_000_000_000_000_950, &[BUG]),
        ],
        false,
    )])
    .await;

    let found = online
        .account
        .search_posts(forum(), "help me", &view(&[BUG], TagMatching::Some))
        .await
        .unwrap();

    assert_eq!(
        online.queries().await,
        vec![format!("name=help%20me&tag={BUG}&tag_setting=match_some")]
    );
    assert_eq!(
        found.unwrap().iter().map(|id| id.get()).collect::<Vec<_>>(),
        vec![OLD_A, OLD_B]
    );
}

// Module 473503 (G-4): the posts' first messages 10 a request, one request at a time, the IDs as
// strings.
#[tokio::test]
async fn previews_are_asked_ten_at_a_time_one_after_the_other() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let posts: Vec<u64> = (0..15)
        .map(|offset| 300_000_000_000_000_100 + offset)
        .collect();
    let answer = |ids: &[u64]| {
        let threads: serde_json::Map<String, Value> = ids
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    json!({"first_message": {
                        "id": id.to_string(), "channel_id": id.to_string(), "type": 0,
                        "content": format!("post {id}"),
                        "author": {"id": "100000000000000002", "username": "mira", "discriminator": "0"},
                        "timestamp": "2024-03-01T08:33:00.000000+00:00", "edited_timestamp": null,
                        "tts": false, "mention_everyone": false, "mentions": [], "mention_roles": [],
                        "attachments": [], "embeds": [], "pinned": false, "flags": 0,
                    }, "owner": null}),
                )
            })
            .collect();
        // The first answer comes slowly: the second request must wait for it.
        let delay = if ids.len() == 10 { 300 } else { 50 };
        ResponseTemplate::new(200)
            .set_body_json(json!({"threads": threads}))
            .set_delay(Duration::from_millis(delay))
    };
    for chunk in [&posts[..10], &posts[10..]] {
        Mock::given(method("POST"))
            .and(path("/api/v9/channels/300000000000000050/post-data"))
            .respond_with(answer(chunk))
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    connection.send(forum_create(2)).await;
    for (seq, id) in posts.iter().enumerate() {
        let mut thread = post(*id, *id, &[]);
        thread["thread_metadata"]["archived"] = false.into();
        connection
            .send(json!({"op": 0, "s": seq + 3, "t": "THREAD_CREATE", "d": thread}))
            .await;
    }
    for _ in 0..200 {
        if account.store().channel(Snowflake::new(posts[14])).is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let ids: Vec<ChannelId> = posts.iter().copied().map(Snowflake::new).collect();
    account.request_post_previews(forum(), &ids);
    let asked = || async {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path().ends_with("/post-data"))
            .count()
    };
    for _ in 0..200 {
        if asked().await > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(asked().await, 1, "one request at a time");
    for _ in 0..300 {
        if account.store().post_preview(ids[14]).is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let bodies: Vec<Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path().ends_with("/post-data"))
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
    let expected =
        |ids: &[u64]| json!({"thread_ids": ids.iter().map(u64::to_string).collect::<Vec<_>>()});
    assert_eq!(bodies, vec![expected(&posts[..10]), expected(&posts[10..])]);
    assert!(
        ids.iter()
            .all(|id| account.store().post_preview(*id).is_some())
    );
}

// The post-data worker ends with the account, letting go of its store and its client.
#[tokio::test]
async fn the_post_data_worker_ends_with_the_account() {
    let fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let account = start_with(&fake, &server);
    let wake = account.shared.previews.clone();
    assert_eq!(
        Arc::strong_count(&wake),
        3,
        "the account's, the worker's and this one"
    );

    account.close();

    for _ in 0..200 {
        if Arc::strong_count(&wake) == 2 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the worker still runs");
}

// A 401 on post-data means the token is gone, as on every other request.
#[tokio::test]
async fn a_401_on_post_data_closes_the_account() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v9/channels/300000000000000050/post-data"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account.request_post_previews(forum(), &[Snowflake::new(300_000_000_000_000_100)]);

    assert_eq!(connection.client_close_code().await, Some(1000));
    events_until(&subscription, "Closed(AuthenticationFailed)").await;
}
