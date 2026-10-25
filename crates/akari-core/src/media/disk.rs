//! The disk tier: one directory per account with the files as Discord sent them and an index of
//! them in SQLite. Only its own thread touches either.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use tokio::sync::oneshot;

use super::url::Form;

const LOG: &str = "akari_core::media";
const INDEX: &str = "index.sqlite";
const FILES: &str = "files";
const TEMPORARY: &str = "tmp";
const SCHEMA: i32 = 1;
pub(crate) const LIMIT: u64 = 256 * 1024 * 1024;
// A file just handed out is being decoded; never evict it from under the host.
const GUARD: Duration = Duration::from_secs(60);

/// A file the index has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) animated: bool,
}

/// A downloaded file for the index, written to `temporary` first.
#[derive(Debug)]
pub(crate) struct NewFile {
    pub(crate) key: String,
    pub(crate) form: Form,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) animated: bool,
    /// Nothing larger exists for this key and form.
    pub(crate) full: bool,
    pub(crate) extension: &'static str,
    pub(crate) bytes: u64,
    pub(crate) temporary: PathBuf,
}

enum Command {
    Find {
        key: String,
        form: Form,
        needed: u32,
        reply: oneshot::Sender<Option<Entry>>,
    },
    Insert {
        file: Box<NewFile>,
        reply: oneshot::Sender<Option<Entry>>,
    },
    Forget {
        path: PathBuf,
    },
    Clear {
        reply: oneshot::Sender<()>,
    },
    Delete {
        reply: oneshot::Sender<()>,
    },
    #[cfg(test)]
    SetClock(i64),
    #[cfg(test)]
    Total(oneshot::Sender<u64>),
    #[cfg(test)]
    Close(oneshot::Sender<()>),
}

/// The account's image directory, served by its own thread until every clone is dropped.
#[derive(Clone)]
pub(crate) struct DiskTier {
    commands: mpsc::Sender<Command>,
    temporary: PathBuf,
}

impl DiskTier {
    pub(crate) fn start(directory: PathBuf, limit: u64) -> Self {
        static STARTED: AtomicU64 = AtomicU64::new(0);
        // Each start writes its downloads apart, so sweeping an earlier run's never races them.
        let run_name = format!(
            "{}-{}",
            std::process::id(),
            STARTED.fetch_add(1, Ordering::Relaxed)
        );
        let temporary = directory.join(TEMPORARY).join(&run_name);
        let (commands, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("akari-images".to_owned())
            .spawn(move || run(&directory, &run_name, limit, &receiver));
        if spawned.is_err() {
            tracing::warn!(target: LOG, "couldn't start the image thread");
        }
        Self {
            commands,
            temporary,
        }
    }

    /// Where downloads are written before `insert`; the caller creates it.
    pub(crate) fn temporary(&self) -> &Path {
        &self.temporary
    }

    async fn ask<T>(&self, command: impl FnOnce(oneshot::Sender<T>) -> Command) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(command(reply)).ok()?;
        answer.await.ok()
    }

    /// The smallest file of `key` in `form` whose longest side has `needed` pixels, or the
    /// source's own size. Marks it used.
    pub(crate) async fn find(&self, key: &str, form: Form, needed: u32) -> Option<Entry> {
        self.ask(|reply| Command::Find {
            key: key.to_owned(),
            form,
            needed,
            reply,
        })
        .await
        .flatten()
    }

    /// Moves the file into place and indexes it; `None` when it couldn't be kept.
    pub(crate) async fn insert(&self, file: NewFile) -> Option<Entry> {
        self.ask(|reply| Command::Insert {
            file: Box::new(file),
            reply,
        })
        .await
        .flatten()
    }

    /// Drops an entry whose file is gone.
    pub(crate) fn forget(&self, path: PathBuf) {
        let _ = self.commands.send(Command::Forget { path });
    }

    /// Deletes every file; the directory stays in use.
    pub(crate) async fn clear(&self) {
        self.ask(|reply| Command::Clear { reply }).await;
    }

