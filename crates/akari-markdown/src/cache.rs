use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use crate::document::Document;

/// Parsed documents by key, the least recently used dropped beyond `capacity`. Each is stored
/// with a fingerprint of its source, such as a hash, which a lookup must match.
#[derive(Debug)]
pub struct Cache<K> {
    capacity: usize,
    index: HashMap<K, usize>,
    entries: Vec<Entry<K>>,
    head: Option<usize>,
    tail: Option<usize>,
}

#[derive(Debug)]
struct Entry<K> {
    key: K,
    fingerprint: u64,
    document: Arc<Document>,
    newer: Option<usize>,
    older: Option<usize>,
}

impl<K: Hash + Eq + Clone> Cache<K> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            index: HashMap::with_capacity(capacity.min(4096)),
            entries: Vec::with_capacity(capacity.min(4096)),
            head: None,
            tail: None,
        }
    }

    /// `None` also when the stored document was parsed from a source with another fingerprint.
    pub fn get(&mut self, key: &K, fingerprint: u64) -> Option<Arc<Document>> {
        let i = *self.index.get(key)?;
        if self.entries[i].fingerprint != fingerprint {
            return None;
        }
        self.unlink(i);
        self.push_front(i);
        Some(self.entries[i].document.clone())
    }

    /// Returns the key it dropped to make room, if any.
    pub fn insert(&mut self, key: K, fingerprint: u64, document: Arc<Document>) -> Option<K> {
        if self.capacity == 0 {
            return None;
        }
        if let Some(&i) = self.index.get(&key) {
            let entry = &mut self.entries[i];
            entry.fingerprint = fingerprint;
            entry.document = document;
            self.unlink(i);
            self.push_front(i);
            return None;
        }
        let mut dropped = None;
        let i = if self.entries.len() < self.capacity {
            self.entries.push(Entry {
                key: key.clone(),
                fingerprint,
                document,
                newer: None,
                older: None,
            });
            self.entries.len() - 1
        } else {
            let oldest = self.tail?;
            self.unlink(oldest);
            let entry = &mut self.entries[oldest];
            self.index.remove(&entry.key);
            dropped = Some(std::mem::replace(&mut entry.key, key.clone()));
            entry.fingerprint = fingerprint;
            entry.document = document;
            oldest
        };
        self.index.insert(key, i);
        self.push_front(i);
        dropped
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn unlink(&mut self, i: usize) {
        let (newer, older) = (self.entries[i].newer, self.entries[i].older);
        match newer {
            Some(n) => self.entries[n].older = older,
            None => self.head = older,
        }
        match older {
            Some(o) => self.entries[o].newer = newer,
            None => self.tail = newer,
        }
        self.entries[i].newer = None;
        self.entries[i].older = None;
    }

    fn push_front(&mut self, i: usize) {
        self.entries[i].older = self.head;
        if let Some(h) = self.head {
            self.entries[h].newer = Some(i);
        }
        self.head = Some(i);
        if self.tail.is_none() {
            self.tail = Some(i);
        }
    }
}
