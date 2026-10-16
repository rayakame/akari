use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request as Received, ResponseTemplate};

use super::*;
use crate::account::acks::{AckTarget, Handed, Pace};
use crate::state::Badge;

const ACK_ROUTE: &str = r"^/api/v9/channels/\d+/messages/\d+/ack$";
const BULK: &str = "/api/v9/read-states/ack-bulk";
const NEWEST: u64 = 400_000_000_000_000_003;
const THREAD_NEWEST: u64 = 400_000_000_000_000_030;
const FORUM: u64 = 300_000_000_000_000_030;
const QUIET: u64 = 300_000_000_000_000_004;
const UNJOINED: u64 = 300_000_000_000_000_021;
const AFTER_JOIN: u64 = 1_300_000_000_000_000_000;

fn id(channel: u64) -> ChannelId {
    Snowflake::new(channel)
}

fn g1() -> GuildId {
    Snowflake::new(200_000_000_000_000_001)
}

fn at(message: u64) -> AckTarget {
    AckTarget::Message(Snowflake::new(message))
}

fn paced(channel: u64, message: u64) -> Handed {
    Handed::One(id(channel), at(message), Pace::Paced)
}

fn handed(account: &Account) -> Vec<Handed> {
    account.shared.acks.handed()
}

fn badge(account: &Account, channel: u64) -> Badge {
    account.store().channel_badge(id(channel))
}

fn general_read(ready: &mut Value) {
    ready["read_state"]["entries"][0]["last_message_id"] = NEWEST.to_string().into();
    ready["read_state"]["entries"][0]["mention_count"] = 0.into();
}

fn thread_unread(ready: &mut Value) {
    ready["read_state"]["entries"][1]["last_message_id"] = "400000000000000029".into();
}

// A guild channel with a newest message and a read state at `read`.
fn add_channel(ready: &mut Value, channel: u64, kind: u8, last: u64, read: u64) {
    ready["guilds"][0]["channels"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": channel.to_string(), "type": kind, "name": "more", "position": 5,
            "permission_overwrites": [], "parent_id": null, "last_message_id": last.to_string(),
            "flags": 0
        }));
    ready["read_state"]["entries"].as_array_mut().unwrap().push(
        json!({"id": channel.to_string(), "last_message_id": read.to_string(), "mention_count": 0}),
    );
}

fn dispatch(seq: u64, name: &str, data: Value) -> Value {
    json!({"op": 0, "s": seq, "t": name, "d": data})
}

async fn mock_acks(server: &MockServer, status: u16) {
    Mock::given(method("POST"))
        .and(path_regex(ACK_ROUTE))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({"token": null})))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(BULK))
        .respond_with(ResponseTemplate::new(204))
        .mount(server)
        .await;
}

struct Online {
    account: Account,
    subscription: Subscription,
    connection: FakeConnection,
    server: MockServer,
    fake: FakeGateway,
}

// Online after a READY changed by `edit`; acks are answered with `status`.
async fn online_answering(status: u16, edit: impl FnOnce(&mut Value)) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    mock_acks(&server, status).await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    assert_eq!(connection.handshake(60_000).await["op"], 2);
    connection.send(ready_payload(1, &fake, edit)).await;
    events_until(&subscription, "Online").await;
    Online {
        account,
        subscription,
        connection,
        server,
        fake,
    }
}

async fn acking(edit: impl FnOnce(&mut Value)) -> Online {
    online_answering(200, edit).await
}

#[derive(Debug, PartialEq, Eq)]
enum Sent {
    Ack(u64, u64),
    Bulk(Vec<(u64, u64)>),
}

fn sent_as(request: &Received) -> Sent {
    if request.url.path() == BULK {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let entries = body["read_states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                (
                    entry["channel_id"].as_str().unwrap().parse().unwrap(),
                    entry["message_id"].as_str().unwrap().parse().unwrap(),
                )
            })
            .collect();
        return Sent::Bulk(entries);
    }
    let segments: Vec<&str> = request.url.path().split('/').collect();
    Sent::Ack(segments[4].parse().unwrap(), segments[6].parse().unwrap())
}

