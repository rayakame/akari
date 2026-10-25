//! An account's images: from disk when a kept file is large enough, else downloaded once per
//! image and size however many ask, with signed links renewed when due.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::Url;
use tokio::sync::watch;
use tokio::task::AbortHandle;

use super::client::{Fetched, MediaClient, Saved};
use super::disk::{DiskTier, NewFile};
use super::signed::{self, Refresher};
use super::sniff;
use super::url::{Form, Planned, is_attachment_link};
use super::{EmbedSlot, ImageError, ImageFile, ImageKey, ImageRequest, MAX_BODY};
use crate::backoff;
use crate::rest::{AccountRest, RequestError};
use crate::state::{ConnectionState, Message, Store};

const LOG: &str = "akari_core::media";
// Discord's client tries 5 times; 3 keeps a dead host from holding slots long.
const ATTEMPTS: u32 = 3;
const MAX_PIXELS: u64 = 64_000_000;
const RENEWED_LIMIT: usize = 4096;
const REFRESH_WINDOW: Duration = Duration::from_millis(50);
// Short in tests, so retries don't slow the suite down.
const RETRY_BASE: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(1)
};
const RETRY_MAX: Duration = if cfg!(test) {
    Duration::from_millis(50)
} else {
    Duration::from_secs(5)
};

type Answer = Result<ImageFile, ImageError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FlightKey {
    disk: String,
    form: Form,
    needed: u32,
}

struct Flight {
    id: u64,
    waiters: usize,
    answer: watch::Receiver<Option<Answer>>,
    abort: AbortHandle,
}

#[derive(Default)]
struct Flights {
    next: u64,
    running: HashMap<FlightKey, Flight>,
}

pub(crate) struct Images {
    media: Arc<MediaClient>,
    tier: Option<Arc<DiskTier>>,
    store: Store,
    rest: Arc<AccountRest>,
    refresher: Refresher,
    runtime: tokio::runtime::Handle,
    flights: Mutex<Flights>,
    // Keys Discord answered "no such image" for, this session.
    missing: Mutex<HashSet<String>>,
    // Renewed links by path, beside the store: a renewal never reloads a row.
    renewed: Mutex<HashMap<String, Url>>,
    downloads: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// A waiter for a download; the last one to go cancels it.
struct Waiting<'a> {
    images: &'a Images,
    key: FlightKey,
    id: u64,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        let mut flights = lock(&self.images.flights);
        if let Some(flight) = flights.running.get_mut(&self.key)
            && flight.id == self.id
        {
            flight.waiters -= 1;
            if flight.waiters == 0 {
                flight.abort.abort();
                flights.running.remove(&self.key);
                tracing::debug!(target: LOG, key = %self.key.disk, "image download cancelled");
            }
        }
    }
}

impl Images {
    pub(crate) fn new(
        media: Arc<MediaClient>,
        tier: Option<Arc<DiskTier>>,
        store: Store,
        rest: Arc<AccountRest>,
        runtime: &tokio::runtime::Handle,
    ) -> Arc<Self> {
        Arc::new(Self {
            media,
            tier,
            refresher: Refresher::start(runtime, rest.clone(), REFRESH_WINDOW),
            store,
            rest,
            runtime: runtime.clone(),
            flights: Mutex::default(),
            missing: Mutex::default(),
            renewed: Mutex::default(),
            downloads: AtomicU64::new(0),
        })
    }

