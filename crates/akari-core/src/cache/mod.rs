//! The disk cache: one SQLite file per account with what a cold start draws before the gateway
//! connects. Only its own thread touches the file.

mod caches;
mod flusher;
mod rows;
mod schema;
mod snapshot;
mod thread;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};

pub(crate) use self::caches::Caches;
pub(crate) use self::flusher::Flusher;
use self::rows::{
    ChannelRow, CurrentUserRow, DisplayRow, FolderRow, GuildRow, GuildSettingsRow, MemberRow,
    MessageRow, ReadStateRow, UserRow,
};
pub(crate) use self::snapshot::Snapshot;
pub(crate) use self::thread::CacheThread;
use crate::model::{ChannelId, GuildId, Snowflake, UserId};
use crate::state::apply::Entities;
use crate::state::{
    AgeAgreements, Channel, CurrentUser, Delivery, DisplaySettings, FolderEntry, Guild,
    GuildSettings, Member, Message, ReadState, User,
};

const LOG: &str = "akari_core::cache";

pub(crate) struct DiskCache {
    pub(crate) thread: CacheThread,
    // Whose file `thread` has open.
    pub(crate) user: UserId,
    pub(crate) every: Duration,
    // Without it the cache stays on `user`'s file whatever READY says.
    pub(crate) caches: Option<Arc<Caches>>,
}

// The DM settings' row: snowflakes are never 0.
const DM_SETTINGS: u64 = 0;
const FOLDERS: &str = "folders";
const NOTIFICATION_FLAGS: &str = "notification_flags";
const DISPLAY: &str = "display";
const AGE_AGREEMENTS: &str = "age_agreements";

pub(crate) fn file(directory: &Path, user: UserId) -> PathBuf {
    directory.join(format!("{}.sqlite", user.get()))
}

// SQLite stores integers signed; snowflakes stay below 2^63 until 2084, and the cast keeps
// every bit either way.
fn sql_id(id: u64) -> i64 {
    id.cast_signed()
}