// The acks the server got, once there are at least `count` (or after WAIT).
async fn sent(server: &MockServer, count: usize) -> Vec<Sent> {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        let acks = posted(server).await;
        if acks.len() >= count || std::time::Instant::now() > deadline {
            return acks.iter().map(sent_as).collect();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// Acks only, without history loads.
async fn posted(server: &MockServer) -> Vec<Received> {
    let mut requests = server.received_requests().await.unwrap();
    requests.retain(|request| request.method.as_str() == "POST");
    requests
}

// The list of GENERAL, loaded at the present.
async fn showing(online: &Online, ids: &[u64]) {
    Mock::given(method("GET"))
        .and(path(MESSAGES))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(ids)))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&online.server)
        .await;
    online
        .account
        .load_messages(general(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();
}

#[tokio::test]
async fn reading_an_unread_channel_acks_it() {
    let online = acking(|_| {}).await;
    showing(&online, &[NEWEST]).await;
    let account = &online.account;

    account.set_reading(Some(general()));

    assert_eq!(badge(account, GENERAL), Badge::default());
    assert_eq!(handed(account), [paced(GENERAL, NEWEST)]);
    assert_eq!(sent(&online.server, 1).await, [Sent::Ack(GENERAL, NEWEST)]);
}

#[tokio::test]
async fn reading_a_read_channel_sends_nothing() {
    let online = acking(general_read).await;
    showing(&online, &[NEWEST]).await;

    online.account.set_reading(Some(general()));

    assert_eq!(handed(&online.account), []);
    assert!(posted(&online.server).await.is_empty());
}

#[tokio::test]
async fn a_message_arriving_while_reading_is_acked() {
    let mut online = acking(general_read).await;
    showing(&online, &[NEWEST]).await;
    online.account.set_reading(Some(general()));

    online.connection.send(message(2, NEWEST + 1)).await;
    let events = sync(&mut online.connection, &online.subscription, 3).await;

    assert!(
        !events
            .iter()
            .any(|event| event.starts_with("GuildBadgeChanged")),
        "{events:?}"
    );
    assert_eq!(badge(&online.account, GENERAL), Badge::default());
    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST + 1)]);
    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Ack(GENERAL, NEWEST + 1)]
    );
}

#[tokio::test]
async fn a_message_arriving_after_reading_stopped_isnt() {
    let mut online = acking(general_read).await;
    showing(&online, &[NEWEST]).await;
    online.account.set_reading(Some(general()));
    online.account.set_reading(None);

    online.connection.send(message(2, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 3).await;

    assert_eq!(handed(&online.account), []);
    assert!(badge(&online.account, GENERAL).unread);
}

#[tokio::test]
async fn switching_channels_stops_the_old_one() {
    let mut online = acking(general_read).await;
    showing(&online, &[NEWEST]).await;
    online.account.set_reading(Some(general()));
    online.connection.send(message(2, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 3).await;

    online.account.set_reading(Some(id(THREAD)));
    online.connection.send(message(4, NEWEST + 2)).await;
    sync(&mut online.connection, &online.subscription, 5).await;

    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST + 1)]);
    assert!(badge(&online.account, GENERAL).unread);
    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Ack(GENERAL, NEWEST + 1)]
    );
}

#[tokio::test]
async fn a_history_load_while_reading_is_acked() {
    let online = acking(general_read).await;
    online.account.set_reading(Some(general()));
    mock_page(
        &online.server,
        ("limit", "50"),
        page(&[NEWEST, NEWEST + 1, NEWEST + 2]),
    )
    .await;

    online
        .account
        .load_messages(general(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();

    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST + 2)]);
    assert_eq!(badge(&online.account, GENERAL), Badge::default());
}

#[tokio::test]
async fn mark_read_clears_the_badge_at_once_and_acks() {
    let online = acking(|_| {}).await;
    let account = &online.account;

    account.mark_read(general());

    assert_eq!(badge(account, GENERAL), Badge::default());
    assert_eq!(account.store().guild_badge(g1()), Badge::default());
    assert_eq!(
        handed(account),
        [Handed::One(general(), at(NEWEST), Pace::Explicit)]
    );
    assert_eq!(sent(&online.server, 1).await, [Sent::Ack(GENERAL, NEWEST)]);
}

#[tokio::test]
async fn mark_read_on_a_read_channel_sends_nothing() {
    let online = acking(general_read).await;
    let before = online.account.store().read_state(general());

    online.account.mark_read(general());

    assert_eq!(handed(&online.account), []);
    assert_eq!(online.account.store().read_state(general()), before);
    assert!(posted(&online.server).await.is_empty());
}

