use std::sync::Arc;
use std::time::{Duration, SystemTime};

use akari_core::model::{ChannelId, MessageId, UserId};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{CATEGORY, FakeGateway, GENERAL, GUILD, VOICE, events_until, fixture, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::account::{Account, MessageLoad, ThreadLocation};
use crate::records::{
    EmbedField, MessageReference, OlderThreads, StarterMessage, ThreadBrowser, ThreadInfo,
};
use crate::subscription::StoreEvent;

// READY's thread in GENERAL, its last message 400000000000000030.
const THREAD: ChannelId = ChannelId::new(300_000_000_000_000_020);
const MIRA: UserId = UserId::new(100_000_000_000_000_002);

// Past its window, the thread is listed only while unread.
fn unread_thread(ready: &mut Value) {
    ready["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
}

#[tokio::test]
async fn a_threads_record_carries_its_state() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();

    let thread = store.channel(THREAD).and_then(|channel| channel.thread);

    assert_eq!(
        thread,
        Some(ThreadInfo {
            joined: true,
            archived: false,
            locked: false,
            private: false,
            message_count: Some(12),
            owner_id: Some(MIRA),
            last_message_id: Some(MessageId::new(400_000_000_000_000_030)),
            created_at: None,
            member_count: Some(3),
            member_ids_preview: Vec::new(),
            archived_at: SystemTime::UNIX_EPOCH + Duration::from_millis(1_709_281_800_250),
        })
    );
    assert_eq!(
        store.channel(GENERAL).and_then(|channel| channel.thread),
        None
    );
    account.close();
}

#[tokio::test]
async fn channel_list_includes_listed_threads() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready_with(unread_thread).await;
    events_until(&subscription, online).await;

    assert_eq!(
        account.store().channel_list(GUILD),
        [VOICE, CATEGORY, GENERAL, THREAD]
    );
    account.close();
}

#[tokio::test]
async fn a_listing_change_reaches_the_subscription() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready_with(unread_thread).await;
    events_until(&subscription, online).await;

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_ACK", "d": {
            "channel_id": THREAD.get().to_string(),
            "message_id": "400000000000000030", "version": 2
        }}),
    )
    .await;
    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::ThreadListingChanged { .. })
    })
    .await;

    assert!(events.contains(&StoreEvent::ThreadListingChanged {
        guild_id: GUILD,
        thread_id: THREAD,
    }));
    assert_eq!(
        account.store().channel_list(GUILD),
        [VOICE, CATEGORY, GENERAL]
    );
    account.close();
}

#[tokio::test]
async fn muted_channels_covers_threads() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway
        .serve_ready_with(|ready| {
            ready["guilds"][0]["threads"][0]["member"]["muted"] = true.into();
        })
        .await;
    events_until(&subscription, online).await;

    assert_eq!(
        account.store().muted_channels(vec![THREAD, GENERAL]),
        [THREAD]
    );
    account.close();
}

const HAS_THREAD: u64 = 1 << 5;

fn page_message(id: u64, channel: ChannelId) -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message["id"] = id.to_string().into();
    message["channel_id"] = channel.get().to_string().into();
    message.as_object_mut().unwrap().remove("nonce");
    message
}

struct Loaded {
    account: Arc<Account>,
    _server: MockServer,
    _ws: WebSocketStream<TcpStream>,
}

// `newest_first`: the channel's only page.
async fn loaded(channel: ChannelId, newest_first: Vec<Value>) -> Loaded {
    loaded_answering(channel, newest_first, None).await
}

