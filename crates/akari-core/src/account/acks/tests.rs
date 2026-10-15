use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::time::{Instant, timeout};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request as Received, Respond, ResponseTemplate};

use super::*;
use crate::Token;
use crate::gateway::fake::client_with;
use crate::model::Snowflake;

const TOKEN: &str = "ack-test-token.secret";
const ACKS: &str = r"^/api/v9/channels/\d+/messages/\d+/ack$";
const BULK: &str = "/api/v9/read-states/ack-bulk";
const HOUR: i64 = 3_600_000;

fn channel(n: u64) -> ChannelId {
    Snowflake::new(300_000_000_000_000_000 + n)
}

fn message(n: u64) -> MessageId {
    Snowflake::new(400_000_000_000_000_000 + n)
}

fn at(id: MessageId) -> AckTarget {
    AckTarget::Message(id)
}

fn now(floor: Option<MessageId>) -> AckTarget {
    AckTarget::Now { floor }
}

type Taken = Arc<Mutex<Vec<(Instant, Request)>>>;

// Takes what `next` hands out, with when, as the sender's loop would.
fn collect(wants: &Arc<Wants>) -> Taken {
    let taken = Taken::default();
    let (wants, into) = (wants.clone(), taken.clone());
    tokio::spawn(async move {
        while let Some(request) = wants.next().await {
            into.lock().unwrap().push((Instant::now(), request));
        }
    });
    taken
}