#[tokio::test]
async fn mark_guilds_read_sends_one_bulk_ack_of_unread_channels_and_joined_threads() {
    let mut online = acking(|ready| {
        thread_unread(ready);
        add_channel(ready, QUIET, 0, AFTER_JOIN, AFTER_JOIN);
    })
    .await;
    let mut unjoined: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/thread_create.json")).unwrap();
    unjoined.as_object_mut().unwrap().remove("member");
    online
        .connection
        .send(dispatch(2, "THREAD_CREATE", unjoined))
        .await;
    let mut ping = message(3, AFTER_JOIN + 1);
    ping["d"]["channel_id"] = UNJOINED.to_string().into();
    online.connection.send(ping).await;
    sync(&mut online.connection, &online.subscription, 4).await;
    assert!(badge(&online.account, UNJOINED).mentions > 0);

    online.account.mark_guilds_read(&[g1()]);

    assert_eq!(
        handed(&online.account),
        [Handed::Bulk(vec![
            (general(), at(NEWEST)),
            (id(THREAD), at(THREAD_NEWEST))
        ])]
    );
    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Bulk(vec![(GENERAL, NEWEST), (THREAD, THREAD_NEWEST)])]
    );
    assert!(!badge(&online.account, THREAD).unread);
    assert!(badge(&online.account, UNJOINED).mentions > 0);
}

#[tokio::test]
async fn every_viewable_forum_goes_into_the_bulk() {
    let online = acking(|ready| {
        thread_unread(ready);
        add_channel(ready, FORUM, 15, AFTER_JOIN, AFTER_JOIN);
    })
    .await;
    let account = &online.account;
    // An answer first, so the sender knows Discord's time.
    account.mark_read(general());
    sent(&online.server, 1).await;
    let deadline = std::time::Instant::now() + WAIT;
    while account.shared.rest.server_now_millis().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "no answer with a Date"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    account.mark_guilds_read(&[g1()]);

    assert_eq!(
        handed(account).last(),
        Some(&Handed::Bulk(vec![
            (id(THREAD), at(THREAD_NEWEST)),
            (
                id(FORUM),
                AckTarget::Now {
                    floor: Some(Snowflake::new(AFTER_JOIN))
                }
            )
        ]))
    );
    let sent = sent(&online.server, 2).await;
    let Some(Sent::Bulk(entries)) = sent.get(1) else {
        panic!("{sent:?}");
    };
    assert_eq!(entries[0], (THREAD, THREAD_NEWEST));
    let (forum, now) = entries[1];
    assert_eq!(forum, FORUM);
    assert!(now > AFTER_JOIN, "{now}");
    assert_eq!(now & ((1 << 22) - 1), 0, "a timestamp-only snowflake");
}

#[tokio::test]
async fn mark_guilds_read_on_a_read_guild_sends_nothing() {
    let online = acking(|ready| {
        general_read(ready);
        add_channel(ready, FORUM, 15, AFTER_JOIN, AFTER_JOIN);
    })
    .await;

    online.account.mark_guilds_read(&[g1()]);

    assert_eq!(handed(&online.account), []);
    assert!(posted(&online.server).await.is_empty());
}

#[tokio::test]
async fn a_message_ack_from_elsewhere_sends_nothing() {
    let mut online = acking(general_read).await;
    showing(&online, &[NEWEST]).await;
    online.account.set_reading(Some(general()));

    online
        .connection
        .send(dispatch(
            2,
            "MESSAGE_ACK",
            json!({"channel_id": GENERAL.to_string(), "message_id": (NEWEST - 1).to_string(),
                   "manual": true, "mention_count": 1, "version": 5}),
        ))
        .await;
    online
        .connection
        .send(dispatch(
            3,
            "MESSAGE_ACK",
            json!({"channel_id": GENERAL.to_string(), "message_id": NEWEST.to_string(), "version": 6}),
        ))
        .await;
    sync(&mut online.connection, &online.subscription, 4).await;

    assert_eq!(handed(&online.account), []);
}