// `general`: how #general answers a starter's fetch.
async fn loaded_answering(
    channel: ChannelId,
    newest_first: Vec<Value>,
    general: Option<ResponseTemplate>,
) -> Loaded {
    let server = MockServer::start().await;
    Mock::given(path(format!("/api/v9/channels/{}/messages", channel.get())))
        .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(newest_first)))
        .mount(&server)
        .await;
    if let Some(general) = general {
        Mock::given(path(format!("/api/v9/channels/{}/messages", GENERAL.get())))
            .respond_with(general)
            .mount(&server)
            .await;
    }
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    account
        .load_messages(channel, MessageLoad::Latest { limit: 50 }, None)
        .await
        .unwrap();
    Loaded {
        account,
        _server: server,
        _ws: ws,
    }
}

#[tokio::test]
async fn a_message_carries_its_reference_and_thread_flag() {
    let mut line = page_message(400_000_000_000_000_040, GENERAL);
    line["type"] = 18.into();
    line["content"] = "t1".into();
    line["message_reference"] = json!({
        "channel_id": THREAD.get().to_string(), "guild_id": GUILD.get().to_string()
    });
    let mut started = page_message(400_000_000_000_000_041, GENERAL);
    started["flags"] = HAS_THREAD.into();
    let plain = page_message(400_000_000_000_000_042, GENERAL);
    let loaded = loaded(GENERAL, vec![plain, started, line]).await;
    let store = loaded.account.store();

    let messages = store.messages(GENERAL, store.window(GENERAL).unwrap().message_ids);

    assert_eq!(
        messages[0].reference,
        Some(MessageReference {
            message_id: None,
            channel_id: Some(THREAD),
            guild_id: Some(GUILD),
        })
    );
    assert_eq!(
        messages.iter().map(|m| m.has_thread).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(messages[2].reference, None);
    loaded.account.close();
}

// What the apps word system lines with.
#[tokio::test]
async fn a_system_messages_data_crosses_the_boundary() {
    let mira = json!({"id": "100000000000000003", "username": "mira", "discriminator": "0"});
    let mut call = page_message(400_000_000_000_000_050, GENERAL);
    call["type"] = 3.into();
    call["mentions"] = json!([mira]);
    call["call"] = json!({
        "participants": ["100000000000000003"],
        "ended_timestamp": "2024-03-01T08:40:00.250000+00:00"
    });
    let mut subscription = page_message(400_000_000_000_000_051, GENERAL);
    subscription["type"] = 25.into();
    subscription["role_subscription_data"] =
        json!({"tier_name": "Gold", "total_months_subscribed": 3, "is_renewal": false});
    subscription["purchase_notification"] =
        json!({"type": 0, "guild_product_purchase": {"listing_id": "1", "product_name": "Badge"}});
    let mut join = page_message(400_000_000_000_000_052, GENERAL);
    join["type"] = 7.into();
    join["actor"] = mira.clone();
    join["application"] = json!({"id": "1", "name": "Akari Bot"});
    let mut poll = page_message(400_000_000_000_000_053, GENERAL);
    poll["type"] = 46.into();
    poll["embeds"] = json!([{"type": "poll_result", "fields": [
        {"name": "poll_question_text", "value": "Lunch?"},
        {"name": "total_votes", "value": "2"}
    ]}]);
    let mut plain = page_message(400_000_000_000_000_054, GENERAL);
    plain["embeds"] = json!([{"type": "rich", "fields": [{"name": "a", "value": "b"}]}]);
    let loaded = loaded(GENERAL, vec![call, subscription, join, poll, plain]).await;
    let store = loaded.account.store();

    let messages = store.messages(GENERAL, store.window(GENERAL).unwrap().message_ids);

    assert_eq!(messages[0].mentions[0].username, "mira");
    let call = messages[0].call.as_ref().unwrap();
    assert_eq!(call.participants, [UserId::new(100_000_000_000_000_003)]);
    assert!(call.ended_timestamp.is_some());
    let data = messages[1].role_subscription.as_ref().unwrap();
    assert_eq!(
        (data.tier_name.as_str(), data.total_months_subscribed),
        ("Gold", 3)
    );
    assert!(!data.is_renewal);
    assert_eq!(messages[1].purchased_product.as_deref(), Some("Badge"));
    assert_eq!(
        messages[2]
            .actor
            .as_ref()
            .map(|user| user.username.as_str()),
        Some("mira")
    );
    assert_eq!(messages[2].application_name.as_deref(), Some("Akari Bot"));
    assert_eq!(
        messages[3].embed_fields,
        [
            EmbedField {
                name: "poll_question_text".into(),
                value: "Lunch?".into()
            },
            EmbedField {
                name: "total_votes".into(),
                value: "2".into()
            },
        ]
    );
    assert!(messages[0].embed_fields.is_empty());
    assert!(
        messages[4].embed_fields.is_empty(),
        "only the kinds whose lines read them"
    );
    loaded.account.close();
}

// `referenced`: `None` when Discord leaves the original out, `Some(null)` when it was deleted.
fn starter(referenced: Option<Value>) -> Value {
    let mut starter = page_message(400_000_000_000_000_031, THREAD);
    starter["type"] = 21.into();
    starter["content"] = "".into();
    starter["message_reference"] = json!({
        "message_id": THREAD.get().to_string(), "channel_id": GENERAL.get().to_string(),
        "guild_id": GUILD.get().to_string()
    });
    if let Some(referenced) = referenced {
        starter["referenced_message"] = referenced;
    }
    starter
}

#[tokio::test]
async fn the_starter_read_gives_the_parent_message() {
    let mut original = page_message(THREAD.get(), GENERAL);
    original["content"] = "the **original**".into();
    let loaded = loaded(
        THREAD,
        vec![
            page_message(400_000_000_000_000_032, THREAD),
            starter(Some(original)),
        ],
    )
    .await;

    let Some(StarterMessage::Loaded { message }) = loaded.account.store().starter_message(THREAD)
    else {
        panic!("expected the original message");
    };

    assert_eq!(message.id, MessageId::new(THREAD.get()));
    assert_eq!(message.channel_id, GENERAL);
    assert_eq!(message.content, "the **original**");
    assert!(!message.formatted.blocks.is_empty());
    loaded.account.close();
}

#[tokio::test]
async fn a_thread_without_a_starter_reads_none() {
    let loaded = loaded(THREAD, vec![page_message(400_000_000_000_000_032, THREAD)]).await;

    assert_eq!(loaded.account.store().starter_message(THREAD), None);
    loaded.account.close();
}

#[tokio::test]
async fn a_starter_being_fetched_reads_loading() {
    let slow = ResponseTemplate::new(200)
        .set_body_json(json!([]))
        .set_delay(Duration::from_secs(5));
    let loaded = loaded_answering(THREAD, vec![starter(None)], Some(slow)).await;

    assert_eq!(
        loaded.account.store().starter_message(THREAD),
        Some(StarterMessage::Loading)
    );
    loaded.account.close();
}

#[tokio::test]
async fn a_deleted_starter_reads_unavailable() {
    let loaded = loaded(THREAD, vec![starter(Some(Value::Null))]).await;

    assert_eq!(
        loaded.account.store().starter_message(THREAD),
        Some(StarterMessage::Unavailable)
    );
    loaded.account.close();
}

// An archived thread under GENERAL that READY didn't bring.
const ARCHIVED: ChannelId = ChannelId::new(300_000_000_000_000_040);

async fn online_answering(answer: ResponseTemplate) -> Loaded {
    let server = MockServer::start().await;
    Mock::given(path(format!("/api/v9/channels/{}", ARCHIVED.get())))
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
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    Loaded {
        account,
        _server: server,
        _ws: ws,
    }
}

#[tokio::test]
async fn a_fetched_thread_can_be_read() {
    let archived = json!({
        "id": ARCHIVED.get().to_string(), "type": 11, "name": "old plans",
        "guild_id": GUILD.get().to_string(), "parent_id": GENERAL.get().to_string(),
        "owner_id": MIRA.get().to_string(),
        "thread_metadata": {"archived": true, "auto_archive_duration": 1440,
            "archive_timestamp": "2024-03-02T08:30:00.250000+00:00", "locked": false}
    });
    let loaded = online_answering(ResponseTemplate::new(200).set_body_json(archived)).await;

    let found = loaded
        .account
        .fetch_thread(GENERAL, ARCHIVED)
        .await
        .unwrap();

    assert!(found);
    let thread = loaded.account.store().channel(ARCHIVED).unwrap();
    assert_eq!(
        thread.thread.map(|info| (info.joined, info.archived)),
        Some((false, true))
    );
    loaded.account.close();
}

#[tokio::test]
async fn an_unavailable_thread_reaches_the_subscription_and_the_read() {
    let loaded = online_answering(
        ResponseTemplate::new(404).set_body_json(json!({"code": 10003, "message": "Unknown"})),
    )
    .await;
    let subscription = loaded.account.store().subscribe();

    let found = loaded
        .account
        .fetch_thread(GENERAL, ARCHIVED)
        .await
        .unwrap();

    assert!(!found);
    assert!(loaded.account.store().thread_unavailable(ARCHIVED));
    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::ThreadUnavailable { .. })
    })
    .await;
    assert!(events.contains(&StoreEvent::ThreadUnavailable {
        guild_id: GUILD,
        thread_id: ARCHIVED,
    }));
    loaded.account.close();
}

