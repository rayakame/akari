use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::*;
use crate::Token;
use crate::gateway::fake::client_with;
use crate::rest::AccountRest;

const NOW: i64 = 1_738_600_000;
const REFRESH: &str = "/api/v9/attachments/refresh-urls";

fn url(text: &str) -> Url {
    Url::parse(text).unwrap()
}

fn signed(ex: i64) -> Url {
    url(&format!(
        "https://cdn.discordapp.com/attachments/1/2/a.png?ex={ex:x}&is={:x}&hm=00ff",
        ex - 86_400
    ))
}

#[test]
fn ex_is_hex_seconds() {
    assert_eq!(expires(&signed(NOW)), Some(NOW));
    assert_eq!(
        expires(&url("https://cdn.discordapp.com/attachments/1/2/a.png")),
        None
    );
    assert_eq!(
        expires(&url(
            "https://cdn.discordapp.com/attachments/1/2/a.png?ex=zz"
        )),
        None
    );
}

#[test]
fn a_link_is_due_within_an_hour_of_its_expiry() {
    assert!(!due(&signed(NOW + 3_601), NOW));
    assert!(due(&signed(NOW + 3_599), NOW));
    assert!(due(&signed(NOW - 10), NOW));
    assert!(
        due(
            &url("https://cdn.discordapp.com/attachments/1/2/a.png"),
            NOW
        ),
        "an attachment link without ex counts as expired"
    );
    assert!(
        !due(
            &url("https://images-ext-1.discordapp.net/external/abc/https/example.com/a.png"),
            NOW
        ),
        "external images aren't signed"
    );
    assert!(!due(
        &url("https://cdn.discordapp.com/avatars/1/2.webp"),
        NOW
    ));
}

#[test]
fn resigning_swaps_the_signature_and_keeps_the_rest() {
    let proxy = url(
        "https://media.discordapp.net/attachments/1/2/a.png?ex=1&is=2&hm=3&format=webp&width=10&height=5",
    );
    let fresh = url("https://cdn.discordapp.com/attachments/1/2/a.png?ex=aa&is=bb&hm=cc");

    assert_eq!(
        resign(&proxy, &fresh).as_str(),
        "https://media.discordapp.net/attachments/1/2/a.png?ex=aa&is=bb&hm=cc&format=webp&width=10&height=5"
    );
    let bare = url("https://media.discordapp.net/attachments/1/2/a.png?format=webp");
    assert_eq!(
        resign(&bare, &fresh).as_str(),
        "https://media.discordapp.net/attachments/1/2/a.png?ex=aa&is=bb&hm=cc&format=webp"
    );
}

// Answers with each URL "refreshed", and notes when each request came.
#[derive(Clone, Default)]
struct Refreshing {
    arrivals: Arc<Mutex<Vec<(Instant, usize)>>>,
    delay: Duration,
}

impl Respond for Refreshing {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let urls = body["attachment_urls"].as_array().unwrap();
        self.arrivals
            .lock()
            .unwrap()
            .push((Instant::now(), urls.len()));
        let refreshed: Vec<Value> = urls
            .iter()
            .map(|original| {
                let original = original.as_str().unwrap();
                json!({"original": original, "refreshed": format!("{original}&fresh=1")})
            })
            .collect();
        ResponseTemplate::new(200)
            .set_body_json(json!({"refreshed_urls": refreshed}))
            .set_delay(self.delay)
    }
}

async fn refresher(server: &MockServer, window: Duration) -> Refresher {
    let client = client_with(
        "ws://127.0.0.1:9/".to_owned(),
        format!("{}/api/v9/", server.uri()),
    );
    let rest = Arc::new(AccountRest::new(
        client,
        Token::new("signed-test-token".to_owned()),
        Duration::from_millis(10),
    ));
    Refresher::start(&tokio::runtime::Handle::current(), rest, window)
}