fn single(request: &Request) -> (ChannelId, AckTarget) {
    assert!(!request.bulk, "{request:?}");
    match request.entries[..] {
        [entry] => entry,
        _ => panic!("expected one entry: {request:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn an_ack_waits_for_a_two_second_lull() {
    let wants = Arc::new(Wants::default());
    let taken = collect(&wants);
    let started = Instant::now();

    wants.want(channel(1), at(message(1)), Pace::Paced);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    wants.want(channel(1), at(message(2)), Pace::Paced);
    tokio::time::sleep(Duration::from_millis(1900)).await;
    assert!(
        taken.lock().unwrap().is_empty(),
        "nothing within 2 s of the last target"
    );
    tokio::time::sleep(Duration::from_secs(30)).await;

    let taken = taken.lock().unwrap();
    assert_eq!(taken.len(), 1, "{taken:?}");
    let (sent_at, request) = &taken[0];
    assert_eq!(single(request), (channel(1), at(message(2))));
    assert_eq!(request.pace, Pace::Paced);
    assert!(*sent_at >= started + Duration::from_millis(3500));
    assert!(*sent_at < started + Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn a_target_that_adds_nothing_doesnt_restart_the_lull() {
    let wants = Arc::new(Wants::default());
    let taken = collect(&wants);
    let started = Instant::now();

    wants.want(channel(1), at(message(2)), Pace::Paced);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    wants.want(channel(1), at(message(2)), Pace::Paced);
    wants.want(channel(1), at(message(1)), Pace::Paced);
    tokio::time::sleep(Duration::from_secs(30)).await;

    let taken = taken.lock().unwrap();
    assert_eq!(taken.len(), 1, "{taken:?}");
    let (sent_at, request) = &taken[0];
    assert_eq!(single(request), (channel(1), at(message(2))));
    assert!(*sent_at >= started + Duration::from_secs(2));
    assert!(
        *sent_at < started + Duration::from_millis(3500),
        "the lull restarted: {:?}",
        sent_at.duration_since(started)
    );
}

#[tokio::test(start_paused = true)]
async fn a_busy_channel_is_acked_at_least_every_ten_seconds() {
    let wants = Arc::new(Wants::default());
    let taken = collect(&wants);
    let mut wanted = Vec::new();

    for n in 1..=25 {
        wants.want(channel(1), at(message(n)), Pace::Paced);
        wanted.push((Instant::now(), message(n)));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;

    let sent: Vec<(Instant, MessageId)> = taken
        .lock()
        .unwrap()
        .iter()
        .map(|(sent_at, request)| match single(request) {
            (_, AckTarget::Message(id)) => (*sent_at, id),
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(
        sent.windows(2).all(|pair| pair[0].1 < pair[1].1),
        "{sent:?}"
    );
    assert_eq!(sent.last().map(|(_, id)| *id), Some(message(25)));
    for (wanted_at, id) in wanted {
        let acked = sent
            .iter()
            .find(|(_, sent_id)| *sent_id >= id)
            .map(|(sent_at, _)| *sent_at)
            .unwrap();
        assert!(
            acked.duration_since(wanted_at) <= Duration::from_millis(10_050),
            "{id:?} waited {:?}",
            acked.duration_since(wanted_at)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn an_explicit_ack_goes_out_at_once() {
    let wants = Wants::default();
    wants.want(channel(1), at(message(5)), Pace::Paced);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let asked = Instant::now();
    wants.want(channel(1), at(message(3)), Pace::Explicit);
    let request = wants.next().await.unwrap();

    assert!(asked.elapsed() < Duration::from_millis(10));
    assert_eq!(single(&request), (channel(1), at(message(5))));
    assert_eq!(request.pace, Pace::Explicit);
    assert!(
        timeout(Duration::from_secs(60), wants.next())
            .await
            .is_err(),
        "the paced target went with it"
    );
}

#[tokio::test(start_paused = true)]
async fn the_oldest_due_ack_goes_first() {
    let wants = Arc::new(Wants::default());
    let taken = collect(&wants);

    wants.want(channel(1), at(message(1)), Pace::Paced);
    tokio::time::sleep(Duration::from_secs(1)).await;
    wants.want(channel(2), at(message(2)), Pace::Explicit);
    wants.want_bulk(vec![(channel(3), at(message(3)))]);
    tokio::time::sleep(Duration::from_secs(5)).await;

    let channels: Vec<Vec<ChannelId>> = taken
        .lock()
        .unwrap()
        .iter()
        .map(|(_, request)| request.entries.iter().map(|(id, _)| *id).collect())
        .collect();
    assert_eq!(
        channels,
        [vec![channel(2)], vec![channel(3)], vec![channel(1)]]
    );
}

#[tokio::test(start_paused = true)]
async fn a_bulk_takes_over_a_pending_ack_for_its_channel() {
    let wants = Wants::default();
    wants.want(channel(1), at(message(7)), Pace::Paced);
    wants.want(channel(2), at(message(8)), Pace::Paced);

    wants.want_bulk(vec![(channel(1), at(message(5))), (channel(3), now(None))]);
    let request = wants.next().await.unwrap();

    assert!(request.bulk);
    assert_eq!(
        request.entries,
        [(channel(1), at(message(7))), (channel(3), now(None))]
    );
    let left = wants.next().await.unwrap();
    assert_eq!(single(&left), (channel(2), at(message(8))));
}

#[test]
fn merged_targets_keep_the_newest() {
    assert_eq!(at(message(2)).merge(at(message(1))), at(message(2)));
    assert_eq!(at(message(1)).merge(at(message(2))), at(message(2)));
    assert_eq!(at(message(2)).merge(now(None)), now(Some(message(2))));
    assert_eq!(
        now(Some(message(1))).merge(at(message(2))),
        now(Some(message(2)))
    );
    assert_eq!(
        at(message(1)).merge(now(Some(message(2)))),
        now(Some(message(2)))
    );
    assert_eq!(
        now(Some(message(3))).merge(now(None)),
        now(Some(message(3)))
    );
}

#[test]
fn an_ack_at_now_keeps_the_cursor_as_its_floor() {
    let ack = |cursor: Option<MessageId>, now: bool| Ack {
        channel: channel(1),
        cursor,
        now,
    };

    assert_eq!(
        AckTarget::from(ack(Some(message(5)), false)),
        at(message(5))
    );
    assert_eq!(
        AckTarget::from(ack(Some(message(5)), true)),
        now(Some(message(5)))
    );
    assert_eq!(AckTarget::from(ack(None, false)), now(None));
}

fn rest(server: &MockServer) -> Arc<AccountRest> {
    Arc::new(AccountRest::new(
        client_with(
            "ws://127.0.0.1:9/".to_owned(),
            format!("{}/api/v9/", server.uri()),
        ),
        Token::new(TOKEN.to_owned()),
        Duration::from_millis(10),
    ))
}

fn sender(server: &MockServer) -> AckSender {
    AckSender::start(
        &tokio::runtime::Handle::current(),
        rest(server),
        Duration::from_millis(10),
        || {},
    )
}

fn answered() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"token": null}))
}

async fn mock_acks(server: &MockServer, answer: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path_regex(ACKS))
        .respond_with(answer)
        .mount(server)
        .await;
}

async fn mock_ack_of(server: &MockServer, channel: ChannelId, answer: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path_regex(format!(
            r"^/api/v9/channels/{}/messages/\d+/ack$",
            channel.get()
        )))
        .respond_with(answer)
        .mount(server)
        .await;
}

async fn mock_bulk(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(BULK))
        .respond_with(ResponseTemplate::new(204))
        .mount(server)
        .await;
}

// The requests the server got, once there are at least `count` (or after 5 s).
async fn received(server: &MockServer, count: usize) -> Vec<Received> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let requests = server.received_requests().await.unwrap();
        if requests.len() >= count || std::time::Instant::now() > deadline {
            return requests;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

// Channel and message of an ack's path; bulks as (0, entries).
fn acked(request: &Received) -> (u64, u64) {
    let segments: Vec<&str> = request.url.path().split('/').collect();
    match segments[..] {
        [
            "",
            "api",
            "v9",
            "channels",
            channel,
            "messages",
            message,
            "ack",
        ] => (channel.parse().unwrap(), message.parse().unwrap()),
        ["", "api", "v9", "read-states", "ack-bulk"] => {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            (0, body["read_states"].as_array().unwrap().len() as u64)
        }
        _ => panic!("unexpected request {}", request.url),
    }
}

fn acks(requests: &[Received]) -> Vec<(u64, u64)> {
    requests.iter().map(acked).collect()
}

fn ack_of(channel: ChannelId, message: MessageId) -> (u64, u64) {
    (channel.get(), message.get())
}

#[tokio::test]
async fn an_ack_posts_the_target_with_a_null_token_and_last_viewed() {
    let server = MockServer::start().await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);
    let before = last_viewed(wall_millis());

    sender.want(channel(1), at(message(7)), Pace::Explicit);
    let requests = received(&server, 1).await;
    let after = last_viewed(wall_millis());

    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.path(),
        "/api/v9/channels/300000000000000001/messages/400000000000000007/ack"
    );
    let body = String::from_utf8(requests[0].body.clone()).unwrap();
    assert!(
        [before, after]
            .iter()
            .any(|day| body == format!(r#"{{"token":null,"last_viewed":{day}}}"#)),
        "{body}"
    );
}

// Records when each request arrived; answers after `SLOW`.
struct Slow(Arc<Mutex<Vec<std::time::Instant>>>);

const SLOW: Duration = Duration::from_millis(100);

impl Respond for Slow {
    fn respond(&self, request: &Received) -> ResponseTemplate {
        self.0.lock().unwrap().push(std::time::Instant::now());
        let answer = if request.url.path() == BULK {
            ResponseTemplate::new(204)
        } else {
            answered()
        };
        answer.set_delay(SLOW)
    }
}

#[tokio::test]
async fn requests_never_overlap() {
    let server = MockServer::start().await;
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .respond_with(Slow(arrivals.clone()))
        .mount(&server)
        .await;
    let sender = sender(&server);

    sender.want(channel(1), at(message(1)), Pace::Explicit);
    sender.want(channel(2), at(message(2)), Pace::Explicit);
    sender.want_bulk(vec![
        (channel(3), at(message(3))),
        (channel(4), at(message(4))),
    ]);
    let requests = received(&server, 3).await;

    let mut sent = acks(&requests);
    sent.sort_unstable();
    assert_eq!(
        sent,
        [
            (0, 2),
            ack_of(channel(1), message(1)),
            ack_of(channel(2), message(2))
        ]
    );
    let arrivals = arrivals.lock().unwrap().clone();
    for pair in arrivals.windows(2) {
        assert!(pair[1] - pair[0] >= SLOW, "{:?}", pair[1] - pair[0]);
    }
}

fn rate_limited() -> ResponseTemplate {
    ResponseTemplate::new(429)
        .set_body_json(json!({"message": "You are being rate limited.", "global": false}))
}

#[tokio::test]
async fn a_rate_limited_ack_is_dropped() {
    let server = MockServer::start().await;
    mock_ack_of(&server, channel(1), rate_limited()).await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);

    sender.want(channel(1), at(message(1)), Pace::Explicit);
    sender.want(channel(2), at(message(2)), Pace::Explicit);
    let requests = received(&server, 2).await;

    assert_eq!(
        acks(&requests),
        [
            ack_of(channel(1), message(1)),
            ack_of(channel(2), message(2))
        ]
    );
}

#[tokio::test]
async fn a_failed_ack_is_retried_once() {
    let server = MockServer::start().await;
    mock_ack_of(&server, channel(1), ResponseTemplate::new(500)).await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);

    sender.want(channel(1), at(message(1)), Pace::Explicit);
    sender.want(channel(2), at(message(2)), Pace::Explicit);
    let requests = received(&server, 3).await;

    assert_eq!(
        acks(&requests),
        [
            ack_of(channel(1), message(1)),
            ack_of(channel(1), message(1)),
            ack_of(channel(2), message(2))
        ]
    );
}

// Holds the first answer until the test releases it; later requests are answered at once.
struct Gated {
    arrived: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    first: std::sync::atomic::AtomicBool,
}

impl Respond for Gated {
    fn respond(&self, _: &Received) -> ResponseTemplate {
        if self.first.swap(false, std::sync::atomic::Ordering::SeqCst) {
            let _ = self.arrived.send(());
            // Blocks wiremock's server thread, which only this request needs meanwhile.
            let _ = self
                .release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10));
        }
        answered()
    }
}