// Marked as unread on another device after Akari acked it.
#[tokio::test]
async fn a_channel_marked_unread_elsewhere_is_acked_again() {
    let mut online = acking(|_| {}).await;
    online.account.mark_read(general());
    sent(&online.server, 1).await;

    online
        .connection
        .send(dispatch(
            2,
            "MESSAGE_ACK",
            json!({"channel_id": GENERAL.to_string(), "message_id": (NEWEST - 1).to_string(),
                   "manual": true, "mention_count": 0, "version": 5}),
        ))
        .await;
    sync(&mut online.connection, &online.subscription, 3).await;
    assert!(badge(&online.account, GENERAL).unread);
    online.account.mark_read(general());

    assert_eq!(
        sent(&online.server, 2).await,
        [Sent::Ack(GENERAL, NEWEST), Sent::Ack(GENERAL, NEWEST)]
    );
}

// READY's read states are Discord's, e.g. after a mark as unread while Akari was offline.
#[tokio::test]
async fn a_new_ready_lets_a_channel_be_acked_again() {
    let mut online = acking(|_| {}).await;
    online.account.mark_read(general());
    sent(&online.server, 1).await;

    online.connection.send(json!({"op": 9, "d": false})).await;
    let mut fresh = online.fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, &online.fake, |_| {})).await;
    events_until(&online.subscription, "Online").await;
    assert!(badge(&online.account, GENERAL).unread);
    online.account.mark_read(general());

    assert_eq!(
        sent(&online.server, 2).await,
        [Sent::Ack(GENERAL, NEWEST), Sent::Ack(GENERAL, NEWEST)]
    );
}

// The host sets the channel again once it's online and its list is refreshed.
#[tokio::test]
async fn ready_acks_nothing_for_a_channel_read_before_it() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    mock_acks(&server, 200).await;
    mock_page(&server, ("limit", "50"), page(&[NEWEST])).await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    account
        .load_messages(general(), MessageLoad::Latest { limit: 50 })
        .await
        .unwrap();
    account.set_reading(Some(general()));

    let _connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    assert_eq!(handed(&account), []);
    assert!(badge(&account, GENERAL).unread);
    account.set_reading(Some(general()));
    assert_eq!(handed(&account), [paced(GENERAL, NEWEST)]);
}

#[tokio::test]
async fn the_core_stops_reading_when_the_connection_drops() {
    let mut online = acking(general_read).await;
    showing(&online, &[NEWEST]).await;
    mock_page(
        &online.server,
        ("limit", "100"),
        page(&[NEWEST, NEWEST + 1, NEWEST + 2]),
    )
    .await;
    online.account.set_reading(Some(general()));

    let mut fresh = new_session(&mut online, |ready| {
        ready["guilds"][0]["channels"][1]["last_message_id"] = (NEWEST + 2).to_string().into();
    })
    .await;
    events_until(
        &online.subscription,
        &format!("MessagesRefreshed({GENERAL})"),
    )
    .await;
    fresh.send(message(2, NEWEST + 3)).await;
    sync(&mut fresh, &online.subscription, 3).await;

    assert_eq!(handed(&online.account), []);
    assert!(badge(&online.account, GENERAL).unread);
    online.account.set_reading(Some(general()));
    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST + 3)]);
}

#[tokio::test]
async fn disconnecting_stops_reading() {
    let mut online = acking(general_read).await;
    showing(&online, &[NEWEST]).await;
    online.account.set_reading(Some(general()));

    online.account.disconnect();
    assert_eq!(online.connection.client_close_code().await, Some(4000));
    online.account.connect().unwrap();
    let mut resumed = online.fake.accept().await;
    assert_eq!(resumed.handshake(60_000).await["op"], 6);
    resumed.dispatch(2, "RESUMED").await;
    events_until(&online.subscription, "Online").await;
    resumed.send(message(3, NEWEST + 1)).await;
    sync(&mut resumed, &online.subscription, 4).await;

    assert_eq!(handed(&online.account), []);
    online.account.set_reading(Some(general()));
    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST + 1)]);
}

#[tokio::test]
async fn a_401_on_an_ack_closes_the_account() {
    let mut online = online_answering(401, thread_unread).await;

    online.account.mark_read(general());

    assert_eq!(online.connection.client_close_code().await, Some(1000));
    let mut events = Vec::new();
    while let Some(event) = next(&online.subscription).await {
        events.push(event);
    }
    assert_eq!(
        events.last().map(String::as_str),
        Some("Closed(AuthenticationFailed)")
    );
    online.account.mark_read(id(THREAD));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        sent(&online.server, 1).await,
        [Sent::Ack(GENERAL, NEWEST)],
        "nothing after the 401"
    );
}

