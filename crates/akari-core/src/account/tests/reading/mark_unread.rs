use wiremock::matchers::query_param;

use super::*;

const FIRST: u64 = NEWEST - 2;
// READY's group DM, without a read state.
const DM: u64 = 300_000_000_000_000_011;
const DM_NEWEST: u64 = 400_000_000_000_000_021;

fn msg(message: u64) -> MessageId {
    Snowflake::new(message)
}

// The timestamp-only ID a millisecond before `message`.
fn millisecond_before(message: u64) -> u64 {
    MessageId::from_unix_millis(msg(message).unix_millis() - 1, 0).get()
}

fn plain(messages: &mut Value, ids: &[u64]) {
    for message in messages.as_array_mut().unwrap() {
        let id: u64 = message["id"].as_str().unwrap().parse().unwrap();
        if ids.contains(&id) {
            message["mentions"] = json!([]);
        }
    }
}

// A channel's list at the present; the messages of `plain_ids` mention nobody.
async fn showing_in(online: &Online, channel: u64, ids: &[u64], plain_ids: &[u64]) {
    let mut messages = page(ids);
    plain(&mut messages, plain_ids);
    for message in messages.as_array_mut().unwrap() {
        message["channel_id"] = channel.to_string().into();
    }
    Mock::given(method("GET"))
        .and(path(format!("/api/v9/channels/{channel}/messages")))
        .respond_with(ResponseTemplate::new(200).set_body_json(messages))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&online.server)
        .await;
    online
        .account
        .load_messages(id(channel), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();
}

// GENERAL read, with three messages shown that all mention the user.
async fn read_general() -> Online {
    let online = acking(general_read).await;
    showing(&online, &[FIRST, FIRST + 1, NEWEST]).await;
    online
}

async fn bodies(online: &Online) -> Vec<Value> {
    posted(&online.server).await.iter().map(body).collect()
}

// Answers once the test lets it.
struct Held {
    arrived: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    status: u16,
}

impl Respond for Held {
    fn respond(&self, _: &Received) -> ResponseTemplate {
        let _ = self.arrived.send(());
        let _ = self.release.lock().unwrap().recv_timeout(WAIT);
        ResponseTemplate::new(self.status).set_body_json(json!({"token": null}))
    }
}

// Holds the next ack: when it arrived, and the release.
async fn hold_next_ack(
    server: &MockServer,
    status: u16,
) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
    let (arrived, on_arrival) = std::sync::mpsc::channel();
    let (release, on_release) = std::sync::mpsc::channel();
    answer_acks_once(
        server,
        Held {
            arrived,
            release: std::sync::Mutex::new(on_release),
            status,
        },
    )
    .await;
    (on_arrival, release)
}

async fn answer_acks_once(server: &MockServer, answer: impl Respond + 'static) {
    Mock::given(method("POST"))
        .and(path_regex(ACK_ROUTE))
        .respond_with(answer)
        .up_to_n_times(1)
        .with_priority(1)
        .mount(server)
        .await;
}

fn missing_access() -> ResponseTemplate {
    ResponseTemplate::new(403).set_body_json(json!({"message": "Missing Access", "code": 50001}))
}

fn manual_ack(seq: u64, channel: u64, message: u64, mentions: u32) -> Value {
    dispatch(
        seq,
        "MESSAGE_ACK",
        json!({"channel_id": channel.to_string(), "message_id": message.to_string(),
               "manual": true, "mention_count": mentions, "version": 6}),
    )
}

#[tokio::test]
async fn marking_unread_acks_the_message_before_with_the_mentions_after_it() {
    let online = read_general().await;

    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Ack(GENERAL, NEWEST - 1)]
    );
    assert_eq!(
        bodies(&online).await,
        [json!({"manual": true, "mention_count": 1})],
        "nothing but the client's two fields"
    );
    assert_eq!(read_state(&online.account, GENERAL), Some((NEWEST - 1, 1)));
}

