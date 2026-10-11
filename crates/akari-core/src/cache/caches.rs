use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use super::thread::WeakCacheThread;
use super::{CacheThread, DiskCache, LOG, delete_files, file};
use crate::model::UserId;

struct Place {
    directory: PathBuf,
    every: Duration,
}

#[derive(Default)]
struct Entry {
    // Kept running from a preload until an account takes it.
    preloaded: Option<CacheThread>,
    open: WeakCacheThread,
}

#[derive(Default)]
pub(crate) struct Caches {
    place: OnceLock<Place>,
    threads: Mutex<HashMap<UserId, Entry>>,
}

impl Caches {
    pub(crate) fn set(&self, directory: PathBuf, every: Duration) {
        if self.place.set(Place { directory, every }).is_err() {
            tracing::warn!(target: LOG, "the cache directory is already set");
        }
    }

    pub(crate) fn running(&self, user: UserId) -> Option<CacheThread> {
        let threads = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = threads.get(&user)?;
        entry.preloaded.clone().or_else(|| entry.open.upgrade())
    }

    pub(crate) fn preload(&self, user: UserId) {
        let Some(place) = self.place.get() else {
            return;
        };
        let mut threads = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = threads.entry(user).or_default();
        if entry.preloaded.is_none() && entry.open.upgrade().is_none() {
            let thread = CacheThread::start(file(&place.directory, user), user);
            entry.open = thread.downgrade();
            entry.preloaded = Some(thread);
        }
    }

    pub(crate) fn thread(&self, user: UserId) -> Option<CacheThread> {
        let place = self.place.get()?;
        let mut threads = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = threads.entry(user).or_default();
        if let Some(thread) = entry.preloaded.take() {
            return Some(thread);
        }
        if let Some(thread) = entry.open.upgrade() {
            return Some(thread);
        }
        let thread = CacheThread::start(file(&place.directory, user), user);
        entry.open = thread.downgrade();
        Some(thread)
    }

    pub(crate) fn account(self: &Arc<Self>, user: UserId) -> Option<DiskCache> {
        let every = self.place.get()?.every;
        Some(DiskCache {
            thread: self.thread(user)?,
            user,
            every,
            caches: Some(self.clone()),
        })
    }

    pub(crate) async fn forget(&self, user: UserId) {
        let Some(place) = self.place.get() else {
            return;
        };
        let running = self.running(user);
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&user);
        if let Some(thread) = running {
            thread.delete().await;
        }
        let path = file(&place.directory, user);
        let deleted = tokio::task::spawn_blocking(move || delete_files(&path)).await;
        if !matches!(deleted, Ok(Ok(()))) {
            tracing::warn!(target: LOG, "couldn't delete the cache");
        }
    }

    pub(crate) async fn clear(&self) {
        let Some(place) = self.place.get() else {
            return;
        };
        let running: BTreeSet<u64> = self
            .threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, entry)| entry.preloaded.is_some() || entry.open.upgrade().is_some())
            .map(|(user, _)| user.get())
            .collect();
        let directory = place.directory.clone();
        let cleared = tokio::task::spawn_blocking(move || clear(&directory, &running)).await;
        if !matches!(cleared, Ok(Ok(()))) {
            tracing::warn!(target: LOG, "couldn't clear the caches");
        }
    }
}

fn clear(directory: &std::path::Path, running: &BTreeSet<u64>) -> std::io::Result<()> {
    let mut users = BTreeSet::new();
    for entry in std::fs::read_dir(directory)? {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let stem = ["-wal", "-shm", ""]
            .iter()
            .find_map(|suffix| name.strip_suffix(suffix)?.strip_suffix(".sqlite"));
        if let Some(user) = stem.and_then(|stem| stem.parse::<u64>().ok())
            && !running.contains(&user)
        {
            users.insert(user);
        }
    }
    for user in users {
        delete_files(&file(directory, UserId::new(user)))?;
    }
    Ok(())
}