#[tokio::test]
async fn nothing_is_sent_after_close() {
    let server = MockServer::start().await;
    let (arrived, on_arrival) = std::sync::mpsc::channel();
    let (release, on_release) = std::sync::mpsc::channel();
    Mock::given(method("POST"))
        .respond_with(Gated {
            arrived,
            release: Mutex::new(on_release),
            first: std::sync::atomic::AtomicBool::new(true),
        })
        .mount(&server)
        .await;
    let sender = sender(&server);

    sender.want(channel(1), at(message(1)), Pace::Explicit);
    tokio::task::spawn_blocking(move || on_arrival.recv_timeout(Duration::from_secs(5)))
        .await
        .unwrap()
        .expect("the first ack never arrived");
    sender.want(channel(2), at(message(2)), Pace::Explicit);
    sender.want(channel(3), at(message(3)), Pace::Paced);
    sender.close();
    sender.want(channel(4), at(message(4)), Pace::Explicit);
    release.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(
        acks(&server.received_requests().await.unwrap()),
        [ack_of(channel(1), message(1))]
    );
}

#[tokio::test]
async fn an_ack_never_moves_backwards() {
    let server = MockServer::start().await;
    mock_acks(&server, answered()).await;
    mock_bulk(&server).await;
    let sender = sender(&server);

    sender.want(channel(1), at(message(10)), Pace::Explicit);
    received(&server, 1).await;
    sender.want(channel(1), at(message(10)), Pace::Explicit);
    sender.want(channel(2), at(message(1)), Pace::Explicit);
    received(&server, 2).await;
    sender.want_bulk(vec![(channel(1), at(message(9)))]);
    sender.want(channel(3), at(message(1)), Pace::Explicit);
    received(&server, 3).await;
    sender.want(channel(1), at(message(11)), Pace::Explicit);
    let requests = received(&server, 4).await;

    assert_eq!(
        acks(&requests),
        [
            ack_of(channel(1), message(10)),
            ack_of(channel(2), message(1)),
            ack_of(channel(3), message(1)),
            ack_of(channel(1), message(11))
        ]
    );
}

