use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Request as Received, Respond, ResponseTemplate};

use super::preparer::Batches;
use super::*;
use crate::state::PinsState;
use crate::{PIN_LIMIT, PinError};

const TARGET: u64 = 400_000_000_000_000_003;
const THREAD: u64 = 300_000_000_000_000_020;
const STARTER_ROW: u64 = 400_000_000_000_000_031;
const PINS: &str = "/api/v9/channels/300000000000000002/messages/pins";
const ACK: &str = "/api/v9/channels/300000000000000002/pins/ack";
const PINNED_AT: &str = "2026-10-02T09:30:00.250000+00:00";

fn target() -> MessageId {
    Snowflake::new(TARGET)
}

fn refusal(status: u16, code: u32, text: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({"message": text, "code": code}))
}

fn pin(id: u64, pinned_at: &str) -> Value {
    let mut message = message(0, id)["d"].clone();
    message["pinned"] = true.into();
    json!({"pinned_at": pinned_at, "message": message})
}

fn listed(pins: &[Value], more: bool) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"items": pins, "has_more": more}))
}

struct Online {
    account: Account,
    subscription: Subscription,
    server: MockServer,
    connection: FakeConnection,
    seq: u64,
    _fake: FakeGateway,
}

// Online with #general viewed and someone's message in its window.
async fn pinning() -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;
    connection.send(message(2, TARGET)).await;
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
    async fn answer(&self, verb: &str, path: &str, answer: ResponseTemplate) {
        Mock::given(method(verb))
            .and(path_regex(path))
            .respond_with(answer)
            .mount(&self.server)
            .await;
    }

    async fn hold(
        &self,
        verb: &str,
        path: &str,
        answer: ResponseTemplate,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (arrived, on_arrival) = std::sync::mpsc::channel();
        let (release, on_release) = std::sync::mpsc::channel();
        Mock::given(method(verb))
            .and(path_regex(path))
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

    // The events since the last call, once everything sent so far is applied.
    async fn events(&mut self) -> Vec<String> {
        self.seq += 1;
        sync(&mut self.connection, &self.subscription, self.seq).await
    }

    // Method, path and query of every request but the window's loads.
    async fn lines(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() != MESSAGES)
            .map(|request| {
                let query = request
                    .url
                    .query()
                    .map(|query| format!("?{query}"))
                    .unwrap_or_default();
                format!("{} {}{query}", request.method, request.url.path())
            })
            .collect()
    }

    fn listed(&self) -> Vec<u64> {
        self.account
            .store()
            .pins(general())
            .items
            .iter()
            .map(|item| item.message.id.get())
            .collect()
    }

    fn state(&self) -> PinsState {
        self.account.store().pins(general()).state
    }

    fn pinned(&self) -> bool {
        self.account
            .store()
            .message(general(), target())
            .is_some_and(|message| message.pinned)
    }
}

#[tokio::test]
async fn the_first_page_of_pins_lands_in_the_store_and_the_next_follows_it() {
    let mut online = pinning().await;
    online
        .answer(
            "GET",
            "/pins$",
            listed(&[pin(TARGET, PINNED_AT), pin(TARGET - 1, PINNED_AT)], true),
        )
        .await;

    online.account.load_pins(general()).await.unwrap();

    assert_eq!(online.listed(), [TARGET, TARGET - 1]);
    assert_eq!(online.state(), PinsState::Loaded);
    let mut events = online.events().await;
    events.retain(|event| event.starts_with("Pins"));
    assert_eq!(
        events,
        [
            format!("PinsChanged({GENERAL})"),
            format!("PinsChanged({GENERAL})"),
        ]
    );
    online.server.reset().await;
    online
        .answer(
            "GET",
            "/pins$",
            listed(&[pin(TARGET - 2, PINNED_AT)], false),
        )
        .await;

    online.account.load_more_pins(general()).await.unwrap();
    online.account.load_more_pins(general()).await.unwrap();
    online.account.load_pins(general()).await.unwrap();

    assert_eq!(
        online.lines().await,
        [format!(
            "GET {PINS}?limit=25&before=2026-10-02T09%3A30%3A00.250Z"
        )],
        "the last page, then nothing more to ask"
    );
    assert_eq!(online.listed(), [TARGET, TARGET - 1, TARGET - 2]);
    assert!(!online.account.store().pins(general()).has_more);
}