fn row_id(id: i64) -> u64 {
    id.cast_unsigned()
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CacheError {
    #[error("the cache file isn't a valid database")]
    Corrupt,
    #[error("the cache file has another schema version")]
    OtherSchema,
    #[error("the cache file belongs to another account")]
    OtherUser,
    #[error("the cache file is in use")]
    Busy,
    #[error("the cache file can't be created or written")]
    Io,
    #[error("SQLite failed")]
    Sqlite(#[source] rusqlite::Error),
}

impl From<rusqlite::Error> for CacheError {
    fn from(err: rusqlite::Error) -> Self {
        match err.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Busy,
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => Self::Corrupt,
            Some(
                ErrorCode::CannotOpen
                | ErrorCode::PermissionDenied
                | ErrorCode::ReadOnly
                | ErrorCode::DiskFull,
            ) => Self::Io,
            _ => Self::Sqlite(err),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Discarded {
    Corrupt,
    OtherSchema,
    OtherUser,
    Unreadable,
}

impl Discarded {
    fn name(self) -> &'static str {
        match self {
            Self::Corrupt => "corrupt",
            Self::OtherSchema => "other_schema",
            Self::OtherUser => "other_user",
            Self::Unreadable => "unreadable",
        }
    }
}

// No cache this session; the file, if any, stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unavailable {
    Busy,
    Io,
}

impl Unavailable {
    fn name(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Io => "io",
        }
    }
}

pub(crate) struct Opened {
    pub(crate) cache: Cache,
    pub(crate) discarded: Option<Discarded>,
}

pub(crate) struct Read<T> {
    pub(crate) value: T,
    pub(crate) skipped: usize,
}

pub(crate) struct Cache {
    conn: Connection,
    #[cfg(test)]
    pub(crate) fail_next_write: bool,
    #[cfg(test)]
    pub(crate) fail_next_checkpoint: bool,
}

impl Cache {
    pub(crate) fn open(path: &Path, user: UserId) -> Result<Self, CacheError> {
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).map_err(|_| CacheError::Io)?;
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        conn.busy_timeout(Duration::from_millis(500))?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let tables: i64 =
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))?;
        if version == 0 && tables == 0 {
            // Before the first table, or it can't be turned on.
            conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
            schema::create(&conn, user)?;
        } else if version != schema::CURRENT {
            return Err(CacheError::OtherSchema);
        } else {
            let check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
            if check != "ok" {
                return Err(CacheError::Corrupt);
            }
            let owner: Option<String> = conn
                .query_row("SELECT value FROM meta WHERE key = 'user_id'", [], |row| {
                    row.get(0)
                })
                .optional()?;
            if owner != Some(user.get().to_string()) {
                return Err(CacheError::OtherUser);
            }
        }
        let journal: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        if !journal.eq_ignore_ascii_case("wal") {
            tracing::info!(target: LOG, reason = "journal_mode", "cache not in WAL mode");
        }
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Deleted message text is overwritten in the file, not left in free pages.
        conn.pragma_update(None, "secure_delete", "ON")?;
        conn.pragma_update(None, "cache_size", -2048)?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        // Another process holding the file shows here rather than at the first flush.
        conn.execute_batch("BEGIN IMMEDIATE; COMMIT;")?;
        Ok(Self {
            conn,
            #[cfg(test)]
            fail_next_write: false,
            #[cfg(test)]
            fail_next_checkpoint: false,
        })
    }

    pub(crate) fn open_or_rebuild(path: &Path, user: UserId) -> Result<Opened, Unavailable> {
        let discarded = match Self::open(path, user) {
            Ok(cache) => {
                return Ok(Opened {
                    cache,
                    discarded: None,
                });
            }
            Err(CacheError::Busy) => return Err(unavailable(Unavailable::Busy)),
            Err(CacheError::Io) => return Err(unavailable(Unavailable::Io)),
            Err(CacheError::Corrupt) => Discarded::Corrupt,
            Err(CacheError::OtherSchema) => Discarded::OtherSchema,
            Err(CacheError::OtherUser) => Discarded::OtherUser,
            Err(CacheError::Sqlite(_)) => Discarded::Unreadable,
        };
        tracing::info!(target: LOG, reason = discarded.name(), "cache discarded");
        if delete_files(path).is_err() {
            return Err(unavailable(Unavailable::Io));
        }
        match Self::open(path, user) {
            Ok(cache) => Ok(Opened {
                cache,
                discarded: Some(discarded),
            }),
            Err(CacheError::Busy) => Err(unavailable(Unavailable::Busy)),
            Err(_) => Err(unavailable(Unavailable::Io)),
        }
    }

    pub(crate) fn read_entities(&self) -> Result<Read<Entities>, CacheError> {
        let mut entities = Entities::default();
        let mut skipped = 0;
        let mut users = HashMap::new();
        skipped += self.each("SELECT id, data FROM current_user", |_, data| {
            let row: CurrentUserRow = serde_json::from_str(data).ok()?;
            entities.current_user = Some(Arc::new(CurrentUser::from(row)));
            Some(())
        })?;
        skipped += self.each("SELECT id, data FROM users", |_, data| {
            let user = User::from(serde_json::from_str::<UserRow>(data).ok()?);
            users.insert(user.id, Arc::new(user));
            Some(())
        })?;
        entities.users = users;
        skipped += self.each("SELECT id, data FROM guilds", |id, data| {
            let row: GuildRow = serde_json::from_str(data).ok()?;
            let id = GuildId::new(id);
            entities.guilds.insert(id, Arc::new(row.into_guild(id)));
            Some(())
        })?;
        skipped += self.each("SELECT guild_id, data FROM members", |id, data| {
            let row: MemberRow = serde_json::from_str(data).ok()?;
            let id = GuildId::new(id);
            entities.members.insert(id, Arc::new(row.into_member(id)));
            Some(())
        })?;
        skipped += self.each("SELECT id, data FROM channels", |id, data| {
            let row: ChannelRow = serde_json::from_str(data).ok()?;
            let id = ChannelId::new(id);
            entities.channels.insert(id, Arc::new(row.into_channel(id)));
            Some(())
        })?;
        let mut statement = self.conn.prepare("SELECT id FROM unavailable_guilds")?;
        for id in statement.query_map([], |row| row.get::<_, i64>(0))? {
            entities.unavailable.insert(Snowflake::new(row_id(id?)));
        }
        skipped += self.each("SELECT guild_id, data FROM guild_settings", |id, data| {
            let row: GuildSettingsRow = serde_json::from_str(data).ok()?;
            let guild = (id != DM_SETTINGS).then(|| GuildId::new(id));
            entities
                .settings
                .guilds
                .insert(guild, Arc::new(row.into_settings()));
            Some(())
        })?;
        skipped += self.each("SELECT channel_id, data FROM read_states", |id, data| {
            let row: ReadStateRow = serde_json::from_str(data).ok()?;
            entities
                .read_states
                .insert(ChannelId::new(id), row.into_read_state());
            Some(())
        })?;
        let mut statement = self.conn.prepare("SELECT key, data FROM user_settings")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, data) = row?;
            let settings = &mut entities.settings;
            let read = match key.as_str() {
                FOLDERS => serde_json::from_str::<Vec<FolderRow>>(&data)
                    .ok()
                    .map(|rows| {
                        settings.folders = Some(rows.into_iter().map(FolderEntry::from).collect())
                    }),
                NOTIFICATION_FLAGS => serde_json::from_str::<u64>(&data)
                    .ok()
                    .map(|flags| settings.notification_flags = flags),
                DISPLAY => serde_json::from_str::<DisplayRow>(&data)
                    .ok()
                    .map(|row| settings.display = row.into()),
                AGE_AGREEMENTS => serde_json::from_str::<Vec<u64>>(&data).ok().map(|guilds| {
                    settings.age_agreements =
                        Arc::new(guilds.into_iter().map(GuildId::new).collect());
                }),
                _ => Some(()),
            };
            if read.is_none() {
                skipped += 1;
            }
        }
        Ok(Read {
            value: entities,
            skipped,
        })
    }

    pub(crate) fn read_window(
        &self,
        channel: ChannelId,
    ) -> Result<Read<Vec<Arc<Message>>>, CacheError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, data FROM messages WHERE channel_id = ?1 ORDER BY id")?;
        let mut users = HashMap::new();
        let mut messages = Vec::new();
        let mut skipped = 0;
        let rows = statement.query_map([sql_id(channel.get())], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, data) = row?;
            match serde_json::from_str::<MessageRow>(&data) {
                Ok(row) => messages.push(Arc::new(row.into_message(
                    Snowflake::new(row_id(id)),
                    channel,
                    &mut users,
                ))),
                Err(_) => skipped += 1,
            }
        }
        Ok(Read {
            value: messages,
            skipped,
        })
    }

    fn each(
        &self,
        sql: &str,
        mut read: impl FnMut(u64, &str) -> Option<()>,
    ) -> Result<usize, CacheError> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut skipped = 0;
        for row in rows {
            let (id, data) = row?;
            if read(row_id(id), &data).is_none() {
                skipped += 1;
            }
        }
        Ok(skipped)
    }

    pub(crate) fn write<T>(
        &mut self,
        body: impl FnOnce(&mut Writer<'_>) -> rusqlite::Result<T>,
    ) -> Result<T, CacheError> {
        let transaction = self.conn.transaction()?;
        let value = body(&mut Writer { conn: &transaction })?;
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_write) {
            return Err(CacheError::Io);
        }
        transaction.commit()?;
        Ok(value)
    }

    pub(crate) fn release_free_pages(&self, above: i64) -> Result<(), CacheError> {
        let free: i64 = self
            .conn
            .pragma_query_value(None, "freelist_count", |row| row.get(0))?;
        if free > above {
            self.conn.execute_batch("PRAGMA incremental_vacuum")?;
        }
        Ok(())
    }

    pub(crate) fn checkpoint(&mut self) -> Result<(), CacheError> {
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_checkpoint) {
            return Err(CacheError::Busy);
        }
        // The first column says whether a reader or writer kept it from finishing.
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Err(CacheError::Busy);
        }
        Ok(())
    }

    pub(crate) fn file_kib(&self) -> u64 {
        let Some(path) = self.conn.path() else {
            return 0;
        };
        let size = |suffix: &str| {
            let mut name = std::ffi::OsString::from(path);
            name.push(suffix);
            fs::metadata(PathBuf::from(name)).map_or(0, |metadata| metadata.len())
        };
        (size("") + size("-wal")) / 1024
    }

    pub(crate) fn counts(&self) -> Result<[i64; 4], CacheError> {
        let count = |table: &str| -> Result<i64, CacheError> {
            Ok(self
                .conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?)
        };
        Ok([
            count("guilds")?,
            count("channels")?,
            count("windows")?,
            count("messages")?,
        ])
    }
}