#[tokio::test]
async fn the_first_loaded_message_acks_the_millisecond_before_it() {
    let online = read_general().await;

    online
        .account
        .mark_unread(general(), msg(FIRST))
        .await
        .unwrap();

    let before = millisecond_before(FIRST);
    assert!(before < FIRST);
    assert_eq!(before & 0x3F_FFFF, 0, "a timestamp alone");
    assert_eq!(sent(&online.server, 1).await, [Sent::Ack(GENERAL, before)]);
    assert_eq!(bodies(&online).await[0]["mention_count"], 3);
    assert_eq!(read_state(&online.account, GENERAL), Some((before, 3)));
}

#[tokio::test]
async fn only_loaded_messages_that_mention_the_user_count() {
    let online = acking(general_read).await;
    showing_in(&online, GENERAL, &[FIRST, FIRST + 1, NEWEST], &[FIRST + 1]).await;

    online
        .account
        .mark_unread(general(), msg(FIRST + 1))
        .await
        .unwrap();

    assert_eq!(sent(&online.server, 1).await, [Sent::Ack(GENERAL, FIRST)]);
    assert_eq!(bodies(&online).await[0]["mention_count"], 1);
}

// Before Discord answers.
#[tokio::test]
async fn the_badges_change_at_once() {
    let mut online = read_general().await;
    let (on_arrival, release) = hold_next_ack(&online.server, 200).await;
    let account = &online.account;
    let marking = account.mark_unread(general(), msg(NEWEST));
    tokio::pin!(marking);

    let waiting = tokio::select! {
        biased;
        result = &mut marking => panic!("answered before the release: {result:?}"),
        () = wait_for(on_arrival, "the ack never arrived") => true,
    };
    assert!(waiting);
    let events = sync(&mut online.connection, &online.subscription, 2).await;

    assert!(
        events.iter().any(|event| event.starts_with("BadgeChanged")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("GuildBadgeChanged")),
        "{events:?}"
    );
    let shown = badge(account, GENERAL);
    assert!(shown.unread);
    assert_eq!(shown.mentions, 1);
    assert_eq!(read_state(account, GENERAL), Some((NEWEST - 1, 1)));
    release.send(()).unwrap();
    marking.await.unwrap();
}

#[tokio::test]
async fn the_echo_changes_nothing_more() {
    let mut online = read_general().await;
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();
    sync(&mut online.connection, &online.subscription, 2).await;

    online
        .connection
        .send(manual_ack(3, GENERAL, NEWEST - 1, 1))
        .await;
    let events = sync(&mut online.connection, &online.subscription, 4).await;

    assert!(
        !events.iter().any(|event| event.contains("BadgeChanged")),
        "{events:?}"
    );
    assert_eq!(read_state(&online.account, GENERAL), Some((NEWEST - 1, 1)));
    assert_eq!(pending(&online.account), 0);
}

#[tokio::test]
async fn a_channel_marked_unread_while_it_is_read_stays_unread() {
    let mut online = read_general().await;
    online.account.set_reading(&[general()]);
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    online.account.set_reading(&[general()]);
    online.connection.send(message(2, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 3).await;

    assert_eq!(handed(&online.account), [], "no ack for reading");
    assert!(badge(&online.account, GENERAL).unread);
    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Ack(GENERAL, NEWEST - 1)]
    );
}

