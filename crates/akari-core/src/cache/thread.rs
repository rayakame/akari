use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::Instant;

use tokio::sync::oneshot;

use super::snapshot::{Flushed, Snapshot, Written, flush};
use super::{Cache, CacheError, LOG, Opened, delete_files};
use crate::model::{ChannelId, UserId};
use crate::state::Message;
use crate::state::apply::Entities;

enum Command {
    ReadEntities(oneshot::Sender<Entities>),
    ReadWindow(ChannelId, oneshot::Sender<Vec<Arc<Message>>>),
    Flush(Box<Snapshot>, oneshot::Sender<Option<Flushed>>),
    Delete(oneshot::Sender<()>),
    #[cfg(test)]
    Hold(mpsc::Receiver<()>),
    #[cfg(test)]
    FailNextWrite,
    #[cfg(test)]
    Checkpoint(oneshot::Sender<()>),
    #[cfg(test)]
    ThreadName(oneshot::Sender<Option<String>>),
}

/// The thread that owns one account's cache file; every SQLite call runs on it, never on a
/// runtime worker. Cheap to clone. Once every clone is dropped, the thread finishes the
/// queued commands and stops. Without a usable file every call answers `None`.
#[derive(Clone)]
pub(crate) struct CacheThread {
    commands: mpsc::Sender<Command>,
}

impl CacheThread {
    pub(crate) fn start(path: PathBuf, user: UserId) -> Self {
        let (commands, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("akari-cache".to_owned())
            .spawn(move || run(&path, user, &receiver));
        if spawned.is_err() {
            tracing::warn!(target: LOG, "couldn't start the cache thread");
        }
        Self { commands }
    }

    async fn ask<T>(&self, command: impl FnOnce(oneshot::Sender<T>) -> Command) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(command(reply)).ok()?;
        answer.await.ok()
    }

    /// The cached state at open, or as the last flush wrote it.
    pub(crate) async fn read_entities(&self) -> Option<Entities> {
        self.ask(Command::ReadEntities).await
    }

    /// The channel's cached messages, oldest first.
    pub(crate) async fn read_window(&self, channel: ChannelId) -> Option<Vec<Arc<Message>>> {
        self.ask(|reply| Command::ReadWindow(channel, reply)).await
    }

    /// `None` when the write failed or there is no cache.
    pub(crate) async fn flush(&self, snapshot: Snapshot) -> Option<Flushed> {
        self.ask(|reply| Command::Flush(Box::new(snapshot), reply))
            .await
            .flatten()
    }

    /// Deletes the file and stops the thread; later calls answer `None`.
    pub(crate) async fn delete(&self) {
        self.ask(Command::Delete).await;
    }

    #[cfg(test)]
    pub(crate) fn hold(&self) -> mpsc::Sender<()> {
        let (release, held) = mpsc::channel();
        let _ = self.commands.send(Command::Hold(held));
        release
    }

    #[cfg(test)]
    pub(crate) fn fail_next_write(&self) {
        let _ = self.commands.send(Command::FailNextWrite);
    }

    #[cfg(test)]
    pub(crate) async fn checkpoint(&self) {
        self.ask(Command::Checkpoint).await;
    }

    #[cfg(test)]
    pub(crate) async fn thread_name(&self) -> Option<String> {
        self.ask(Command::ThreadName).await.flatten()
    }
}

fn run(path: &std::path::Path, user: UserId, commands: &mpsc::Receiver<Command>) {
    let started = Instant::now();
    let Ok(Opened { mut cache, .. }) = Cache::open_or_rebuild(path, user) else {
        return;
    };
    let mut written = match cache.read_entities() {
        Ok(read) => {
            log_opened(&cache, read.skipped, started);
            Written::read(read.value)
        }
        Err(err) => {
            tracing::info!(target: LOG, reason = kind(&err), "cache not read");
            Written::default()
        }
    };
    for command in commands {
        match command {
            Command::ReadEntities(reply) => {
                let _ = reply.send(written.entities.clone());
            }
            Command::ReadWindow(channel, reply) => {
                let messages = match cache.read_window(channel) {
                    Ok(read) => read.value,
                    Err(err) => {
                        tracing::info!(target: LOG, reason = kind(&err), "cache window not read");
                        Vec::new()
                    }
                };
                let _ = reply.send(messages);
            }
            Command::Flush(snapshot, reply) => {
                let started = Instant::now();
                let flushed = match flush(&mut cache, &mut written, *snapshot) {
                    Ok(flushed) => {
                        if flushed != Flushed::default() {
                            tracing::info!(
                                target: LOG,
                                guilds = flushed.guilds,
                                channels = flushed.channels,
                                windows = flushed.windows,
                                messages = flushed.messages,
                                other = flushed.other,
                                deleted = flushed.deleted,
                                write_ms = crate::millis(started.elapsed()),
                                file_kib = cache.file_kib(),
                                "cache written"
                            );
                        }
                        Some(flushed)
                    }
                    Err(err) => {
                        tracing::warn!(target: LOG, reason = kind(&err), "cache write failed");
                        None
                    }
                };
                let _ = reply.send(flushed);
            }
            Command::Delete(reply) => {
                drop(cache);
                if delete_files(path).is_err() {
                    tracing::warn!(target: LOG, "couldn't delete the cache");
                }
                let _ = reply.send(());
                return;
            }
            #[cfg(test)]
            Command::Hold(held) => {
                let _ = held.recv();
            }
            #[cfg(test)]
            Command::FailNextWrite => cache.fail_next_write = true,
            #[cfg(test)]
            Command::Checkpoint(reply) => {
                let _ = cache.checkpoint();
                let _ = reply.send(());
            }
            #[cfg(test)]
            Command::ThreadName(reply) => {
                let _ = reply.send(std::thread::current().name().map(str::to_owned));
            }
        }
    }
    if cache.checkpoint().is_err() {
        tracing::info!(target: LOG, "cache checkpoint failed");
    }
}

fn log_opened(cache: &Cache, skipped: usize, started: Instant) {
    let [guilds, channels, windows, messages] = cache.counts().unwrap_or_default();
    tracing::info!(
        target: LOG,
        guilds,
        channels,
        windows,
        messages,
        skipped,
        read_ms = crate::millis(started.elapsed()),
        file_kib = cache.file_kib(),
        "cache opened"
    );
}

fn kind(err: &CacheError) -> &'static str {
    match err {
        CacheError::Corrupt => "corrupt",
        CacheError::OtherSchema => "other_schema",
        CacheError::OtherUser => "other_user",
        CacheError::Busy => "busy",
        CacheError::Io => "io",
        CacheError::Sqlite(_) => "sqlite",
    }
}
