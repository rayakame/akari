//! Where each account's images live, and which directories an open account is using.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use super::disk::{DiskTier, LIMIT};
use crate::model::UserId;

const LOG: &str = "akari_core::media";

#[derive(Default)]
pub(crate) struct ImageDirs {
    directory: OnceLock<PathBuf>,
    running: Mutex<HashMap<UserId, Weak<DiskTier>>>,
}

impl ImageDirs {
    pub(crate) fn set(&self, directory: PathBuf) {
        if self.directory.set(directory).is_err() {
            tracing::warn!(target: LOG, "the image directory is already set");
        }
    }

    fn running(&self) -> std::sync::MutexGuard<'_, HashMap<UserId, Weak<DiskTier>>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `user`'s directory, shared with an account already using it; `None` without a directory.
    pub(crate) fn tier(&self, user: UserId) -> Option<Arc<DiskTier>> {
        let directory = self.directory.get()?;
        let mut running = self.running();
        if let Some(tier) = running.get(&user).and_then(Weak::upgrade) {
            return Some(tier);
        }
        let tier = Arc::new(DiskTier::start(
            directory.join(user.get().to_string()),
            LIMIT,
        ));
        running.insert(user, Arc::downgrade(&tier));
        Some(tier)
    }

    /// Deletes `user`'s images, also while the account is open.
    pub(crate) async fn forget(&self, user: UserId) {
        let Some(directory) = self.directory.get() else {
            return;
        };
        let open = self.running().remove(&user).and_then(|tier| tier.upgrade());
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
        let open: BTreeSet<String> = self
            .running()
            .iter()
            .filter(|(_, tier)| tier.strong_count() > 0)
            .map(|(user, _)| user.get().to_string())
            .collect();
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
