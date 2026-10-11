use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::model::{
    ChannelType, MessageReferenceType, MessageType, OverwriteType, Permissions, PremiumType,
    Snowflake, StickerFormatType, Timestamp,
};
use crate::state::apply::Entities;
use crate::state::{
    Attachment, Channel, CurrentUser, Delivery, Embed, EmbedAuthor, EmbedField, EmbedFooter,
    EmbedMedia, EmbedProvider, Guild, ImageHash, Member, Message, MessageReference,
    PermissionOverwrite, ReferencedMessage, Role, Sticker, ThreadInfo, User,
};

/// A directory of its own under the system's temporary directory, removed on drop.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "akari-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn file(&self, user: u64) -> PathBuf {
        file(&self.0, UserId::new(user))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ME: u64 = 1;

pub(crate) fn time(unix_nanos: i64) -> Timestamp {
    Timestamp::from_unix_nanos(unix_nanos)
}

pub(crate) fn user(id: u64, name: &str) -> User {
    User {
        id: Snowflake::new(id),
        username: name.to_lowercase().into(),
        global_name: Some(name.into()),
        discriminator: 0,
        avatar: Some(ImageHash::parse("a_0123456789abcdef0123456789abcdef")),
        bot: false,
        system: false,
        public_flags: 64,
    }
}

pub(crate) fn current_user() -> CurrentUser {
    CurrentUser {
        user: user(ME, "Me"),
        premium_type: PremiumType::Tier2,
        mfa_enabled: true,
        verified: true,
    }
}

pub(crate) fn guild(id: u64, name: &str) -> Guild {
    Guild {
        id: Snowflake::new(id),
        name: name.into(),
        icon: Some(ImageHash::parse("0123456789abcdef0123456789abcdef")),
        banner: None,
        owner_id: Some(Snowflake::new(7)),
        roles: vec![
            Role {
                id: Snowflake::new(id),
                name: "@everyone".into(),
                position: 0,
                permissions: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
                color: 0,
                hoist: false,
            },
            Role {
                id: Snowflake::new(id + 1),
                name: "Mods".into(),
                position: 3,
                permissions: Permissions::ADMINISTRATOR,
                color: 0x00ff_8800,
                hoist: true,
            },
        ]
        .into(),
        member_count: Some(1234),
        large: true,
    }
}

pub(crate) fn member(guild: u64) -> Member {
    Member {
        guild_id: Snowflake::new(guild),
        nick: Some("nick".into()),
        avatar: None,
        roles: vec![Snowflake::new(guild + 1)].into(),
        joined_at: Some(time(1_600_000_000_123_456_000)),
        communication_disabled_until: None,
        flags: 2,
        pending: false,
    }
}

pub(crate) fn channel(id: u64, guild: Option<u64>, name: &str) -> Channel {
    Channel {
        id: Snowflake::new(id),
        kind: if guild.is_some() {
            ChannelType::GuildText
        } else {
            ChannelType::Dm
        },
        guild_id: guild.map(Snowflake::new),
        parent_id: guild.map(|_| Snowflake::new(900)),
        name: guild.map(|_| name.into()),
        position: 4,
        topic: Some("a topic".into()),
        nsfw: false,
        rate_limit_per_user: 30,
        last_message_id: Some(Snowflake::new(id * 1000)),
        permission_overwrites: vec![PermissionOverwrite {
            id: Snowflake::new(guild.unwrap_or(0) + 1),
            kind: OverwriteType::Role,
            allow: Permissions::SEND_MESSAGES,
            deny: Permissions::MENTION_EVERYONE,
        }]
        .into(),
        recipients: if guild.is_none() {
            vec![Snowflake::new(20)].into()
        } else {
            Box::new([])
        },
        icon: None,
        owner_id: None,
        thread: None,
        flags: 0,
    }
}

pub(crate) fn thread(id: u64, guild: u64, parent: u64) -> Channel {
    Channel {
        kind: ChannelType::PublicThread,
        parent_id: Some(Snowflake::new(parent)),
        owner_id: Some(Snowflake::new(ME)),
        thread: Some(Box::new(ThreadInfo {
            archived: false,
            locked: true,
            auto_archive_duration: 1440,
            archive_timestamp: time(1_700_000_000_000_000_000),
            create_timestamp: Some(time(1_690_000_000_000_001_000)),
            message_count: Some(12),
            member_count: Some(3),
        })),
        ..channel(id, Some(guild), "a thread")
    }
}

