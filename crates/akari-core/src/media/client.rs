//! The HTTP client for images, shared by every account: no token, none of Discord's headers,
//! no cookies or redirects, only Discord's image hosts, and at most eight downloads at once,
//! the newest first.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use reqwest::Url;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT};
use tokio::io::AsyncWriteExt as _;
use tokio::sync::oneshot;
use tokio::time::Instant;

use super::url::Hosts;
use crate::error::{TransportError, TransportErrorKind};
use crate::properties::ClientProperties;
use crate::rest::BuildError;

const IN_FLIGHT: usize = 8;
// Chromium's Accept for an <img>.
const IMAGES: &str = "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
// A host that asks for longer is treated as down for now.
const MAX_PAUSE: Duration = Duration::from_secs(60);

/// What one request got.
#[derive(Debug)]
pub(crate) enum Fetched {
    Body(Vec<u8>),
    Status(u16),
    /// 429: the host is paused for `Retry-After`.
    RateLimited,
    /// Not `image/*`.
    NotAnImage,
    TooLarge,
    /// The host isn't one of Discord's image hosts.
    Refused,
    Transport(TransportErrorKind),
}

pub(crate) struct MediaClient {
    http: reqwest::Client,
    hosts: Hosts,
    permits: Arc<Permits>,
    // Until when each host (and port) asked for a pause.
    pauses: Mutex<HashMap<(String, u16), Instant>>,
}

