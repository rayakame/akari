use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use akari_core::auth::CaptchaChallenge as CoreChallenge;
use akari_core::model::MessageId;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, events_until, fixture, online};
use super::support::{block_on, refused_connection, token};
use crate::account::{Account, MessageLoad};
use crate::errors::{NetworkErrorKind, RequestError};
use crate::login::CaptchaChallenge;
use crate::records::Delivery;
use crate::subscription::{StoreEvent, StoreSubscription};

const MESSAGES: &str = "/api/v9/channels/300000000000000002/messages";

struct Online {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    _ws: WebSocketStream<TcpStream>,
}

async fn online_with(server: &MockServer) -> Online {
    let gateway = FakeGateway::start().await;
    let account = gateway
        .client_with_api(&format!("{}/api/v9/", server.uri()))
        .account(token("t"))
        .unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    Online {
        account,
        subscription,
        _ws: ws,
    }
}

/// Answers sends with `statuses` in turn (the last one repeats); 200 echoes the nonce.
async fn mount_sends(server: &MockServer, statuses: &'static [u16], delay: Duration) {
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path(MESSAGES))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let call = calls.fetch_add(1, Ordering::SeqCst);
            let status = statuses[call.min(statuses.len() - 1)];
            let template = ResponseTemplate::new(status).set_delay(delay);
            if status == 200 {
                let mut message = fixture(include_str!(
                    "../../../akari-core/tests/fixtures/message_create.json"
                ));
                message["id"] = (400_000_000_000_000_050 + call as u64).to_string().into();
                message["content"] = body["content"].clone();
                message["nonce"] = body["nonce"].clone();
                message["author"] = json!({"id": "100000000000000001", "username": "akari_tester"});
                template.set_body_json(message)
            } else {
                template.set_body_json(json!({"message": "Missing Permissions", "code": 50013}))
            }
        })
        .mount(server)
        .await;
}

#[tokio::test]
async fn loading_latest_fills_the_window() {
    let server = MockServer::start().await;
    Mock::given(path(MESSAGES))
        .and(query_param("limit", "50"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture(include_str!(
                "../../../akari-core/tests/fixtures/messages_page.json"
            ))),
        )
        .mount(&server)
        .await;
    let online = online_with(&server).await;

    online
        .account
        .load_messages(GENERAL, MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();

    let events = events_until(&online.subscription, |event| {
        matches!(event, StoreEvent::MessagesLoaded { .. })
    })
    .await;
    let ids = [10, 11, 12].map(|n| MessageId::new(400_000_000_000_000_000 + n));
    assert_eq!(
        events.last(),
        Some(&StoreEvent::MessagesLoaded {
            channel_id: GENERAL,
            first: ids[0],
            last: ids[2]
        })
    );
    assert_eq!(
        online.account.store().window(GENERAL).unwrap().message_ids,
        ids
    );
    online.account.close();
}

#[tokio::test]
async fn sending_shows_a_pending_message_then_replaces_it() {
    let server = MockServer::start().await;
    mount_sends(&server, &[200], Duration::from_millis(300)).await;
    let online = online_with(&server).await;
    online.account.view_channel(GENERAL);

    let sending = {
        let account = online.account.clone();
        tokio::spawn(async move { account.send_message(GENERAL, "hello".to_owned()).await })
    };
    let events = events_until(&online.subscription, |event| {
        matches!(event, StoreEvent::MessageInserted { .. })
    })
    .await;
    let Some(StoreEvent::MessageInserted {
        message_id: pending,
        ..
    }) = events.last().cloned()
    else {
        panic!("no pending message: {events:?}");
    };
    let shown = online.account.store().messages(GENERAL, vec![pending]);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].delivery, Delivery::Pending);
    assert_eq!(shown[0].content, "hello");

    let sent = sending.await.unwrap().unwrap();
    let events = events_until(&online.subscription, |event| {
        matches!(event, StoreEvent::MessageReplaced { .. })
    })
    .await;
    assert_eq!(sent, MessageId::new(400_000_000_000_000_050));
    assert_eq!(
        events.last(),
        Some(&StoreEvent::MessageReplaced {
            channel_id: GENERAL,
            pending_id: pending,
            message_id: sent
        })
    );
    online.account.close();
}