    fn now_seconds(&self) -> i64 {
        self.rest.server_now_millis().map_or_else(
            || {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| {
                        i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
                    })
            },
            |millis| millis / 1000,
        )
    }

    fn message(&self, key: &ImageKey) -> Option<Arc<Message>> {
        match key {
            ImageKey::Attachment {
                channel, message, ..
            }
            | ImageKey::EmbedMedia {
                channel, message, ..
            } => self.store.message(*channel, *message),
            _ => None,
        }
    }

    fn plan(&self, request: &ImageRequest) -> Result<Planned, ImageError> {
        let hosts = self.media.hosts();
        let wanted = (request.width.max(1), request.height.max(1));
        match &request.key {
            ImageKey::Attachment { id, .. } => {
                let message = self.message(&request.key).ok_or(ImageError::UnknownKey)?;
                let attachment = message
                    .attachments
                    .iter()
                    .find(|attachment| attachment.id == *id)
                    .ok_or(ImageError::UnknownKey)?;
                hosts.attachment(attachment, wanted, request.animated)
            }
            ImageKey::EmbedMedia { embed, slot, .. } => {
                let message = self.message(&request.key).ok_or(ImageError::UnknownKey)?;
                let embed = message
                    .embeds
                    .get(usize::from(*embed))
                    .ok_or(ImageError::UnknownKey)?;
                match slot {
                    EmbedSlot::Image | EmbedSlot::Thumbnail => {
                        let media = if *slot == EmbedSlot::Image {
                            embed.image.as_ref()
                        } else {
                            embed.thumbnail.as_ref()
                        };
                        hosts.embed(
                            media.ok_or(ImageError::UnknownKey)?,
                            wanted,
                            request.animated,
                        )
                    }
                    EmbedSlot::AuthorIcon => hosts.embed_icon(
                        embed
                            .author
                            .as_ref()
                            .and_then(|author| author.proxy_icon_url.as_deref())
                            .ok_or(ImageError::UnknownKey)?,
                    ),
                    EmbedSlot::FooterIcon => hosts.embed_icon(
                        embed
                            .footer
                            .as_ref()
                            .and_then(|footer| footer.proxy_icon_url.as_deref())
                            .ok_or(ImageError::UnknownKey)?,
                    ),
                }
            }
            key => hosts.identity(key, wanted.0.max(wanted.1), request.animated),
        }
    }

    fn closed(&self) -> bool {
        matches!(self.rest.usable(), Err(RequestError::Closed))
            || matches!(self.store.connection(), ConnectionState::Closed { .. })
    }

    pub(crate) async fn image(self: &Arc<Self>, request: ImageRequest) -> Answer {
        if self.closed() {
            return Err(ImageError::Closed);
        }
        let planned = match self.plan(&request) {
            Ok(planned) => planned,
            // An attachment kept from an earlier session answers without its message.
            Err(ImageError::UnknownKey) => return self.kept_without_message(&request).await,
            Err(err) => return Err(err),
        };
        if lock(&self.missing).contains(&planned.disk) {
            return Err(ImageError::NotFound);
        }
        let tier = self.tier.as_ref().ok_or(ImageError::Unavailable)?;
        if let Some(entry) = tier.find(&planned.disk, planned.form, planned.needed).await {
            if entry.path.exists() {
                return Ok(ImageFile {
                    path: entry.path,
                    width: entry.width,
                    height: entry.height,
                    animated: entry.animated,
                    from_disk: true,
                });
            }
            tier.forget(entry.path);
        }
        if matches!(self.store.connection(), ConnectionState::Offline) {
            return Err(ImageError::Unavailable);
        }
        self.shared(planned).await
    }

    async fn kept_without_message(&self, request: &ImageRequest) -> Answer {
        let ImageKey::Attachment { id, .. } = &request.key else {
            return Err(ImageError::UnknownKey);
        };
        let tier = self.tier.as_ref().ok_or(ImageError::UnknownKey)?;
        let needed = request.width.max(request.height);
        let entry = tier
            .find(&format!("attachment/{}", id.get()), Form::Still, needed)
            .await
            .filter(|entry| entry.path.exists())
            .ok_or(ImageError::UnknownKey)?;
        Ok(ImageFile {
            path: entry.path,
            width: entry.width,
            height: entry.height,
            animated: entry.animated,
            from_disk: true,
        })
    }

    async fn shared(self: &Arc<Self>, planned: Planned) -> Answer {
        let key = FlightKey {
            disk: planned.disk.clone(),
            form: planned.form,
            needed: planned.needed,
        };
        let (id, mut answer) = {
            let mut flights = lock(&self.flights);
            if let Some(flight) = flights.running.get_mut(&key) {
                flight.waiters += 1;
                (flight.id, flight.answer.clone())
            } else {
                let id = flights.next;
                flights.next += 1;
                let (sender, answer) = watch::channel(None);
                let images = self.clone();
                let finished = key.clone();
                let task = self.runtime.spawn(async move {
                    let result = images.download(planned).await;
                    {
                        let mut flights = lock(&images.flights);
                        if flights
                            .running
                            .get(&finished)
                            .is_some_and(|flight| flight.id == id)
                        {
                            flights.running.remove(&finished);
                        }
                    }
                    let _ = sender.send(Some(result));
                });
                flights.running.insert(
                    key.clone(),
                    Flight {
                        id,
                        waiters: 1,
                        answer: answer.clone(),
                        abort: task.abort_handle(),
                    },
                );
                (id, answer)
            }
        };
        let _waiting = Waiting {
            images: self,
            key,
            id,
        };
        match answer.wait_for(Option::is_some).await {
            Ok(result) => result.clone().unwrap_or(Err(ImageError::Unavailable)),
            Err(_) => Err(ImageError::Unavailable),
        }
    }

    // The link to fetch: its renewed signature if it has one, renewed now when due.
    async fn signed(&self, planned: &Planned, force: bool) -> Result<Url, ImageError> {
        let Some(link) = &planned.signed else {
            return Ok(planned.url.clone());
        };
        let current = self.current_link(link);
        let current = if force || signed::due(&current, self.now_seconds()) {
            match self.refresher.refresh(&current).await {
                Some(fresh) => {
                    self.keep_renewed(link, fresh.clone());
                    fresh
                }
                None if !force
                    && signed::expires(&current).is_some_and(|ex| ex > self.now_seconds()) =>
                {
                    current
                }
                None => return Err(ImageError::Unavailable),
            }
        } else {
            current
        };
        Ok(signed::resign(&planned.url, &current))
    }

    fn current_link(&self, link: &Url) -> Url {
        lock(&self.renewed)
            .get(link.path())
            .cloned()
            .unwrap_or_else(|| link.clone())
    }

    fn keep_renewed(&self, link: &Url, fresh: Url) {
        let mut renewed = lock(&self.renewed);
        if renewed.len() >= RENEWED_LIMIT {
            renewed.clear();
        }
        renewed.insert(link.path().to_owned(), fresh);
    }

    async fn download(&self, planned: Planned) -> Answer {
        let started = Instant::now();
        let mut url = self.signed(&planned, false).await?;
        let mut renewed_after_denial = false;
        let mut attempts = 0;
        let outcome = loop {
            attempts += 1;
            match self.media.fetch(&url, MAX_BODY).await {
                Fetched::Body(bytes) => break self.keep(&planned, bytes).await,
                Fetched::Status(403 | 404)
                    if planned.signed.is_some()
                        && !renewed_after_denial
                        && signed::expires(&url).is_none_or(|ex| ex <= self.now_seconds()) =>
                {
                    renewed_after_denial = true;
                    url = self.signed(&planned, true).await?;
                }
                Fetched::Status(403 | 404 | 410) => {
                    lock(&self.missing).insert(planned.disk.clone());
                    break Err(ImageError::NotFound);
                }
                Fetched::Status(415) | Fetched::NotAnImage => break Err(ImageError::NotAnImage),
                Fetched::TooLarge => break Err(ImageError::TooLarge),
                Fetched::Refused => break Err(ImageError::Unsupported),
                Fetched::RateLimited if attempts < ATTEMPTS => {}
                Fetched::Status(status @ 500..) if attempts < ATTEMPTS => {
                    tracing::debug!(target: LOG, key = %planned.disk, attempts, status, "image download retried");
                    tokio::time::sleep(retry_wait(attempts)).await;
                }
                Fetched::Transport(kind) if attempts < ATTEMPTS => {
                    tracing::debug!(target: LOG, key = %planned.disk, attempts, transport = %kind, "image download retried");
                    tokio::time::sleep(retry_wait(attempts)).await;
                }
                _ => break Err(ImageError::Unavailable),
            }
        };
        tracing::debug!(
            target: LOG,
            key = %planned.disk,
            outcome = match &outcome {
                Ok(_) => "kept",
                Err(err) => error_name(*err),
            },
            attempts,
            after_ms = crate::millis(started.elapsed()),
            "image downloaded"
        );
        outcome
    }

    async fn keep(&self, planned: &Planned, bytes: Vec<u8>) -> Answer {
        let inspected = sniff::inspect(&bytes).ok_or(ImageError::NotAnImage)?;
        if u64::from(inspected.width) * u64::from(inspected.height) > MAX_PIXELS {
            return Err(ImageError::TooLarge);
        }
        let tier = self.tier.as_ref().ok_or(ImageError::Unavailable)?;
        let directory = tier.temporary().to_path_buf();
        let temporary = directory.join(format!(
            "{}.part",
            self.downloads.fetch_add(1, Ordering::Relaxed)
        ));
        let length = bytes.len() as u64;
        let written = async {
            tokio::fs::create_dir_all(&directory).await?;
            tokio::fs::write(&temporary, bytes).await
        }
        .await;
        if written.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(ImageError::Io);
        }
        let longest = inspected.width.max(inspected.height);
        let entry = tier
            .insert(NewFile {
                key: planned.disk.clone(),
                form: planned.form,
                width: inspected.width,
                height: inspected.height,
                animated: inspected.animated,
                // The CDN never answers larger than the source.
                full: planned.full || longest < planned.needed,
                extension: inspected.format.extension(),
                bytes: length,
                temporary,
            })
            .await
            .ok_or(ImageError::Io)?;
        Ok(ImageFile {
            path: entry.path,
            width: entry.width,
            height: entry.height,
            animated: entry.animated,
            from_disk: false,
        })
    }

    // The original as Discord links it: a renewed signature for attachments.
    async fn original(&self, key: &ImageKey) -> Result<Url, ImageError> {
        match key {
            ImageKey::Attachment { id, .. } => {
                let message = self.message(key).ok_or(ImageError::UnknownKey)?;
                let attachment = message
                    .attachments
                    .iter()
                    .find(|attachment| attachment.id == *id)
                    .ok_or(ImageError::UnknownKey)?;
                let link = Url::parse(&attachment.url).map_err(|_| ImageError::UnknownKey)?;
                self.renewed_link(link).await
            }
            ImageKey::EmbedMedia { embed, slot, .. } => {
                let message = self.message(key).ok_or(ImageError::UnknownKey)?;
                let embed = message
                    .embeds
                    .get(usize::from(*embed))
                    .ok_or(ImageError::UnknownKey)?;
                let link = match slot {
                    EmbedSlot::Image => embed.image.as_ref().map(|media| &*media.url),
                    EmbedSlot::Thumbnail => embed.thumbnail.as_ref().map(|media| &*media.url),
                    EmbedSlot::AuthorIcon => embed
                        .author
                        .as_ref()
                        .and_then(|author| author.proxy_icon_url.as_deref()),
                    EmbedSlot::FooterIcon => embed
                        .footer
                        .as_ref()
                        .and_then(|footer| footer.proxy_icon_url.as_deref()),
                }
                .ok_or(ImageError::UnknownKey)?;
                let link = Url::parse(link).map_err(|_| ImageError::UnknownKey)?;
                if is_attachment_link(&link) {
                    self.renewed_link(link).await
                } else {
                    Ok(link)
                }
            }
            key => self.media.hosts().original(key),
        }
    }

    async fn renewed_link(&self, link: Url) -> Result<Url, ImageError> {
        let current = self.current_link(&link);
        if !signed::due(&current, self.now_seconds()) {
            return Ok(current);
        }
        let fresh = self
            .refresher
            .refresh(&current)
            .await
            .ok_or(ImageError::Unavailable)?;
        self.keep_renewed(&link, fresh.clone());
        Ok(fresh)
    }

    pub(crate) async fn original_url(&self, key: &ImageKey) -> Result<String, ImageError> {
        if self.closed() {
            return Err(ImageError::Closed);
        }
        self.original(key).await.map(String::from)
    }

    pub(crate) async fn save_original(
        &self,
        key: &ImageKey,
        destination: &Path,
    ) -> Result<u64, ImageError> {
        if self.closed() {
            return Err(ImageError::Closed);
        }
        if matches!(self.store.connection(), ConnectionState::Offline) {
            return Err(ImageError::Unavailable);
        }
        let mut url = self.original(key).await?;
        // External images are saved from the proxy's copy: Akari fetches only from Discord.
        if !self.media.hosts().allows(&url)
            && let ImageKey::EmbedMedia { .. } = key
        {
            url = self.embed_proxy(key)?;
        }
        let started = Instant::now();
        let saved = self.media.save(&url, destination).await;
        tracing::debug!(
            target: LOG,
            outcome = match saved {
                Ok(_) => "saved",
                Err(Saved::Refused) => "refused",
                Err(Saved::Network) => "network",
                Err(Saved::Status(_)) => "status",
                Err(Saved::Io) => "io",
            },
            after_ms = crate::millis(started.elapsed()),
            "original saved"
        );
        saved.map_err(|err| match err {
            Saved::Refused => ImageError::Unsupported,
            Saved::Status(404 | 403 | 410) => ImageError::NotFound,
            Saved::Network | Saved::Status(_) => ImageError::Unavailable,
            Saved::Io => ImageError::Io,
        })
    }

    fn embed_proxy(&self, key: &ImageKey) -> Result<Url, ImageError> {
        let ImageKey::EmbedMedia { embed, slot, .. } = key else {
            return Err(ImageError::UnknownKey);
        };
        let message = self.message(key).ok_or(ImageError::UnknownKey)?;
        let embed = message
            .embeds
            .get(usize::from(*embed))
            .ok_or(ImageError::UnknownKey)?;
        let media = match slot {
            EmbedSlot::Image => embed.image.as_ref(),
            EmbedSlot::Thumbnail => embed.thumbnail.as_ref(),
            EmbedSlot::AuthorIcon | EmbedSlot::FooterIcon => None,
        };
        media
            .and_then(|media| media.proxy_url.as_deref())
            .and_then(|proxy| Url::parse(proxy).ok())
            .ok_or(ImageError::Unsupported)
    }

    pub(crate) async fn clear(&self) {
        lock(&self.missing).clear();
        if let Some(tier) = &self.tier {
            tier.clear().await;
        }
    }
}

fn retry_wait(attempts: u32) -> Duration {
    backoff::delay(attempts, RETRY_BASE, RETRY_MAX, crate::random::unit())
}

fn error_name(err: ImageError) -> &'static str {
    match err {
        ImageError::NotFound => "not_found",
        ImageError::Unavailable => "unavailable",
        ImageError::TooLarge => "too_large",
        ImageError::NotAnImage => "not_an_image",
        ImageError::UnknownKey => "unknown_key",
        ImageError::Unsupported => "unsupported",
        ImageError::Io => "io",
        ImageError::Closed => "closed",
    }
}
