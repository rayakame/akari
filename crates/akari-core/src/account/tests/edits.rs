use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Request as Received, Respond, ResponseTemplate};

use super::*;
use crate::EditError;
use crate::state::MessageEdit;

const TARGET: u64 = 400_000_000_000_000_003;
const THREAD: u64 = 300_000_000_000_000_020;
const STARTER_ROW: u64 = 400_000_000_000_000_031;
const PATH: &str = "/api/v9/channels/300000000000000002/messages/400000000000000003";
const ONE_MESSAGE: &str = "/messages/[0-9]+$";
const EDITED_AT: &str = "2026-11-15T12:00:00.000000+00:00";

fn target() -> MessageId {
    Snowflake::new(TARGET)
}

// The user's own message, as MESSAGE_CREATE brings it.
fn mine(seq: u64, id: u64) -> Value {
    let mut created = message(seq, id);
    created["d"]["author"]["id"] = ME.into();
    created["d"]["author"]["username"] = "akari_tester".into();
    created
}

// Discord's answer to an edit: the message with `text`.
fn edited(text: &str) -> ResponseTemplate {
    let mut answer = mine(0, TARGET)["d"].clone();
    answer["content"] = text.into();
    answer["edited_timestamp"] = EDITED_AT.into();
    ResponseTemplate::new(200).set_body_json(answer)
}

fn refusal(status: u16, code: u32, text: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({"message": text, "code": code}))
}

struct Online {
    account: Account,
    subscription: Subscription,
    server: MockServer,
    connection: FakeConnection,
    seq: u64,
    _fake: FakeGateway,
}

// Online with #general viewed and the user's message in its window.
async fn editing() -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;
    connection.send(mine(2, TARGET)).await;
    events_until(&subscription, &format!("MessageInserted({TARGET})")).await;
    Online {
        account,
        subscription,
        server,
        connection,
        seq: 2,
        _fake: fake,
    }
}

// Answers once the test lets it.
struct Held {
    arrived: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    answer: ResponseTemplate,
}

impl Respond for Held {
    fn respond(&self, _: &Received) -> ResponseTemplate {
        let _ = self.arrived.send(());
        let _ = self.release.lock().unwrap().recv_timeout(WAIT);
        self.answer.clone()
    }
}

async fn arrival(signal: &std::sync::mpsc::Receiver<()>) {
    timeout(WAIT, async {
        while signal.try_recv().is_err() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("the request never arrived");
}

impl Online {
    async fn answer(&self, verb: &str, answer: ResponseTemplate) {
        Mock::given(method(verb))
            .and(path_regex(ONE_MESSAGE))
            .respond_with(answer)
            .mount(&self.server)
            .await;
    }

    // Holds the next request of `verb`: when it arrived, and the release.
    async fn hold(
        &self,
        verb: &str,
        answer: ResponseTemplate,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (arrived, on_arrival) = std::sync::mpsc::channel();
        let (release, on_release) = std::sync::mpsc::channel();
        Mock::given(method(verb))
            .and(path_regex(ONE_MESSAGE))
            .respond_with(Held {
                arrived,
                release: std::sync::Mutex::new(on_release),
                answer,
            })
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&self.server)
            .await;
        (on_arrival, release)
    }

    async fn dispatch(&mut self, name: &str, data: Value) {
        self.seq += 1;
        self.connection
            .send(json!({"op": 0, "s": self.seq, "t": name, "d": data}))
            .await;
    }

    // The message events since the last call, once everything sent so far is applied.
    async fn events(&mut self) -> Vec<String> {
        self.seq += 1;
        let mut events = sync(&mut self.connection, &self.subscription, self.seq).await;
        events.retain(|event| event.starts_with("Message"));
        events
    }

    // Method and path of every edit, delete and thread opening, as they went over the wire.
    async fn lines(&self) -> Vec<String> {
        self.requests()
            .await
            .iter()
            .map(|request| format!("{} {}", request.method, request.url.path()))
            .collect()
    }

    async fn requests(&self) -> Vec<Received> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| matches!(request.method.as_str(), "PATCH" | "DELETE"))
            .collect()
    }

    async fn bodies(&self) -> Vec<String> {
        self.requests()
            .await
            .iter()
            .filter(|request| request.url.path().contains("/messages/"))
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }

    fn text(&self) -> String {
        self.text_of(GENERAL, TARGET)
    }

    fn text_of(&self, channel: u64, id: u64) -> String {
        self.account
            .store()
            .message(Snowflake::new(channel), Snowflake::new(id))
            .map_or_else(
                || "(gone)".to_owned(),
                |message| message.content.to_string(),
            )
    }

    fn edit(&self) -> String {
        match self.account.store().message_edit(general(), target()) {
            None => "none".to_owned(),
            Some(MessageEdit::Pending) => "pending".to_owned(),
            Some(MessageEdit::Failed { content, error }) => format!("failed {content:?}: {error}"),
        }
    }

    fn deleting(&self) -> bool {
        self.account.store().deleting(general(), target())
    }

    async fn edit_to(&self, text: &str) -> Result<(), EditError> {
        self.account.edit_message(general(), target(), text).await
    }

    async fn delete(&self) -> Result<(), RequestError> {
        self.account.delete_message(general(), target()).await
    }
}