#[tokio::test]
async fn a_failed_send_stays_until_retried_or_discarded() {
    let server = MockServer::start().await;
    mount_sends(&server, &[403, 200, 403], Duration::ZERO).await;
    let online = online_with(&server).await;
    online.account.view_channel(GENERAL);
    let store = online.account.store();

    let failed = online
        .account
        .send_message(GENERAL, "hello".to_owned())
        .await;
    assert_eq!(
        failed,
        Err(RequestError::Discord {
            status: 403,
            code: 50013,
            message: "Missing Permissions".to_owned()
        })
    );
    let pending = store.window(GENERAL).unwrap().pending_ids;
    assert_eq!(pending.len(), 1);
    assert_eq!(
        store.messages(GENERAL, pending.clone())[0].delivery,
        Delivery::Failed
    );

    let retried = online.account.retry_message(GENERAL, pending[0]).await;
    assert_eq!(retried, Ok(MessageId::new(400_000_000_000_000_051)));

    online
        .account
        .send_message(GENERAL, "again".to_owned())
        .await
        .unwrap_err();
    let failed = store.window(GENERAL).unwrap().pending_ids;
    online.account.discard_message(GENERAL, failed[0]);
    let events = events_until(&online.subscription, |event| {
        matches!(event, StoreEvent::MessageDeleted { .. })
    })
    .await;
    assert_eq!(
        events.last(),
        Some(&StoreEvent::MessageDeleted {
            channel_id: GENERAL,
            message_id: failed[0]
        })
    );
    assert!(store.window(GENERAL).unwrap().pending_ids.is_empty());
    online.account.close();
}

#[tokio::test]
async fn request_errors_map_one_to_one() {
    use akari_core::RequestError as Core;

    let challenge = CoreChallenge {
        service: "hcaptcha".to_owned(),
        sitekey: Some("site-key".to_owned()),
        rqdata: None,
        rqtoken: Some("rq-token".to_owned()),
        session_id: None,
        should_serve_invisible: false,
    };
    let cases = [
        (Core::Unauthorized, RequestError::Unauthorized),
        (
            Core::RateLimited {
                retry_after: Some(Duration::from_millis(1500)),
            },
            RequestError::RateLimited {
                retry_after: Some(Duration::from_millis(1500)),
            },
        ),
        (
            Core::CaptchaRequired(Box::new(challenge)),
            RequestError::CaptchaRequired {
                challenge: Box::new(CaptchaChallenge {
                    service: "hcaptcha".to_owned(),
                    sitekey: Some("site-key".to_owned()),
                    rqdata: None,
                    rqtoken: Some("rq-token".to_owned()),
                    session_id: None,
                    should_serve_invisible: false,
                }),
            },
        ),
        (
            Core::Discord {
                status: 400,
                code: 50035,
                message: "Invalid Form Body".to_owned(),
            },
            RequestError::Discord {
                status: 400,
                code: 50035,
                message: "Invalid Form Body".to_owned(),
            },
        ),
        (
            Core::ServerError { status: 502 },
            RequestError::ServerError { status: 502 },
        ),
        (Core::UnexpectedResponse, RequestError::UnexpectedResponse),
        (Core::InvalidRequest, RequestError::InvalidRequest),
        (Core::Closed, RequestError::Closed),
        (
            Core::Network(refused_connection().await),
            RequestError::Network {
                kind: NetworkErrorKind::Connect,
            },
        ),
    ];

    for (core, expected) in cases {
        assert_eq!(RequestError::from(core), expected);
    }
}

#[tokio::test]
async fn a_load_without_network_reports_the_kind() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t")).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    let err = account
        .load_messages(GENERAL, MessageLoad::Latest { limit: 50 })
        .await;

    assert_eq!(
        err,
        Err(RequestError::Network {
            kind: NetworkErrorKind::Connect
        })
    );
    account.close();
}

#[tokio::test]
async fn message_actions_work_without_a_tokio_context() {
    let server = MockServer::start().await;
    mount_sends(&server, &[200], Duration::ZERO).await;
    Mock::given(path(MESSAGES))
        .and(query_param("limit", "50"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture(include_str!(
                "../../../akari-core/tests/fixtures/messages_page.json"
            ))),
        )
        .mount(&server)
        .await;
    let online = online_with(&server).await;
    let account = online.account.clone();

    let (loaded, sent) = std::thread::spawn(move || {
        assert!(tokio::runtime::Handle::try_current().is_err());
        let loaded = block_on(account.load_messages(GENERAL, MessageLoad::Latest { limit: 50 }));
        let sent = block_on(account.send_message(GENERAL, "hello".to_owned()));
        (loaded, sent)
    })
    .join()
    .unwrap();

    assert_eq!(loaded, Ok(()));
    assert_eq!(sent, Ok(MessageId::new(400_000_000_000_000_050)));
    online.account.close();
}