#[tokio::test]
async fn closing_the_account_stops_the_sender() {
    let online = acking(thread_unread).await;
    showing(&online, &[NEWEST]).await;
    online.account.set_reading(Some(general()));
    assert_eq!(handed(&online.account), [paced(GENERAL, NEWEST)]);

    online.account.close();
    online.account.mark_read(id(THREAD));
    tokio::time::sleep(Duration::from_secs(3)).await;

    assert!(posted(&online.server).await.is_empty());
}

// The first ack waits out a 1.5 s rate limit before it's tried again.
async fn limit_the_first_ack(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path_regex(ACK_ROUTE))
        .respond_with(ResponseTemplate::new(429).set_body_json(
            json!({"message": "You are being rate limited.", "retry_after": 1.5, "global": false}),
        ))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(server)
        .await;
}

async fn new_session(online: &mut Online, edit: impl FnOnce(&mut Value)) -> FakeConnection {
    online.connection.send(json!({"op": 9, "d": false})).await;
    let mut fresh = online.fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, &online.fake, edit)).await;
    events_until(&online.subscription, "Online").await;
    fresh
}

#[tokio::test]
async fn ready_drops_a_pending_ack_whose_position_changed_elsewhere() {
    let mut online = acking(|_| {}).await;
    limit_the_first_ack(&online.server).await;
    online.account.mark_read(general());
    sent(&online.server, 1).await;

    // Marked unread further back on another device meanwhile.
    let _fresh = new_session(&mut online, |ready| {
        ready["read_state"]["entries"][0]["last_message_id"] = "400000000000000001".into();
    })
    .await;
    tokio::time::sleep(Duration::from_millis(2500)).await;

    assert_eq!(sent(&online.server, 1).await, [Sent::Ack(GENERAL, NEWEST)]);
    assert!(badge(&online.account, GENERAL).unread);
}

#[tokio::test]
async fn ready_keeps_a_pending_ack_whose_position_is_unchanged() {
    let mut online = acking(|_| {}).await;
    limit_the_first_ack(&online.server).await;
    online.account.mark_read(general());
    sent(&online.server, 1).await;

    let _fresh = new_session(&mut online, |_| {}).await;

    assert_eq!(
        sent(&online.server, 2).await,
        [Sent::Ack(GENERAL, NEWEST), Sent::Ack(GENERAL, NEWEST)]
    );
}

#[tokio::test]
async fn a_manual_ack_elsewhere_stops_akaris_retry() {
    let mut online = acking(|_| {}).await;
    limit_the_first_ack(&online.server).await;
    online.account.mark_read(general());
    sent(&online.server, 1).await;

    online
        .connection
        .send(dispatch(
            2,
            "MESSAGE_ACK",
            json!({"channel_id": GENERAL.to_string(), "message_id": (NEWEST - 1).to_string(),
                   "manual": true, "mention_count": 1, "version": 5}),
        ))
        .await;
    sync(&mut online.connection, &online.subscription, 3).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;

    assert_eq!(sent(&online.server, 1).await, [Sent::Ack(GENERAL, NEWEST)]);
}

// Escape while a new session's READY is converted: the ack still goes, after the retry it joined.
#[tokio::test]
async fn a_mark_read_during_ready_preparation_is_sent() {
    let mut online = acking(|_| {}).await;
    limit_the_first_ack(&online.server).await;
    online.account.mark_read(general());
    sent(&online.server, 1).await;
    online.connection.send(message(2, NEWEST + 1)).await;
    sync(&mut online.connection, &online.subscription, 3).await;
    assert!(badge(&online.account, GENERAL).unread);
    let (parked, release) = online.account.store().park_next_conversion();

    online.connection.send(json!({"op": 9, "d": false})).await;
    let mut fresh = online.fake.accept().await;
    assert_eq!(fresh.handshake(60_000).await["op"], 2);
    fresh.send(ready_payload(1, &online.fake, |_| {})).await;
    tokio::task::spawn_blocking(move || parked.recv_timeout(WAIT))
        .await
        .unwrap()
        .expect("READY never reached its conversion");
    online.account.mark_read(general());
    release.send(()).unwrap();
    events_until(&online.subscription, "Online").await;

    assert_eq!(badge(&online.account, GENERAL), Badge::default());
    assert_eq!(
        sent(&online.server, 2).await,
        [Sent::Ack(GENERAL, NEWEST), Sent::Ack(GENERAL, NEWEST + 1)]
    );
}
