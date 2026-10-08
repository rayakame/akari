use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use reqwest::header::HeaderMap;
use serde::Deserialize;
use tokio::sync::OwnedMutexGuard;
use tokio::time::{Instant, sleep_until};

// Discord allows 50 requests per second per user token.
const GLOBAL_LIMIT: usize = 50;
const GLOBAL_WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Method {
    Get,
    Post,
}

// With its top-level resource ID: limits apply per channel, guild or webhook.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RouteKey {
    method: Method,
    route: &'static str,
    major: u64,
}

impl RouteKey {
    pub(crate) fn new(method: Method, route: &'static str, major: u64) -> Self {
        Self {
            method,
            route,
            major,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResponseLimits {
    pub(crate) bucket: Option<String>,
    pub(crate) remaining: Option<u32>,
    pub(crate) reset_after: Option<Duration>,
    pub(crate) limited: bool,
    pub(crate) global: bool,
    pub(crate) retry_after: Option<Duration>,
}

#[derive(Deserialize)]
struct LimitedBody {
    retry_after: Option<f64>,
    global: Option<bool>,
}

fn seconds(value: f64) -> Option<Duration> {
    Duration::try_from_secs_f64(value).ok()
}

impl ResponseLimits {
    pub(crate) fn from_response(status: u16, headers: &HeaderMap, body: &[u8]) -> Self {
        let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
        let number = |name: &str| header(name).and_then(|value| value.parse::<f64>().ok());
        let reset_after = number("x-ratelimit-reset-after").and_then(seconds);
        let limited = status == 429;
        let body = limited
            .then(|| serde_json::from_slice::<LimitedBody>(body).ok())
            .flatten();
        let retry_after = limited
            .then(|| {
                body.as_ref()
                    .and_then(|body| body.retry_after)
                    .and_then(seconds)
                    .or(reset_after)
                    .or_else(|| number("retry-after").and_then(seconds))
            })
            .flatten();
        let global = limited
            && (body.as_ref().and_then(|body| body.global) == Some(true)
                || header("x-ratelimit-global") == Some("true"));
        Self {
            bucket: header("x-ratelimit-bucket").map(str::to_owned),
            remaining: header("x-ratelimit-remaining").and_then(|value| value.parse().ok()),
            reset_after,
            limited,
            global,
            retry_after,
        }
    }
}

#[derive(Default)]
struct Bucket {
    remaining: Option<u32>,
    reset_at: Option<Instant>,
}

#[derive(Default)]
struct Limits {
    // A route's bucket hash, learned from X-RateLimit-Bucket and shared across channels.
    hashes: HashMap<(Method, &'static str), String>,
    buckets: HashMap<(String, u64), Bucket>,
    paused: HashMap<RouteKey, Instant>,
    global_until: Option<Instant>,
    starts: VecDeque<Instant>,
}

impl Limits {
    fn bucket_key(&self, route: &RouteKey) -> (String, u64) {
        let hash = self
            .hashes
            .get(&(route.method, route.route))
            .cloned()
            .unwrap_or_else(|| format!("{:?} {}", route.method, route.route));
        (hash, route.major)
    }

    // None: the request may start now and is counted. Some: try again then.
    fn reserve(&mut self, route: &RouteKey, now: Instant) -> Option<Instant> {
        let later = |at: Option<Instant>| at.filter(|at| *at > now);
        if let Some(at) = later(self.global_until) {
            return Some(at);
        }
        if let Some(at) = later(self.paused.get(route).copied()) {
            return Some(at);
        }
        let key = self.bucket_key(route);
        if let Some(bucket) = self.buckets.get_mut(&key) {
            match later(bucket.reset_at) {
                Some(at) if bucket.remaining == Some(0) => return Some(at),
                Some(_) => {}
                None => *bucket = Bucket::default(),
            }
        }
        while self
            .starts
            .front()
            .is_some_and(|start| now.duration_since(*start) >= GLOBAL_WINDOW)
        {
            self.starts.pop_front();
        }
        if self.starts.len() >= GLOBAL_LIMIT {
            return self.starts.front().map(|oldest| *oldest + GLOBAL_WINDOW);
        }
        self.starts.push_back(now);
        if let Some(bucket) = self.buckets.get_mut(&key) {
            bucket.remaining = bucket
                .remaining
                .map(|remaining| remaining.saturating_sub(1));
        }
        None
    }

    fn finished(&mut self, route: &RouteKey, limits: &ResponseLimits, now: Instant) {
        if let Some(hash) = &limits.bucket {
            self.hashes
                .insert((route.method, route.route), hash.clone());
        }
        let key = self.bucket_key(route);
        if limits.remaining.is_some() || limits.reset_after.is_some() {
            let bucket = self.buckets.entry(key).or_default();
            bucket.remaining = limits.remaining;
            bucket.reset_at = limits.reset_after.map(|after| now + after);
        }
        if let Some(after) = limits.retry_after.filter(|_| limits.limited) {
            if limits.global {
                self.global_until = Some(now + after);
            } else {
                self.paused.insert(route.clone(), now + after);
            }
        }
    }
}

// One per token. Requests on one route key run one at a time, in the order they asked;
// a permit holds its route until dropped.
#[derive(Default)]
pub(crate) struct RateLimiter {
    lanes: Mutex<HashMap<RouteKey, Arc<tokio::sync::Mutex<()>>>>,
    limits: Mutex<Limits>,
}

pub(crate) struct Permit<'a> {
    limiter: &'a RateLimiter,
    route: RouteKey,
    _lane: OwnedMutexGuard<()>,
}

impl RateLimiter {
    pub(crate) async fn acquire(&self, route: RouteKey) -> Permit<'_> {
        let lane = {
            let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
            // tokio's Mutex is fair, so waiters on one route start in the order they came.
            lanes.retain(|_, lane| Arc::strong_count(lane) > 1);
            lanes.entry(route.clone()).or_default().clone()
        };
        let guard = lane.lock_owned().await;
        self.reserve(&route).await;
        Permit {
            limiter: self,
            route,
            _lane: guard,
        }
    }

    async fn reserve(&self, route: &RouteKey) {
        loop {
            let wait = self
                .limits
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .reserve(route, Instant::now());
            match wait {
                Some(at) => sleep_until(at).await,
                None => break,
            }
        }
    }
}

impl Permit<'_> {
    /// Waits until the route may start again, so a retry goes before later requests.
    pub(crate) async fn renew(&self) {
        self.limiter.reserve(&self.route).await;
    }

    pub(crate) fn finish(&self, limits: &ResponseLimits) {
        self.limiter
            .limits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .finished(&self.route, limits, Instant::now());
    }
}

#[cfg(test)]
mod tests;
