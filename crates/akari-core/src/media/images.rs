//! An account's images: from disk when a kept file is large enough, else downloaded once per
//! image and size however many ask, with signed links renewed when due.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::Url;
use tokio::sync::watch;
use tokio::task::AbortHandle;

use super::client::{Fetched, MediaClient, Saved};
use super::disk::{DiskTier, NewFile};
use super::part::PartFile;
use super::signed::{self, Refresher, Renewal};
use super::sniff;
use super::url::{Form, Planned, is_attachment_link};
use super::{EmbedSlot, ImageError, ImageFile, ImageKey, ImageRequest, MAX_BODY, MAX_STREAMED};
use crate::DiscordClient;
use crate::backoff;
use crate::model::UserId;
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

impl FlightKey {
    fn of(planned: &Planned) -> Self {
        Self {
            disk: planned.disk.clone(),
            form: planned.form,
            needed: planned.needed,
        }
    }
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
    client: DiscordClient,
    // Whose directory: the host's account until READY names one.
    owner: Option<UserId>,
    tier: Mutex<Option<(UserId, Arc<DiskTier>)>>,
    store: Store,
    rest: Arc<AccountRest>,
    refresher: Refresher,
    runtime: tokio::runtime::Handle,
    flights: Mutex<Flights>,
    // Keys Discord answered "no such image" for, this session.
    missing: Mutex<HashSet<String>>,
    // Asks that ended over the cap or as no image this session: the same ask would again.
    refused: Mutex<HashMap<FlightKey, ImageError>>,
    // One heavy file streams at a time: an ask for the same one waits for the file it keeps.
    streaming: tokio::sync::Mutex<()>,
    // Renewed links by path, beside the store: a renewal never reloads a row.
    renewed: Mutex<HashMap<String, Url>>,
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
        client: &DiscordClient,
        owner: Option<UserId>,
        store: Store,
        rest: Arc<AccountRest>,
        runtime: &tokio::runtime::Handle,
    ) -> Arc<Self> {
        if let Some(owner) = owner {
            client.open_images(owner);
        }
        Arc::new(Self {
            media: client.media(),
            client: client.clone(),
            owner,
            tier: Mutex::default(),
            refresher: Refresher::start(runtime, rest.clone(), REFRESH_WINDOW),
            store,
            rest,
            runtime: runtime.clone(),
            flights: Mutex::default(),
            missing: Mutex::default(),
            refused: Mutex::default(),
            streaming: tokio::sync::Mutex::default(),
            renewed: Mutex::default(),
        })
    }

    // The directory of the account READY named, or of the host's account before READY.
    async fn tier(&self) -> Option<Arc<DiskTier>> {
        let user = self
            .store
            .current_user()
            .map(|current| current.user.id)
            .or(self.owner)?;
        if let Some((owner, kept)) = &*lock(&self.tier)
            && *owner == user
            && !kept.is_deleted()
        {
            return Some(kept.clone());
        }
        let kept = self.client.image_tier(user).await?;
        *lock(&self.tier) = Some((user, kept.clone()));
        Some(kept)
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
                    EmbedSlot::Image | EmbedSlot::Thumbnail | EmbedSlot::Video => {
                        let media = match slot {
                            EmbedSlot::Image => embed.image.as_ref(),
                            EmbedSlot::Thumbnail => embed.thumbnail.as_ref(),
                            _ => embed.video.as_ref(),
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
            Err(err) => {
                tracing::debug!(target: LOG, kind = key_kind(&request.key), reason = error_name(err), "image not asked for");
                return Err(err);
            }
        };
        if lock(&self.missing).contains(&planned.disk) {
            tracing::debug!(target: LOG, key = %planned.disk, "image known to be missing");
            return Err(ImageError::NotFound);
        }
        let flight = FlightKey {
            disk: planned.disk.clone(),
            form: planned.form,
            needed: planned.needed,
        };
        let Some(tier) = self.tier().await.filter(|tier| !tier.is_deleted()) else {
            tracing::debug!(target: LOG, key = %planned.disk, "no image directory");
            return Err(ImageError::Unavailable);
        };
        let mut kept = tier.find(&planned.disk, planned.form, planned.needed).await;
        // A GIF over the cap kept as the proxy's WebP, only for a host that plays WebP.
        if kept.is_none() && planned.webp.is_some() {
            kept = tier.find(&planned.disk, Form::WebP, planned.needed).await;
        }
        if let Some(entry) = kept {
            return Ok(ImageFile {
                path: entry.path,
                width: entry.width,
                height: entry.height,
                animated: entry.animated,
                from_disk: true,
            });
        }
        if let Some(err) = lock(&self.refused).get(&flight).copied() {
            tracing::debug!(target: LOG, key = %planned.disk, reason = error_name(err), "image known not to show");
            return Err(err);
        }
        if matches!(self.store.connection(), ConnectionState::Offline) {
            tracing::debug!(target: LOG, key = %planned.disk, "offline, image not on disk");
            return Err(ImageError::Unavailable);
        }
        if request.animated.is_some() && planned.form == Form::Still {
            tracing::debug!(target: LOG, key = %planned.disk, "no animated form, the still asked for");
        }
        self.shared(planned).await
    }

    async fn kept_without_message(&self, request: &ImageRequest) -> Answer {
        let ImageKey::Attachment { id, .. } = &request.key else {
            return Err(ImageError::UnknownKey);
        };
        let tier = self.tier().await.ok_or(ImageError::UnknownKey)?;
        let needed = request.width.max(request.height);
        let entry = tier
            .find(&format!("attachment/{}", id.get()), Form::Still, needed)
            .await
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
    async fn signed(&self, planned: &Planned, force: bool, mark: bool) -> Result<Url, ImageError> {
        let Some(link) = &planned.signed else {
            return Ok(planned.url.clone());
        };
        let current = self.current_link(link);
        if !force && !signed::due(&current, self.now_seconds()) {
            return Ok(signed::resign(&planned.url, &current));
        }
        let usable = !force && signed::expires(&current).is_some_and(|ex| ex > self.now_seconds());
        let current = match self.refresher.refresh(&current).await {
            Renewal::Renewed(fresh) => {
                self.keep_renewed(link, fresh.clone());
                fresh
            }
            // Not expired yet: it still works for now.
            Renewal::Refused | Renewal::Failed if usable => current,
            Renewal::Refused => {
                if mark {
                    lock(&self.missing).insert(planned.disk.clone());
                }
                return Err(ImageError::NotFound);
            }
            Renewal::Failed => return Err(ImageError::Unavailable),
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
        let flight = FlightKey {
            disk: planned.disk.clone(),
            form: planned.form,
            needed: planned.needed,
        };
        let mut url = self.signed(&planned, false, true).await?;
        let mut renewed_after_denial = false;
        let mut from_original = false;
        let mut attempts = 0;
        let outcome = loop {
            attempts += 1;
            match self.media.fetch(&url, MAX_BODY).await {
                Fetched::Body(bytes) => break self.keep(&planned, planned.form, bytes).await,
                Fetched::Status(403 | 404)
                    if planned.signed.is_some()
                        && !renewed_after_denial
                        && signed::expires(&url).is_none_or(|ex| ex <= self.now_seconds()) =>
                {
                    renewed_after_denial = true;
                    // The proxy still has its copy: an upload Discord won't sign marks nothing.
                    let renewed = match self.signed(&planned, true, !from_original).await {
                        Ok(renewed) => renewed,
                        Err(err) => break Err(err),
                    };
                    url = match &planned.original {
                        Some(original) if from_original => signed::resign(original, &renewed),
                        _ => renewed,
                    };
                }
                Fetched::TooLarge if !from_original && let Some(original) = &planned.original => {
                    tracing::debug!(target: LOG, key = %planned.disk, "proxy copy too large, original asked");
                    from_original = true;
                    attempts = 0;
                    url = match self.signed(&planned, false, false).await {
                        Ok(renewed) => signed::resign(original, &renewed),
                        Err(err) => break Err(err),
                    };
                }
                Fetched::Status(403 | 404 | 410) if from_original => {
                    break Err(ImageError::NotFound);
                }
                Fetched::Status(403 | 404 | 410) => {
                    lock(&self.missing).insert(planned.disk.clone());
                    break Err(ImageError::NotFound);
                }
                Fetched::Status(415) | Fetched::NotAnImage => break Err(ImageError::NotAnImage),
                Fetched::TooLarge => {
                    tracing::debug!(
                        target: LOG,
                        key = %planned.disk,
                        copy = if from_original { "original" } else { "proxy" },
                        has_original = planned.original.is_some(),
                        cap = MAX_BODY,
                        "image over the display cap"
                    );
                    break if planned.form == Form::Gif {
                        self.heavy(&planned).await
                    } else {
                        Err(ImageError::TooLarge)
                    };
                }
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
        if outcome == Err(ImageError::TooLarge) {
            self.refuse(flight, ImageError::TooLarge);
        }
        outcome
    }

    // Whether to ask again after `fetched`, once the wait is over.
    async fn retried(fetched: &Fetched, attempts: u32) -> bool {
        if attempts >= ATTEMPTS {
            return false;
        }
        match fetched {
            Fetched::RateLimited => true,
            Fetched::Status(500..) | Fetched::Transport(_) => {
                tokio::time::sleep(retry_wait(attempts)).await;
                true
            }
            _ => false,
        }
    }

    fn refuse(&self, flight: FlightKey, err: ImageError) {
        let mut refused = lock(&self.refused);
        if refused.len() >= RENEWED_LIMIT {
            refused.clear();
        }
        refused.insert(flight, err);
    }

    // A GIF no form of which fits the display cap: the proxy's animated WebP at the box's size
    // where the host plays it, else the file streamed to disk.
    async fn heavy(&self, planned: &Planned) -> Answer {
        let signature = self.signed(planned, false, false).await?;
        // A WebP that failed might fit next time: then nothing is remembered as too large.
        let mut webp_may_fit = false;
        if let Some(webp) = &planned.webp {
            let url = resigned(planned, webp, &signature);
            let mut attempts = 0;
            let fetched = loop {
                attempts += 1;
                let fetched = self.media.fetch(&url, MAX_BODY).await;
                if !Self::retried(&fetched, attempts).await {
                    break fetched;
                }
            };
            webp_may_fit = matches!(
                fetched,
                Fetched::RateLimited | Fetched::Status(500..) | Fetched::Transport(_)
            );
            if let Fetched::Body(bytes) = fetched {
                let plays = sniff::inspect(&bytes).is_some_and(|inspected| {
                    inspected.animated && inspected.format == sniff::Format::WebP
                });
                if plays {
                    match self.keep(planned, Form::WebP, bytes).await {
                        Ok(file) => {
                            tracing::debug!(target: LOG, key = %planned.disk, "heavy GIF kept as the proxy's WebP");
                            return Ok(file);
                        }
                        Err(ImageError::TooLarge) => {}
                        Err(_) => webp_may_fit = true,
                    }
                }
            }
            tracing::debug!(target: LOG, key = %planned.disk, "no WebP of a heavy GIF, the file streamed");
        }
        match self.streamed(planned, signature).await {
            Err(ImageError::TooLarge) if webp_may_fit => Err(ImageError::Unavailable),
            answer => answer,
        }
    }

    async fn streamed(&self, planned: &Planned, mut signature: Url) -> Answer {
        let _one = self.streaming.lock().await;
        let tier = self
            .tier()
            .await
            .filter(|tier| !tier.is_deleted())
            .ok_or(ImageError::Unavailable)?;
        // Another box's stream of the same file may have kept it meanwhile.
        if let Some(entry) = tier.find(&planned.disk, planned.form, planned.needed).await {
            return Ok(ImageFile {
                path: entry.path,
                width: entry.width,
                height: entry.height,
                animated: entry.animated,
                from_disk: true,
            });
        }
        let source = planned.upload.as_ref().unwrap_or(&planned.url);
        let mut url = resigned(planned, source, &signature);
        if tokio::fs::create_dir_all(tier.temporary()).await.is_err() {
            return Err(ImageError::Io);
        }
        let part = PartFile::new(tier.temporary_file());
        let mut renewed_after_denial = false;
        let mut attempts = 0;
        let length = loop {
            attempts += 1;
            match self.media.stream(&url, part.path(), MAX_STREAMED).await {
                Fetched::File(length) => break length,
                Fetched::TooLarge => {
                    tracing::debug!(target: LOG, key = %planned.disk, cap = MAX_STREAMED, "image over the streamed cap");
                    return Err(ImageError::TooLarge);
                }
                Fetched::Status(403 | 404)
                    if planned.signed.is_some()
                        && !renewed_after_denial
                        && signed::expires(&url).is_none_or(|ex| ex <= self.now_seconds()) =>
                {
                    renewed_after_denial = true;
                    signature = self.signed(planned, true, false).await?;
                    url = resigned(planned, source, &signature);
                }
                Fetched::Status(403 | 404 | 410) => return Err(ImageError::NotFound),
                Fetched::Status(415) | Fetched::NotAnImage => return Err(ImageError::NotAnImage),
                Fetched::Refused => return Err(ImageError::Unsupported),
                Fetched::Io => return Err(ImageError::Io),
                fetched => {
                    if !Self::retried(&fetched, attempts).await {
                        return Err(ImageError::Unavailable);
                    }
                    tracing::debug!(target: LOG, key = %planned.disk, attempts, "image stream retried");
                }
            }
        };
        let path = part.path().to_path_buf();
        let inspected = tokio::task::spawn_blocking(move || sniff::inspect_file(&path))
            .await
            .map_err(|_| ImageError::Io)?
            .map_err(|_| ImageError::Io)?;
        let Some(inspected) = inspected else {
            // The same bytes would come again: not streamed twice in a session.
            self.refuse(FlightKey::of(planned), ImageError::NotAnImage);
            return Err(ImageError::NotAnImage);
        };
        if !inspected.animated {
            tracing::debug!(target: LOG, key = %planned.disk, "heavy file is a still, not kept");
            return Err(ImageError::TooLarge);
        }
        if over_pixels(planned, inspected) {
            return Err(ImageError::TooLarge);
        }
        self.index(planned, planned.form, inspected, part, length)
            .await
    }

    async fn keep(&self, planned: &Planned, form: Form, bytes: Vec<u8>) -> Answer {
        let inspected = sniff::inspect(&bytes).ok_or(ImageError::NotAnImage)?;
        if over_pixels(planned, inspected) {
            return Err(ImageError::TooLarge);
        }
        if form != Form::Still && !inspected.animated {
            tracing::debug!(target: LOG, key = %planned.disk, "animation asked for, the file is a still");
        }
        let tier = self
            .tier()
            .await
            .filter(|tier| !tier.is_deleted())
            .ok_or(ImageError::Unavailable)?;
        let directory = tier.temporary().to_path_buf();
        let part = PartFile::new(tier.temporary_file());
        let length = bytes.len() as u64;
        let written = async {
            tokio::fs::create_dir_all(&directory).await?;
            tokio::fs::write(part.path(), bytes).await
        }
        .await;
        if written.is_err() {
            return Err(ImageError::Io);
        }
        self.index(planned, form, inspected, part, length).await
    }

    // Moves a downloaded file into the account's directory, under `form`.
    async fn index(
        &self,
        planned: &Planned,
        form: Form,
        inspected: sniff::Inspected,
        part: PartFile,
        length: u64,
    ) -> Answer {
        let tier = self
            .tier()
            .await
            .filter(|tier| !tier.is_deleted())
            .ok_or(ImageError::Unavailable)?;
        let longest = inspected.width.max(inspected.height);
        let full = planned.full
            || match planned.source {
                Some(source) => longest >= source,
                // The CDN never answers larger than the source.
                None => longest < planned.needed,
            };
        let entry = tier
            .insert(NewFile {
                key: planned.disk.clone(),
                form,
                width: inspected.width,
                height: inspected.height,
                animated: inspected.animated,
                full,
                extension: inspected.format.extension(),
                bytes: length,
                temporary: part.keep(),
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
                    EmbedSlot::Video => embed
                        .video
                        .as_ref()
                        .map(|media| media.proxy_url.as_deref().unwrap_or(&media.url)),
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
        match self.refresher.refresh(&current).await {
            Renewal::Renewed(fresh) => {
                self.keep_renewed(&link, fresh.clone());
                Ok(fresh)
            }
            Renewal::Refused => Err(ImageError::NotFound),
            Renewal::Failed => Err(ImageError::Unavailable),
        }
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
            EmbedSlot::Video => embed.video.as_ref(),
            EmbedSlot::AuthorIcon | EmbedSlot::FooterIcon => None,
        };
        media
            .and_then(|media| media.proxy_url.as_deref())
            .and_then(|proxy| Url::parse(proxy).ok())
            .ok_or(ImageError::Unsupported)
    }

    pub(crate) async fn clear(&self) {
        lock(&self.missing).clear();
        lock(&self.refused).clear();
        if let Some(tier) = self.tier().await {
            tier.clear().await;
        }
    }
}

impl Drop for Images {
    fn drop(&mut self) {
        if let Some(owner) = self.owner {
            self.client.release_images(owner);
        }
    }
}

fn over_pixels(planned: &Planned, inspected: sniff::Inspected) -> bool {
    let over = u64::from(inspected.width) * u64::from(inspected.height) > MAX_PIXELS;
    if over {
        tracing::debug!(target: LOG, key = %planned.disk, width = inspected.width, height = inspected.height, "image over the pixel cap");
    }
    over
}

// `target` with the plan's current signature; a link Discord doesn't sign stays as it is.
fn resigned(planned: &Planned, target: &Url, signature: &Url) -> Url {
    if planned.signed.is_some() {
        signed::resign(target, signature)
    } else {
        target.clone()
    }
}

fn retry_wait(attempts: u32) -> Duration {
    backoff::delay(attempts, RETRY_BASE, RETRY_MAX, crate::random::unit())
}

fn key_kind(key: &ImageKey) -> &'static str {
    match key {
        ImageKey::Avatar { .. } => "avatar",
        ImageKey::MemberAvatar { .. } => "member_avatar",
        ImageKey::DefaultAvatar { .. } => "default_avatar",
        ImageKey::GuildIcon { .. } => "guild_icon",
        ImageKey::GroupIcon { .. } => "group_icon",
        ImageKey::RoleIcon { .. } => "role_icon",
        ImageKey::Emoji { .. } => "emoji",
        ImageKey::Sticker { .. } => "sticker",
        ImageKey::Attachment { .. } => "attachment",
        ImageKey::EmbedMedia { .. } => "embed",
    }
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