#[tokio::test]
async fn a_failed_first_page_is_told_apart_and_asked_again() {
    let online = pinning().await;
    online
        .answer("GET", "/pins$", ResponseTemplate::new(500))
        .await;

    let failed = online.account.load_pins(general()).await;

    assert!(
        matches!(failed, Err(RequestError::ServerError { status: 500 })),
        "{failed:?}"
    );
    assert_eq!(online.state(), PinsState::Failed);
    assert!(online.listed().is_empty());
    online.server.reset().await;
    online.answer("GET", "/pins$", listed(&[], false)).await;
    online.account.load_pins(general()).await.unwrap();
    assert_eq!(online.state(), PinsState::Loaded);
}

#[tokio::test]
async fn a_load_dropped_before_its_answer_can_be_asked_again() {
    let online = pinning().await;
    let (arrived, release) = online.hold("GET", "/pins$", listed(&[], false)).await;

    let load = online.account.load_pins(general());
    tokio::select! {
        _ = load => panic!("answered before the release"),
        () = arrival(&arrived) => {}
    }
    release.send(()).unwrap();

    assert_eq!(online.state(), PinsState::NotLoaded);
    online.answer("GET", "/pins$", listed(&[], false)).await;
    online.account.load_pins(general()).await.unwrap();
    assert_eq!(online.state(), PinsState::Loaded);
}

#[tokio::test]
async fn the_hosts_preparer_sees_a_page_of_pins_before_the_store_has_it() {
    let online = pinning().await;
    let batches = Batches::register(&online.account);
    online
        .answer(
            "GET",
            "/pins$",
            listed(&[pin(TARGET + 5, PINNED_AT)], false),
        )
        .await;

    online.account.load_pins(general()).await.unwrap();

    let seen = batches.seen();
    assert!(
        seen.iter().any(|batch| batch.ids == [TARGET + 5]),
        "{seen:?}"
    );
}

// D162: nothing changes until the gateway says so.
#[tokio::test]
async fn a_pin_puts_the_message_under_the_pins_and_the_gateway_shows_it() {
    let mut online = pinning().await;
    online
        .answer("PUT", "/pins/", ResponseTemplate::new(204))
        .await;

    online.account.pin(general(), target()).await.unwrap();

    assert_eq!(online.lines().await, [format!("PUT {PINS}/{TARGET}")]);
    assert!(!online.pinned(), "not before the gateway");
    let mut echo = message(0, TARGET)["d"].clone();
    echo["pinned"] = true.into();
    online.dispatch("MESSAGE_UPDATE", echo).await;
    online.events().await;
    assert!(online.pinned());
    let actions = online.account.store().message_actions(general(), target());
    assert!(actions.unpin && !actions.pin);
}

#[tokio::test]
async fn an_unpin_from_the_list_reaches_a_message_outside_the_window() {
    let online = pinning().await;
    let outside = TARGET + 50;
    online
        .answer("GET", "/pins$", listed(&[pin(outside, PINNED_AT)], false))
        .await;
    online
        .answer("DELETE", "/pins/", ResponseTemplate::new(204))
        .await;
    online.account.load_pins(general()).await.unwrap();

    online
        .account
        .unpin(general(), Snowflake::new(outside))
        .await
        .unwrap();

    assert!(
        online
            .lines()
            .await
            .contains(&format!("DELETE {PINS}/{outside}"))
    );
}

#[tokio::test]
async fn a_pin_the_message_doesnt_allow_is_refused_before_asking() {
    let online = pinning().await;

    let unknown = online.account.pin(general(), Snowflake::new(77)).await;
    let not_pinned = online.account.unpin(general(), target()).await;

    for refused in [unknown, not_pinned] {
        assert!(
            matches!(
                refused,
                Err(PinError::Request(RequestError::InvalidRequest))
            ),
            "{refused:?}"
        );
    }
    assert!(online.lines().await.is_empty());
}