#[tokio::test]
async fn leaving_and_coming_back_reads_it_again() {
    let mut online = read_general().await;
    online.account.set_reading(&[general()]);
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();
    online.connection.send(message(2, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 3).await;

    online.account.set_reading(&[]);
    online.account.set_reading(&[general()]);

    assert_eq!(badge(&online.account, GENERAL), Badge::default());
    assert_eq!(
        sent(&online.server, 2).await,
        [
            Sent::Ack(GENERAL, NEWEST - 1),
            Sent::Ack(GENERAL, NEWEST + 1)
        ]
    );
}

// A reconnect stops the reading on its own; the user didn't leave.
#[tokio::test]
async fn a_new_session_leaves_it_unread() {
    let mut online = read_general().await;
    online.account.set_reading(&[general()]);
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    let mut fresh = new_session(&mut online, |ready| {
        ready["read_state"]["entries"][0]["last_message_id"] = (NEWEST - 1).to_string().into();
        ready["read_state"]["entries"][0]["mention_count"] = 1.into();
    })
    .await;
    online.account.set_reading(&[general()]);
    fresh.send(message(2, NEWEST + 1)).await;
    sync(&mut fresh, &online.subscription, 3).await;

    assert_eq!(handed(&online.account), []);
    assert!(badge(&online.account, GENERAL).unread);
}

#[tokio::test]
async fn a_channel_marked_unread_elsewhere_while_it_is_read_stays_unread() {
    let mut online = read_general().await;
    online.account.set_reading(&[general()]);
    online
        .connection
        .send(manual_ack(2, GENERAL, NEWEST - 1, 1))
        .await;

    online.connection.send(message(3, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 4).await;

    assert_eq!(handed(&online.account), []);
    assert!(badge(&online.account, GENERAL).unread);
}

#[tokio::test]
async fn marking_read_ends_it() {
    let mut online = read_general().await;
    online.account.set_reading(&[general()]);
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    online.account.mark_read(general());
    online.connection.send(message(2, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 3).await;

    assert_eq!(badge(&online.account, GENERAL), Badge::default());
    assert_eq!(
        sent(&online.server, 3).await,
        [
            Sent::Ack(GENERAL, NEWEST - 1),
            Sent::Ack(GENERAL, NEWEST),
            Sent::Ack(GENERAL, NEWEST + 1)
        ]
    );
}

#[tokio::test]
async fn the_users_own_message_ends_it() {
    let mut online = read_general().await;
    online.account.set_reading(&[general()]);
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    let mut own = message(2, NEWEST + 1);
    own["d"]["author"]["id"] = ME.into();
    online.connection.send(own).await;
    online.connection.send(message(3, NEWEST + 2)).await;
    sync(&mut online.connection, &online.subscription, 4).await;

    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST + 2)]);
}

