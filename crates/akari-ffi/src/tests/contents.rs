use std::sync::Arc;
use std::time::{Duration, Instant};

use akari_core::MessagePreparer;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, events_until, fixture, online};
use super::gateway::send;
use super::support::{MemoryStore, USER, core_client, token};
use crate::account::{Account, MessageLoad};
use crate::client::DiscordClient;
use crate::contents::Contents;
use crate::formatted::SpanKind;
use crate::records::{ContentStats, Message};
use crate::subscription::{StoreEvent, StoreSubscription};

const MESSAGES: &str = "/api/v9/channels/300000000000000002/messages";

fn page_message(id: u64, content: &str) -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message["id"] = id.to_string().into();
    message["content"] = content.into();
    message.as_object_mut().unwrap().remove("nonce");
    message
}

fn page(first: u64, contents: &[String]) -> Value {
    Value::Array(
        contents
            .iter()
            .enumerate()
            .rev()
            .map(|(i, content)| page_message(first + i as u64, content))
            .collect(),
    )
}

async fn mock_page(server: &MockServer, body: Value, delay: Duration) {
    Mock::given(path(MESSAGES))
        .and(query_param("limit", "50"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(body)
                .set_delay(delay),
        )
        .mount(server)
        .await;
}

struct Online {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    ws: WebSocketStream<TcpStream>,
    _server: MockServer,
}

async fn online_with(body: Value) -> Online {
    let server = MockServer::start().await;
    mock_page(&server, body, Duration::ZERO).await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    Online {
        account,
        subscription,
        ws,
        _server: server,
    }
}

fn read_all(account: &Account) -> Vec<Message> {
    let store = account.store();
    let ids = store.window(GENERAL).unwrap().message_ids;
    store.messages(GENERAL, ids)
}

fn stats(account: &Account) -> ContentStats {
    account.store().content_stats()
}

fn texts(message: &Message) -> String {
    message
        .formatted
        .blocks
        .iter()
        .flat_map(|block| &block.spans)
        .filter_map(|span| match span.kind {
            SpanKind::Text { start, end } => {
                let units: Vec<u16> = message.content.encode_utf16().collect();
                Some(String::from_utf16(&units[start as usize..end as usize]).unwrap())
            }
            _ => None,
        })
        .collect()
}

fn three() -> Value {
    page(
        400_000_000_000_000_010,
        &["**a**".to_owned(), "b".to_owned(), "c".to_owned()],
    )
}

async fn load(account: &Account) -> crate::records::LoadReport {
    account
        .load_messages(GENERAL, MessageLoad::Latest { limit: 50 })
        .await
        .unwrap()
        .expect("a page was loaded")
}

#[tokio::test]
async fn a_load_leaves_nothing_to_parse_on_read() {
    let online = online_with(three()).await;

    let report = load(&online.account).await;
    let read = read_all(&online.account);

    assert_eq!(read.len(), 3);
    assert_eq!(texts(&read[0]), "a");
    assert_eq!(
        stats(&online.account),
        ContentStats {
            parsed: 3,
            misses: 0,
            event_parses: 0,
            cached: 3
        }
    );
    assert_eq!(report.parsed, 3);
    online.account.close();
}

#[tokio::test]
async fn the_load_report_counts_the_parse() {
    let online = online_with(three()).await;

    let started = Instant::now();
    let report = load(&online.account).await;
    let wall = started.elapsed();

    assert_eq!(report.parsed, 3);
    assert!(report.parse <= wall, "{report:?} in {wall:?}");
    online.account.close();
}

#[tokio::test]
async fn parsing_runs_on_the_runtime() {
    let online = online_with(three()).await;

    load(&online.account).await;

    let threads = online.account.contents().prepared_on();
    let here = std::thread::current().name().map(str::to_owned);
    assert!(!threads.is_empty());
    assert!(
        threads
            .iter()
            .all(|name| name.as_deref() == Some("akari") && *name != here),
        "{threads:?}"
    );
    online.account.close();
}

// Each 4,000 characters, made of the parser's worst cases.
fn adversarial(i: usize) -> String {
    let unit = [
        "*", "_", "~", "|", "`", "[", "<", ">", "**a", "||a", "[a](", "<t:", "# \n", "> ", "-# # ",
        "😀", "\\*",
    ][i % 17];
    unit.repeat(4000 / unit.chars().count() + 1)
        .chars()
        .take(4000)
        .collect()
}

#[tokio::test]
async fn a_page_of_adversarial_messages_is_parsed_off_the_main_thread() {
    let contents: Vec<String> = (0..50).map(adversarial).collect();
    assert!(
        contents
            .iter()
            .all(|content| content.chars().count() == 4000)
    );
    let online = online_with(page(400_000_000_000_000_100, &contents)).await;

    let report = load(&online.account).await;
    let read = read_all(&online.account);

    assert_eq!(read.len(), 50);
    assert_eq!(report.parsed, 50);
    let stats = stats(&online.account);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 0, 50));
    let threads = online.account.contents().prepared_on();
    assert!(
        threads.iter().all(|name| name.as_deref() == Some("akari")),
        "{threads:?}"
    );
    online.account.close();
}