// A split view the host remembers from an earlier session, deleted meanwhile.
#[tokio::test]
async fn a_checked_thread_discord_refuses_leaves_the_store() {
    let server = MockServer::start().await;
    Mock::given(path(format!("/api/v9/channels/{}", THREAD.get())))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"code": 10003, "message": "Unknown"})),
        )
        .mount(&server)
        .await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    assert!(account.store().channel(THREAD).is_some());

    let found = account.check_thread(GENERAL, THREAD).await.unwrap();

    assert!(!found);
    assert!(account.store().channel(THREAD).is_none());
    assert!(account.store().thread_unavailable(THREAD));
    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::ChannelRemoved { .. })
    })
    .await;
    assert!(events.contains(&StoreEvent::ChannelRemoved {
        channel_id: THREAD,
        guild_id: Some(GUILD),
    }));
    account.close();
}

#[tokio::test]
async fn the_latest_message_read_gives_the_threads_newest() {
    let mut newest = page_message(400_000_000_000_000_033, THREAD);
    newest["content"] = "the **newest**".into();
    let loaded = loaded(
        THREAD,
        vec![newest, page_message(400_000_000_000_000_032, THREAD)],
    )
    .await;

    let latest = loaded
        .account
        .store()
        .latest_message(THREAD)
        .expect("the newest message");

    assert_eq!(latest.id, MessageId::new(400_000_000_000_000_033));
    assert_eq!(latest.content, "the **newest**");
    assert!(!latest.formatted.blocks.is_empty());
    loaded.account.close();
}