pub(crate) fn entities() -> Entities {
    Entities {
        current_user: Some(Arc::new(current_user())),
        users: [(Snowflake::new(20), Arc::new(user(20, "Mira")))].into(),
        guilds: [guild(100, "One"), guild(200, "Two")]
            .map(|guild| (guild.id, Arc::new(guild)))
            .into(),
        unavailable: [Snowflake::new(300)].into(),
        members: [(Snowflake::new(100), Arc::new(member(100)))].into(),
        channels: [
            channel(10, Some(100), "general"),
            channel(11, Some(200), "random"),
            thread(12, 100, 10),
            channel(13, None, ""),
        ]
        .map(|channel| (channel.id, Arc::new(channel)))
        .into(),
    }
}

pub(crate) fn message(channel: u64, id: u64, content: &str) -> Message {
    Message {
        id: Snowflake::new(id),
        channel_id: Snowflake::new(channel),
        kind: MessageType::Default,
        author: Arc::new(user(20, "Mira")),
        webhook_id: None,
        content: content.into(),
        timestamp: time(
            1_700_000_000_000_000_000 + i64::try_from(id % 1_000_000).unwrap() * 1_000_000,
        ),
        edited_timestamp: None,
        flags: 0,
        pinned: false,
        tts: false,
        mention_everyone: false,
        mentions: Box::new([]),
        mention_roles: Box::new([]),
        attachments: Box::new([]),
        embeds: Box::new([]),
        stickers: Box::new([]),
        reference: None,
        referenced_message: ReferencedMessage::NotIncluded,
        delivery: Delivery::Sent,
    }
}

fn full_message() -> Message {
    let replied = message(10, 499, "the original");
    Message {
        kind: MessageType::Reply,
        webhook_id: Some(Snowflake::new(55)),
        edited_timestamp: Some(time(1_700_000_100_000_123_000)),
        flags: 4,
        pinned: true,
        tts: true,
        mention_everyone: true,
        mentions: vec![Arc::new(user(21, "Noa"))].into(),
        mention_roles: vec![Snowflake::new(101)].into(),
        attachments: vec![Attachment {
            id: Snowflake::new(77),
            filename: "photo.png".into(),
            description: Some("alt text".into()),
            content_type: Some("image/png".into()),
            size: 2_000_000,
            url: "https://cdn.example/photo.png".into(),
            proxy_url: "https://media.example/photo.png".into(),
            width: Some(640),
            height: Some(480),
            flags: 0,
        }]
        .into(),
        embeds: vec![Embed {
            kind: Some("rich".into()),
            title: Some("Title".into()),
            description: Some("Description".into()),
            url: Some("https://example.com".into()),
            timestamp: Some(time(1_699_000_000_000_000_000)),
            color: Some(0x0011_2233),
            author: Some(EmbedAuthor {
                name: "Author".into(),
                url: Some("https://example.com/a".into()),
                proxy_icon_url: None,
            }),
            provider: Some(EmbedProvider {
                name: Some("Provider".into()),
                url: None,
            }),
            footer: Some(EmbedFooter {
                text: "Footer".into(),
                proxy_icon_url: Some("https://media.example/f.png".into()),
            }),
            image: Some(EmbedMedia {
                url: "https://example.com/i.png".into(),
                proxy_url: Some("https://media.example/i.png".into()),
                width: Some(10),
                height: Some(20),
            }),
            thumbnail: None,
            video: None,
            fields: vec![
                EmbedField {
                    name: "One".into(),
                    value: "1".into(),
                    inline: true,
                },
                EmbedField {
                    name: "Two".into(),
                    value: "2".into(),
                    inline: false,
                },
            ]
            .into(),
        }]
        .into(),
        stickers: vec![Sticker {
            id: Snowflake::new(88),
            name: "wave".into(),
            format: StickerFormatType::Apng,
        }]
        .into(),
        reference: Some(Box::new(MessageReference {
            kind: MessageReferenceType::Default,
            message_id: Some(Snowflake::new(499)),
            channel_id: Some(Snowflake::new(10)),
            guild_id: Some(Snowflake::new(100)),
        })),
        referenced_message: ReferencedMessage::Message(Arc::new(replied)),
        ..message(10, 500, "hello there")
    }
}

