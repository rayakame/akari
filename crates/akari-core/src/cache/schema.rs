use rusqlite::Connection;

use crate::model::UserId;

// A file with any other version is deleted and started over, never migrated.
pub(super) const CURRENT: i32 = 1;

pub(super) fn create(conn: &Connection, user: UserId) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "BEGIN;
         CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE current_user (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
         CREATE TABLE users (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
         CREATE TABLE guilds (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
         CREATE TABLE unavailable_guilds (id INTEGER PRIMARY KEY);
         CREATE TABLE members (guild_id INTEGER PRIMARY KEY, data TEXT NOT NULL);
         CREATE TABLE channels (id INTEGER PRIMARY KEY, guild_id INTEGER, data TEXT NOT NULL);
         CREATE TABLE windows (channel_id INTEGER PRIMARY KEY, viewed_at INTEGER NOT NULL);
         CREATE TABLE messages (
             channel_id INTEGER NOT NULL,
             id INTEGER NOT NULL,
             data TEXT NOT NULL,
             PRIMARY KEY (channel_id, id)
         ) WITHOUT ROWID;
         INSERT INTO meta (key, value) VALUES ('user_id', '{user}');
         PRAGMA user_version = {CURRENT};
         COMMIT;",
        user = user.get(),
    ))
}