#[tokio::test]
async fn bulk_acks_go_out_fifty_at_a_time() {
    let server = MockServer::start().await;
    mock_bulk(&server).await;
    let sender = sender(&server);
    let entries: Vec<(ChannelId, AckTarget)> =
        (1..=120).map(|n| (channel(n), at(message(n)))).collect();

    sender.want_bulk(entries.clone());
    let requests = received(&server, 3).await;

    let bodies: Vec<String> = requests
        .iter()
        .map(|request| String::from_utf8(request.body.clone()).unwrap())
        .collect();
    let expected: Vec<String> = entries
        .chunks(50)
        .map(|chunk| {
            let entries: Vec<String> = chunk
                .iter()
                .map(|(channel, target)| {
                    let AckTarget::Message(message) = target else {
                        unreachable!()
                    };
                    format!(
                        r#"{{"channel_id":"{}","message_id":"{}","read_state_type":0}}"#,
                        channel.get(),
                        message.get()
                    )
                })
                .collect();
            format!(r#"{{"read_states":[{}]}}"#, entries.join(","))
        })
        .collect();
    assert_eq!(bodies, expected);
    assert!(requests.iter().all(|request| request.url.path() == BULK));
}

// IMF-fixdate, as in Discord's `Date` header.
fn http_date(unix_millis: i64) -> String {
    let date = time::OffsetDateTime::from_unix_timestamp(unix_millis.div_euclid(1000)).unwrap();
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        &date.weekday().to_string()[..3],
        date.day(),
        &date.month().to_string()[..3],
        date.year(),
        date.hour(),
        date.minute(),
        date.second()
    )
}