    /// Deletes the directory; later calls find nothing and keep nothing.
    pub(crate) async fn delete(&self) {
        self.ask(|reply| Command::Delete { reply }).await;
    }

    #[cfg(test)]
    pub(crate) fn set_clock(&self, unix_seconds: i64) {
        let _ = self.commands.send(Command::SetClock(unix_seconds));
    }

    #[cfg(test)]
    pub(crate) async fn total(&self) -> u64 {
        self.ask(Command::Total).await.unwrap_or(0)
    }

    // Closes the index, as the thread's end would, without waiting for it.
    #[cfg(test)]
    pub(crate) async fn close(&self) {
        self.ask(Command::Close).await;
    }
}

struct Index {
    conn: Connection,
    directory: PathBuf,
    limit: u64,
    total: u64,
    #[cfg(test)]
    clock: Option<i64>,
}

fn run(directory: &Path, run_name: &str, limit: u64, commands: &mpsc::Receiver<Command>) {
    let mut index = Index::open_or_rebuild(directory, run_name, limit);
    while let Ok(command) = commands.recv() {
        let Some(open) = index.as_mut() else {
            answer_closed(command);
            continue;
        };
        match command {
            Command::Find {
                key,
                form,
                needed,
                reply,
            } => {
                let _ = reply.send(open.find(&key, form, needed));
            }
            Command::Insert { file, reply } => {
                let _ = reply.send(open.insert(&file));
            }
            Command::Forget { path } => open.forget(&path),
            Command::Clear { reply } => {
                open.clear();
                let _ = reply.send(());
            }
            Command::Delete { reply } => {
                drop(index.take());
                if remove(directory).is_err() {
                    tracing::warn!(target: LOG, "couldn't delete the image directory");
                }
                let _ = reply.send(());
            }
            #[cfg(test)]
            Command::SetClock(now) => open.clock = Some(now),
            #[cfg(test)]
            Command::Total(reply) => {
                let _ = reply.send(open.total);
            }
            #[cfg(test)]
            Command::Close(reply) => {
                drop(index.take());
                let _ = reply.send(());
            }
        }
    }
}

// After a delete, or when the directory can't be used.
fn answer_closed(command: Command) {
    match command {
        Command::Find { reply, .. } | Command::Insert { reply, .. } => {
            let _ = reply.send(None);
        }
        Command::Clear { reply } | Command::Delete { reply } => {
            let _ = reply.send(());
        }
        Command::Forget { .. } => {}
        #[cfg(test)]
        Command::SetClock(_) => {}
        #[cfg(test)]
        Command::Total(reply) => {
            let _ = reply.send(0);
        }
        #[cfg(test)]
        Command::Close(reply) => {
            let _ = reply.send(());
        }
    }
}