fn open(dir: &TempDir) -> Cache {
    Cache::open(&dir.file(ME), UserId::new(ME)).unwrap()
}

fn write_entities(cache: &mut Cache, entities: &Entities) {
    cache
        .write(|writer| {
            if let Some(current) = &entities.current_user {
                writer.put_current_user(current)?;
            }
            for user in entities.users.values() {
                writer.put_user(user)?;
            }
            for guild in entities.guilds.values() {
                writer.put_guild(guild)?;
            }
            for id in &entities.unavailable {
                writer.put_unavailable(*id)?;
            }
            for member in entities.members.values() {
                writer.put_member(member)?;
            }
            for channel in entities.channels.values() {
                writer.put_channel(channel)?;
            }
            Ok(())
        })
        .unwrap();
}

fn raw(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

#[test]
fn every_state_type_round_trips_through_its_row() {
    let dir = TempDir::new();
    let mut cache = open(&dir);
    let entities = entities();
    let sent = full_message();

    write_entities(&mut cache, &entities);
    cache
        .write(|writer| writer.put_window(Snowflake::new(10), 42, &[Arc::new(sent.clone())]))
        .unwrap();
    drop(cache);
    let cache = open(&dir);

    let read = cache.read_entities().unwrap();
    assert_eq!(read.skipped, 0);
    assert!(read.value == entities);
    let window = cache.read_window(Snowflake::new(10)).unwrap();
    let expected = Message {
        referenced_message: ReferencedMessage::NotIncluded,
        ..sent
    };
    assert_eq!(window.skipped, 0);
    assert_eq!(window.value, vec![Arc::new(expected)]);
}

#[test]
fn a_window_reads_oldest_first_and_shares_its_authors() {
    let dir = TempDir::new();
    let mut cache = open(&dir);
    let messages: Vec<_> = [503, 501, 502]
        .map(|id| Arc::new(message(10, id, "text")))
        .into();

    cache
        .write(|writer| writer.put_window(Snowflake::new(10), 42, &messages))
        .unwrap();

    let read = cache.read_window(Snowflake::new(10)).unwrap().value;
    let ids: Vec<u64> = read.iter().map(|message| message.id.get()).collect();
    assert_eq!(ids, [501, 502, 503]);
    assert!(Arc::ptr_eq(&read[0].author, &read[2].author));
    assert!(
        cache
            .read_window(Snowflake::new(11))
            .unwrap()
            .value
            .is_empty()
    );
}

#[test]
fn the_file_uses_wal_incremental_vacuum_and_secure_delete() {
    let dir = TempDir::new();
    let cache = open(&dir);

    let pragma = |name: &str| -> String {
        cache
            .conn
            .query_row(&format!("PRAGMA {name}"), [], |row| {
                row.get::<_, rusqlite::types::Value>(0)
            })
            .map(|value| match value {
                rusqlite::types::Value::Integer(n) => n.to_string(),
                rusqlite::types::Value::Text(text) => text,
                other => format!("{other:?}"),
            })
            .unwrap()
    };
    assert_eq!(pragma("journal_mode"), "wal");
    assert_eq!(pragma("auto_vacuum"), "2");
    assert_eq!(pragma("secure_delete"), "1");
    assert_eq!(pragma("user_version"), schema::CURRENT.to_string());
}

#[test]
fn a_corrupt_file_is_discarded_and_rebuilt() {
    let dir = TempDir::new();
    let path = dir.file(ME);
    fs::write(&path, b"garbage!".repeat(512)).unwrap();

    let opened = Cache::open_or_rebuild(&path, UserId::new(ME)).unwrap();

    assert_eq!(opened.discarded, Some(Discarded::Corrupt));
    let read = opened.cache.read_entities().unwrap().value;
    assert!(read == Entities::default());
    drop(opened);
    let bytes = fs::read(&path).unwrap();
    assert!(!bytes.windows(8).any(|window| window == b"garbage!"));
}

#[test]
fn any_other_schema_version_is_discarded() {
    for version in [schema::CURRENT + 1, 0] {
        let dir = TempDir::new();
        let mut cache = open(&dir);
        write_entities(&mut cache, &entities());
        drop(cache);
        raw(&dir.file(ME))
            .pragma_update(None, "user_version", version)
            .unwrap();

        let opened = Cache::open_or_rebuild(&dir.file(ME), UserId::new(ME)).unwrap();

        assert_eq!(opened.discarded, Some(Discarded::OtherSchema), "{version}");
        assert!(opened.cache.read_entities().unwrap().value == Entities::default());
    }
}

#[test]
fn an_unwritable_directory_runs_without_a_cache() {
    let dir = TempDir::new();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

    let opened = Cache::open_or_rebuild(&dir.file(ME), UserId::new(ME));

    assert!(matches!(opened, Err(Unavailable::Io)));
    assert!(!dir.file(ME).exists());
}

#[test]
fn a_busy_file_is_left_alone() {
    let dir = TempDir::new();
    let mut cache = open(&dir);
    write_entities(&mut cache, &entities());
    drop(cache);
    let holder = raw(&dir.file(ME));
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();

    let opened = Cache::open_or_rebuild(&dir.file(ME), UserId::new(ME));

    assert!(matches!(opened, Err(Unavailable::Busy)));
    holder.execute_batch("COMMIT").unwrap();
    drop(holder);
    assert!(open(&dir).read_entities().unwrap().value == entities());
}

#[test]
fn another_users_file_is_discarded() {
    let dir = TempDir::new();
    let mut cache = open(&dir);
    write_entities(&mut cache, &entities());
    drop(cache);

    let opened = Cache::open_or_rebuild(&dir.file(ME), UserId::new(2)).unwrap();

    assert_eq!(opened.discarded, Some(Discarded::OtherUser));
    assert!(opened.cache.read_entities().unwrap().value == Entities::default());
    drop(opened);
    assert!(Cache::open(&dir.file(ME), UserId::new(2)).is_ok());
}

#[test]
fn a_row_that_does_not_decode_is_skipped() {
    let dir = TempDir::new();
    let mut cache = open(&dir);
    write_entities(&mut cache, &entities());
    cache
        .conn
        .execute("UPDATE channels SET data = '{' WHERE id = 11", [])
        .unwrap();

    let read = cache.read_entities().unwrap();

    assert_eq!(read.skipped, 1);
    assert!(read.value.channels.contains_key(&Snowflake::new(10)));
    assert!(!read.value.channels.contains_key(&Snowflake::new(11)));
}

#[test]
fn old_rows_read_with_new_fields_defaulted() {
    let dir = TempDir::new();
    let mut cache = open(&dir);
    let mut stored = channel(10, Some(100), "general");
    stored.flags = 16;
    cache.write(|writer| writer.put_channel(&stored)).unwrap();
    let data: String = cache
        .conn
        .query_row("SELECT data FROM channels WHERE id = 10", [], |row| {
            row.get(0)
        })
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&data).unwrap();
    json.as_object_mut().unwrap().remove("flags").unwrap();
    cache
        .conn
        .execute(
            "UPDATE channels SET data = ?1 WHERE id = 10",
            [json.to_string()],
        )
        .unwrap();

    let read = cache.read_entities().unwrap();

    assert_eq!(read.skipped, 0);
    let channel = &read.value.channels[&Snowflake::new(10)];
    assert_eq!(channel.flags, 0);
    assert_eq!(**channel, Channel { flags: 0, ..stored });
}

#[test]
fn snowflakes_round_trip_as_integers() {
    let dir = TempDir::new();
    let mut cache = open(&dir);
    let big = u64::MAX >> 1;
    let stored = guild(big - 1, "Big");
    let sent = Arc::new(message(big, big, "big"));

    cache
        .write(|writer| {
            writer.put_guild(&stored)?;
            writer.put_window(Snowflake::new(big), 1, std::slice::from_ref(&sent))
        })
        .unwrap();

    let kind: String = cache
        .conn
        .query_row("SELECT typeof(id) FROM guilds", [], |row| row.get(0))
        .unwrap();
    assert_eq!(kind, "integer");
    let read = cache.read_entities().unwrap().value;
    assert_eq!(*read.guilds[&Snowflake::new(big - 1)], stored);
    assert_eq!(
        cache.read_window(Snowflake::new(big)).unwrap().value,
        vec![sent]
    );
}