fn invalid(outcome: &Result<(), EditError>) -> bool {
    matches!(
        outcome,
        Err(EditError::Request(RequestError::InvalidRequest))
    )
}

#[tokio::test]
async fn an_edit_shows_at_once_and_patches_the_message_with_its_text() {
    let mut online = editing().await;
    let (arrived, release) = online.hold("PATCH", edited("new")).await;

    let (outcome, ()) = tokio::join!(online.edit_to("new"), async {
        arrival(&arrived).await;
        assert_eq!(online.text(), "new", "before the answer");
        assert_eq!(online.edit(), "pending");
        release.send(()).unwrap();
    });

    outcome.unwrap();
    assert_eq!(online.lines().await, [format!("PATCH {PATH}")]);
    assert_eq!(online.bodies().await, [r#"{"content":"new"}"#]);
    assert_eq!(online.edit(), "none");
    assert_eq!(online.text(), "new");
    let message = online.account.store().message(general(), target()).unwrap();
    assert!(message.edited_timestamp.is_some(), "the answer's edit time");
    assert!(
        online
            .events()
            .await
            .contains(&format!("MessageUpdated({TARGET})"))
    );
    let mut echo = mine(0, TARGET)["d"].clone();
    echo["content"] = "new".into();
    echo["edited_timestamp"] = EDITED_AT.into();
    online.dispatch("MESSAGE_UPDATE", echo).await;
    assert_eq!(online.events().await, [] as [&str; 0], "the echo");
}

#[tokio::test]
async fn an_edit_of_a_reply_sent_without_the_ping_keeps_it_off() {
    let mut online = editing().await;
    online.connection.send(message(3, TARGET + 1)).await;
    let mut reply = mine(4, TARGET + 2);
    reply["d"]["type"] = 19.into();
    reply["d"]["mentions"] = json!([]);
    reply["d"]["message_reference"] = json!({
        "message_id": (TARGET + 1).to_string(), "channel_id": GENERAL.to_string(),
    });
    online.connection.send(reply).await;
    online.seq = 4;
    online.events().await;
    online.answer("PATCH", edited("again")).await;

    online
        .account
        .edit_message(general(), Snowflake::new(TARGET + 2), "again")
        .await
        .unwrap();

    assert_eq!(
        online.bodies().await,
        [
            r#"{"content":"again","allowed_mentions":{"parse":["users","roles","everyone"],"replied_user":false}}"#
        ]
    );
}

// D162: the editor opens again with the typed text and says why.
#[tokio::test]
async fn automod_refuses_an_edit_with_its_reason_and_the_text_is_kept() {
    let mut online = editing().await;
    online
        .answer(
            "PATCH",
            refusal(400, 200_000, "Blocked by the server's rules"),
        )
        .await;

    let outcome = online.edit_to("bad word").await;

    match outcome {
        Err(EditError::MessageBlocked { reason }) => {
            assert_eq!(reason, "Blocked by the server's rules");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(online.text(), "hi", "Discord's text back");
    assert_eq!(
        online.edit(),
        "failed \"bad word\": AutoMod refused the message"
    );
    assert!(
        online
            .events()
            .await
            .contains(&format!("MessageUpdated({TARGET})"))
    );

    online.account.dismiss_edit(general(), target());
    assert_eq!(online.edit(), "none");
    assert_eq!(online.events().await, [format!("MessageUpdated({TARGET})")]);
}

#[tokio::test]
async fn each_refusal_is_told_apart() {
    type Expected = fn(&EditError) -> bool;
    let too_long = |text: &str| {
        ResponseTemplate::new(400).set_body_json(json!({
            "message": "Invalid Form Body", "code": 50035,
            "errors": {"content": {"_errors": [{"code": "BASE_TYPE_MAX_LENGTH", "message": text}]}},
        }))
    };
    let cases: [(ResponseTemplate, Expected); 10] = [
        (
            refusal(400, 200_001, "No"),
            |err| matches!(err, EditError::MessageBlocked { reason } if reason == "No"),
        ),
        (refusal(400, 200_002, "No"), |err| {
            matches!(err, EditError::MessageBlocked { .. })
        }),
        (refusal(403, 50005, "Not yours"), |err| {
            matches!(err, EditError::Forbidden)
        }),
        (refusal(403, 50013, "Missing Permissions"), |err| {
            matches!(err, EditError::Forbidden)
        }),
        (refusal(403, 40333, "Something else"), |err| {
            matches!(err, EditError::Forbidden)
        }),
        (refusal(404, 10008, "Unknown Message"), |err| {
            matches!(err, EditError::Deleted)
        }),
        (
            ResponseTemplate::new(429).set_body_json(json!({
                "message": "This message cannot be edited due to announcement rate limits.",
                "code": 20022, "retry_after": 1200.0, "global": false,
            })),
            |err| matches!(err, EditError::TooManyEdits { retry_after: Some(wait) } if wait.as_secs() == 1200),
        ),
        (too_long("Must be 4000 or fewer in length."), |err| {
            matches!(
                err,
                EditError::Request(RequestError::TooLong { limit: 4000 })
            )
        }),
        // Without a number in Discord's message, the account's own limit.
        (too_long("Too long."), |err| {
            matches!(
                err,
                EditError::Request(RequestError::TooLong { limit: 2000 })
            )
        }),
        (refusal(500, 0, "Oops"), |err| {
            matches!(
                err,
                EditError::Request(RequestError::ServerError { status: 500 })
            )
        }),
    ];
    for (index, (answer, expected)) in cases.into_iter().enumerate() {
        let online = editing().await;
        online.answer("PATCH", answer).await;

        let outcome = online.edit_to("new").await;

        let err = outcome.expect_err("refused");
        assert!(expected(&err), "case {index}: {err:?}");
        assert_eq!(online.text(), "hi", "case {index}");
        assert!(online.edit().starts_with("failed"), "case {index}");
        assert_eq!(online.lines().await.len(), 1, "case {index}: asked once");
    }
}

#[tokio::test]
async fn the_latest_save_wins_and_the_earlier_caller_is_answered_at_once() {
    let online = editing().await;
    let (arrived, release) = online.hold("PATCH", edited("first")).await;
    online.answer("PATCH", edited("second")).await;

    let first = online.edit_to("first");
    let (first, second) = tokio::join!(first, async {
        arrival(&arrived).await;
        let second = online.edit_to("second");
        tokio::pin!(second);
        assert!(
            timeout(Duration::from_millis(50), &mut second)
                .await
                .is_err(),
            "the second waits for its request"
        );
        assert_eq!(online.text(), "second");
        release.send(()).unwrap();
        second.await
    });

    first.unwrap();
    second.unwrap();
    assert_eq!(
        online.bodies().await,
        [r#"{"content":"first"}"#, r#"{"content":"second"}"#]
    );
    assert_eq!(online.text(), "second");
    assert_eq!(online.edit(), "none");
}

// A save made while the store takes Discord's answer to the one before is asked for too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_save_while_the_answer_is_taken_is_still_asked() {
    let online = editing().await;
    let (arrived, release) = online.hold("PATCH", edited("first")).await;
    online.answer("PATCH", edited("second")).await;

    let (first, second) = tokio::join!(online.edit_to("first"), async {
        arrival(&arrived).await;
        online
            .account
            .store()
            .delay_next_write(Duration::from_millis(600));
        release.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        online.edit_to("second").await
    });

    first.unwrap();
    second.unwrap();
    assert_eq!(
        online.bodies().await,
        [r#"{"content":"first"}"#, r#"{"content":"second"}"#]
    );
    assert_eq!(online.edit(), "none");
    assert_eq!(online.text(), "second");
}

#[tokio::test]
async fn a_refused_save_that_a_newer_one_replaced_never_reaches_its_caller() {
    let online = editing().await;
    let (arrived, release) = online.hold("PATCH", refusal(400, 200_000, "Blocked")).await;
    online.answer("PATCH", edited("fine")).await;

    let (first, second) = tokio::join!(online.edit_to("blocked"), async {
        arrival(&arrived).await;
        let second = online.edit_to("fine");
        tokio::pin!(second);
        let _ = timeout(Duration::from_millis(20), &mut second).await;
        release.send(()).unwrap();
        second.await
    });

    assert!(first.is_ok(), "{first:?}");
    assert!(second.is_ok(), "{second:?}");
    assert_eq!(online.text(), "fine");
    assert_eq!(online.edit(), "none");
}

#[tokio::test]
async fn an_unchanged_blank_too_long_or_foreign_edit_asks_nothing() {
    let mut online = editing().await;
    online.connection.send(message(3, TARGET + 1)).await;
    online.seq = 3;
    online.events().await;
    online.answer("PATCH", edited("x")).await;

    let unchanged = online.edit_to("hi").await;
    let blank = online.edit_to("  \n").await;
    let too_long = online.edit_to(&"a".repeat(2001)).await;
    let foreign = online
        .account
        .edit_message(general(), Snowflake::new(TARGET + 1), "mine now")
        .await;

    assert!(unchanged.is_ok(), "{unchanged:?}");
    assert!(invalid(&blank), "{blank:?}");
    assert!(
        matches!(
            too_long,
            Err(EditError::Request(RequestError::TooLong { limit: 2000 }))
        ),
        "{too_long:?}"
    );
    assert!(invalid(&foreign), "{foreign:?}");
    assert_eq!(online.lines().await, [] as [&str; 0]);
    assert_eq!(online.text(), "hi");
    assert_eq!(online.edit(), "none");
}

#[tokio::test]
async fn the_gateways_echo_before_the_answer_ends_the_pending_look() {
    let mut online = editing().await;
    let (arrived, release) = online.hold("PATCH", edited("new")).await;

    let Online {
        account,
        subscription,
        connection,
        ..
    } = &mut online;
    let store = account.store().clone();
    let edited = account.edit_message(general(), target(), "new");
    let (outcome, ()) = tokio::join!(edited, async {
        arrival(&arrived).await;
        let mut echo = mine(0, TARGET)["d"].clone();
        echo["content"] = "new".into();
        echo["edited_timestamp"] = EDITED_AT.into();
        connection
            .send(json!({"op": 0, "s": 3, "t": "MESSAGE_UPDATE", "d": echo}))
            .await;
        sync(connection, subscription, 4).await;
        assert!(
            store.message_edit(general(), target()).is_none(),
            "before the answer"
        );
        release.send(()).unwrap();
    });

    outcome.unwrap();
    assert_eq!(online.text(), "new");
}

#[tokio::test]
async fn a_starter_rows_edit_goes_to_the_message_it_shows() {
    let mut online = editing().await;
    online.answer("PATCH", edited("new")).await;
    online.account.view_channel(Snowflake::new(THREAD));
    let mut row = mine(0, STARTER_ROW)["d"].clone();
    row["channel_id"] = THREAD.to_string().into();
    row["type"] = 21.into();
    row["content"] = "".into();
    row["message_reference"] = json!({
        "message_id": TARGET.to_string(), "channel_id": GENERAL.to_string(),
        "guild_id": "200000000000000001",
    });
    row["referenced_message"] = mine(0, TARGET)["d"].clone();
    online.dispatch("MESSAGE_CREATE", row).await;
    online.events().await;

    online
        .account
        .edit_message(Snowflake::new(THREAD), Snowflake::new(STARTER_ROW), "new")
        .await
        .unwrap();

    assert_eq!(online.lines().await, [format!("PATCH {PATH}")]);
    assert_eq!(online.text(), "new");
    match online
        .account
        .store()
        .starter_message(Snowflake::new(THREAD))
    {
        Some(crate::state::StarterMessage::Loaded(message)) => assert_eq!(&*message.content, "new"),
        other => panic!("{other:?}"),
    }
}

// The viewed thread archived, with the user's message in it.
async fn archived_thread(online: &mut Online) {
    online.account.view_channel(Snowflake::new(THREAD));
    let mut sent = mine(0, TARGET + 1)["d"].clone();
    sent["channel_id"] = THREAD.to_string().into();
    online.dispatch("MESSAGE_CREATE", sent).await;
    let mut archived: Value = serde_json::from_str(READY).unwrap();
    let mut archived = archived["d"]["guilds"][0]["threads"][0].take();
    archived["thread_metadata"]["archived"] = true.into();
    online.dispatch("THREAD_UPDATE", archived).await;
    online.events().await;
}

#[tokio::test]
async fn an_archived_thread_is_opened_before_an_edit_or_a_delete() {
    let mut online = editing().await;
    Mock::given(method("PATCH"))
        .and(path_regex(format!("/channels/{THREAD}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&online.server)
        .await;
    online.answer("PATCH", edited("new")).await;
    online.answer("DELETE", ResponseTemplate::new(204)).await;
    archived_thread(&mut online).await;
    let (thread, id) = (Snowflake::new(THREAD), Snowflake::new(TARGET + 1));

    online
        .account
        .edit_message(thread, id, "new")
        .await
        .unwrap();
    online.account.delete_message(thread, id).await.unwrap();

    let at = format!("/api/v9/channels/{THREAD}/messages/{}", TARGET + 1);
    assert_eq!(
        online.lines().await,
        [
            format!("PATCH /api/v9/channels/{THREAD}"),
            format!("PATCH {at}"),
            format!("PATCH /api/v9/channels/{THREAD}"),
            format!("DELETE {at}"),
        ]
    );
}

#[tokio::test]
async fn a_delete_dims_the_row_until_discord_answers_then_takes_it_away() {
    let mut online = editing().await;
    let (arrived, release) = online.hold("DELETE", ResponseTemplate::new(204)).await;

    let (outcome, ()) = tokio::join!(online.delete(), async {
        arrival(&arrived).await;
        assert!(online.deleting(), "before the answer");
        assert_eq!(online.text(), "hi");
        let actions = online.account.store().message_actions(general(), target());
        assert!(!actions.delete && !actions.edit);
        release.send(()).unwrap();
    });

    outcome.unwrap();
    assert_eq!(online.lines().await, [format!("DELETE {PATH}")]);
    assert!(online.requests().await[0].body.is_empty());
    assert_eq!(online.text(), "(gone)");
    assert!(!online.deleting());
    let events = online.events().await;
    assert_eq!(
        events,
        [
            format!("MessageUpdated({TARGET})"),
            format!("MessageDeleted({TARGET})")
        ]
    );
    online
        .dispatch(
            "MESSAGE_DELETE",
            json!({"id": TARGET.to_string(), "channel_id": GENERAL.to_string()}),
        )
        .await;
    assert_eq!(online.events().await, [] as [&str; 0], "the echo");
}

#[tokio::test]
async fn a_refused_delete_leaves_the_message_and_says_why() {
    let mut online = editing().await;
    online
        .answer("DELETE", refusal(403, 50013, "Missing Permissions"))
        .await;

    let outcome = online.delete().await;

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
    assert!(!online.deleting());
    assert_eq!(online.text(), "hi");
    assert_eq!(
        online.events().await,
        [
            format!("MessageUpdated({TARGET})"),
            format!("MessageUpdated({TARGET})")
        ],
        "dimmed, then back"
    );
}

#[tokio::test]
async fn deleting_a_message_discord_doesnt_know_takes_it_away() {
    let online = editing().await;
    online
        .answer("DELETE", refusal(404, 10008, "Unknown Message"))
        .await;

    online.delete().await.unwrap();

    assert_eq!(online.text(), "(gone)");
}

#[tokio::test]
async fn a_failed_send_is_deleted_without_a_request() {
    let mut online = editing().await;
    Mock::given(method("POST"))
        .respond_with(refusal(403, 50013, "Missing Permissions"))
        .mount(&online.server)
        .await;
    let sent = online
        .account
        .send_message(general(), "lost".to_owned(), None)
        .await;
    assert!(sent.is_err());
    let failed = online.account.store().messages(general()).unwrap().pending[0].id;
    online.events().await;

    online
        .account
        .delete_message(general(), failed)
        .await
        .unwrap();

    assert_eq!(online.lines().await, [] as [&str; 0]);
    assert!(
        online
            .account
            .store()
            .messages(general())
            .unwrap()
            .pending
            .is_empty()
    );
    assert_eq!(
        online.events().await,
        [format!("MessageDeleted({})", failed.get())]
    );
}

#[tokio::test]
async fn a_delete_where_it_isnt_allowed_asks_nothing() {
    let mut online = editing().await;
    let mut renamed = mine(0, TARGET + 1)["d"].clone();
    renamed["type"] = 4.into();
    online.dispatch("MESSAGE_CREATE", renamed).await;
    online.events().await;

    let outcome = online
        .account
        .delete_message(general(), Snowflake::new(TARGET + 1))
        .await;

    assert!(
        matches!(outcome, Err(RequestError::InvalidRequest)),
        "{outcome:?}"
    );
    assert_eq!(online.lines().await, [] as [&str; 0]);
}

#[tokio::test]
async fn a_delete_given_up_before_the_answer_leaves_the_row_as_it_was() {
    let online = editing().await;
    let (arrived, release) = online.hold("DELETE", ResponseTemplate::new(204)).await;

    let given_up = timeout(Duration::from_millis(300), async {
        let delete = online.delete();
        tokio::pin!(delete);
        tokio::select! {
            _ = &mut delete => panic!("answered while held"),
            () = arrival(&arrived) => {}
        }
    })
    .await;

    given_up.expect("the request arrived");
    assert!(!online.deleting());
    assert_eq!(online.text(), "hi");
    release.send(()).unwrap();
}

// Whichever answer comes first, the edit's caller hears Discord's word and nothing is kept.
#[tokio::test]
async fn a_delete_while_an_edit_runs_answers_the_edit_with_discords_word() {
    let online = editing().await;
    online
        .answer(
            "PATCH",
            refusal(404, 10008, "Unknown Message").set_delay(Duration::from_millis(300)),
        )
        .await;
    online.answer("DELETE", ResponseTemplate::new(204)).await;

    let (edit, ()) = tokio::join!(online.edit_to("new"), async {
        timeout(WAIT, async {
            while online.lines().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("the edit was never asked");
        online.delete().await.unwrap();
        assert_eq!(online.text(), "(gone)");
    });

    assert!(matches!(edit, Err(EditError::Deleted)), "{edit:?}");
    assert_eq!(online.edit(), "none", "nothing kept for a deleted message");
}

#[tokio::test]
async fn a_closed_account_refuses_edits_and_deletes() {
    let online = editing().await;
    online.account.close();

    let edit = online.edit_to("new").await;
    let delete = online.delete().await;

    assert!(
        matches!(edit, Err(EditError::Request(RequestError::Closed))),
        "{edit:?}"
    );
    assert!(matches!(delete, Err(RequestError::Closed)), "{delete:?}");
    assert_eq!(online.text(), "hi");
    assert!(!online.deleting());
}