#[tokio::test]
async fn leaving_and_joining_cross_the_boundary() {
    let server = MockServer::start().await;
    let member = format!("/api/v9/channels/{}/thread-members/@me", THREAD.get());
    for verb in ["DELETE", "POST"] {
        Mock::given(method(verb))
            .and(path(member.as_str()))
            .and(query_param("location", "Toolbar Overflow"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
    }
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let joined = || {
        account
            .store()
            .channel(THREAD)
            .and_then(|channel| channel.thread)
            .map(|thread| thread.joined)
    };

    account
        .leave_thread(THREAD, ThreadLocation::ToolbarOverflow)
        .await
        .unwrap();
    assert_eq!(joined(), Some(false));

    account
        .join_thread(THREAD, ThreadLocation::ToolbarOverflow)
        .await
        .unwrap();
    assert_eq!(joined(), Some(true));
    account.close();
}

const SEARCH_PATH: &str = "/api/v9/channels/300000000000000002/threads/search";
const OLD: ChannelId = ChannelId::new(300_000_000_000_000_041);

async fn searching(answer: Value) -> (Arc<Account>, MockServer, WebSocketStream<TcpStream>) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(SEARCH_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(&server)
        .await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    (account, server, ws)
}

fn archived_thread(id: ChannelId) -> Value {
    json!({
        "id": id.get().to_string(), "type": 11, "name": "old plans",
        "guild_id": GUILD.get().to_string(), "parent_id": GENERAL.get().to_string(),
        "owner_id": MIRA.get().to_string(), "last_message_id": null,
        "message_count": 0, "member_count": 2, "rate_limit_per_user": 0, "flags": 0,
        "member_ids_preview": [MIRA.get().to_string()],
        "thread_metadata": {"archived": true, "auto_archive_duration": 1440,
            "archive_timestamp": "2024-03-02T08:30:00.250000+00:00", "locked": false}
    })
}

#[tokio::test]
async fn the_thread_browser_reads_its_sections() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    let browser = account.store().thread_browser(GENERAL);

    assert_eq!(
        browser,
        ThreadBrowser {
            joined: vec![THREAD],
            others: Vec::new(),
            archived: Vec::new(),
            older: OlderThreads::NotLoaded,
        }
    );
    account.close();
}

#[tokio::test]
async fn older_threads_load_into_the_browser() {
    let (account, _server, _ws) = searching(json!({
        "threads": [archived_thread(OLD)], "members": [], "has_more": false, "total_results": 1
    }))
    .await;

    account.load_older_threads(GENERAL).await.unwrap();

    let browser = account.store().thread_browser(GENERAL);
    assert_eq!(browser.archived, vec![OLD]);
    assert_eq!(browser.older, OlderThreads::All);
    let thread = account
        .store()
        .channel(OLD)
        .and_then(|channel| channel.thread);
    assert_eq!(thread.map(|info| info.member_ids_preview), Some(vec![MIRA]));
    account.close();
}

#[tokio::test]
async fn a_search_returns_its_threads_in_order() {
    let (account, server, _ws) = searching(json!({
        "threads": [archived_thread(OLD)], "members": [], "has_more": false, "total_results": 1
    }))
    .await;

    let found = account
        .search_threads(GENERAL, "old".to_owned())
        .await
        .unwrap();

    assert_eq!(found, Some(vec![OLD]));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.last().and_then(|request| request.url.query()),
        Some("name=old&tag_setting=match_some")
    );
    account.close();
}

