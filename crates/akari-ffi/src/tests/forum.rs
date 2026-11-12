use akari_core::model::{ChannelId, EmojiId, TagId};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, events_until, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::errors::{CreateThreadError, RequestError};
use crate::images::ImageKey;
use crate::records::{
    ForumLayout, ForumSort, ForumTag, ForumView, OlderThreads, PostCreation, PostNewCount,
    PostPreview, TagEmoji, TagMatching,
};
use crate::subscription::StoreEvent;

const FORUM: ChannelId = ChannelId::new(300_000_000_000_000_050);
const POST: ChannelId = ChannelId::new(300_000_000_000_000_051);
// Made in May 2024, after the fixture's member joined the server.
const FRESH: ChannelId = ChannelId::new(1_246_000_000_000_000_000);
const BUG: TagId = TagId::new(500_000_000_000_000_001);

// The forum's settings, a post's tags and pin, and its page cross as records (task 171).
#[tokio::test]
async fn a_forum_and_its_posts_cross_as_records() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "CHANNEL_CREATE", "d": {
            "id": FORUM.get().to_string(), "type": 15, "guild_id": "200000000000000001",
            "name": "help", "position": 3, "flags": 0, "permission_overwrites": [],
            "default_sort_order": 1, "default_forum_layout": 2,
            "available_tags": [{"id": BUG.get().to_string(), "name": "Bug", "moderated": true, "emoji_id": "600000000000000001"}],
        }}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 3, "t": "THREAD_CREATE", "d": {
            "id": POST.get().to_string(), "type": 11, "guild_id": "200000000000000001",
            "parent_id": FORUM.get().to_string(), "name": "a question", "flags": 2,
            "owner_id": "100000000000000002", "applied_tags": [BUG.get().to_string()],
            "total_message_sent": 4, "message_count": 4, "member_count": 1,
            "last_message_id": "1300000000000000000",
            "thread_metadata": {"archived": false, "auto_archive_duration": 1440,
                "archive_timestamp": "2024-03-02T08:30:00.250000+00:00", "locked": false},
        }}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 4, "t": "THREAD_CREATE", "d": {
            "id": FRESH.get().to_string(), "type": 11, "guild_id": "200000000000000001",
            "parent_id": FORUM.get().to_string(), "name": "another", "flags": 0,
            "owner_id": "100000000000000002", "applied_tags": [],
            "total_message_sent": 0, "message_count": 0, "member_count": 1,
            "last_message_id": FRESH.get().to_string(),
            "thread_metadata": {"archived": false, "auto_archive_duration": 1440,
                "archive_timestamp": "2024-05-31T08:30:00.250000+00:00", "locked": false},
        }}),
    )
    .await;
    events_until(
        &subscription,
        |event| matches!(event, StoreEvent::ChannelAdded { channel_id, .. } if *channel_id == FRESH),
    )
    .await;
    let store = account.store();

    let forum = store
        .channel(FORUM)
        .and_then(|forum| forum.forum)
        .expect("settings");
    assert_eq!(forum.sort, ForumSort::CreationDate);
    assert_eq!(forum.layout, ForumLayout::Gallery);
    assert_eq!(forum.tag_matching, TagMatching::Some);
    let emoji = EmojiId::new(600_000_000_000_000_001);
    assert_eq!(
        forum.tags,
        [ForumTag {
            id: BUG,
            name: "Bug".to_owned(),
            moderated: true,
            emoji: Some(TagEmoji::Custom {
                id: emoji,
                key: ImageKey::Emoji {
                    emoji_id: emoji,
                    animated: false
                },
            }),
        }]
    );
    let post = store
        .channel(POST)
        .and_then(|post| post.thread)
        .expect("a post");
    assert_eq!(post.applied_tags, [BUG]);
    assert_eq!(post.total_message_sent, Some(4));
    assert!(post.pinned);
    assert_eq!(store.unread_posts(vec![POST, FORUM]), [POST]);
    assert!(store.new_posts(vec![POST, FRESH, FORUM]).is_empty());

    account.open_forum(Some(FORUM));
    events_until(
        &subscription,
        |event| matches!(event, StoreEvent::ForumPostsChanged { forum_id } if *forum_id == FORUM),
    )
    .await;
    assert_eq!(store.new_posts(vec![POST, FRESH, FORUM]), [FRESH]);
    let view = ForumView {
        tags: vec![BUG],
        ..ForumView::default()
    };
    let posts = store.forum_posts(FORUM, view);
    assert_eq!(posts.active, [POST]);
    assert_eq!(posts.older, OlderThreads::NotLoaded);
    account.close();
}

