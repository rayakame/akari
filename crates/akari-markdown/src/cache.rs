use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use crate::document::Document;

/// Parsed documents by key, the least recently used dropped beyond `capacity`.
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
    source_len: usize,
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

    /// `None` also when the stored document was parsed from a source of another length.
    pub fn get(&mut self, key: &K, source_len: usize) -> Option<Arc<Document>> {
        let i = *self.index.get(key)?;
        if self.entries[i].source_len != source_len {
            return None;
        }
        self.unlink(i);
        self.push_front(i);
        Some(self.entries[i].document.clone())
    }

    pub fn insert(&mut self, key: K, source_len: usize, document: Arc<Document>) {
        if self.capacity == 0 {
            return;
        }
        if let Some(&i) = self.index.get(&key) {
            let entry = &mut self.entries[i];
            entry.source_len = source_len;
            entry.document = document;
            self.unlink(i);
            self.push_front(i);
            return;
        }
        let i = if self.entries.len() < self.capacity {
            self.entries.push(Entry {
                key: key.clone(),
                source_len,
                document,
                newer: None,
                older: None,
            });
            self.entries.len() - 1
        } else {
            let Some(oldest) = self.tail else {
                return;
            };
            self.unlink(oldest);
            let entry = &mut self.entries[oldest];
            self.index.remove(&entry.key);
            entry.key = key.clone();
            entry.source_len = source_len;
            entry.document = document;
            oldest
        };
        self.index.insert(key, i);
        self.push_front(i);
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