fn attachment(n: usize) -> Url {
    url(&format!(
        "https://cdn.discordapp.com/attachments/1/{n}/a.png?ex=1&is=0&hm=00"
    ))
}

#[tokio::test]
async fn refreshes_close_together_go_in_one_request() {
    let server = MockServer::start().await;
    let responder = Refreshing::default();
    Mock::given(method("POST"))
        .and(path(REFRESH))
        .and(header("authorization", "signed-test-token"))
        .respond_with(responder.clone())
        .mount(&server)
        .await;
    let refresher = refresher(&server, Duration::from_millis(50)).await;

    let answers = futures_util::future::join_all((0..3).map(|n| {
        let refresher = refresher.clone();
        async move { refresher.refresh(&attachment(n)).await }
    }))
    .await;

    for (n, answer) in answers.into_iter().enumerate() {
        assert_eq!(
            answer,
            Renewal::Renewed(url(&format!("{}&fresh=1", attachment(n)))),
            "{n}"
        );
    }
    assert_eq!(responder.arrivals.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn at_most_fifty_per_request_and_one_request_at_a_time() {
    let server = MockServer::start().await;
    let responder = Refreshing {
        delay: Duration::from_millis(100),
        ..Refreshing::default()
    };
    Mock::given(path(REFRESH))
        .respond_with(responder.clone())
        .mount(&server)
        .await;
    let refresher = refresher(&server, Duration::from_millis(50)).await;

    let answers = futures_util::future::join_all((0..120).map(|n| {
        let refresher = refresher.clone();
        async move { refresher.refresh(&attachment(n)).await }
    }))
    .await;

    assert!(
        answers
            .iter()
            .all(|answer| matches!(answer, Renewal::Renewed(_)))
    );
    let arrivals = responder.arrivals.lock().unwrap().clone();
    assert!(
        arrivals.iter().all(|(_, count)| *count <= 50),
        "{arrivals:?}"
    );
    assert_eq!(arrivals.iter().map(|(_, count)| count).sum::<usize>(), 120);
    for pair in arrivals.windows(2) {
        assert!(
            pair[1].0 - pair[0].0 >= Duration::from_millis(100),
            "the next request waited for the answer"
        );
    }
}

#[tokio::test]
async fn the_same_link_is_asked_once_per_request() {
    let server = MockServer::start().await;
    let responder = Refreshing::default();
    Mock::given(path(REFRESH))
        .respond_with(responder.clone())
        .mount(&server)
        .await;
    let refresher = refresher(&server, Duration::from_millis(50)).await;

    let link = attachment(7);
    let (first, second) = tokio::join!(refresher.refresh(&link), refresher.refresh(&link));

    assert_eq!(first, second);
    assert!(matches!(first, Renewal::Renewed(_)));
    let arrivals = responder.arrivals.lock().unwrap().clone();
    assert_eq!(
        arrivals.iter().map(|(_, count)| *count).collect::<Vec<_>>(),
        [1]
    );
}

#[tokio::test]
async fn a_failed_refresh_answers_failed_and_the_next_asks_again() {
    let server = MockServer::start().await;
    Mock::given(path(REFRESH))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path(REFRESH))
        .respond_with(Refreshing::default())
        .mount(&server)
        .await;
    let refresher = refresher(&server, Duration::from_millis(10)).await;

    assert_eq!(refresher.refresh(&attachment(1)).await, Renewal::Failed);
    assert!(matches!(
        refresher.refresh(&attachment(1)).await,
        Renewal::Renewed(_)
    ));
}

#[tokio::test]
async fn an_answer_without_the_link_is_a_refusal() {
    let server = MockServer::start().await;
    Mock::given(path(REFRESH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"refreshed_urls": []})))
        .mount(&server)
        .await;
    let refresher = refresher(&server, Duration::from_millis(10)).await;

    assert_eq!(refresher.refresh(&attachment(1)).await, Renewal::Refused);
}