// A post's new-message count crosses with its event (task 175): asked when the forum's list
// refreshes, answered by FORUM_UNREADS.
#[tokio::test]
async fn a_posts_new_count_crosses_with_its_event() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let counts_changed = |event: &StoreEvent| matches!(event, StoreEvent::PostCountsChanged { forum_id } if *forum_id == FORUM);
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "CHANNEL_CREATE", "d": {
            "id": FORUM.get().to_string(), "type": 15, "guild_id": "200000000000000001",
            "name": "help", "position": 3, "flags": 0, "permission_overwrites": [],
        }}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 3, "t": "THREAD_CREATE", "d": {
            "id": POST.get().to_string(), "type": 11, "guild_id": "200000000000000001",
            "parent_id": FORUM.get().to_string(), "name": "a question", "flags": 0,
            "owner_id": "100000000000000002", "applied_tags": [],
            "total_message_sent": 4, "message_count": 4, "member_count": 1,
            "last_message_id": "1300000000000000000",
            "thread_metadata": {"archived": false, "auto_archive_duration": 1440,
                "archive_timestamp": "2024-03-02T08:30:00.250000+00:00", "locked": false},
        }}),
    )
    .await;
    events_until(
        &subscription,
        |event| matches!(event, StoreEvent::ChannelAdded { channel_id, .. } if *channel_id == POST),
    )
    .await;
    let store = account.store();
    account.open_forum(Some(FORUM));
    assert!(store.post_new_counts(vec![POST]).is_empty(), "never opened");

    send(
        &mut ws,
        json!({"op": 0, "s": 4, "t": "MESSAGE_ACK", "d": {
            "channel_id": POST.get().to_string(), "message_id": "1200000000000000000", "version": 1,
        }}),
    )
    .await;
    events_until(&subscription, counts_changed).await;
    let unknown = PostNewCount {
        post_id: POST,
        count: None,
    };
    assert_eq!(store.post_new_counts(vec![POST, FORUM]), [unknown]);

    account.refresh_forum(FORUM);
    send(
        &mut ws,
        json!({"op": 0, "s": 5, "t": "FORUM_UNREADS", "d": {
            "channel_id": FORUM.get().to_string(),
            "threads": [{"thread_id": POST.get().to_string(), "count": 3}],
        }}),
    )
    .await;
    events_until(&subscription, counts_changed).await;

    let counted = PostNewCount {
        post_id: POST,
        count: Some(3),
    };
    assert_eq!(store.post_new_counts(vec![POST, FORUM]), [counted]);
    account.close();
}

const POSTS_PATH: &str = "/api/v9/channels/300000000000000050/threads";
const NEW: ChannelId = ChannelId::new(300_000_000_000_000_077);
const STAFF: TagId = TagId::new(500_000_000_000_000_002);

fn new_post() -> Value {
    json!({
        "id": NEW.get().to_string(), "type": 11, "guild_id": "200000000000000001",
        "parent_id": FORUM.get().to_string(), "name": "a question", "flags": 0,
        "owner_id": USER.get().to_string(), "applied_tags": [BUG.get().to_string()],
        "total_message_sent": 0, "message_count": 0, "member_count": 1,
        "last_message_id": NEW.get().to_string(),
        "thread_metadata": {"archived": false, "auto_archive_duration": 4320,
            "archive_timestamp": "2026-11-12T12:00:00.000000+00:00", "locked": false},
        "member": {"id": NEW.get().to_string(), "user_id": USER.get().to_string(),
            "join_timestamp": "2026-11-12T12:00:00.000000+00:00", "flags": 1},
        "message": {
            "id": NEW.get().to_string(), "channel_id": NEW.get().to_string(), "type": 0,
            "content": "steps to reproduce", "timestamp": "2026-11-12T12:00:00.000000+00:00",
            "author": {"id": USER.get().to_string(), "username": "tester", "discriminator": "0"},
        },
    })
}

