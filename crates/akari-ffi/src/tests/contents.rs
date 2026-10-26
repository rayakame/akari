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
use crate::contents::{Contents, Found};
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
        .load_messages(GENERAL, MessageLoad::Latest { limit: 50 }, None)
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

    assert_eq!(read[1].embeds.len(), 1);
    let stats = stats(&online.account);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 0, 3));
    online.account.close();
}

struct Restoring {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    gateway: FakeGateway,
    _client: Arc<DiscordClient>,
    _server: MockServer,
    _dir: super::cache::TempDir,
}

// A previous session cached `body`'s messages; a new account restores them, and its first page
// (the same messages) answers after `delay`.
async fn restoring(body: Value, delay: Duration) -> Restoring {
    let dir = super::cache::TempDir::new();
    let server = MockServer::start().await;
    let gateway = FakeGateway::start().await;
    let endpoints = akari_core::Endpoints {
        api: format!("{}/api/v9/", server.uri()),
        ..gateway.endpoints()
    };
    let core = core_client(endpoints, Arc::new(MemoryStore::default())).with_cache(dir.path());
    let client = DiscordClient::from_core(core).unwrap();
    mock_page(&server, body.clone(), Duration::ZERO).await;
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
    mock_page(&server, body, delay).await;

    let account = client.account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    if account.store().current_user().is_none() {
        events_until(&subscription, |event| *event == StoreEvent::Restored).await;
    }
    Restoring {
        account,
        subscription,
        gateway,
        _client: client,
        _server: server,
        _dir: dir,
    }
}

#[tokio::test]
async fn a_restore_leaves_nothing_to_parse_on_read() {
    let restoring = restoring(three(), Duration::from_millis(500)).await;
    let again = restoring.account.clone();

    let loading = tokio::spawn(async move { load(&again).await });
    events_until(&restoring.subscription, |event| {
        matches!(event, StoreEvent::MessagesLoaded { .. })
    })
    .await;
    assert!(!loading.is_finished(), "the page came before the cache");
    let shown = read_all(&restoring.account);

    assert_eq!(shown.len(), 3);
    let stats = stats(&restoring.account);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 0, 3));
    loading.await.unwrap();
    restoring.account.close();
}

// akari-markdown's realistic samples, the same ones its parse_cost measures.
fn samples() -> Vec<String> {
    fn unescape(quoted: &str) -> String {
        let mut out = String::new();
        let mut chars = quoted.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('u') => {
                    let hex: String = chars.by_ref().skip(1).take_while(|&c| c != '}').collect();
                    out.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
                }
                Some(other) => out.push(other),
                None => {}
            }
        }
        out
    }
    include_str!("../../../akari-markdown/tests/corpus/samples.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("input: \"")?.strip_suffix('"'))
        .map(unescape)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "prints a restored launch's parse (observe-only); run with --release"]
async fn restored_parse_cost() {
    let samples = samples();
    let typical: Vec<String> = samples.iter().cycle().take(50).cloned().collect();
    let worst: Vec<String> = (0..50).map(adversarial).collect();
    for (name, contents) in [("typical", typical), ("adversarial", worst)] {
        let bytes: usize = contents.iter().map(String::len).sum();
        let restoring = restoring(
            page(400_000_000_000_000_300, &contents),
            Duration::from_millis(200),
        )
        .await;

        let report = load(&restoring.account).await;
        let reading = Instant::now();
        let shown = read_all(&restoring.account);
        let read = reading.elapsed();

        println!(
            "{name}: 50 messages, {bytes} bytes: parse {:.2} ms ({} parsed), read {:.2} ms \
             for {} rows, {:?}",
            report.parse.as_secs_f64() * 1000.0,
            report.parsed,
            read.as_secs_f64() * 1000.0,
            shown.len(),
            stats(&restoring.account)
        );
        restoring.account.close();
    }
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

#[tokio::test]
async fn only_documents_a_load_parsed_count_as_misses() {
    let contents: Vec<String> = (0..4).map(|i| format!("message {i}")).collect();
    let online = online_with(page(400_000_000_000_000_200, &contents)).await;
    load(&online.account).await;
    let window = online.account.store().core().messages(GENERAL).unwrap();
    let messages = &window.messages;

    let small = Contents::with_capacity(2);
    small.prepare(&messages[..3]);

    assert_eq!(
        small.document(&messages[0]).1,
        Found::Miss,
        "prepared, then dropped"
    );
    assert_eq!(
        small.document(&messages[3]).1,
        Found::EventParse,
        "never prepared"
    );
    assert_eq!(small.document(&messages[3]).1, Found::Cached);
    online.account.close();
}

#[tokio::test]
async fn a_message_held_during_a_restore_is_an_event_parse() {
    let restoring = restoring(three(), Duration::from_millis(500)).await;
    let account = restoring.account.clone();
    account.connect().unwrap();
    let mut ws = restoring.gateway.serve_ready().await;
    events_until(&restoring.subscription, online).await;

    let loading = tokio::spawn(async move { load(&account).await });
    events_until(&restoring.subscription, |event| {
        matches!(event, StoreEvent::MessagesLoaded { .. })
    })
    .await;
    let mut created = page_message(400_000_000_000_000_013, "*live*");
    created["channel_id"] = GENERAL.get().to_string().into();
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": created}),
    )
    .await;
    loading.await.unwrap();
    // The page can land before the store has taken the live message in.
    for _ in 0..500 {
        let window = restoring.account.store().core().messages(GENERAL).unwrap();
        if window.messages.len() == 4 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let read = read_all(&restoring.account);

    assert_eq!(read.len(), 4);
    assert_eq!(texts(&read[3]), "live");
    let stats = stats(&restoring.account);
    assert_eq!((stats.misses, stats.event_parses, stats.parsed), (0, 1, 4));
    restoring.account.close();
}

#[tokio::test]
async fn a_same_length_update_without_an_edit_time_is_parsed_again() {
    let mut online = online_with(three()).await;
    load(&online.account).await;
    read_all(&online.account);

    update(
        &mut online,
        2,
        json!({"id": "400000000000000010", "channel_id": GENERAL.get().to_string(),
               "content": "__a__"}),
    )
    .await;
    let read = read_all(&online.account);

    assert_eq!(read[0].content, "__a__");
    assert_eq!(read[0].edited_timestamp, None);
    let span = &read[0].formatted.blocks[0].spans[0];
    assert!(span.underline && !span.bold, "{span:?}");
    online.account.close();
}