#[tokio::test]
async fn a_refused_mark_goes_back_to_read() {
    let online = read_general().await;
    answer_acks_once(&online.server, missing_access()).await;

    let refused = online.account.mark_unread(general(), msg(NEWEST)).await;

    assert!(
        matches!(
            refused,
            Err(RequestError::Discord {
                status: 403,
                code: 50001,
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(read_state(&online.account, GENERAL), Some((NEWEST, 0)));
    assert_eq!(badge(&online.account, GENERAL), Badge::default());
    assert_eq!(pending(&online.account), 0);
}

#[tokio::test]
async fn a_refused_mark_lets_reading_go_on() {
    let mut online = read_general().await;
    online.account.set_reading(&[general()]);
    answer_acks_once(&online.server, missing_access()).await;
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap_err();

    online.connection.send(message(2, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 3).await;

    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST + 1)]);
}

#[tokio::test]
async fn a_rate_limited_mark_goes_out_again() {
    let online = read_general().await;
    answer_acks_once(
        &online.server,
        ResponseTemplate::new(429).set_body_json(
            json!({"message": "You are being rate limited.", "retry_after": 0.05, "global": false}),
        ),
    )
    .await;

    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    let requests = sent(&online.server, 2).await;
    assert!(requests.len() >= 2, "{requests:?}");
    assert!(
        requests
            .iter()
            .all(|request| *request == Sent::Ack(GENERAL, NEWEST - 1)),
        "{requests:?}"
    );
    assert!(
        bodies(&online)
            .await
            .iter()
            .all(|body| *body == json!({"manual": true, "mention_count": 1}))
    );
    assert_eq!(read_state(&online.account, GENERAL), Some((NEWEST - 1, 1)));
}

#[tokio::test]
async fn a_mark_answered_with_502_goes_out_again() {
    let online = read_general().await;
    answer_acks_once(&online.server, ResponseTemplate::new(502)).await;

    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    let requests = sent(&online.server, 2).await;
    assert!(requests.len() >= 2, "{requests:?}");
    assert_eq!(requests.last(), Some(&Sent::Ack(GENERAL, NEWEST - 1)));
    assert_eq!(read_state(&online.account, GENERAL), Some((NEWEST - 1, 1)));
}

// The ordinary ack is on its way when the user marks the channel unread: the mark goes after
// it, and neither its answer nor its echo brings the channel back to read.
#[tokio::test]
async fn a_mark_waits_for_the_ack_on_its_way_and_wins() {
    let mut online = acking(|_| {}).await;
    showing(&online, &[FIRST, FIRST + 1, NEWEST]).await;
    let (on_arrival, release) = hold_next_ack(&online.server, 200).await;
    online.account.mark_read(general());
    wait_for(on_arrival, "the ack never arrived").await;

    let account = &online.account;
    let marking = account.mark_unread(general(), msg(NEWEST));
    tokio::pin!(marking);
    assert!(
        timeout(Duration::from_millis(50), &mut marking)
            .await
            .is_err(),
        "one request at a time"
    );
    assert_eq!(posted(&online.server).await.len(), 1);
    online
        .connection
        .send(dispatch(
            2,
            "MESSAGE_ACK",
            json!({"channel_id": GENERAL.to_string(), "message_id": NEWEST.to_string(),
                   "mention_count": 0, "version": 5}),
        ))
        .await;
    sync(&mut online.connection, &online.subscription, 3).await;
    assert_eq!(read_state(account, GENERAL), Some((NEWEST - 1, 1)));
    release.send(()).unwrap();
    marking.await.unwrap();

    assert_eq!(
        sent(&online.server, 2).await,
        [Sent::Ack(GENERAL, NEWEST), Sent::Ack(GENERAL, NEWEST - 1)]
    );
    assert_eq!(read_state(account, GENERAL), Some((NEWEST - 1, 1)));
    assert!(badge(account, GENERAL).unread);
    assert_eq!(pending(account), 0);
}

// Escape right after: the later one wins, and Discord gets the read position again.
#[tokio::test]
async fn marking_read_after_the_mark_is_sent_too() {
    let online = read_general().await;
    online
        .account
        .mark_unread(general(), msg(NEWEST))
        .await
        .unwrap();

    online.account.mark_read(general());

    assert_eq!(
        sent(&online.server, 2).await,
        [Sent::Ack(GENERAL, NEWEST - 1), Sent::Ack(GENERAL, NEWEST)]
    );
    assert_eq!(read_state(&online.account, GENERAL), Some((NEWEST, 0)));
}

#[tokio::test]
async fn a_message_the_store_doesnt_have_is_refused_without_a_request() {
    let online = read_general().await;

    let refused = online.account.mark_unread(general(), msg(NEWEST + 9)).await;

    assert!(matches!(refused, Err(RequestError::InvalidRequest)));
    assert!(posted(&online.server).await.is_empty());
    assert_eq!(read_state(&online.account, GENERAL), Some((NEWEST, 0)));
}

#[tokio::test]
async fn a_thread_is_marked_with_the_same_body() {
    let online = acking(|_| {}).await;
    showing_in(&online, THREAD, &[THREAD_NEWEST - 1, THREAD_NEWEST], &[]).await;

    online
        .account
        .mark_unread(id(THREAD), msg(THREAD_NEWEST))
        .await
        .unwrap();

    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Ack(THREAD, THREAD_NEWEST - 1)]
    );
    assert_eq!(
        bodies(&online).await,
        [json!({"manual": true, "mention_count": 1})],
        "no flags, as the client's"
    );
    assert!(badge(&online.account, THREAD).unread);
}

async fn other_requests(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method.as_str() != "GET")
        .map(|request| {
            let query = request
                .url
                .query()
                .map_or(String::new(), |q| format!("?{q}"));
            format!("{} {}{query}", request.method, request.url.path())
        })
        .collect()
}

