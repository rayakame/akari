use rusqlite::Connection;

use crate::model::UserId;

// A file with any other version is deleted and started over, never migrated.
pub(super) const CURRENT: i32 = 2;

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
         CREATE TABLE guild_settings (guild_id INTEGER PRIMARY KEY, data TEXT NOT NULL);
         CREATE TABLE user_settings (key TEXT PRIMARY KEY, data TEXT NOT NULL);
         CREATE TABLE read_states (channel_id INTEGER PRIMARY KEY, data TEXT NOT NULL);
         INSERT INTO meta (key, value) VALUES ('user_id', '{user}');
         PRAGMA user_version = {CURRENT};
         COMMIT;",
        user = user.get(),
    ))
}
