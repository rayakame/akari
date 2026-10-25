//! Where each account's images live, and which directories an open account is using.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use super::disk::{DiskTier, LIMIT};
use crate::model::UserId;

const LOG: &str = "akari_core::media";

#[derive(Default)]
struct Registry {
    running: HashMap<UserId, Weak<DiskTier>>,
    // Accounts open in this process, whether or not they asked for an image yet.
    open: HashMap<UserId, usize>,
    // Forgotten in this process: only the host opening the account again brings images back.
    forgotten: HashSet<UserId>,
}

#[derive(Default)]
pub(crate) struct ImageDirs {
    directory: OnceLock<PathBuf>,
    registry: Mutex<Registry>,
    // Held while a directory is deleted, so no tier starts in it meanwhile.
    lifecycle: tokio::sync::Mutex<()>,
}

impl ImageDirs {
    pub(crate) fn set(&self, directory: PathBuf) {
        if self.directory.set(directory).is_err() {
            tracing::warn!(target: LOG, "the image directory is already set");
        }
    }

    fn registry(&self) -> std::sync::MutexGuard<'_, Registry> {
        self.registry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// An account of `user` opened: its images are kept, also before its first ask, and a
    /// forget from before doesn't apply to it.
    pub(crate) fn open(&self, user: UserId) {
        let mut registry = self.registry();
        registry.forgotten.remove(&user);
        *registry.open.entry(user).or_default() += 1;
    }

    pub(crate) fn release(&self, user: UserId) {
        let mut registry = self.registry();
        if let Some(count) = registry.open.get_mut(&user) {
            *count -= 1;
            if *count == 0 {
                registry.open.remove(&user);
            }
        }
    }

    /// `user`'s directory, shared with an account already using it; `None` without a directory,
    /// or for a user forgotten since the account was opened.
    pub(crate) async fn tier(&self, user: UserId) -> Option<Arc<DiskTier>> {
        let directory = self.directory.get()?;
        let _not_deleting = self.lifecycle.lock().await;
        let mut registry = self.registry();
        if registry.forgotten.contains(&user) {
            return None;
        }
        if let Some(tier) = registry.running.get(&user).and_then(Weak::upgrade) {
            return Some(tier);
        }
        let tier = Arc::new(DiskTier::start(
            directory.join(user.get().to_string()),
            LIMIT,
        ));
        registry.running.insert(user, Arc::downgrade(&tier));
        Some(tier)
    }

    /// Deletes `user`'s images, also while the account is open; it gets none until the host
    /// opens it again.
    pub(crate) async fn forget(&self, user: UserId) {
        let Some(directory) = self.directory.get() else {
            return;
        };
        let _deleting = self.lifecycle.lock().await;
        let open = {
            let mut registry = self.registry();
            registry.forgotten.insert(user);
            registry
                .running
                .remove(&user)
                .and_then(|tier| tier.upgrade())
        };
        match open {
            Some(tier) => tier.delete().await,
            None => {
                let path = directory.join(user.get().to_string());
                let deleted =
                    tokio::task::spawn_blocking(move || match std::fs::remove_dir_all(&path) {
                        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
                        _ => Ok(()),
                    })
                    .await;
                if !matches!(deleted, Ok(Ok(()))) {
                    tracing::warn!(target: LOG, "couldn't delete the images");
                }
            }
        }
    }

    /// Deletes the images of every account that isn't open.
    pub(crate) async fn clear(&self) {
        let Some(directory) = self.directory.get().cloned() else {
            return;
        };
        let _deleting = self.lifecycle.lock().await;
        let open: BTreeSet<String> = {
            let registry = self.registry();
            registry
                .open
                .keys()
                .chain(
                    registry
                        .running
                        .iter()
                        .filter(|(_, tier)| tier.strong_count() > 0)
                        .map(|(user, _)| user),
                )
                .map(|user| user.get().to_string())
                .collect()
        };
        let cleared = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            for entry in std::fs::read_dir(&directory)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.parse::<u64>().is_ok() && !open.contains(&name) {
                    std::fs::remove_dir_all(entry.path())?;
                }
            }
            Ok(())
        })
        .await;
        if !matches!(cleared, Ok(Ok(()))) {
            tracing::warn!(target: LOG, "couldn't clear the images");
        }
    }
}
