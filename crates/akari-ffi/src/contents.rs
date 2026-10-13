use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use akari_core::MessagePreparer;
use akari_core::model::{MessageId, Timestamp};
use akari_core::state;
use akari_markdown::{Cache, Document};

use crate::records::ContentStats;

// About 1–2 MB of documents per account (D57).
const CAPACITY: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ContentKey {
    id: MessageId,
    edited: Option<Timestamp>,
}

impl ContentKey {
    fn of(message: &state::Message) -> Self {
        Self {
            id: message.id,
            edited: message.edited_timestamp,
        }
    }
}

/// How a read found a message's document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Found {
    Cached,
    // A live change `next()` handed out, parsed when read.
    EventParse,
    Miss,
}

/// An account's parsed message contents. Pages and the disk cache's fill are parsed on the
/// runtime before they enter a window (D56, amendment 84); live changes when they're read.
pub(crate) struct Contents {
    cache: Mutex<Cache<ContentKey>>,
    // Live changes `next()` handed out and nobody read yet; past the capacity it starts over,
    // so a forgotten one counts as a miss, never the other way round.
    announced: Mutex<HashSet<ContentKey>>,
    capacity: usize,
    parsed: AtomicU64,
    misses: AtomicU64,
    event_parses: AtomicU64,
    #[cfg(test)]
    prepared_on: Mutex<Vec<Option<String>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Contents {
    pub(crate) fn new() -> Arc<Self> {
        Self::with_capacity(CAPACITY)
    }

    pub(crate) fn with_capacity(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            cache: Mutex::new(Cache::new(capacity)),
            announced: Mutex::default(),
            capacity,
            parsed: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            event_parses: AtomicU64::new(0),
            #[cfg(test)]
            prepared_on: Mutex::default(),
        })
    }

    pub(crate) fn document(&self, message: &state::Message) -> (Arc<Document>, Found) {
        let key = ContentKey::of(message);
        if let Some(document) = lock(&self.cache).get(&key, message.content.len()) {
            return (document, Found::Cached);
        }
        let document = Arc::new(akari_markdown::parse(&message.content));
        lock(&self.cache).insert(key, message.content.len(), document.clone());
        self.parsed.fetch_add(1, Ordering::Relaxed);
        let found = if lock(&self.announced).remove(&key) {
            self.event_parses.fetch_add(1, Ordering::Relaxed);
            Found::EventParse
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            Found::Miss
        };
        (document, found)
    }

    pub(crate) fn announce(&self, message: &state::Message) {
        let mut announced = lock(&self.announced);
        if announced.len() >= self.capacity {
            announced.clear();
        }
        announced.insert(ContentKey::of(message));
    }

    pub(crate) fn stats(&self) -> ContentStats {
        ContentStats {
            parsed: self.parsed.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            event_parses: self.event_parses.load(Ordering::Relaxed),
            cached: u32::try_from(lock(&self.cache).len()).unwrap_or(u32::MAX),
        }
    }

    #[cfg(test)]
    pub(crate) fn prepared_on(&self) -> Vec<Option<String>> {
        lock(&self.prepared_on).clone()
    }
}

impl MessagePreparer for Contents {
    fn prepare(&self, messages: &[Arc<state::Message>]) -> u32 {
        #[cfg(test)]
        lock(&self.prepared_on).push(std::thread::current().name().map(str::to_owned));
        let missing: Vec<&state::Message> = {
            let mut cache = lock(&self.cache);
            messages
                .iter()
                .filter(|message| {
                    cache
                        .get(&ContentKey::of(message), message.content.len())
                        .is_none()
                })
                .map(|message| &**message)
                .collect()
        };
        // Parsed without the lock, so reads aren't held up.
        let parsed: Vec<_> = missing
            .iter()
            .map(|message| Arc::new(akari_markdown::parse(&message.content)))
            .collect();
        let mut cache = lock(&self.cache);
        for (message, document) in missing.iter().zip(parsed) {
            cache.insert(ContentKey::of(message), message.content.len(), document);
        }
        let count = missing.len() as u64;
        self.parsed.fetch_add(count, Ordering::Relaxed);
        u32::try_from(count).unwrap_or(u32::MAX)
    }
}