#[tokio::test]
async fn thread_activity_reaches_the_subscription() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    let mut message = page_message(400_000_000_000_000_040, THREAD);
    message["guild_id"] = GUILD.get().to_string().into();
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": message}),
    )
    .await;
    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::ThreadActivity { .. })
    })
    .await;

    assert!(events.contains(&StoreEvent::ThreadActivity {
        guild_id: GUILD,
        parent_id: GENERAL,
        thread_id: THREAD,
    }));
    account.close();
}

#[tokio::test]
async fn a_page_of_older_threads_reaches_the_subscription() {
    let (account, _server, _ws) = searching(json!({
        "threads": [archived_thread(OLD)], "members": [], "has_more": false, "total_results": 1
    }))
    .await;
    let subscription = account.store().subscribe();

    account.load_older_threads(GENERAL).await.unwrap();
    let changed = StoreEvent::OlderThreadsChanged { parent_id: GENERAL };
    let mut events = Vec::new();
    // The page's start, then its end.
    while events.iter().filter(|event| **event == changed).count() < 2 {
        events.extend(events_until(&subscription, |event| *event == changed).await);
    }

    let added = events.iter().position(
        |event| matches!(event, StoreEvent::ChannelAdded { channel_id, .. } if *channel_id == OLD),
    );
    let finished = events.iter().rposition(|event| *event == changed);
    assert!(added.is_some() && added < finished, "{events:?}");
    account.close();
}