fn remove(directory: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(directory) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

fn form_code(form: Form) -> i64 {
    match form {
        Form::Still => 0,
        Form::WebP => 1,
        Form::Gif => 2,
        Form::Apng => 3,
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

// The key's digest names its files, so no key text reaches a path.
fn file_name(key: &str, form: Form, longest: u32, extension: &str) -> String {
    let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, key.as_bytes());
    let hex: String = digest.as_ref()[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        "{}/{hex}-{}-{longest}.{extension}",
        &hex[..2],
        form_code(form)
    )
}

impl Index {
    fn open_or_rebuild(directory: &Path, run_name: &str, limit: u64) -> Option<Self> {
        match Self::open(directory, run_name, limit) {
            Ok(index) => return Some(index),
            Err(err) => {
                tracing::info!(target: LOG, error = %err, "image index discarded");
            }
        }
        if remove(directory).is_err() {
            tracing::warn!(target: LOG, "couldn't delete the image directory");
            return None;
        }
        Self::open(directory, run_name, limit)
            .inspect_err(|err| tracing::warn!(target: LOG, error = %err, "no image index"))
            .ok()
    }

    fn open(directory: &Path, run_name: &str, limit: u64) -> Result<Self, rusqlite::Error> {
        let io = |_| rusqlite::Error::InvalidPath(directory.to_path_buf());
        fs::create_dir_all(directory.join(FILES)).map_err(io)?;
        // Downloads of earlier runs that never made it into the index.
        if let Ok(runs) = fs::read_dir(directory.join(TEMPORARY)) {
            for run in runs.flatten() {
                if run.file_name() != run_name {
                    let _ = fs::remove_dir_all(run.path());
                }
            }
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(directory.join(INDEX), flags)?;
        conn.busy_timeout(Duration::from_millis(500))?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let tables: i64 =
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))?;
        if version == 0 && tables == 0 {
            conn.execute_batch(
                "CREATE TABLE images (
                    key TEXT NOT NULL,
                    form INTEGER NOT NULL,
                    longest INTEGER NOT NULL,
                    width INTEGER NOT NULL,
                    height INTEGER NOT NULL,
                    animated INTEGER NOT NULL,
                    full INTEGER NOT NULL,
                    file TEXT NOT NULL UNIQUE,
                    bytes INTEGER NOT NULL,
                    used INTEGER NOT NULL,
                    PRIMARY KEY (key, form, longest)
                ) WITHOUT ROWID;
                CREATE INDEX images_used ON images (used);",
            )?;
            conn.pragma_update(None, "user_version", SCHEMA)?;
        } else if version != SCHEMA {
            return Err(rusqlite::Error::InvalidQuery);
        } else {
            let check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
            if check != "ok" {
                return Err(rusqlite::Error::InvalidQuery);
            }
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let total: i64 =
            conn.query_row("SELECT coalesce(sum(bytes), 0) FROM images", [], |row| {
                row.get(0)
            })?;
        let mut index = Self {
            conn,
            directory: directory.to_path_buf(),
            limit,
            total: u64::try_from(total).unwrap_or(0),
            #[cfg(test)]
            clock: None,
        };
        index.sweep();
        Ok(index)
    }

    fn now(&self) -> i64 {
        #[cfg(test)]
        if let Some(now) = self.clock {
            return now;
        }
        now_seconds()
    }

    fn path(&self, file: &str) -> PathBuf {
        self.directory.join(FILES).join(file)
    }

    // Files a crash left between the rename and the index.
    fn sweep(&mut self) {
        let known: std::collections::HashSet<String> = match self
            .conn
            .prepare("SELECT file FROM images")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<_, _>>()
            }) {
            Ok(known) => known,
            Err(_) => return,
        };
        let Ok(folders) = fs::read_dir(self.directory.join(FILES)) else {
            return;
        };
        let mut swept = 0;
        for folder in folders.flatten() {
            let Ok(files) = fs::read_dir(folder.path()) else {
                continue;
            };
            for file in files.flatten() {
                let name = format!(
                    "{}/{}",
                    folder.file_name().to_string_lossy(),
                    file.file_name().to_string_lossy()
                );
                if !known.contains(&name) && fs::remove_file(file.path()).is_ok() {
                    swept += 1;
                }
            }
        }
        if swept > 0 {
            tracing::info!(target: LOG, swept, "unindexed images deleted");
        }
    }

    fn find(&mut self, key: &str, form: Form, needed: u32) -> Option<Entry> {
        let found = self
            .conn
            .query_row(
                "SELECT file, width, height, animated, longest FROM images
                 WHERE key = ?1 AND form = ?2 AND (longest >= ?3 OR full = 1)
                 ORDER BY longest LIMIT 1",
                params![key, form_code(form), needed],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, u32>(4)?,
                    ))
                },
            )
            .optional()
            .ok()
            .flatten()?;
        let (file, width, height, animated, longest) = found;
        let now = self.now();
        let _ = self.conn.execute(
            "UPDATE images SET used = ?1 WHERE key = ?2 AND form = ?3 AND longest = ?4",
            params![now, key, form_code(form), longest],
        );
        Some(Entry {
            path: self.path(&file),
            width,
            height,
            animated,
        })
    }

    fn insert(&mut self, file: &NewFile) -> Option<Entry> {
        let longest = file.width.max(file.height);
        let name = file_name(&file.key, file.form, longest, file.extension);
        let path = self.path(&name);
        let placed = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::rename(&file.temporary, &path));
        if placed.is_err() {
            let _ = fs::remove_file(&file.temporary);
            tracing::warn!(target: LOG, "couldn't keep an image");
            return None;
        }
        let replaced: Option<i64> = self
            .conn
            .query_row(
                "SELECT bytes FROM images WHERE key = ?1 AND form = ?2 AND longest = ?3",
                params![file.key, form_code(file.form), longest],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        let written = self.conn.execute(
            "INSERT OR REPLACE INTO images
             (key, form, longest, width, height, animated, full, file, bytes, used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                file.key,
                form_code(file.form),
                longest,
                file.width,
                file.height,
                file.animated,
                file.full,
                name,
                i64::try_from(file.bytes).unwrap_or(i64::MAX),
                self.now(),
            ],
        );
        if written.is_err() {
            let _ = fs::remove_file(&path);
            tracing::warn!(target: LOG, "couldn't index an image");
            return None;
        }
        self.total = self
            .total
            .saturating_sub(replaced.map_or(0, |bytes| u64::try_from(bytes).unwrap_or(0)))
            .saturating_add(file.bytes);
        self.evict();
        Some(Entry {
            path,
            width: file.width,
            height: file.height,
            animated: file.animated,
        })
    }

    // Least recently used first, down to 90 % of the limit; nothing used within the guard.
    fn evict(&mut self) {
        if self.total <= self.limit {
            return;
        }
        let target = self.limit / 10 * 9;
        let cutoff = self.now() - i64::try_from(GUARD.as_secs()).unwrap_or(0);
        let mut evicted = 0;
        while self.total > target {
            let oldest: Vec<(String, i64, i64, String, i64)> = match self
                .conn
                .prepare(
                    "SELECT key, form, longest, file, bytes FROM images
                     WHERE used < ?1 ORDER BY used LIMIT 64",
                )
                .and_then(|mut statement| {
                    statement
                        .query_map([cutoff], |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        })?
                        .collect::<Result<_, _>>()
                }) {
                Ok(oldest) => oldest,
                Err(_) => break,
            };
            if oldest.is_empty() {
                break;
            }
            for (key, form, longest, file, bytes) in oldest {
                if self.total <= target {
                    break;
                }
                let deleted = self.conn.execute(
                    "DELETE FROM images WHERE key = ?1 AND form = ?2 AND longest = ?3",
                    params![key, form, longest],
                );
                if deleted.is_ok() {
                    let _ = fs::remove_file(self.path(&file));
                    self.total = self.total.saturating_sub(u64::try_from(bytes).unwrap_or(0));
                    evicted += 1;
                }
            }
        }
        tracing::debug!(target: LOG, evicted, total = self.total, "images evicted");
    }

    fn forget(&mut self, path: &Path) {
        let Ok(relative) = path.strip_prefix(self.directory.join(FILES)) else {
            return;
        };
        let name = relative.to_string_lossy().into_owned();
        let bytes: Option<i64> = self
            .conn
            .query_row(
                "DELETE FROM images WHERE file = ?1 RETURNING bytes",
                [&name],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if let Some(bytes) = bytes {
            self.total = self.total.saturating_sub(u64::try_from(bytes).unwrap_or(0));
        }
    }

    fn clear(&mut self) {
        let _ = self.conn.execute("DELETE FROM images", []);
        self.total = 0;
        let files = self.directory.join(FILES);
        let _ = fs::remove_dir_all(&files);
        let _ = fs::create_dir_all(&files);
    }
}

#[cfg(test)]
mod tests;
