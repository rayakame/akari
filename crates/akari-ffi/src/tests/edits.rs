use std::sync::Arc;
use std::time::Duration;

use akari_core::model::MessageId;
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fake::{FakeGateway, GENERAL, MESSAGE, events_until, fixture, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::account::Account;
use crate::errors::{EditError, NetworkErrorKind, RequestError};
use crate::records::{Message, MessageEdit};
use crate::subscription::{StoreEvent, StoreSubscription};

const ONE_MESSAGE: &str = "/messages/[0-9]+$";

struct Editing {
    account: Arc<Account>,
    subscription: Arc<StoreSubscription>,
    server: MockServer,
    _ws: WebSocketStream<TcpStream>,
}

async fn editing() -> Editing {
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
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message.as_object_mut().unwrap().remove("nonce");
    message["author"]["id"] = USER.get().to_string().into();
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": message}),
    )
    .await;
    events_until(&subscription, |event| {
        matches!(event, StoreEvent::MessageInserted { .. })
    })
    .await;
    Editing {
        account,
        subscription,
        server,
        _ws: ws,
    }
}

impl Editing {
    async fn answer(&self, verb: &str, answer: ResponseTemplate) {
        Mock::given(method(verb))
            .and(path_regex(ONE_MESSAGE))
            .respond_with(answer)
            .mount(&self.server)
            .await;
    }

    fn record(&self) -> Option<Message> {
        self.account
            .store()
            .messages(GENERAL, vec![MESSAGE])
            .into_iter()
            .next()
    }

    async fn updated(&self) {
        events_until(&self.subscription, |event| {
            *event
                == StoreEvent::MessageUpdated {
                    channel_id: GENERAL,
                    message_id: MESSAGE,
                }
        })
        .await;
    }
}

fn edited(text: &str) -> ResponseTemplate {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message["author"]["id"] = USER.get().to_string().into();
    message["content"] = text.into();
    message["edited_timestamp"] = "2026-11-15T12:00:00.000000+00:00".into();
    ResponseTemplate::new(200).set_body_json(message)
}

#[tokio::test]
async fn a_saved_edit_shows_pending_on_the_record_until_discord_answers() {
    let editing = editing().await;
    editing
        .answer("PATCH", edited("new").set_delay(Duration::from_millis(200)))
        .await;

    let account = editing.account.clone();
    let saving = tokio::spawn(async move {
        account
            .edit_message(GENERAL, MESSAGE, "new".to_owned())
            .await
    });
    editing.updated().await;
    let pending = editing.record().unwrap();
    saving.await.unwrap().unwrap();

    assert_eq!(pending.content, "new");
    assert_eq!(pending.edit, Some(MessageEdit::Pending));
    let record = editing.record().unwrap();
    assert_eq!(record.content, "new");
    assert_eq!(record.edit, None);
    assert!(record.edited_timestamp.is_some());
    assert!(!record.deleting);
    editing.account.close();
}

#[tokio::test]
async fn a_refused_edit_keeps_its_text_and_reason_on_the_record_until_dismissed() {
    let editing = editing().await;
    editing
        .answer(
            "PATCH",
            ResponseTemplate::new(400)
                .set_body_json(json!({"message": "Blocked here", "code": 200_000})),
        )
        .await;

    let outcome = editing
        .account
        .edit_message(GENERAL, MESSAGE, "bad".to_owned())
        .await;

    let blocked = EditError::MessageBlocked {
        reason: "Blocked here".to_owned(),
    };
    assert_eq!(outcome, Err(blocked.clone()));
    let record = editing.record().unwrap();
    assert_eq!(record.content, "hi");
    assert_eq!(
        record.edit,
        Some(MessageEdit::Failed {
            content: "bad".to_owned(),
            error: blocked,
        })
    );
    editing.account.dismiss_edit(GENERAL, MESSAGE);
    assert_eq!(editing.record().unwrap().edit, None);
    editing.account.close();
}

#[tokio::test]
async fn a_delete_dims_the_record_then_takes_it_away() {
    let editing = editing().await;
    editing
        .answer(
            "DELETE",
            ResponseTemplate::new(204).set_delay(Duration::from_millis(200)),
        )
        .await;

    let account = editing.account.clone();
    let deleting = tokio::spawn(async move { account.delete_message(GENERAL, MESSAGE).await });
    editing.updated().await;
    let dimmed = editing.record().unwrap();
    deleting.await.unwrap().unwrap();

    assert!(dimmed.deleting);
    assert!(
        !editing
            .account
            .store()
            .message_actions(GENERAL, MESSAGE)
            .delete
    );
    events_until(&editing.subscription, |event| {
        *event
            == StoreEvent::MessageDeleted {
                channel_id: GENERAL,
                message_id: MESSAGE,
            }
    })
    .await;
    assert!(editing.record().is_none());
    editing.account.close();
}

#[tokio::test]
async fn a_refused_delete_crosses_as_a_request_error() {
    let editing = editing().await;
    editing
        .answer(
            "DELETE",
            ResponseTemplate::new(403)
                .set_body_json(json!({"message": "Missing Permissions", "code": 50013})),
        )
        .await;

    let outcome = editing.account.delete_message(GENERAL, MESSAGE).await;

    assert!(
        matches!(
            outcome,
            Err(RequestError::Discord {
                status: 403,
                code: 50013,
                ..
            })
        ),
        "{outcome:?}"
    );
    assert!(!editing.record().unwrap().deleting);
    editing.account.close();
}

#[tokio::test]
async fn up_arrows_pick_is_the_newest_message_the_user_may_edit() {
    let editing = editing().await;

    let picked = editing.account.store().newest_editable_message(GENERAL);

    assert_eq!(picked, Some(MESSAGE));
    assert_eq!(
        editing
            .account
            .store()
            .newest_editable_message(MessageId::new(1).cast()),
        None
    );
    editing.account.close();
}

#[test]
fn every_edit_error_crosses_with_what_the_ui_says() {
    use akari_core::EditError as Core;

    let cases = [
        (
            Core::MessageBlocked {
                reason: "No".to_owned(),
            },
            EditError::MessageBlocked {
                reason: "No".to_owned(),
            },
        ),
        (
            Core::TooManyEdits {
                retry_after: Some(Duration::from_secs(60)),
            },
            EditError::TooManyEdits {
                retry_after: Some(Duration::from_secs(60)),
            },
        ),
        (Core::Forbidden, EditError::Forbidden),
        (Core::Deleted, EditError::Deleted),
        (
            Core::RateLimited { retry_after: None },
            EditError::RateLimited { retry_after: None },
        ),
        (
            Core::Request(akari_core::RequestError::TooLong { limit: 4000 }),
            EditError::Request {
                error: RequestError::TooLong { limit: 4000 },
            },
        ),
    ];
    for (core, expected) in cases {
        assert_eq!(EditError::from(core), expected);
    }
    let network = EditError::Request {
        error: RequestError::Network {
            kind: NetworkErrorKind::Connect,
        },
    };
    assert_eq!(network.to_string(), "network error");
}