impl MediaClient {
    pub(crate) fn new(
        tls: &rustls::ClientConfig,
        properties: &ClientProperties,
        hosts: Hosts,
        https_only: bool,
    ) -> Result<Self, BuildError> {
        let mut tls = tls.clone();
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&properties.browser_user_agent)
                .map_err(|_| BuildError::InvalidHeader("user-agent"))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static(IMAGES));
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .default_headers(headers)
            .https_only(https_only)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            // Per read, so a large file on a slow line still finishes.
            .read_timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(300))
            .build()
            .map_err(|err| BuildError::Transport(TransportError::from_reqwest(err)))?;
        Ok(Self {
            http,
            hosts,
            permits: Permits::new(IN_FLIGHT),
            pauses: Mutex::default(),
        })
    }

    pub(crate) fn hosts(&self) -> &Hosts {
        &self.hosts
    }

    fn host(url: &Url) -> (String, u16) {
        (
            url.host_str().unwrap_or_default().to_owned(),
            url.port_or_known_default().unwrap_or(0),
        )
    }

    async fn wait_for_host(&self, url: &Url) {
        let until = self
            .pauses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&Self::host(url))
            .copied();
        if let Some(until) = until {
            tokio::time::sleep_until(until).await;
        }
    }

    fn pause(&self, url: &Url, headers: &HeaderMap) {
        let wait = headers
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
            .map_or(Duration::from_secs(1), Duration::from_secs_f64)
            .min(MAX_PAUSE);
        self.pauses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(Self::host(url), Instant::now() + wait);
    }

    /// One request for an image of at most `cap` bytes.
    pub(crate) async fn fetch(&self, url: &Url, cap: usize) -> Fetched {
        if !self.hosts.allows(url) {
            return Fetched::Refused;
        }
        self.wait_for_host(url).await;
        let _permit = self.permits.acquire().await;
        let mut response = match self.http.get(url.clone()).send().await {
            Ok(response) => response,
            Err(err) => return Fetched::Transport(TransportError::from_reqwest(err).kind()),
        };
        let status = response.status().as_u16();
        if status == 429 {
            self.pause(url, response.headers());
            return Fetched::RateLimited;
        }
        if status != 200 {
            return Fetched::Status(status);
        }
        let image = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim_start().starts_with("image/"));
        if !image {
            return Fetched::NotAnImage;
        }
        if response
            .content_length()
            .is_some_and(|length| length > cap as u64)
        {
            return Fetched::TooLarge;
        }
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > cap {
                        return Fetched::TooLarge;
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => return Fetched::Body(body),
                Err(err) => return Fetched::Transport(TransportError::from_reqwest(err).kind()),
            }
        }
    }

    /// Streams `url` into `destination` whatever its size and type; deletes what it wrote when
    /// it fails.
    pub(crate) async fn save(&self, url: &Url, destination: &Path) -> Result<u64, Saved> {
        if !self.hosts.allows(url) {
            return Err(Saved::Refused);
        }
        self.wait_for_host(url).await;
        let _permit = self.permits.acquire().await;
        let mut response = self
            .http
            .get(url.clone())
            .send()
            .await
            .map_err(|_| Saved::Network)?;
        match response.status().as_u16() {
            200 => {}
            429 => {
                self.pause(url, response.headers());
                return Err(Saved::Network);
            }
            status => return Err(Saved::Status(status)),
        }
        let mut file = tokio::fs::File::create(destination)
            .await
            .map_err(|_| Saved::Io)?;
        let mut written = 0_u64;
        let copied: Result<(), Saved> = async {
            while let Some(chunk) = response.chunk().await.map_err(|_| Saved::Network)? {
                file.write_all(&chunk).await.map_err(|_| Saved::Io)?;
                written += chunk.len() as u64;
            }
            file.flush().await.map_err(|_| Saved::Io)
        }
        .await;
        if let Err(err) = copied {
            drop(file);
            let _ = tokio::fs::remove_file(destination).await;
            return Err(err);
        }
        Ok(written)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Saved {
    Refused,
    Network,
    Status(u16),
    Io,
}

/// Slots for downloads, given to the newest waiter first: what's on screen now.
pub(crate) struct Permits {
    state: Mutex<PermitState>,
}

struct PermitState {
    free: usize,
    next: u64,
    waiting: BTreeMap<u64, oneshot::Sender<Permit>>,
}

/// A slot, given back when dropped.
pub(crate) struct Permit {
    permits: Option<Arc<Permits>>,
}

impl Permits {
    pub(crate) fn new(slots: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(PermitState {
                free: slots,
                next: 0,
                waiting: BTreeMap::new(),
            }),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, PermitState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) async fn acquire(self: &Arc<Self>) -> Permit {
        loop {
            let waiting = {
                let mut state = self.state();
                if state.free > 0 {
                    state.free -= 1;
                    return Permit {
                        permits: Some(self.clone()),
                    };
                }
                let (sender, receiver) = oneshot::channel();
                let turn = state.next;
                state.next += 1;
                state.waiting.insert(turn, sender);
                receiver
            };
            if let Ok(permit) = waiting.await {
                return permit;
            }
        }
    }

    // A waiter that gave up refuses the slot, which goes to the next newest.
    fn release(self: &Arc<Self>) {
        let mut state = self.state();
        while let Some((_, sender)) = state.waiting.pop_last() {
            let permit = Permit {
                permits: Some(self.clone()),
            };
            match sender.send(permit) {
                Ok(()) => return,
                Err(mut refused) => refused.permits = None,
            }
        }
        state.free += 1;
    }

    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.state().waiting.len()
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(permits) = self.permits.take() {
            permits.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    async fn queued(permits: &Arc<Permits>, count: usize) {
        while permits.waiting() < count {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn the_newest_waiter_gets_the_next_slot() {
        let permits = Permits::new(1);
        let held = permits.acquire().await;
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut tasks = Vec::new();
        for name in ["first", "second", "third"] {
            let shared = permits.clone();
            let order = order.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = shared.acquire().await;
                order.lock().unwrap().push(name);
                tokio::task::yield_now().await;
            }));
            queued(&permits, tasks.len()).await;
        }

        drop(held);
        for task in tasks {
            task.await.unwrap();
        }

        assert_eq!(*order.lock().unwrap(), ["third", "second", "first"]);
    }

    #[tokio::test]
    async fn a_waiter_that_gave_up_passes_its_slot_on() {
        let permits = Permits::new(1);
        let held = permits.acquire().await;
        let gave_up = {
            let permits = permits.clone();
            tokio::spawn(async move { permits.acquire().await })
        };
        queued(&permits, 1).await;
        gave_up.abort();
        let _ = gave_up.await;

        drop(held);

        let again = tokio::time::timeout(Duration::from_secs(1), permits.acquire()).await;
        assert!(again.is_ok(), "the slot came back");
    }

    #[tokio::test]
    async fn no_more_than_the_slots_at_once() {
        let permits = Permits::new(3);
        let running = Arc::new(Mutex::new((0, 0)));
        let tasks: Vec<_> = (0..20)
            .map(|_| {
                let permits = permits.clone();
                let running = running.clone();
                tokio::spawn(async move {
                    let _permit = permits.acquire().await;
                    {
                        let mut running = running.lock().unwrap();
                        running.0 += 1;
                        running.1 = running.1.max(running.0);
                    }
                    tokio::task::yield_now().await;
                    running.lock().unwrap().0 -= 1;
                })
            })
            .collect();

        for task in tasks {
            task.await.unwrap();
        }

        let (now, most) = *running.lock().unwrap();
        assert_eq!(now, 0);
        assert!((1..=3).contains(&most), "{most}");
    }
}
