use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, MESSAGE, events_until, fixture, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::account::Account;
use crate::errors::{PinError, RequestError};
use crate::records::{PinsPage, PinsState};
use crate::subscription::{StoreEvent, StoreSubscription};

const PINNED_AT: &str = "2026-10-02T09:30:00.250000+00:00";
const PINNED_MILLIS: u64 = 1_790_933_400_250;

struct Pinning {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    server: MockServer,
    ws: WebSocketStream<TcpStream>,
}

fn message_json() -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message.as_object_mut().unwrap().remove("nonce");
    message
}

async fn pinning() -> Pinning {
    let server = MockServer::start().await;
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"), USER)
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    account.view_channel(GENERAL);
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": message_json()}),
    )
    .await;
    events_until(&subscription, |event| {
        matches!(event, StoreEvent::MessageInserted { .. })
    })
    .await;
    Pinning {
        account,
        subscription,
        server,
        ws,
    }
}

impl Pinning {
    async fn answer(&self, verb: &str, path: &str, answer: ResponseTemplate) {
        Mock::given(method(verb))
            .and(path_regex(path))
            .respond_with(answer)
            .mount(&self.server)
            .await;
    }

    fn pins(&self) -> PinsPage {
        self.account.store().pins(GENERAL)
    }

    async fn pins_changed(&self) {
        events_until(&self.subscription, |event| {
            *event
                == StoreEvent::PinsChanged {
                    channel_id: GENERAL,
                }
        })
        .await;
    }
}

fn listed(more: bool) -> ResponseTemplate {
    let mut message = message_json();
    message["pinned"] = true.into();
    message["content"] = "**pinned** text".into();
    ResponseTemplate::new(200).set_body_json(json!({
        "items": [{"pinned_at": PINNED_AT, "message": message}],
        "has_more": more,
    }))
}

#[tokio::test]
async fn a_page_of_pins_crosses_as_records_with_their_times_and_formatted_text() {
    let pinning = pinning().await;
    assert_eq!(pinning.pins().state, PinsState::NotLoaded);
    pinning.answer("GET", "/pins$", listed(true)).await;

    pinning.account.load_pins(GENERAL).await.unwrap();
    pinning.pins_changed().await;

    let page = pinning.pins();
    assert_eq!(page.state, PinsState::Loaded);
    assert!(page.has_more);
    assert_eq!(page.items.len(), 1);
    let item = &page.items[0];
    assert_eq!(item.message.id, MESSAGE);
    assert_eq!(item.message.content, "**pinned** text");
    assert!(item.message.pinned);
    assert_eq!(item.message.formatted.blocks.len(), 1);
    assert_eq!(
        item.pinned_at,
        UNIX_EPOCH + Duration::from_millis(PINNED_MILLIS)
    );
    let actions = pinning.account.store().message_actions(GENERAL, MESSAGE);
    assert!(actions.unpin);
    pinning.account.close();
}

#[tokio::test]
async fn a_failed_first_load_reads_failed_and_the_next_page_follows() {
    let pinning = pinning().await;
    pinning
        .answer("GET", "/pins$", ResponseTemplate::new(500))
        .await;

    let failed = pinning.account.load_pins(GENERAL).await;

    assert_eq!(failed, Err(RequestError::ServerError { status: 500 }));
    assert_eq!(pinning.pins().state, PinsState::Failed);
    assert!(pinning.pins().items.is_empty());
    pinning.server.reset().await;
    pinning.answer("GET", "/pins$", listed(true)).await;
    pinning.account.load_pins(GENERAL).await.unwrap();
    pinning.account.load_more_pins(GENERAL).await.unwrap();
    let asked: Vec<String> = pinning
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter_map(|request| request.url.query().map(str::to_owned))
        .collect();
    assert_eq!(
        asked,
        ["limit=25", "limit=25&before=2026-10-02T09%3A30%3A00.250Z"]
    );
    pinning.account.close();
}

#[tokio::test]
async fn pinning_and_unpinning_cross_with_their_errors() {
    let pinning = pinning().await;
    pinning
        .answer(
            "PUT",
            "/pins/",
            ResponseTemplate::new(400).set_body_json(
                json!({"message": "Maximum number of pins reached (250)", "code": 30003}),
            ),
        )
        .await;

    let too_many = pinning.account.pin(GENERAL, MESSAGE).await;
    let not_pinned = pinning.account.unpin(GENERAL, MESSAGE).await;

    assert_eq!(too_many, Err(PinError::TooMany { limit: 250 }));
    assert_eq!(
        not_pinned,
        Err(PinError::Request {
            error: RequestError::InvalidRequest
        })
    );
    assert!(too_many.unwrap_err().to_string().contains("250"));
    pinning.account.close();
}

#[test]
fn every_pin_error_crosses_with_what_the_ui_says() {
    use akari_core::PinError as Core;

    let cases = [
        (
            Core::TooMany { limit: 250 },
            PinError::TooMany { limit: 250 },
        ),
        (Core::Archived, PinError::Archived),
        (Core::SystemMessage, PinError::SystemMessage),
        (Core::WrongChannel, PinError::WrongChannel),
        (Core::Forbidden, PinError::Forbidden),
        (Core::Gone, PinError::Gone),
        (
            Core::Request(akari_core::RequestError::ServerError { status: 502 }),
            PinError::Request {
                error: RequestError::ServerError { status: 502 },
            },
        ),
    ];
    for (core, expected) in cases {
        assert_eq!(PinError::from(core), expected);
    }
}

#[tokio::test]
async fn a_page_on_its_way_reads_loading() {
    let pinning = pinning().await;
    pinning
        .answer(
            "GET",
            "/pins$",
            listed(false).set_delay(Duration::from_secs(10)),
        )
        .await;
    let account = Arc::clone(&pinning.account);

    let loading = tokio::spawn(async move { account.load_pins(GENERAL).await });
    pinning.pins_changed().await;

    assert_eq!(pinning.pins().state, PinsState::Loading);
    loading.abort();
    pinning.account.close();
}

#[tokio::test]
async fn acking_pins_takes_the_dot_and_says_so() {
    let mut pinning = pinning().await;
    pinning
        .answer("POST", "/pins/ack$", ResponseTemplate::new(204))
        .await;
    send(
        &mut pinning.ws,
        json!({"op": 0, "s": 3, "t": "CHANNEL_PINS_UPDATE", "d": {
            "channel_id": GENERAL.get().to_string(), "last_pin_timestamp": PINNED_AT,
        }}),
    )
    .await;
    events_until(&pinning.subscription, |event| {
        matches!(event, StoreEvent::PinsUnreadChanged { .. })
    })
    .await;
    assert!(pinning.account.store().pins_unread(GENERAL));

    pinning.account.ack_pins(GENERAL);

    assert!(!pinning.account.store().pins_unread(GENERAL));
    events_until(&pinning.subscription, |event| {
        *event
            == StoreEvent::PinsUnreadChanged {
                channel_id: GENERAL,
            }
    })
    .await;
    pinning.account.close();
}