// Discord's clock `offset` ms from the Mac's; returns its `Date` and the "now" ID's time.
async fn a_now_ack_with_discord_off_by(offset: i64, floor: Option<MessageId>) -> (i64, i64) {
    let date = (wall_millis() + offset).div_euclid(1000) * 1000;
    let server = MockServer::start().await;
    mock_acks(&server, answered().insert_header("date", http_date(date))).await;
    let sender = sender(&server);

    sender.want(channel(1), at(message(1)), Pace::Explicit);
    sender.want(channel(2), now(floor), Pace::Explicit);
    let requests = received(&server, 2).await;

    let sent = acks(&requests);
    assert_eq!(sent[0], ack_of(channel(1), message(1)));
    assert_eq!(sent[1].0, channel(2).get());
    (date, MessageId::new(sent[1].1).unix_millis())
}

#[tokio::test]
async fn a_now_target_follows_discords_clock_when_the_mac_runs_ahead() {
    let (date, sent) = a_now_ack_with_discord_off_by(-HOUR, None).await;

    assert!((date..date + 5_000).contains(&sent), "{date} {sent}");
    assert!(wall_millis() - sent > HOUR / 2);
}

#[tokio::test]
async fn a_now_target_follows_discords_clock_when_the_mac_runs_behind() {
    let (date, sent) = a_now_ack_with_discord_off_by(HOUR, None).await;

    assert!((date..date + 5_000).contains(&sent), "{date} {sent}");
    assert!(sent - wall_millis() > HOUR / 2);
}

#[tokio::test]
async fn a_now_target_never_goes_below_the_newest_known_message() {
    let newest = MessageId::from_unix_millis(wall_millis(), 5);
    let server = MockServer::start().await;
    let date = http_date(wall_millis() - HOUR);
    mock_acks(&server, answered().insert_header("date", date)).await;
    let sender = sender(&server);

    sender.want(channel(1), at(message(1)), Pace::Explicit);
    sender.want(channel(2), now(Some(newest)), Pace::Explicit);
    let requests = received(&server, 2).await;

    assert_eq!(acks(&requests)[1], ack_of(channel(2), newest));
}

#[tokio::test]
async fn without_a_server_time_a_now_target_sends_its_floor() {
    let server = MockServer::start().await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);

    sender.want(channel(1), now(None), Pace::Explicit);
    sender.want(channel(2), now(Some(message(7))), Pace::Explicit);
    let requests = received(&server, 1).await;

    assert_eq!(acks(&requests), [ack_of(channel(2), message(7))]);
}
