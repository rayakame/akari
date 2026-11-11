use std::collections::HashSet;
use std::hash::{BuildHasher, RandomState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use akari_core::MessagePreparer;
use akari_core::model::{MessageId, Timestamp};
use akari_core::state;
use akari_markdown::{Cache, Document, EmbedText};

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

// One text of one embed: 0 its title, 1 its description, then each field's name and value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EmbedKey {
    id: MessageId,
    embed: u8,
    part: u16,
}

/// One text of an embed and how it parses.
pub(crate) struct EmbedPart<'a> {
    pub(crate) part: u16,
    pub(crate) text: std::borrow::Cow<'a, str>,
    /// `None`: the plain string.
    pub(crate) rules: Option<EmbedText>,
}

/// The texts `embed` shows, by its rules: titles and field names on one line.
pub(crate) fn embed_parts(
    embed: &state::Embed,
    rules: state::EmbedTextRules,
) -> Vec<EmbedPart<'_>> {
    let markdown = |text| rules.markdown.then_some(text);
    let title = EmbedText::Title {
        links: rules.title_links,
    };
    let mut parts = Vec::new();
    if let Some(text) = embed.title.as_deref() {
        parts.push(EmbedPart {
            part: 0,
            text: akari_markdown::one_line(text),
            rules: markdown(title),
        });
    }
    if let Some(text) = embed.description.as_deref().filter(|_| rules.description) {
        parts.push(EmbedPart {
            part: 1,
            text: text.into(),
            rules: markdown(EmbedText::Description { lists: rules.lists }),
        });
    }
    if rules.fields {
        for (index, field) in embed.fields.iter().enumerate() {
            let Ok(index) = u16::try_from(index) else {
                break;
            };
            let part = index.saturating_mul(2).saturating_add(2);
            parts.push(EmbedPart {
                part,
                text: akari_markdown::one_line(&field.name),
                rules: markdown(title),
            });
            parts.push(EmbedPart {
                part: part.saturating_add(1),
                text: (&*field.value).into(),
                rules: markdown(EmbedText::FieldValue { lists: rules.lists }),
            });
        }
    }
    parts
}

fn parse_part(part: &EmbedPart<'_>) -> Document {
    match part.rules {
        Some(rules) => akari_markdown::parse_embed(&part.text, rules),
        None => akari_markdown::plain(&part.text),
    }
}

fn part_fingerprint(part: &EmbedPart<'_>) -> u64 {
    static KEY: OnceLock<RandomState> = OnceLock::new();
    KEY.get_or_init(RandomState::new)
        .hash_one((&*part.text, part.rules))
}

// How a read found a message's document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Found {
    Cached,
    // Never parsed by a load: a live message, an edit, or one the store held during a load.
    EventParse,
    // A load parsed it, and the cache dropped it since.
    Miss,
}

struct Documents {
    cache: Cache<ContentKey>,
    // The cached documents a load parsed.
    prepared: HashSet<ContentKey>,
    // Those the cache dropped; starts over past the capacity, so an old one counts as an
    // event parse.
    dropped: HashSet<ContentKey>,
    capacity: usize,
}

impl Documents {
    fn insert(&mut self, key: ContentKey, source: u64, document: Arc<Document>, prepared: bool) {
        self.dropped.remove(&key);
        if prepared {
            self.prepared.insert(key);
        } else {
            self.prepared.remove(&key);
        }
        let Some(old) = self.cache.insert(key, source, document) else {
            return;
        };
        if self.prepared.remove(&old) {
            if self.dropped.len() >= self.capacity {
                self.dropped.clear();
            }
            self.dropped.insert(old);
        }
    }
}

// An account's parsed message contents. Pages and the disk cache's fill are parsed on the
// runtime before they enter a window (D56); everything else when it's read.
pub(crate) struct Contents {
    documents: Mutex<Documents>,
    embed_documents: Mutex<Cache<EmbedKey>>,
    parsed: AtomicU64,
    misses: AtomicU64,
    event_parses: AtomicU64,
    #[cfg(test)]
    prepared_on: Mutex<Vec<Option<String>>>,
}

