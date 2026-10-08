use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::header::{HeaderMap, HeaderValue};
use tokio::time::{Duration, Instant, timeout};

use super::*;

const SECOND: Duration = Duration::from_secs(1);
const HOUR: Duration = Duration::from_secs(3600);

fn route(method: Method, channel: u64) -> RouteKey {
    RouteKey::new(method, "channels/{}/messages", channel)
}

fn send(channel: u64) -> RouteKey {
    route(Method::Post, channel)
}

fn limited(retry_after: Duration, global: bool) -> ResponseLimits {
    ResponseLimits {
        limited: true,
        retry_after: Some(retry_after),
        global,
        ..ResponseLimits::default()
    }
}

async fn starts_within(limiter: &RateLimiter, route: RouteKey, wait: Duration) -> bool {
    matches!(timeout(wait, limiter.acquire(route)).await, Ok(Ok(_)))
}

#[tokio::test(start_paused = true)]
async fn requests_on_one_route_run_one_at_a_time_in_order() {
    let limiter = Arc::new(RateLimiter::new(HOUR));
    let first = limiter.acquire(send(1)).await.unwrap();
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for index in 0..5 {
        let (limiter, order, active, most) =
            (limiter.clone(), order.clone(), active.clone(), most.clone());
        tasks.push(tokio::spawn(async move {
            let permit = limiter.acquire(send(1)).await.unwrap();
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            order.lock().unwrap().push(index);
            tokio::time::sleep(Duration::from_millis(10)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            permit.finish(&ResponseLimits::default());
        }));
        tokio::task::yield_now().await;
    }

    drop(first);
    for task in tasks {
        task.await.unwrap();
    }

    assert_eq!(*order.lock().unwrap(), [0, 1, 2, 3, 4]);
    assert_eq!(most.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_slow_load_never_blocks_a_send_in_the_same_channel() {
    let limiter = RateLimiter::new(HOUR);
    let _loading = limiter.acquire(route(Method::Get, 1)).await.unwrap();

    assert!(starts_within(&limiter, send(1), Duration::from_millis(1)).await);
}

#[tokio::test(start_paused = true)]
async fn different_channels_run_in_parallel() {
    let limiter = RateLimiter::new(HOUR);
    let _first = limiter.acquire(send(1)).await.unwrap();

    assert!(starts_within(&limiter, send(2), Duration::from_millis(1)).await);
}

#[tokio::test(start_paused = true)]
async fn known_limits_wait_for_the_reset() {
    let limiter = RateLimiter::new(HOUR);
    let started = Instant::now();
    limiter
        .acquire(send(1))
        .await
        .unwrap()
        .finish(&ResponseLimits {
            remaining: Some(0),
            reset_after: Some(2 * SECOND),
            ..ResponseLimits::default()
        });

    let _next = limiter.acquire(send(1)).await.unwrap();

    assert!(started.elapsed() >= 2 * SECOND, "{:?}", started.elapsed());
}

#[tokio::test(start_paused = true)]
async fn a_shared_bucket_header_joins_two_routes() {
    let limiter = RateLimiter::new(HOUR);
    let other = RouteKey::new(Method::Get, "channels/{}/pins", 1);
    let bucket = |remaining, reset_after| ResponseLimits {
        bucket: Some("shared".to_owned()),
        remaining: Some(remaining),
        reset_after: Some(reset_after),
        ..ResponseLimits::default()
    };
    limiter
        .acquire(other.clone())
        .await
        .unwrap()
        .finish(&bucket(5, SECOND));
    let started = Instant::now();
    limiter
        .acquire(send(1))
        .await
        .unwrap()
        .finish(&bucket(0, 2 * SECOND));

    let _next = limiter.acquire(other).await.unwrap();

    assert!(started.elapsed() >= 2 * SECOND, "{:?}", started.elapsed());
}

#[tokio::test(start_paused = true)]
async fn never_more_than_50_requests_in_any_second() {
    let limiter = Arc::new(RateLimiter::new(HOUR));
    let starts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let tasks: Vec<_> = (0..200)
        .map(|channel| {
            let (limiter, starts) = (limiter.clone(), starts.clone());
            tokio::spawn(async move {
                let permit = limiter.acquire(send(channel)).await.unwrap();
                starts.lock().unwrap().push(Instant::now());
                permit.finish(&ResponseLimits::default());
            })
        })
        .collect();
    for task in tasks {
        task.await.unwrap();
    }

    let mut starts = starts.lock().unwrap().clone();
    starts.sort();
    assert_eq!(starts.len(), 200);
    for (index, start) in starts.iter().enumerate() {
        let in_window = starts[index..]
            .iter()
            .take_while(|later| **later - *start < SECOND)
            .count();
        assert!(in_window <= 50, "{in_window} requests within one second");
    }
}

#[tokio::test(start_paused = true)]
async fn a_route_429_pauses_only_that_route() {
    let limiter = RateLimiter::new(HOUR);
    let started = Instant::now();
    limiter
        .acquire(send(1))
        .await
        .unwrap()
        .finish(&limited(3 * SECOND, false));

    assert!(starts_within(&limiter, send(2), Duration::from_millis(1)).await);
    let _again = limiter.acquire(send(1)).await.unwrap();
    assert!(started.elapsed() >= 3 * SECOND, "{:?}", started.elapsed());
}

#[tokio::test(start_paused = true)]
async fn a_global_429_pauses_every_route() {
    let limiter = RateLimiter::new(HOUR);
    let started = Instant::now();
    limiter
        .acquire(send(1))
        .await
        .unwrap()
        .finish(&limited(3 * SECOND, true));

    let _other = limiter.acquire(send(2)).await.unwrap();

    assert!(started.elapsed() >= 3 * SECOND, "{:?}", started.elapsed());
}

#[tokio::test(start_paused = true)]
async fn a_retry_keeps_its_place_on_the_route() {
    let limiter = Arc::new(RateLimiter::new(HOUR));
    let permit = limiter.acquire(send(1)).await.unwrap();
    let waiting = tokio::spawn({
        let limiter = limiter.clone();
        async move {
            let _later = limiter.acquire(send(1)).await.unwrap();
        }
    });
    tokio::task::yield_now().await;

    permit.finish(&limited(SECOND, false));
    let renewed = timeout(2 * SECOND, permit.renew()).await;

    assert!(matches!(renewed, Ok(Ok(()))));
    assert!(!waiting.is_finished());
    drop(permit);
    waiting.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_wait_past_the_limit_fails_at_once() {
    let limiter = RateLimiter::new(10 * SECOND);
    limiter
        .acquire(send(1))
        .await
        .unwrap()
        .finish(&limited(60 * SECOND, false));

    let refused = timeout(Duration::from_millis(1), limiter.acquire(send(1))).await;
    let other = timeout(Duration::from_millis(1), limiter.acquire(send(2))).await;

    assert!(matches!(refused, Ok(Err(wait)) if wait > 50 * SECOND));
    assert!(matches!(other, Ok(Ok(_))));
}

#[tokio::test(start_paused = true)]
async fn a_dropped_permit_frees_its_route() {
    let limiter = RateLimiter::new(HOUR);
    drop(limiter.acquire(send(1)).await.unwrap());

    assert!(starts_within(&limiter, send(1), Duration::from_millis(1)).await);
}

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        headers.insert(*name, HeaderValue::from_str(value).unwrap());
    }
    headers
}

#[test]
fn retry_after_prefers_the_body_then_reset_after_then_the_header() {
    let all = headers(&[("retry-after", "9"), ("x-ratelimit-reset-after", "4.5")]);
    let body =
        br#"{"message": "You are being rate limited.", "retry_after": 1.25, "global": false}"#;

    let from_body = ResponseLimits::from_response(429, &all, body);
    let from_reset = ResponseLimits::from_response(429, &all, b"");
    let from_header = ResponseLimits::from_response(429, &headers(&[("retry-after", "9")]), b"");
    let none = ResponseLimits::from_response(429, &HeaderMap::new(), b"");

    assert_eq!(from_body.retry_after, Some(Duration::from_millis(1250)));
    assert!(from_body.limited && !from_body.global);
    assert_eq!(from_reset.retry_after, Some(Duration::from_millis(4500)));
    assert_eq!(from_header.retry_after, Some(Duration::from_secs(9)));
    assert_eq!(none.retry_after, None);
}

#[test]
fn bucket_headers_are_read() {
    let parsed = ResponseLimits::from_response(
        200,
        &headers(&[
            ("x-ratelimit-bucket", "abcd1234"),
            ("x-ratelimit-limit", "5"),
            ("x-ratelimit-remaining", "4"),
            ("x-ratelimit-reset-after", "1.5"),
        ]),
        b"[]",
    );

    assert_eq!(parsed.bucket.as_deref(), Some("abcd1234"));
    assert_eq!(parsed.remaining, Some(4));
    assert_eq!(parsed.reset_after, Some(Duration::from_millis(1500)));
    assert!(!parsed.limited);
}

#[test]
fn a_global_429_is_recognized_from_header_or_body() {
    let header = ResponseLimits::from_response(
        429,
        &headers(&[("x-ratelimit-global", "true"), ("retry-after", "2")]),
        b"",
    );
    let body = ResponseLimits::from_response(
        429,
        &HeaderMap::new(),
        br#"{"retry_after": 2.0, "global": true}"#,
    );

    assert!(header.global && body.global);
}
