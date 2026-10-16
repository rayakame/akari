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

fn ack(channel: ChannelId, target: AckTarget) -> Ack {
    ack_over(channel, target, None, None)
}

// An ack the store took over `before`, leaving `after`.
fn ack_over(
    channel: ChannelId,
    target: AckTarget,
    before: Option<ReadState>,
    after: Option<ReadState>,
) -> Ack {
    let (cursor, now) = match target {
        AckTarget::Message(id) => (Some(id), false),
        AckTarget::Now { floor } => (floor, true),
    };
    Ack {
        channel,
        cursor,
        now,
        before,
        after,
    }
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

fn single(request: &Request) -> (ChannelId, MessageId) {
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

    wants.want(channel(1), message(1), None, None, Pace::Paced);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    wants.want(channel(1), message(2), None, None, Pace::Paced);
    tokio::time::sleep(Duration::from_millis(1900)).await;
    assert!(
        taken.lock().unwrap().is_empty(),
        "nothing within 2 s of the last target"
    );
    tokio::time::sleep(Duration::from_secs(30)).await;

    let taken = taken.lock().unwrap();
    assert_eq!(taken.len(), 1, "{taken:?}");
    let (sent_at, request) = &taken[0];
    assert_eq!(single(request), (channel(1), message(2)));
    assert_eq!(request.pace, Pace::Paced);
    assert!(*sent_at >= started + Duration::from_millis(3500));
    assert!(*sent_at < started + Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn a_target_that_adds_nothing_doesnt_restart_the_lull() {
    let wants = Arc::new(Wants::default());
    let taken = collect(&wants);
    let started = Instant::now();

    wants.want(channel(1), message(2), None, None, Pace::Paced);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    wants.want(channel(1), message(2), None, None, Pace::Paced);
    wants.want(channel(1), message(1), None, None, Pace::Paced);
    tokio::time::sleep(Duration::from_secs(30)).await;

    let taken = taken.lock().unwrap();
    assert_eq!(taken.len(), 1, "{taken:?}");
    let (sent_at, request) = &taken[0];
    assert_eq!(single(request), (channel(1), message(2)));
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
        wants.want(channel(1), message(n), None, None, Pace::Paced);
        wanted.push((Instant::now(), message(n)));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;

    let sent: Vec<(Instant, MessageId)> = taken
        .lock()
        .unwrap()
        .iter()
        .map(|(sent_at, request)| (*sent_at, single(request).1))
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
    wants.want(channel(1), message(5), None, None, Pace::Paced);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let asked = Instant::now();
    wants.want(channel(1), message(3), None, None, Pace::Explicit);
    let request = wants.next().await.unwrap();

    assert!(asked.elapsed() < Duration::from_millis(10));
    assert_eq!(single(&request), (channel(1), message(5)));
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

    wants.want(channel(1), message(1), None, None, Pace::Paced);
    tokio::time::sleep(Duration::from_secs(1)).await;
    wants.want(channel(2), message(2), None, None, Pace::Explicit);
    wants.want_bulk(vec![(channel(3), message(3), None, None)]);
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
    wants.want(channel(1), message(7), None, None, Pace::Paced);
    wants.want(channel(2), message(8), None, None, Pace::Paced);

    wants.want_bulk(vec![
        (channel(1), message(5), None, None),
        (channel(3), message(9), None, None),
    ]);
    let request = wants.next().await.unwrap();

    assert!(request.bulk);
    assert_eq!(
        request.entries,
        [(channel(1), message(7)), (channel(3), message(9))]
    );
    let left = wants.next().await.unwrap();
    assert_eq!(single(&left), (channel(2), message(8)));
}

#[test]
fn an_ack_at_now_keeps_the_cursor_as_its_floor() {
    let ack = |cursor: Option<MessageId>, now: bool| Ack {
        channel: channel(1),
        cursor,
        now,
        before: None,
        after: None,
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
    sender_retrying_after(server, Duration::from_millis(10))
}

fn sender_retrying_after(server: &MockServer, first_retry: Duration) -> AckSender {
    sender_on(rest(server), first_retry)
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

    sender.want(ack(channel(1), at(message(7))), Pace::Explicit);
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

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    sender.want(ack(channel(2), at(message(2))), Pace::Explicit);
    sender.want_bulk(vec![
        ack(channel(3), at(message(3))),
        ack(channel(4), at(message(4))),
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

// Answers the first `failing` requests with `failure`.
async fn failing_first(server: &MockServer, failing: u64, failure: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path_regex(ACKS))
        .respond_with(failure)
        .up_to_n_times(failing)
        .with_priority(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_rate_limited_ack_is_retried_after_the_acks_due_before_it() {
    let server = MockServer::start().await;
    failing_first(&server, 1, rate_limited()).await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    sender.want(ack(channel(2), at(message(2))), Pace::Explicit);
    let requests = received(&server, 3).await;

    assert_eq!(
        acks(&requests),
        [
            ack_of(channel(1), message(1)),
            ack_of(channel(2), message(2)),
            ack_of(channel(1), message(1))
        ]
    );
}

#[tokio::test]
async fn a_failed_ack_is_retried_until_it_goes_through() {
    let server = MockServer::start().await;
    failing_first(&server, 4, ResponseTemplate::new(503)).await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    let requests = received(&server, 5).await;

    assert_eq!(acks(&requests), [ack_of(channel(1), message(1)); 5]);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 5);
}

#[tokio::test]
async fn a_retry_takes_the_channels_newer_target_with_it() {
    let server = MockServer::start().await;
    failing_first(&server, 1, ResponseTemplate::new(503)).await;
    mock_acks(&server, answered()).await;
    let sender = sender_retrying_after(&server, Duration::from_millis(300));

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    received(&server, 1).await;
    sender.want(ack(channel(1), at(message(2))), Pace::Explicit);
    let requests = received(&server, 2).await;

    assert_eq!(
        acks(&requests),
        [
            ack_of(channel(1), message(1)),
            ack_of(channel(1), message(2))
        ]
    );
}

#[tokio::test]
async fn closing_drops_the_acks_waiting_for_a_retry() {
    let server = MockServer::start().await;
    mock_acks(&server, ResponseTemplate::new(503)).await;
    let sender = sender_retrying_after(&server, Duration::from_millis(300));

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    received(&server, 1).await;
    sender.close();
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// Fails the first `failing` requests; every answer comes after `SLOW`.
struct SlowlyFailing {
    arrivals: Arc<Mutex<Vec<std::time::Instant>>>,
    failing: usize,
}

impl Respond for SlowlyFailing {
    fn respond(&self, request: &Received) -> ResponseTemplate {
        let mut arrivals = self.arrivals.lock().unwrap();
        arrivals.push(std::time::Instant::now());
        let answer = if arrivals.len() <= self.failing {
            ResponseTemplate::new(503)
        } else if request.url.path() == BULK {
            ResponseTemplate::new(204)
        } else {
            answered()
        };
        answer.set_delay(SLOW)
    }
}

#[tokio::test]
async fn retries_never_overlap() {
    let server = MockServer::start().await;
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .respond_with(SlowlyFailing {
            arrivals: arrivals.clone(),
            failing: 3,
        })
        .mount(&server)
        .await;
    let sender = sender(&server);

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    sender.want(ack(channel(2), at(message(2))), Pace::Explicit);
    sender.want_bulk(vec![
        ack(channel(3), at(message(3))),
        ack(channel(4), at(message(4))),
    ]);
    let requests = received(&server, 6).await;

    let mut went_through = acks(&requests[3..]);
    went_through.sort_unstable();
    assert_eq!(
        went_through,
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

#[test]
fn the_wait_before_a_retry_doubles_up_to_a_minute() {
    let first = Duration::from_secs(2);
    let waits: Vec<Duration> = (1..=8).map(|failures| backoff(first, failures)).collect();

    assert!(waits.windows(2).all(|pair| pair[0] <= pair[1]), "{waits:?}");
    assert_eq!(waits[0], first);
    assert_eq!(waits[1], first * 2);
    assert_eq!(backoff(first, 40), RETRY_CAP);
    assert!(RETRY_CAP <= Duration::from_secs(64));
}

#[test]
fn only_rate_limits_and_transient_errors_are_retried() {
    let first = Duration::from_secs(2);
    let limited = |after: Option<u64>| RequestError::RateLimited {
        retry_after: after.map(Duration::from_secs),
    };

    assert_eq!(
        retry_wait(&RequestError::ServerError { status: 503 }, 2, first),
        Some(backoff(first, 2))
    );
    assert_eq!(retry_wait(&limited(None), 1, first), Some(first));
    assert_eq!(
        retry_wait(&limited(Some(30)), 1, first),
        Some(Duration::from_secs(30)),
        "Discord's wait when it's longer"
    );
    for dropped in [
        RequestError::Discord {
            status: 403,
            code: 50001,
            message: String::new(),
        },
        RequestError::UnexpectedResponse,
        RequestError::Unauthorized,
        RequestError::Closed,
    ] {
        assert_eq!(retry_wait(&dropped, 1, first), None, "{dropped:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_retried_target_waits_for_its_backoff_with_a_newer_one() {
    let wants = Wants::default();
    wants.want(channel(1), message(1), None, None, Pace::Explicit);
    wants.next().await.unwrap();
    let retry_at = Instant::now() + Duration::from_secs(2);

    wants.retry(retry_at);
    wants.want(channel(1), message(2), None, None, Pace::Explicit);
    wants.want(channel(2), message(3), None, None, Pace::Explicit);

    assert_eq!(
        single(&wants.next().await.unwrap()),
        (channel(2), message(3))
    );
    let retried = wants.next().await.unwrap();
    assert!(Instant::now() >= retry_at);
    assert_eq!(single(&retried), (channel(1), message(2)));
    assert_eq!(retried.failures, 1);
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

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    tokio::task::spawn_blocking(move || on_arrival.recv_timeout(Duration::from_secs(5)))
        .await
        .unwrap()
        .expect("the first ack never arrived");
    sender.want(ack(channel(2), at(message(2))), Pace::Explicit);
    sender.want(ack(channel(3), at(message(3))), Pace::Paced);
    sender.close();
    sender.want(ack(channel(4), at(message(4))), Pace::Explicit);
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

    sender.want(ack(channel(1), at(message(10))), Pace::Explicit);
    received(&server, 1).await;
    sender.want(ack(channel(1), at(message(10))), Pace::Explicit);
    sender.want(ack(channel(2), at(message(1))), Pace::Explicit);
    received(&server, 2).await;
    sender.want_bulk(vec![ack(channel(1), at(message(9)))]);
    sender.want(ack(channel(3), at(message(1))), Pace::Explicit);
    received(&server, 3).await;
    sender.want(ack(channel(1), at(message(11))), Pace::Explicit);
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
    let entries: Vec<(ChannelId, MessageId)> =
        (1..=120).map(|n| (channel(n), message(n))).collect();

    sender.want_bulk(
        entries
            .iter()
            .map(|(channel, message)| ack(*channel, at(*message)))
            .collect(),
    );
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
                .map(|(channel, message)| {
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
    let rest = rest(&server);
    let sender = sender_on(rest.clone(), Duration::from_millis(10));

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    discords_time(&rest).await;
    sender.want(ack(channel(2), now(floor)), Pace::Explicit);
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
    let rest = rest(&server);
    let sender = sender_on(rest.clone(), Duration::from_millis(10));

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    discords_time(&rest).await;
    sender.want(ack(channel(2), now(Some(newest))), Pace::Explicit);
    let requests = received(&server, 2).await;

    assert_eq!(acks(&requests)[1], ack_of(channel(2), newest));
}

#[tokio::test]
async fn without_a_server_time_a_now_target_sends_its_floor() {
    let server = MockServer::start().await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);

    sender.want(ack(channel(1), now(None)), Pace::Explicit);
    sender.want(ack(channel(2), now(Some(message(7)))), Pace::Explicit);
    let requests = received(&server, 1).await;

    assert_eq!(acks(&requests), [ack_of(channel(2), message(7))]);
}

fn sender_on(rest: Arc<AccountRest>, first_retry: Duration) -> AckSender {
    AckSender::start(
        &tokio::runtime::Handle::current(),
        rest,
        first_retry,
        || {},
        |_| {},
    )
}

type HandedBack = Arc<Mutex<Vec<Refusal>>>;

fn sender_handing_back(server: &MockServer) -> (AckSender, HandedBack) {
    let handed_back = HandedBack::default();
    let into = handed_back.clone();
    let sender = AckSender::start(
        &tokio::runtime::Handle::current(),
        rest(server),
        Duration::from_millis(10),
        || {},
        move |refusal| into.lock().unwrap().push(refusal),
    );
    (sender, handed_back)
}

// What was handed back, once there are at least `count` (or after 5 s).
async fn handed_back(handed: &HandedBack, count: usize) -> Vec<Refusal> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let handed = handed.lock().unwrap().clone();
        if handed.len() >= count || std::time::Instant::now() > deadline {
            return handed;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn refusal(
    channel: ChannelId,
    undo: Option<ReadState>,
    after: Option<ReadState>,
    sent: MessageId,
) -> Refusal {
    Refusal {
        channel,
        undo,
        after,
        sent,
    }
}

// Discord's time once an answer brought it.
async fn discords_time(rest: &AccountRest) -> i64 {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(now) = rest.server_now_millis() {
            return now;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no answer with a Date"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn a_now_ack_is_resolved_when_it_is_queued() {
    let server = MockServer::start().await;
    mock_acks(&server, answered()).await;
    let rest = rest(&server);
    let sender = sender_on(rest.clone(), Duration::from_millis(10));
    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    let queued = discords_time(&rest).await;

    sender.want(ack(channel(2), now(None)), Pace::Paced);
    let requests = received(&server, 2).await;

    let sent = MessageId::new(acks(&requests)[1].1).unix_millis();
    assert!(
        (queued..queued + 1_000).contains(&sent),
        "queued at {queued}, sent {sent}: the lull isn't in the ID"
    );
}

#[tokio::test]
async fn a_retry_sends_the_id_resolved_when_queued() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(answered())
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(answered())
        .with_priority(3)
        .mount(&server)
        .await;
    let rest = rest(&server);
    let sender = sender_on(rest.clone(), Duration::from_millis(300));
    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    discords_time(&rest).await;

    sender.want(ack(channel(2), now(None)), Pace::Explicit);
    let requests = received(&server, 3).await;

    let sent = acks(&requests);
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert_eq!(sent[1].0, channel(2).get());
    assert_eq!(sent[1], sent[2], "the retry sent another ID");
}

#[test]
fn a_retry_after_over_a_minute_is_honored() {
    let first = Duration::from_secs(2);
    let limited = |secs: u64| RequestError::RateLimited {
        retry_after: Some(Duration::from_secs(secs)),
    };

    assert_eq!(
        retry_wait(&limited(90), 1, first),
        Some(Duration::from_secs(90))
    );
    assert_eq!(
        retry_wait(&limited(3600), 8, first),
        Some(Duration::from_secs(3600))
    );
}

// Answers the first bulk with a 429; every answer comes after `SLOW`.
struct LimitedBulk {
    arrivals: Arc<Mutex<Vec<std::time::Instant>>>,
    limited: std::sync::atomic::AtomicBool,
    retry_after: f64,
}

impl Respond for LimitedBulk {
    fn respond(&self, request: &Received) -> ResponseTemplate {
        self.arrivals
            .lock()
            .unwrap()
            .push(std::time::Instant::now());
        let answer = if request.url.path() != BULK {
            answered()
        } else if !self.limited.swap(true, std::sync::atomic::Ordering::SeqCst) {
            ResponseTemplate::new(429).set_body_json(
                json!({"message": "You are being rate limited.", "retry_after": self.retry_after, "global": false}),
            )
        } else {
            ResponseTemplate::new(204)
        };
        answer.set_delay(SLOW)
    }
}

#[tokio::test]
async fn an_explicit_ack_goes_ahead_of_a_rate_limited_bulk() {
    let server = MockServer::start().await;
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .respond_with(LimitedBulk {
            arrivals: arrivals.clone(),
            limited: std::sync::atomic::AtomicBool::new(false),
            retry_after: 1.0,
        })
        .mount(&server)
        .await;
    let sender = sender(&server);

    sender.want_bulk(vec![
        ack(channel(1), at(message(1))),
        ack(channel(2), at(message(2))),
    ]);
    received(&server, 1).await;
    sender.want(ack(channel(3), at(message(3))), Pace::Explicit);
    let requests = received(&server, 3).await;

    assert_eq!(
        acks(&requests),
        [(0, 2), ack_of(channel(3), message(3)), (0, 2)]
    );
    let arrivals = arrivals.lock().unwrap().clone();
    assert!(
        arrivals[2] - arrivals[0] >= Duration::from_secs(1),
        "the bulk went again before Discord's retry_after"
    );
    for pair in arrivals.windows(2) {
        assert!(pair[1] - pair[0] >= SLOW, "{:?}", pair[1] - pair[0]);
    }
}

#[tokio::test]
async fn an_exhausted_bucket_doesnt_hold_other_acks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(ACKS))
        .respond_with(
            answered()
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset-after", "1"),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    mock_acks(&server, answered()).await;
    let sender = sender(&server);

    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    received(&server, 1).await;
    sender.want(ack(channel(1), at(message(2))), Pace::Explicit);
    sender.want(ack(channel(2), at(message(3))), Pace::Explicit);
    let requests = received(&server, 3).await;

    assert_eq!(
        acks(&requests),
        [
            ack_of(channel(1), message(1)),
            ack_of(channel(2), message(3)),
            ack_of(channel(1), message(2))
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn an_explicit_ack_doesnt_wait_for_its_channels_bulk_retry() {
    let wants = Wants::default();
    wants.want_bulk(vec![
        (channel(1), message(1), None, None),
        (channel(2), message(2), None, None),
    ]);
    wants.next().await.unwrap();
    let retry_at = Instant::now() + Duration::from_secs(60);
    wants.retry(retry_at);

    let asked = Instant::now();
    wants.want(channel(1), message(3), None, None, Pace::Explicit);
    let escaped = wants.next().await.unwrap();

    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(single(&escaped), (channel(1), message(3)));
    let retried = wants.next().await.unwrap();
    assert!(Instant::now() >= retry_at);
    assert!(retried.bulk);
    assert_eq!(retried.entries, [(channel(2), message(2))]);
}

fn read_at(n: u64, mentions: u32) -> Option<ReadState> {
    Some(ReadState {
        acked: Some(message(n)),
        mentions,
        estimated: false,
    })
}

fn refused() -> ResponseTemplate {
    ResponseTemplate::new(403).set_body_json(json!({"message": "Missing Access", "code": 50001}))
}

// Refuses every bulk with more than one entry, and the entries of `refusing`.
struct Refusing(Vec<ChannelId>);

impl Respond for Refusing {
    fn respond(&self, request: &Received) -> ResponseTemplate {
        let channels = bulk_channels(request);
        let refused_entry = channels
            .iter()
            .any(|channel| self.0.iter().any(|refusing| refusing.get() == *channel));
        if channels.len() > 1 || refused_entry {
            refused()
        } else {
            ResponseTemplate::new(204)
        }
    }
}

fn bulk_channels(request: &Received) -> Vec<u64> {
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    body["read_states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["channel_id"].as_str().unwrap().parse().unwrap())
        .collect()
}

#[tokio::test]
async fn a_refused_bulk_is_split_into_single_entries() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BULK))
        .respond_with(Refusing(vec![channel(2)]))
        .mount(&server)
        .await;
    let (sender, _) = sender_handing_back(&server);

    sender.want_bulk((1..=3).map(|n| ack(channel(n), at(message(n)))).collect());
    let requests = received(&server, 4).await;

    let bodies: Vec<Vec<u64>> = requests.iter().map(bulk_channels).collect();
    assert_eq!(bodies.len(), 4, "{bodies:?}");
    assert_eq!(bodies[0], [1, 2, 3].map(|n| channel(n).get()));
    let mut singles: Vec<u64> = bodies[1..]
        .iter()
        .map(|body| match body[..] {
            [channel] => channel,
            _ => panic!("not split: {bodies:?}"),
        })
        .collect();
    singles.sort_unstable();
    assert_eq!(singles, [1, 2, 3].map(|n| channel(n).get()));
}

#[tokio::test]
async fn a_refused_entry_is_handed_back_to_be_undone() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BULK))
        .respond_with(Refusing(vec![channel(2)]))
        .mount(&server)
        .await;
    let (sender, handed) = sender_handing_back(&server);

    sender.want_bulk(
        (1..=3)
            .map(|n| {
                ack_over(
                    channel(n),
                    at(message(n + 10)),
                    read_at(n, 1),
                    read_at(n + 10, 0),
                )
            })
            .collect(),
    );
    received(&server, 4).await;

    assert_eq!(
        handed_back(&handed, 1).await,
        [refusal(
            channel(2),
            read_at(2, 1),
            read_at(12, 0),
            message(12)
        )]
    );
}

#[tokio::test]
async fn a_refused_channel_ack_is_handed_back_to_be_undone() {
    let server = MockServer::start().await;
    mock_acks(&server, refused()).await;
    let (sender, handed) = sender_handing_back(&server);

    sender.want(
        ack_over(channel(1), at(message(5)), read_at(3, 2), read_at(5, 0)),
        Pace::Explicit,
    );
    received(&server, 1).await;

    assert_eq!(
        handed_back(&handed, 1).await,
        [refusal(
            channel(1),
            read_at(3, 2),
            read_at(5, 0),
            message(5)
        )]
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_refused_ack_leaves_its_undo_to_the_channels_newer_one() {
    let wants = Wants::default();
    wants.want(
        channel(1),
        message(5),
        read_at(3, 2),
        read_at(5, 0),
        Pace::Explicit,
    );
    wants.next().await.unwrap();
    wants.want(
        channel(1),
        message(6),
        read_at(5, 0),
        read_at(6, 0),
        Pace::Explicit,
    );

    assert_eq!(wants.refuse(), [], "the newer ack still goes out");
    let newer = wants.next().await.unwrap();
    assert_eq!(single(&newer), (channel(1), message(6)));
    assert_eq!(
        wants.refuse(),
        [refusal(
            channel(1),
            read_at(3, 2),
            read_at(6, 0),
            message(6)
        )]
    );
}

// What's left to send, until nothing comes for a minute; each request goes through.
async fn drain(wants: &Wants) -> Vec<(ChannelId, MessageId)> {
    let mut sent = Vec::new();
    while let Ok(Some(request)) = timeout(Duration::from_secs(60), wants.next()).await {
        sent.extend(request.entries);
        wants.confirmed();
    }
    sent.sort_unstable();
    sent
}

#[tokio::test(start_paused = true)]
async fn a_manual_ack_elsewhere_drops_the_pending_ack() {
    let wants = Wants::default();
    wants.want(channel(1), message(1), None, None, Pace::Explicit);
    wants.next().await.unwrap();
    wants.retry(Instant::now() + Duration::from_secs(2));
    wants.want(channel(2), message(2), None, None, Pace::Paced);
    wants.want(channel(3), message(3), None, None, Pace::Explicit);
    wants.next().await.unwrap();
    wants.want(channel(4), message(4), None, None, Pace::Paced);

    for n in 1..=3 {
        wants.forget(channel(n));
    }
    wants.retry(Instant::now());

    assert_eq!(
        drain(&wants).await,
        [(channel(4), message(4))],
        "retrying, queued and on its way: all dropped"
    );
}

#[tokio::test(start_paused = true)]
async fn merging_keeps_the_first_seen_and_undo() {
    let wants = Wants::default();
    wants.want(
        channel(1),
        message(5),
        read_at(3, 2),
        read_at(5, 0),
        Pace::Paced,
    );
    wants.want(
        channel(1),
        message(6),
        read_at(5, 0),
        read_at(6, 0),
        Pace::Explicit,
    );
    wants.next().await.unwrap();
    wants.want(
        channel(1),
        message(7),
        read_at(6, 0),
        read_at(7, 0),
        Pace::Explicit,
    );
    wants.retry(Instant::now());

    wants.forget_all();
    wants.reconcile(|_| read_at(3, 2));
    let request = timeout(Duration::from_secs(60), wants.next())
        .await
        .expect("dropped at READY")
        .unwrap();

    assert_eq!(single(&request), (channel(1), message(7)));
    assert_eq!(
        wants.refuse(),
        [refusal(
            channel(1),
            read_at(3, 2),
            read_at(7, 0),
            message(7)
        )],
        "the first undo, the newest after"
    );
}

#[tokio::test(start_paused = true)]
async fn ready_checks_the_acks_from_before_it_and_no_later_ones() {
    let wants = Wants::default();
    wants.want(channel(1), message(11), read_at(1, 1), None, Pace::Explicit);
    wants.next().await.unwrap();
    wants.want(channel(2), message(12), read_at(2, 1), None, Pace::Paced);
    wants.want(channel(3), message(13), read_at(3, 1), None, Pace::Paced);
    wants.want(channel(5), message(15), read_at(5, 1), None, Pace::Paced);

    wants.forget_all();
    wants.want(channel(4), message(14), read_at(4, 1), None, Pace::Paced);
    wants.reconcile(|id| {
        if id == channel(2) {
            read_at(2, 3)
        } else if id == channel(5) {
            None
        } else {
            read_at(9, 0)
        }
    });
    wants.retry(Instant::now());

    assert_eq!(
        drain(&wants).await,
        [(channel(2), message(12)), (channel(4), message(14))]
    );
}

#[tokio::test(start_paused = true)]
async fn an_ack_on_its_way_at_ready_doesnt_hold_back_the_next() {
    let wants = Wants::default();
    wants.want(channel(1), message(11), read_at(1, 1), None, Pace::Explicit);
    wants.next().await.unwrap();

    wants.forget_all();
    wants.confirmed();
    wants.reconcile(|_| read_at(1, 1));
    wants.want(channel(1), message(11), read_at(1, 1), None, Pace::Explicit);

    assert_eq!(drain(&wants).await, [(channel(1), message(11))]);
}

#[test]
fn an_estimated_position_is_none_of_discords() {
    let estimated = Some(ReadState {
        acked: Some(message(1)),
        mentions: 1,
        estimated: true,
    });

    assert_eq!(seen(estimated), None);
    assert_eq!(seen(read_at(1, 1)), Some(message(1)));
    assert_eq!(seen(None), None);
}

#[tokio::test(start_paused = true)]
async fn nothing_from_before_ready_goes_out_until_its_checked() {
    let wants = Wants::default();
    wants.want(channel(1), message(1), read_at(1, 1), None, Pace::Explicit);

    wants.forget_all();
    assert!(
        timeout(Duration::from_secs(60), wants.next())
            .await
            .is_err(),
        "sent before READY's check"
    );
    wants.reconcile(|_| read_at(1, 1));

    assert_eq!(
        single(&wants.next().await.unwrap()),
        (channel(1), message(1))
    );
}

#[tokio::test(start_paused = true)]
async fn an_ack_taken_while_ready_is_swapped_in_isnt_dropped_with_an_older_one() {
    let wants = Wants::default();
    wants.want(channel(1), message(1), read_at(1, 1), None, Pace::Paced);

    wants.forget_all();
    wants.want(channel(1), message(10), read_at(1, 1), None, Pace::Explicit);
    wants.reconcile(|_| read_at(9, 0));

    assert_eq!(drain(&wants).await, [(channel(1), message(10))]);

    wants.want(channel(2), message(3), read_at(2, 1), None, Pace::Explicit);
    wants.next().await.unwrap();
    wants.forget_all();
    wants.want(channel(2), message(12), read_at(2, 1), None, Pace::Explicit);
    wants.retry(Instant::now());
    wants.reconcile(|_| read_at(9, 0));

    assert_eq!(
        drain(&wants).await,
        [(channel(2), message(12))],
        "one on its way came back into it"
    );
}

// The store the acks were taken over is gone; undone, a channel goes back to READY's state, but
// not past an ack Discord took since.
#[tokio::test(start_paused = true)]
async fn a_refused_ack_from_before_ready_goes_back_to_ready_s_read_state() {
    let wants = Wants::default();
    wants.want(
        channel(1),
        message(11),
        read_at(1, 1),
        read_at(11, 0),
        Pace::Explicit,
    );
    wants.forget_all();
    wants.want(
        channel(2),
        message(12),
        read_at(2, 1),
        read_at(12, 0),
        Pace::Explicit,
    );
    wants.want(
        channel(3),
        message(13),
        read_at(3, 1),
        read_at(13, 0),
        Pace::Explicit,
    );
    let first = wants.next().await.unwrap();
    assert_eq!(single(&first), (channel(2), message(12)));
    wants.confirmed();
    wants.want(
        channel(2),
        message(14),
        read_at(12, 0),
        read_at(14, 0),
        Pace::Explicit,
    );
    let ready = |id: ChannelId| read_at(id.get() - 300_000_000_000_000_000, 7);
    wants.reconcile(ready);

    let mut refused = Vec::new();
    while let Ok(Some(_)) = timeout(Duration::from_secs(60), wants.next()).await {
        refused.extend(wants.refuse());
    }
    refused.sort_unstable_by_key(|refusal| refusal.channel);
    assert_eq!(
        refused,
        [
            refusal(channel(1), read_at(1, 7), read_at(11, 0), message(11)),
            refusal(channel(2), read_at(12, 0), read_at(14, 0), message(14)),
            refusal(channel(3), read_at(3, 7), read_at(13, 0), message(13)),
        ]
    );
}

// Holds the first bulk until the test lets it go, then rate limits it for 30 s.
struct GatedLimit {
    arrived: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    first: std::sync::atomic::AtomicBool,
}

impl Respond for GatedLimit {
    fn respond(&self, request: &Received) -> ResponseTemplate {
        if request.url.path() != BULK {
            return answered();
        }
        if !self.first.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return ResponseTemplate::new(204);
        }
        let _ = self.arrived.send(());
        let _ = self
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10));
        ResponseTemplate::new(429).set_body_json(
            json!({"message": "You are being rate limited.", "retry_after": 30.0, "global": false}),
        )
    }
}

#[tokio::test]
async fn an_escape_during_a_bulks_flight_doesnt_wait_for_its_rate_limit() {
    let server = MockServer::start().await;
    let (arrived, on_arrival) = std::sync::mpsc::channel();
    let (release, on_release) = std::sync::mpsc::channel();
    Mock::given(method("POST"))
        .respond_with(GatedLimit {
            arrived,
            release: Mutex::new(on_release),
            first: std::sync::atomic::AtomicBool::new(true),
        })
        .mount(&server)
        .await;
    let sender = sender(&server);
    sender.want_bulk(vec![
        ack(channel(1), at(message(1))),
        ack(channel(2), at(message(2))),
    ]);
    tokio::task::spawn_blocking(move || on_arrival.recv_timeout(Duration::from_secs(5)))
        .await
        .unwrap()
        .expect("the bulk never arrived");

    sender.want(ack(channel(1), at(message(3))), Pace::Explicit);
    release.send(()).unwrap();
    let released = std::time::Instant::now();
    let requests = received(&server, 2).await;

    assert_eq!(
        acks(&requests),
        [(0, 2), ack_of(channel(1), message(3))],
        "the Escape waited for the bulk's 30 s"
    );
    assert!(released.elapsed() < Duration::from_secs(1));
}

#[tokio::test(start_paused = true)]
async fn a_reading_ack_doesnt_wait_for_its_channels_bulk_retry() {
    let wants = Wants::default();
    wants.want_bulk(vec![
        (channel(1), message(1), None, None),
        (channel(2), message(2), None, None),
    ]);
    wants.next().await.unwrap();
    let retry_at = Instant::now() + Duration::from_secs(60);
    wants.retry(retry_at);

    let asked = Instant::now();
    wants.want(channel(1), message(3), None, None, Pace::Paced);
    let read = wants.next().await.unwrap();

    assert!(asked.elapsed() <= CAP, "{:?}", asked.elapsed());
    assert_eq!(single(&read), (channel(1), message(3)));
    assert_eq!(read.pace, Pace::Paced);
    let retried = wants.next().await.unwrap();
    assert!(Instant::now() >= retry_at);
    assert_eq!(retried.entries, [(channel(2), message(2))]);
}

#[tokio::test(start_paused = true)]
async fn a_request_the_limiter_holds_waits_without_a_failure() {
    let wants = Wants::default();
    wants.want(channel(1), message(1), None, None, Pace::Explicit);
    wants.next().await.unwrap();
    wants.retry(Instant::now());
    wants.next().await.unwrap();

    let held_until = Instant::now() + Duration::from_millis(300);
    wants.hold(held_until);
    let request = wants.next().await.unwrap();

    assert!(Instant::now() >= held_until);
    assert_eq!(single(&request), (channel(1), message(1)));
    assert_eq!(request.failures, 1, "the hold counted as a failure");
}

#[tokio::test]
async fn a_bucket_the_limiter_holds_is_waited_out_without_a_backoff() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(ACKS))
        .respond_with(
            answered()
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset-after", "0.3"),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    mock_acks(&server, answered()).await;
    let sender = sender_retrying_after(&server, Duration::from_secs(2));
    sender.want(ack(channel(1), at(message(1))), Pace::Explicit);
    received(&server, 1).await;

    let asked = std::time::Instant::now();
    sender.want(ack(channel(1), at(message(2))), Pace::Explicit);
    let requests = received(&server, 2).await;

    assert_eq!(
        acks(&requests),
        [
            ack_of(channel(1), message(1)),
            ack_of(channel(1), message(2))
        ]
    );
    assert!(
        asked.elapsed() < Duration::from_millis(1500),
        "waited {:?}, a failure's backoff",
        asked.elapsed()
    );
}

// The channel ack's route may be what's limited, not the bulk's.
#[tokio::test(start_paused = true)]
async fn a_bulk_doesnt_wait_for_a_channel_acks_retry() {
    let wants = Wants::default();
    wants.want(channel(1), message(1), None, None, Pace::Explicit);
    wants.next().await.unwrap();
    wants.want_bulk(vec![
        (channel(1), message(2), None, None),
        (channel(2), message(3), None, None),
    ]);
    wants.retry(Instant::now() + Duration::from_secs(60));
    let joined = timeout(Duration::from_secs(1), wants.next())
        .await
        .expect("waited for the channel ack's retry")
        .unwrap();
    assert_eq!(
        joined.entries,
        [(channel(1), message(2)), (channel(2), message(3))],
        "a bulk that came during the channel ack's flight"
    );

    wants.want(channel(3), message(4), None, None, Pace::Explicit);
    wants.next().await.unwrap();
    wants.retry(Instant::now() + Duration::from_secs(60));
    wants.want_bulk(vec![
        (channel(3), message(5), None, None),
        (channel(4), message(6), None, None),
    ]);
    let taken_over = timeout(Duration::from_secs(1), wants.next())
        .await
        .expect("waited for the channel ack's retry")
        .unwrap();
    assert_eq!(
        taken_over.entries,
        [(channel(3), message(5)), (channel(4), message(6))],
        "a bulk that took over a waiting channel ack"
    );
}

// J1 went through and READY's position lies between it and J2: an undo of J2 stops at READY's.
#[tokio::test(start_paused = true)]
async fn a_refused_ack_never_goes_back_below_ready() {
    let wants = Wants::default();
    wants.forget_all();
    wants.want(
        channel(1),
        message(11),
        read_at(1, 1),
        read_at(11, 0),
        Pace::Explicit,
    );
    wants.next().await.unwrap();
    wants.confirmed();
    wants.want(
        channel(1),
        message(13),
        read_at(11, 0),
        read_at(13, 0),
        Pace::Explicit,
    );
    wants.reconcile(|_| read_at(12, 2));

    wants.next().await.unwrap();
    assert_eq!(
        wants.refuse(),
        [refusal(
            channel(1),
            read_at(12, 2),
            read_at(13, 0),
            message(13)
        )]
    );
}

// A dispatch's ack reached the queue before the host's ack the store took first.
#[tokio::test(start_paused = true)]
async fn acks_merged_out_of_store_order_can_still_be_undone() {
    let wants = Wants::default();
    wants.want(
        channel(1),
        message(7),
        read_at(5, 0),
        read_at(7, 0),
        Pace::Paced,
    );
    wants.want(
        channel(1),
        message(5),
        read_at(1, 2),
        read_at(5, 0),
        Pace::Explicit,
    );

    wants.next().await.unwrap();
    assert_eq!(
        wants.refuse(),
        [refusal(
            channel(1),
            read_at(1, 2),
            read_at(7, 0),
            message(7)
        )]
    );
}