// An update can change the content without an edit time. SipHash under a random key, so no
// one can craft two contents that collide.
fn fingerprint(message: &state::Message) -> u64 {
    static KEY: OnceLock<RandomState> = OnceLock::new();
    KEY.get_or_init(RandomState::new)
        .hash_one(&*message.content)
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
            documents: Mutex::new(Documents {
                cache: Cache::new(capacity),
                prepared: HashSet::new(),
                dropped: HashSet::new(),
                capacity,
            }),
            embed_documents: Mutex::new(Cache::new(capacity)),
            parsed: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            event_parses: AtomicU64::new(0),
            #[cfg(test)]
            prepared_on: Mutex::default(),
        })
    }

    pub(crate) fn document(&self, message: &state::Message) -> (Arc<Document>, Found) {
        let key = ContentKey::of(message);
        let source = fingerprint(message);
        if let Some(document) = lock(&self.documents).cache.get(&key, source) {
            return (document, Found::Cached);
        }
        let document = Arc::new(akari_markdown::parse(&message.content));
        let found = {
            let mut documents = lock(&self.documents);
            let found = if documents.dropped.contains(&key) {
                Found::Miss
            } else {
                Found::EventParse
            };
            documents.insert(key, source, document.clone(), false);
            found
        };
        self.parsed.fetch_add(1, Ordering::Relaxed);
        let counter = match found {
            Found::Miss => &self.misses,
            _ => &self.event_parses,
        };
        counter.fetch_add(1, Ordering::Relaxed);
        (document, found)
    }

    /// An embed's text as `part` says it parses: from the cache, which the page's preparing
    /// filled, or parsed now.
    pub(crate) fn embed_document(
        &self,
        message: &state::Message,
        embed: u8,
        part: &EmbedPart<'_>,
    ) -> Arc<Document> {
        let key = EmbedKey {
            id: message.id,
            embed,
            part: part.part,
        };
        let source = part_fingerprint(part);
        if let Some(document) = lock(&self.embed_documents).get(&key, source) {
            return document;
        }
        let document = Arc::new(parse_part(part));
        lock(&self.embed_documents).insert(key, source, document.clone());
        self.parsed.fetch_add(1, Ordering::Relaxed);
        self.event_parses.fetch_add(1, Ordering::Relaxed);
        document
    }

    // A page's embed texts, off the lock while they parse.
    fn prepare_embeds(&self, messages: &[Arc<state::Message>]) -> u64 {
        let mut missing = Vec::new();
        {
            let mut cache = lock(&self.embed_documents);
            for message in messages {
                for (index, embed) in message.embeds.iter().enumerate() {
                    let Ok(embed_index) = u8::try_from(index) else {
                        break;
                    };
                    let rules = state::embed_text_rules(message, embed);
                    for part in embed_parts(embed, rules) {
                        let key = EmbedKey {
                            id: message.id,
                            embed: embed_index,
                            part: part.part,
                        };
                        let source = part_fingerprint(&part);
                        if cache.get(&key, source).is_none() {
                            missing.push((key, source, part));
                        }
                    }
                }
            }
        }
        let parsed: Vec<_> = missing
            .iter()
            .map(|(_, _, part)| Arc::new(parse_part(part)))
            .collect();
        let mut cache = lock(&self.embed_documents);
        for ((key, source, _), document) in missing.iter().zip(parsed) {
            cache.insert(*key, *source, document);
        }
        missing.len() as u64
    }

    pub(crate) fn stats(&self) -> ContentStats {
        ContentStats {
            parsed: self.parsed.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            event_parses: self.event_parses.load(Ordering::Relaxed),
            cached: u32::try_from(lock(&self.documents).cache.len()).unwrap_or(u32::MAX),
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
            let mut documents = lock(&self.documents);
            messages
                .iter()
                .filter(|message| {
                    documents
                        .cache
                        .get(&ContentKey::of(message), fingerprint(message))
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
        let mut documents = lock(&self.documents);
        for (message, document) in missing.iter().zip(parsed) {
            documents.insert(
                ContentKey::of(message),
                fingerprint(message),
                document,
                true,
            );
        }
        drop(documents);
        let count = missing.len() as u64 + self.prepare_embeds(messages);
        self.parsed.fetch_add(count, Ordering::Relaxed);
        u32::try_from(count).unwrap_or(u32::MAX)
    }
}