#[tokio::test]
async fn an_unjoined_thread_is_joined_first() {
    let mut online = acking(|_| {}).await;
    let mut thread: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ))
    .unwrap();
    thread.as_object_mut().unwrap().remove("member");
    online
        .connection
        .send(dispatch(2, "THREAD_CREATE", thread))
        .await;
    sync(&mut online.connection, &online.subscription, 3).await;
    showing_in(&online, UNJOINED, &[NEWEST + 5, NEWEST + 6], &[]).await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v9/channels/{UNJOINED}/thread-members/@me"
        )))
        .and(query_param("location", "Mark Unread"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&online.server)
        .await;

    online
        .account
        .mark_unread(id(UNJOINED), msg(NEWEST + 6))
        .await
        .unwrap();

    assert_eq!(
        other_requests(&online.server).await,
        [
            format!("POST /api/v9/channels/{UNJOINED}/thread-members/@me?location=Mark%20Unread"),
            format!(
                "POST /api/v9/channels/{UNJOINED}/messages/{}/ack",
                NEWEST + 5
            ),
        ]
    );
    let stored = online.account.store().channel(id(UNJOINED)).unwrap();
    assert!(stored.thread.as_deref().unwrap().joined());
}

#[tokio::test]
async fn an_archived_thread_is_opened_first() {
    let online = acking(|ready| {
        ready["guilds"][0]["threads"][0]["thread_metadata"]["archived"] = true.into();
    })
    .await;
    showing_in(&online, THREAD, &[THREAD_NEWEST - 1, THREAD_NEWEST], &[]).await;
    Mock::given(method("PATCH"))
        .and(path(format!("/api/v9/channels/{THREAD}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&online.server)
        .await;

    online
        .account
        .mark_unread(id(THREAD), msg(THREAD_NEWEST))
        .await
        .unwrap();

    assert_eq!(
        other_requests(&online.server).await,
        [
            format!("PATCH /api/v9/channels/{THREAD}"),
            format!(
                "POST /api/v9/channels/{THREAD}/messages/{}/ack",
                THREAD_NEWEST - 1
            ),
        ]
    );
    let requests = online.server.received_requests().await.unwrap();
    let patch = requests
        .iter()
        .find(|request| request.method.as_str() == "PATCH")
        .unwrap();
    assert_eq!(body(patch), json!({"archived": false}));
}

#[tokio::test]
async fn a_thread_that_cant_be_opened_isnt_marked() {
    let online = acking(|ready| {
        ready["guilds"][0]["threads"][0]["thread_metadata"]["archived"] = true.into();
    })
    .await;
    showing_in(&online, THREAD, &[THREAD_NEWEST - 1, THREAD_NEWEST], &[]).await;
    let before = read_state(&online.account, THREAD);
    Mock::given(method("PATCH"))
        .and(path(format!("/api/v9/channels/{THREAD}")))
        .respond_with(missing_access())
        .mount(&online.server)
        .await;

    let refused = online
        .account
        .mark_unread(id(THREAD), msg(THREAD_NEWEST))
        .await;

    assert!(
        matches!(refused, Err(RequestError::Discord { status: 403, .. })),
        "{refused:?}"
    );
    assert!(posted(&online.server).await.is_empty());
    assert_eq!(read_state(&online.account, THREAD), before);
}

// Discord counts every message of a DM; the mark carries the client's count of mentions.
#[tokio::test]
async fn a_dm_is_marked_like_a_channel() {
    let online = acking(|_| {}).await;
    showing_in(&online, DM, &[DM_NEWEST - 1, DM_NEWEST], &[DM_NEWEST]).await;
    assert_eq!(badge(&online.account, DM), Badge::default());

    online
        .account
        .mark_unread(id(DM), msg(DM_NEWEST))
        .await
        .unwrap();

    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Ack(DM, DM_NEWEST - 1)]
    );
    assert_eq!(
        bodies(&online).await,
        [json!({"manual": true, "mention_count": 0})]
    );
    assert_eq!(read_state(&online.account, DM), Some((DM_NEWEST - 1, 0)));
    assert!(badge(&online.account, DM).unread);
}