// Online with the open forum `flags` describe, where posting is answered with `answer`.
async fn posting(
    answer: ResponseTemplate,
    flags: u64,
) -> (
    std::sync::Arc<crate::account::Account>,
    MockServer,
    tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(POSTS_PATH))
        .respond_with(answer)
        .mount(&server)
        .await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "CHANNEL_CREATE", "d": {
            "id": FORUM.get().to_string(), "type": 15, "guild_id": "200000000000000001",
            "name": "help", "position": 3, "flags": flags, "permission_overwrites": [],
            "available_tags": [
                {"id": BUG.get().to_string(), "name": "Bug", "moderated": false},
                {"id": STAFF.get().to_string(), "name": "Staff", "moderated": true},
            ],
        }}),
    )
    .await;
    events_until(
        &subscription,
        |event| matches!(event, StoreEvent::ChannelAdded { channel_id, .. } if *channel_id == FORUM),
    )
    .await;
    account.open_forum(Some(FORUM));
    (account, server, ws)
}

#[tokio::test]
async fn creating_a_post_returns_it_and_the_store_says_what_may_be_posted() {
    let (account, server, _ws) =
        posting(ResponseTemplate::new(201).set_body_json(new_post()), 0).await;
    let store = account.store();
    assert_eq!(
        store.post_creation(FORUM),
        PostCreation {
            allowed: true,
            tags: vec![BUG, STAFF],
            requires_tag: false,
        }
    );
    assert_eq!(
        store.post_creation(ChannelId::new(300_000_000_000_000_002)),
        PostCreation::default()
    );

    let post = account
        .create_post(
            FORUM,
            " a question ".to_owned(),
            vec![BUG],
            "steps to reproduce".to_owned(),
        )
        .await
        .unwrap();

    assert_eq!(post, NEW);
    assert_eq!(store.forum_posts(FORUM, ForumView::default()).active, [NEW]);
    assert!(store.new_posts(vec![NEW]).is_empty());
    assert!(
        matches!(store.post_preview(NEW), Some(PostPreview::Loaded { message }) if message.content == "steps to reproduce")
    );
    let requests = server.received_requests().await.unwrap();
    let created: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == POSTS_PATH)
        .collect();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].url.query(), Some("use_nested_fields=true"));
    assert_eq!(
        String::from_utf8(created[0].body.clone()).unwrap(),
        r#"{"name":"a question","auto_archive_duration":4320,"applied_tags":["500000000000000001"],"message":{"content":"steps to reproduce"}}"#
    );
    account.close();
}

#[tokio::test]
async fn a_refused_post_says_why() {
    let (account, _server, _ws) = posting(
        ResponseTemplate::new(400)
            .set_body_json(json!({"code": 200_000, "message": "Blocked by AutoMod."})),
        1 << 4,
    )
    .await;
    assert!(account.store().post_creation(FORUM).requires_tag);

    let untagged = account
        .create_post(FORUM, "t".to_owned(), Vec::new(), "m".to_owned())
        .await;
    let empty = account
        .create_post(FORUM, "t".to_owned(), vec![BUG], " ".to_owned())
        .await;
    let blocked = account
        .create_post(FORUM, "t".to_owned(), vec![BUG], "m".to_owned())
        .await;

    assert_eq!(untagged, Err(CreateThreadError::TagRequired));
    assert_eq!(
        empty,
        Err(CreateThreadError::Request {
            error: RequestError::InvalidRequest
        })
    );
    assert_eq!(
        blocked,
        Err(CreateThreadError::MessageBlocked {
            reason: "Blocked by AutoMod.".to_owned()
        })
    );
    account.close();
}