fn unavailable(reason: Unavailable) -> Unavailable {
    tracing::info!(target: LOG, reason = reason.name(), "cache unavailable");
    reason
}

pub(crate) fn delete_files(path: &Path) -> std::io::Result<()> {
    let mut result = Ok(());
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        match fs::remove_file(PathBuf::from(name)) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => result = Err(err),
            _ => {}
        }
    }
    result
}

fn json<T: serde::Serialize>(row: &T) -> rusqlite::Result<String> {
    serde_json::to_string(row).map_err(|err| rusqlite::Error::ToSqlConversionFailure(err.into()))
}

pub(crate) struct Writer<'a> {
    conn: &'a Connection,
}

impl Writer<'_> {
    fn put(&mut self, sql: &str, id: u64, data: String) -> rusqlite::Result<()> {
        self.conn.execute(sql, params![sql_id(id), data])?;
        Ok(())
    }

    fn delete(&mut self, sql: &str, id: u64) -> rusqlite::Result<usize> {
        self.conn.execute(sql, [sql_id(id)])
    }

    pub(crate) fn put_current_user(&mut self, current: &CurrentUser) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM current_user", [])?;
        let data = json(&CurrentUserRow::from(current))?;
        self.put(
            "INSERT INTO current_user (id, data) VALUES (?1, ?2)",
            current.user.id.get(),
            data,
        )
    }

    pub(crate) fn put_user(&mut self, user: &User) -> rusqlite::Result<()> {
        let data = json(&UserRow::from(user))?;
        self.put(
            "INSERT OR REPLACE INTO users (id, data) VALUES (?1, ?2)",
            user.id.get(),
            data,
        )
    }

    pub(crate) fn delete_user(&mut self, id: UserId) -> rusqlite::Result<()> {
        self.delete("DELETE FROM users WHERE id = ?1", id.get())?;
        Ok(())
    }

    pub(crate) fn put_guild_settings(
        &mut self,
        guild: Option<GuildId>,
        settings: &GuildSettings,
    ) -> rusqlite::Result<()> {
        let data = json(&GuildSettingsRow::from(settings))?;
        self.put(
            "INSERT OR REPLACE INTO guild_settings (guild_id, data) VALUES (?1, ?2)",
            guild.map_or(DM_SETTINGS, GuildId::get),
            data,
        )
    }

    pub(crate) fn delete_guild_settings(&mut self, guild: Option<GuildId>) -> rusqlite::Result<()> {
        self.delete(
            "DELETE FROM guild_settings WHERE guild_id = ?1",
            guild.map_or(DM_SETTINGS, GuildId::get),
        )?;
        Ok(())
    }

    pub(crate) fn put_read_state(
        &mut self,
        channel: ChannelId,
        state: &ReadState,
    ) -> rusqlite::Result<()> {
        let data = json(&ReadStateRow::from(state))?;
        self.put(
            "INSERT OR REPLACE INTO read_states (channel_id, data) VALUES (?1, ?2)",
            channel.get(),
            data,
        )
    }

    pub(crate) fn delete_read_state(&mut self, channel: ChannelId) -> rusqlite::Result<()> {
        self.delete(
            "DELETE FROM read_states WHERE channel_id = ?1",
            channel.get(),
        )?;
        Ok(())
    }

    // No row: the settings proto had no folders field, which differs from an empty list.
    pub(crate) fn put_folders(&mut self, folders: Option<&[FolderEntry]>) -> rusqlite::Result<()> {
        match folders {
            Some(folders) => {
                let rows: Vec<FolderRow> = folders.iter().map(FolderRow::from).collect();
                self.put_user_setting(FOLDERS, json(&rows)?)
            }
            None => {
                self.conn
                    .execute("DELETE FROM user_settings WHERE key = ?1", [FOLDERS])?;
                Ok(())
            }
        }
    }

    pub(crate) fn put_notification_flags(&mut self, flags: u64) -> rusqlite::Result<()> {
        self.put_user_setting(NOTIFICATION_FLAGS, flags.to_string())
    }

    pub(crate) fn put_display_settings(
        &mut self,
        display: &DisplaySettings,
    ) -> rusqlite::Result<()> {
        self.put_user_setting(DISPLAY, json(&DisplayRow::from(display))?)
    }

    pub(crate) fn put_age_agreements(&mut self, guilds: &AgeAgreements) -> rusqlite::Result<()> {
        let ids: Vec<u64> = guilds.iter().map(|guild| guild.get()).collect();
        self.put_user_setting(AGE_AGREEMENTS, json(&ids)?)
    }

    fn put_user_setting(&mut self, key: &str, data: String) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO user_settings (key, data) VALUES (?1, ?2)",
            params![key, data],
        )?;
        Ok(())
    }

    pub(crate) fn put_guild(&mut self, guild: &Guild) -> rusqlite::Result<()> {
        let data = json(&GuildRow::from(guild))?;
        self.put(
            "INSERT OR REPLACE INTO guilds (id, data) VALUES (?1, ?2)",
            guild.id.get(),
            data,
        )
    }

    pub(crate) fn delete_guild(&mut self, id: GuildId) -> rusqlite::Result<()> {
        self.delete("DELETE FROM guilds WHERE id = ?1", id.get())?;
        Ok(())
    }

    pub(crate) fn put_unavailable(&mut self, id: GuildId) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO unavailable_guilds (id) VALUES (?1)",
            [sql_id(id.get())],
        )?;
        Ok(())
    }

    pub(crate) fn delete_unavailable(&mut self, id: GuildId) -> rusqlite::Result<()> {
        self.delete("DELETE FROM unavailable_guilds WHERE id = ?1", id.get())?;
        Ok(())
    }

    pub(crate) fn put_member(&mut self, member: &Member) -> rusqlite::Result<()> {
        let data = json(&MemberRow::from(member))?;
        self.put(
            "INSERT OR REPLACE INTO members (guild_id, data) VALUES (?1, ?2)",
            member.guild_id.get(),
            data,
        )
    }

    pub(crate) fn delete_member(&mut self, guild: GuildId) -> rusqlite::Result<()> {
        self.delete("DELETE FROM members WHERE guild_id = ?1", guild.get())?;
        Ok(())
    }

    pub(crate) fn put_channel(&mut self, channel: &Channel) -> rusqlite::Result<()> {
        let data = json(&ChannelRow::from(channel))?;
        self.conn.execute(
            "INSERT OR REPLACE INTO channels (id, guild_id, data) VALUES (?1, ?2, ?3)",
            params![
                sql_id(channel.id.get()),
                channel.guild_id.map(|guild| sql_id(guild.get())),
                data
            ],
        )?;
        Ok(())
    }

    pub(crate) fn delete_channel(&mut self, id: ChannelId) -> rusqlite::Result<()> {
        self.delete("DELETE FROM channels WHERE id = ?1", id.get())?;
        Ok(())
    }

    // The outbox is never cached, even if a pending message slips into a window.
    pub(crate) fn put_window(
        &mut self,
        channel: ChannelId,
        viewed_at: i64,
        messages: &[Arc<Message>],
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO windows (channel_id, viewed_at) VALUES (?1, ?2)",
            params![sql_id(channel.get()), viewed_at],
        )?;
        self.delete("DELETE FROM messages WHERE channel_id = ?1", channel.get())?;
        let mut insert = self
            .conn
            .prepare_cached("INSERT INTO messages (channel_id, id, data) VALUES (?1, ?2, ?3)")?;
        for message in messages
            .iter()
            .filter(|message| message.delivery == Delivery::Sent)
        {
            insert.execute(params![
                sql_id(channel.get()),
                sql_id(message.id.get()),
                json(&MessageRow::from(&**message))?
            ])?;
        }
        Ok(())
    }

    pub(crate) fn evict(&mut self, keep: usize) -> rusqlite::Result<(Vec<ChannelId>, usize)> {
        let mut statement = self.conn.prepare(
            "SELECT channel_id FROM windows WHERE channel_id NOT IN (SELECT id FROM channels)
             UNION
             SELECT channel_id FROM (
                 SELECT channel_id FROM windows ORDER BY viewed_at DESC, channel_id DESC
                 LIMIT -1 OFFSET ?1
             )",
        )?;
        let keep = i64::try_from(keep).unwrap_or(i64::MAX);
        let channels = statement
            .query_map([keep], |row| row.get::<_, i64>(0))?
            .map(|id| id.map(|id| ChannelId::new(row_id(id))))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut deleted = 0;
        for channel in &channels {
            deleted += self.delete("DELETE FROM windows WHERE channel_id = ?1", channel.get())?;
            deleted += self.delete("DELETE FROM messages WHERE channel_id = ?1", channel.get())?;
        }
        Ok((channels, deleted))
    }
}

#[cfg(test)]
pub(crate) mod tests;