#[tokio::test]
async fn a_live_message_is_an_event_parse() {
    let mut online = online_with(three()).await;
    load(&online.account).await;
    read_all(&online.account);

    let mut created = page_message(400_000_000_000_000_013, "*new*");
    created["channel_id"] = GENERAL.get().to_string().into();
    send(
        &mut online.ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": created}),
    )
    .await;
    events_until(&online.subscription, |event| {
        matches!(event, StoreEvent::MessageInserted { .. })
    })
    .await;
    let read = read_all(&online.account);

    assert_eq!(texts(&read[3]), "new");
    let stats = stats(&online.account);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 1, 4));
    online.account.close();
}

async fn update(online: &mut Online, seq: u64, data: Value) {
    send(
        &mut online.ws,
        json!({"op": 0, "s": seq, "t": "MESSAGE_UPDATE", "d": data}),
    )
    .await;
    events_until(&online.subscription, |event| {
        matches!(event, StoreEvent::MessageUpdated { .. })
    })
    .await;
}

#[tokio::test]
async fn an_edit_after_preparing_is_parsed_on_read() {
    let mut online = online_with(three()).await;
    load(&online.account).await;

    update(
        &mut online,
        2,
        json!({"id": "400000000000000011", "channel_id": GENERAL.get().to_string(),
               "content": "__edited__", "edited_timestamp": "2024-06-01T12:00:00.000000+00:00"}),
    )
    .await;
    let read = read_all(&online.account);

    assert_eq!(read[1].content, "__edited__");
    assert_eq!(texts(&read[1]), "edited");
    assert!(read[1].formatted.blocks[0].spans[0].underline);
    let stats = stats(&online.account);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 1, 4));
    online.account.close();
}

#[tokio::test]
async fn an_update_without_an_edit_reuses_the_document() {
    let mut online = online_with(three()).await;
    load(&online.account).await;
    read_all(&online.account);

    update(
        &mut online,
        2,
        json!({"id": "400000000000000011", "channel_id": GENERAL.get().to_string(),
               "embeds": [{"type": "link", "url": "https://example.com/"}]}),
    )
    .await;
    let read = read_all(&online.account);

    assert_eq!(read[1].embed_count, 1);
    let stats = stats(&online.account);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 0, 3));
    online.account.close();
}

#[tokio::test]
async fn a_restore_leaves_nothing_to_parse_on_read() {
    let dir = super::cache::TempDir::new();
    let server = MockServer::start().await;
    let gateway = FakeGateway::start().await;
    let endpoints = akari_core::Endpoints {
        api: format!("{}/api/v9/", server.uri()),
        ..gateway.endpoints()
    };
    let core = core_client(endpoints, Arc::new(MemoryStore::default())).with_cache(dir.path());
    let client = DiscordClient::from_core(core).unwrap();
    mock_page(&server, three(), Duration::ZERO).await;
    let first = client.account(token("t"), USER).unwrap();
    let subscription = first.store().subscribe();
    first.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    load(&first).await;
    first.flush_cache().await;
    first.close();
    drop(first);
    server.reset().await;
    mock_page(&server, three(), Duration::from_millis(500)).await;

    let again = client.account(token("t"), USER).unwrap();
    let restored = again.store().subscribe();
    if again.store().current_user().is_none() {
        events_until(&restored, |event| *event == StoreEvent::Restored).await;
    }
    let loading = {
        let again = again.clone();
        tokio::spawn(async move { load(&again).await })
    };
    events_until(&restored, |event| {
        matches!(event, StoreEvent::MessagesLoaded { .. })
    })
    .await;
    assert!(!loading.is_finished(), "the page came before the cache");
    let shown = read_all(&again);

    assert_eq!(shown.len(), 3);
    let stats = stats(&again);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 0, 3));
    loading.await.unwrap();
    again.close();
}

#[tokio::test]
async fn contents_keep_at_most_their_capacity() {
    let contents: Vec<String> = (0..20).map(|i| format!("message {i}")).collect();
    let online = online_with(page(400_000_000_000_000_200, &contents)).await;
    load(&online.account).await;
    let window = online.account.store().core().messages(GENERAL).unwrap();

    let small = Contents::with_capacity(8);
    let prepared = small.prepare(&window.messages);

    assert_eq!(prepared, 20);
    assert!(small.stats().cached <= 8);
    online.account.close();
}