#[tokio::test]
async fn each_refusal_of_a_pin_is_told_apart() {
    type Expected = fn(&PinError) -> bool;
    let cases: [(ResponseTemplate, Expected); 9] = [
        (
            refusal(400, 30003, "Maximum number of pins reached (250)"),
            |err| matches!(err, PinError::TooMany { limit: PIN_LIMIT }),
        ),
        (refusal(400, 50083, "Thread is archived"), |err| {
            matches!(err, PinError::Archived)
        }),
        (
            refusal(400, 50021, "Cannot execute action on a system message"),
            |err| matches!(err, PinError::SystemMessage),
        ),
        (
            refusal(
                400,
                50019,
                "A message can only be pinned to the channel it was sent in",
            ),
            |err| matches!(err, PinError::WrongChannel),
        ),
        (refusal(403, 50001, "Missing Access"), |err| {
            matches!(err, PinError::Forbidden)
        }),
        (refusal(403, 50013, "Missing Permissions"), |err| {
            matches!(err, PinError::Forbidden)
        }),
        (refusal(403, 40333, "Something else"), |err| {
            matches!(err, PinError::Forbidden)
        }),
        (refusal(404, 10008, "Unknown Message"), |err| {
            matches!(err, PinError::Gone)
        }),
        (
            refusal(400, 12345, "Something new"),
            |err| matches!(err, PinError::Request(RequestError::Discord { code: 12345, message, .. }) if message == "Something new"),
        ),
    ];
    for (index, (answer, expected)) in cases.into_iter().enumerate() {
        let online = pinning().await;
        online.answer("PUT", "/pins/", answer).await;

        let err = online
            .account
            .pin(general(), target())
            .await
            .expect_err("refused");

        assert!(expected(&err), "case {index}: {err:?}");
        assert_eq!(online.lines().await.len(), 1, "case {index}: asked once");
    }
}

// What the user wanted: the pin went with the message.
#[tokio::test]
async fn unpinning_a_message_that_is_gone_is_done() {
    let online = pinning().await;
    online
        .answer("GET", "/pins$", listed(&[pin(TARGET, PINNED_AT)], false))
        .await;
    online
        .answer("DELETE", "/pins/", refusal(404, 10008, "Unknown Message"))
        .await;
    online.account.load_pins(general()).await.unwrap();

    let unpinned = online.account.unpin(general(), target()).await;

    assert!(unpinned.is_ok(), "{unpinned:?}");
}

#[tokio::test]
async fn an_unpin_refused_otherwise_is_returned() {
    let online = pinning().await;
    online
        .answer("GET", "/pins$", listed(&[pin(TARGET, PINNED_AT)], false))
        .await;
    online
        .answer(
            "DELETE",
            "/pins/",
            refusal(403, 50013, "Missing Permissions"),
        )
        .await;
    online.account.load_pins(general()).await.unwrap();

    let refused = online.account.unpin(general(), target()).await;

    assert!(matches!(refused, Err(PinError::Forbidden)), "{refused:?}");
}

async fn archived_thread(online: &mut Online) {
    online.account.view_channel(Snowflake::new(THREAD));
    let mut sent = message(0, TARGET + 1)["d"].clone();
    sent["channel_id"] = THREAD.to_string().into();
    online.dispatch("MESSAGE_CREATE", sent).await;
    let mut archived: Value = serde_json::from_str(READY).unwrap();
    let mut archived = archived["d"]["guilds"][0]["threads"][0].take();
    archived["thread_metadata"]["archived"] = true.into();
    online.dispatch("THREAD_UPDATE", archived).await;
    online.events().await;
}

#[tokio::test]
async fn an_archived_thread_is_opened_before_a_pin_or_an_unpin() {
    let mut online = pinning().await;
    online
        .answer(
            "PATCH",
            &format!("/channels/{THREAD}$"),
            ResponseTemplate::new(200).set_body_json(json!({})),
        )
        .await;
    online
        .answer("PUT", "/pins/", ResponseTemplate::new(204))
        .await;
    archived_thread(&mut online).await;
    let (thread, id) = (Snowflake::new(THREAD), Snowflake::new(TARGET + 1));

    online.account.pin(thread, id).await.unwrap();

    assert_eq!(
        online.lines().await,
        [
            format!("PATCH /api/v9/channels/{THREAD}"),
            format!("PUT /api/v9/channels/{THREAD}/messages/pins/{}", TARGET + 1),
        ]
    );
}

#[tokio::test]
async fn a_starter_rows_pin_goes_to_the_message_it_shows() {
    let mut online = pinning().await;
    online
        .answer("PUT", "/pins/", ResponseTemplate::new(204))
        .await;
    online.account.view_channel(Snowflake::new(THREAD));
    let mut row = message(0, STARTER_ROW)["d"].clone();
    row["channel_id"] = THREAD.to_string().into();
    row["type"] = 21.into();
    row["content"] = "".into();
    row["message_reference"] = json!({
        "message_id": TARGET.to_string(), "channel_id": GENERAL.to_string(),
        "guild_id": "200000000000000001",
    });
    row["referenced_message"] = message(0, TARGET)["d"].clone();
    online.dispatch("MESSAGE_CREATE", row).await;
    online.events().await;

    online
        .account
        .pin(Snowflake::new(THREAD), Snowflake::new(STARTER_ROW))
        .await
        .unwrap();

    assert_eq!(online.lines().await, [format!("PUT {PINS}/{TARGET}")]);
}

fn pins_update(time: &str) -> Value {
    json!({
        "channel_id": GENERAL.to_string(), "guild_id": "200000000000000001",
        "last_pin_timestamp": time,
    })
}

// The dot shows: #general's mute in the fixture has ended, and its newest pin is unseen.
async fn unread_pins(online: &mut Online) {
    online
        .dispatch("CHANNEL_PINS_UPDATE", pins_update(PINNED_AT))
        .await;
    online.events().await;
    assert!(online.account.store().pins_unread(general()));
}

#[tokio::test]
async fn acking_pins_takes_the_dot_at_once_and_posts_once() {
    let mut online = pinning().await;
    online
        .answer("POST", "/pins/ack$", ResponseTemplate::new(204))
        .await;
    online.account.ack_pins(general());
    unread_pins(&mut online).await;

    online.account.ack_pins(general());
    online.account.ack_pins(general());

    assert!(!online.account.store().pins_unread(general()));
    assert!(
        online
            .events()
            .await
            .contains(&format!("PinsUnreadChanged({GENERAL})"))
    );
    timeout(WAIT, async {
        while online.lines().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("no ack");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(online.lines().await, [format!("POST {ACK}")]);
}

async fn ack_requests(online: &Online, settled: usize) -> Vec<String> {
    timeout(WAIT, async {
        loop {
            let lines = online.lines().await;
            if lines.len() >= settled {
                return lines;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("fewer acks than expected")
}

#[tokio::test]
async fn a_pins_ack_that_failed_is_asked_again_up_to_three_times() {
    let mut online = pinning().await;
    Mock::given(method("POST"))
        .and(path_regex("/pins/ack$"))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&online.server)
        .await;
    online
        .answer("POST", "/pins/ack$", ResponseTemplate::new(204))
        .await;
    unread_pins(&mut online).await;

    online.account.ack_pins(general());

    assert_eq!(ack_requests(&online, 2).await.len(), 2);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(online.lines().await.len(), 2, "done after the second");

    let mut failing = pinning().await;
    failing
        .answer("POST", "/pins/ack$", ResponseTemplate::new(503))
        .await;
    unread_pins(&mut failing).await;
    failing.account.ack_pins(general());
    assert_eq!(ack_requests(&failing, 3).await.len(), 3);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(failing.lines().await.len(), 3, "never a fourth");
    assert!(
        !failing.account.store().pins_unread(general()),
        "no rollback, as the official client"
    );
}

#[tokio::test]
async fn a_refused_pins_ack_is_not_asked_again() {
    let mut online = pinning().await;
    online
        .answer("POST", "/pins/ack$", refusal(403, 50001, "Missing Access"))
        .await;
    unread_pins(&mut online).await;

    online.account.ack_pins(general());

    assert_eq!(ack_requests(&online, 1).await.len(), 1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(online.lines().await.len(), 1);
}

#[tokio::test]
async fn a_rate_limited_pins_ack_is_asked_again_after_its_wait() {
    let mut online = pinning().await;
    Mock::given(method("POST"))
        .and(path_regex("/pins/ack$"))
        .respond_with(ResponseTemplate::new(429).set_body_json(
            json!({"message": "You are being rate limited.", "retry_after": 0.05, "global": false}),
        ))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&online.server)
        .await;
    online
        .answer("POST", "/pins/ack$", ResponseTemplate::new(204))
        .await;
    unread_pins(&mut online).await;

    online.account.ack_pins(general());

    assert_eq!(ack_requests(&online, 2).await.len(), 2);
}

#[tokio::test]
async fn a_pin_answered_401_ends_the_session() {
    let online = pinning().await;
    online
        .answer("PUT", "/pins/", ResponseTemplate::new(401))
        .await;

    let failed = online.account.pin(general(), target()).await;

    assert!(
        matches!(failed, Err(PinError::Request(RequestError::Unauthorized))),
        "{failed:?}"
    );
    assert!(matches!(
        online.account.store().connection(),
        ConnectionState::Closed { .. }
    ));
}

#[tokio::test]
async fn a_pins_page_answered_401_ends_the_session() {
    let online = pinning().await;
    online
        .answer("GET", "/pins$", ResponseTemplate::new(401))
        .await;

    let failed = online.account.load_pins(general()).await;

    assert!(
        matches!(failed, Err(RequestError::Unauthorized)),
        "{failed:?}"
    );
    assert!(matches!(
        online.account.store().connection(),
        ConnectionState::Closed { .. }
    ));
}
