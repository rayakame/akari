use std::sync::Arc;

use super::types::{
    Attachment, Channel, CurrentUser, Delivery, Embed, EmbedAuthor, EmbedField, EmbedFooter,
    EmbedMedia, EmbedProvider, Guild, ImageHash, Member, Message, MessageReference,
    PermissionOverwrite, ReferencedMessage, Role, Sticker, ThreadInfo, User,
};
use crate::gateway::{
    AvailableGuild, ChannelUpdate, GuildMemberUpdate, GuildUpdate, MessageUpdate, UserUpdate,
};
use crate::model::{self, ChannelId, GuildId, MessageId, Timestamp};

fn text(value: String) -> Box<str> {
    value.into_boxed_str()
}

fn optional_text(value: Option<String>) -> Option<Box<str>> {
    value.map(String::into_boxed_str)
}

fn image(hash: Option<String>) -> Option<ImageHash> {
    hash.as_deref().map(ImageHash::parse)
}

fn list<T, U>(values: Vec<T>, convert: impl FnMut(T) -> U) -> Box<[U]> {
    values.into_iter().map(convert).collect()
}

impl User {
    pub(crate) fn from_wire(user: model::User) -> Self {
        Self {
            id: user.id,
            username: text(user.username),
            global_name: optional_text(user.global_name),
            discriminator: user.discriminator.parse().unwrap_or(0),
            avatar: image(user.avatar),
            bot: user.bot,
            system: user.system,
            public_flags: user.public_flags,
        }
    }
}

impl CurrentUser {
    pub(crate) fn from_wire(current: model::CurrentUser) -> Self {
        Self {
            user: User::from_wire(current.user),
            premium_type: current.premium_type,
            mfa_enabled: current.mfa_enabled,
            verified: current.verified,
        }
    }

    pub(crate) fn patch(&self, update: UserUpdate) -> Self {
        let mut next = self.clone();
        let user = &mut next.user;
        if let Some(username) = update.username {
            user.username = text(username);
        }
        if let Some(global_name) = update.global_name {
            user.global_name = optional_text(global_name);
        }
        if let Some(discriminator) = update.discriminator {
            user.discriminator = discriminator.parse().unwrap_or(0);
        }
        if let Some(avatar) = update.avatar {
            user.avatar = image(avatar);
        }
        user.bot = update.bot.unwrap_or(user.bot);
        user.system = update.system.unwrap_or(user.system);
        user.public_flags = update.public_flags.unwrap_or(user.public_flags);
        next.premium_type = update.premium_type.unwrap_or(next.premium_type);
        next.mfa_enabled = update.mfa_enabled.unwrap_or(next.mfa_enabled);
        next.verified = update.verified.unwrap_or(next.verified);
        next
    }
}

impl Guild {
    pub(crate) fn from_wire(guild: AvailableGuild) -> Self {
        let properties = guild.properties;
        Self {
            id: properties.id,
            name: text(properties.name),
            icon: image(properties.icon),
            banner: image(properties.banner),
            owner_id: properties.owner_id,
            roles: guild.roles.into_iter().map(Role::from_wire).collect(),
            member_count: guild.member_count,
            large: guild.large,
        }
    }

    pub(crate) fn patch(&self, update: GuildUpdate) -> Self {
        let mut next = self.clone();
        if let Some(name) = update.name {
            next.name = text(name);
        }
        if let Some(icon) = update.icon {
            next.icon = image(icon);
        }
        if let Some(banner) = update.banner {
            next.banner = image(banner);
        }
        next.owner_id = update.owner_id.or(next.owner_id);
        if let Some(roles) = update.roles {
            next.roles = roles.into_iter().map(Role::from_wire).collect();
        }
        next
    }
}

impl Role {
    pub(crate) fn from_wire(role: model::Role) -> Self {
        Self {
            id: role.id,
            name: text(role.name),
            position: role.position,
            permissions: role.permissions,
            color: role
                .colors
                .map_or(role.color, |colors| colors.primary_color),
            hoist: role.hoist,
        }
    }
}

impl Member {
    pub(crate) fn from_wire(guild_id: GuildId, member: model::GuildMember) -> Self {
        Self {
            guild_id,
            nick: optional_text(member.nick),
            avatar: image(member.avatar),
            roles: member.roles.into_boxed_slice(),
            joined_at: member.joined_at,
            communication_disabled_until: member.communication_disabled_until,
            flags: member.flags,
            pending: member.pending,
        }
    }

    pub(crate) fn patch(&self, update: GuildMemberUpdate) -> Self {
        let mut next = self.clone();
        if let Some(nick) = update.nick {
            next.nick = optional_text(nick);
        }
        if let Some(avatar) = update.avatar {
            next.avatar = image(avatar);
        }
        if let Some(roles) = update.roles {
            next.roles = roles.into_boxed_slice();
        }
        next.joined_at = update.joined_at.or(next.joined_at);
        if let Some(until) = update.communication_disabled_until {
            next.communication_disabled_until = until;
        }
        next.flags = update.flags.unwrap_or(next.flags);
        next.pending = update.pending.unwrap_or(next.pending);
        next
    }
}

impl Channel {
    // READY's guild channels don't carry their own `guild_id`.
    pub(crate) fn from_wire(channel: model::Channel, guild_id: Option<GuildId>) -> Self {
        let recipients = if channel.recipient_ids.is_empty() {
            channel.recipients.iter().map(|user| user.id).collect()
        } else {
            channel.recipient_ids.into_boxed_slice()
        };
        let thread = channel.thread_metadata.map(|metadata| {
            Box::new(thread_info(
                metadata,
                channel.message_count,
                channel.member_count,
            ))
        });
        Self {
            id: channel.id,
            kind: channel.kind,
            guild_id: channel.guild_id.or(guild_id),
            parent_id: channel.parent_id,
            name: optional_text(channel.name),
            position: channel.position.unwrap_or(0),
            topic: optional_text(channel.topic),
            nsfw: channel.nsfw,
            rate_limit_per_user: channel.rate_limit_per_user.unwrap_or(0),
            permission_overwrites: list(channel.permission_overwrites, overwrite),
            recipients,
            icon: image(channel.icon),
            owner_id: channel.owner_id,
            thread,
            flags: channel.flags,
        }
    }

    pub(crate) fn patch(&self, update: ChannelUpdate) -> Self {
        let mut next = self.clone();
        next.kind = update.kind.unwrap_or(next.kind);
        next.guild_id = update.guild_id.or(next.guild_id);
        if let Some(parent_id) = update.parent_id {
            next.parent_id = parent_id;
        }
        if let Some(name) = update.name {
            next.name = optional_text(name);
        }
        next.position = update.position.unwrap_or(next.position);
        if let Some(topic) = update.topic {
            next.topic = optional_text(topic);
        }
        next.nsfw = update.nsfw.unwrap_or(next.nsfw);
        next.rate_limit_per_user = update
            .rate_limit_per_user
            .unwrap_or(next.rate_limit_per_user);
        if let Some(overwrites) = update.permission_overwrites {
            next.permission_overwrites = list(overwrites, overwrite);
        }
        if let Some(recipients) = update.recipients {
            next.recipients = recipients.iter().map(|user| user.id).collect();
        }
        if let Some(icon) = update.icon {
            next.icon = image(icon);
        }
        next.owner_id = update.owner_id.or(next.owner_id);
        next.flags = update.flags.unwrap_or(next.flags);
        let counts = |info: Option<&ThreadInfo>| {
            (
                update
                    .message_count
                    .or(info.and_then(|info| info.message_count)),
                update
                    .member_count
                    .or(info.and_then(|info| info.member_count)),
            )
        };
        if let Some(metadata) = update.thread_metadata {
            let (messages, members) = counts(next.thread.as_deref());
            next.thread = Some(Box::new(thread_info(metadata, messages, members)));
        } else if let Some(info) = next.thread.as_deref_mut() {
            (info.message_count, info.member_count) = counts(Some(info));
        }
        next
    }
}

fn thread_info(
    metadata: model::ThreadMetadata,
    message_count: Option<u32>,
    member_count: Option<u32>,
) -> ThreadInfo {
    ThreadInfo {
        archived: metadata.archived,
        locked: metadata.locked,
        auto_archive_duration: metadata.auto_archive_duration,
        archive_timestamp: metadata.archive_timestamp,
        create_timestamp: metadata.create_timestamp,
        message_count,
        member_count,
    }
}

fn overwrite(overwrite: model::PermissionOverwrite) -> PermissionOverwrite {
    PermissionOverwrite {
        id: overwrite.id,
        kind: overwrite.kind,
        allow: overwrite.allow,
        deny: overwrite.deny,
    }
}

pub(crate) type Intern<'a> = &'a mut dyn FnMut(model::User) -> Arc<User>;

impl Message {
    pub(crate) fn from_wire(message: model::Message, intern: Intern<'_>) -> Self {
        let referenced_message = match message.referenced_message {
            None => ReferencedMessage::NotIncluded,
            Some(None) => ReferencedMessage::Deleted,
            Some(Some(original)) => {
                ReferencedMessage::Message(Arc::new(Self::from_wire(*original, intern)))
            }
        };
        Self {
            id: message.id,
            channel_id: message.channel_id,
            kind: message.kind,
            author: intern(message.author),
            webhook_id: message.webhook_id,
            content: text(message.content),
            timestamp: message.timestamp,
            edited_timestamp: message.edited_timestamp,
            flags: message.flags,
            pinned: message.pinned,
            tts: message.tts,
            mention_everyone: message.mention_everyone,
            mentions: list(message.mentions, &mut *intern),
            mention_roles: message.mention_roles.into_boxed_slice(),
            attachments: list(message.attachments, attachment),
            embeds: list(message.embeds, embed),
            stickers: list(message.sticker_items, sticker),
            reference: message.message_reference.map(|reference| {
                Box::new(MessageReference {
                    kind: reference.kind,
                    message_id: reference.message_id,
                    channel_id: reference.channel_id,
                    guild_id: reference.guild_id,
                })
            }),
            referenced_message,
            delivery: Delivery::Sent,
        }
    }

    pub(crate) fn pending(
        id: MessageId,
        channel_id: ChannelId,
        author: Arc<User>,
        content: String,
        unix_millis: i64,
    ) -> Self {
        Self {
            id,
            channel_id,
            kind: model::MessageType::Default,
            author,
            webhook_id: None,
            content: text(content),
            timestamp: Timestamp::from_unix_millis(unix_millis),
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
            delivery: Delivery::Pending,
        }
    }

    pub(crate) fn patch(&self, update: MessageUpdate, intern: Intern<'_>) -> Self {
        let mut next = self.clone();
        next.kind = update.kind.unwrap_or(next.kind);
        if let Some(author) = update.author {
            next.author = intern(author);
        }
        next.webhook_id = update.webhook_id.or(next.webhook_id);
        if let Some(content) = update.content {
            next.content = text(content);
        }
        if let Some(edited) = update.edited_timestamp {
            next.edited_timestamp = edited;
        }
        next.mention_everyone = update.mention_everyone.unwrap_or(next.mention_everyone);
        if let Some(mentions) = update.mentions {
            next.mentions = list(mentions, &mut *intern);
        }
        if let Some(roles) = update.mention_roles {
            next.mention_roles = roles.into_boxed_slice();
        }
        if let Some(attachments) = update.attachments {
            next.attachments = list(attachments, attachment);
        }
        if let Some(embeds) = update.embeds {
            next.embeds = list(embeds, embed);
        }
        if let Some(stickers) = update.sticker_items {
            next.stickers = list(stickers, sticker);
        }
        next.pinned = update.pinned.unwrap_or(next.pinned);
        next.flags = update.flags.unwrap_or(next.flags);
        next
    }
}

fn attachment(attachment: model::Attachment) -> Attachment {
    Attachment {
        id: attachment.id,
        filename: text(attachment.filename),
        description: optional_text(attachment.description),
        content_type: optional_text(attachment.content_type),
        size: attachment.size,
        url: text(attachment.url),
        proxy_url: text(attachment.proxy_url),
        width: attachment.width,
        height: attachment.height,
        flags: attachment.flags,
    }
}

fn embed(embed: model::Embed) -> Embed {
    let media = |media: model::EmbedMedia| EmbedMedia {
        url: text(media.url),
        proxy_url: optional_text(media.proxy_url),
        width: media.width,
        height: media.height,
    };
    Embed {
        kind: optional_text(embed.kind),
        title: optional_text(embed.title),
        description: optional_text(embed.description),
        url: optional_text(embed.url),
        timestamp: embed.timestamp,
        color: embed.color,
        author: embed.author.map(|author| EmbedAuthor {
            name: text(author.name),
            url: optional_text(author.url),
            proxy_icon_url: optional_text(author.proxy_icon_url),
        }),
        provider: embed.provider.map(|provider| EmbedProvider {
            name: optional_text(provider.name),
            url: optional_text(provider.url),
        }),
        footer: embed.footer.map(|footer| EmbedFooter {
            text: text(footer.text),
            proxy_icon_url: optional_text(footer.proxy_icon_url),
        }),
        image: embed.image.map(media),
        thumbnail: embed.thumbnail.map(media),
        video: embed.video.map(media),
        fields: list(embed.fields, |field| EmbedField {
            name: text(field.name),
            value: text(field.value),
            inline: field.inline,
        }),
    }
}

fn sticker(sticker: model::StickerItem) -> Sticker {
    Sticker {
        id: sticker.id,
        name: text(sticker.name),
        format: sticker.format_type,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde::de::DeserializeOwned;
    use serde_json::Value;

    use super::*;
    use crate::gateway::{GatewayGuild, Ready};
    use crate::model::{self, ChannelType, MessageType, Permissions, PremiumType, Snowflake};
    use crate::state::types::{
        Channel, CurrentUser, Guild, ImageHash, Member, Message, ReferencedMessage, User,
    };

    #[track_caller]
    fn parse<T: DeserializeOwned>(json: &str) -> T {
        serde_json::from_str(json).unwrap_or_else(|err| panic!("failed to parse: {err}"))
    }

    #[track_caller]
    fn edited<T: DeserializeOwned>(json: &str, edit: impl FnOnce(&mut Value)) -> T {
        let mut value: Value = parse(json);
        edit(&mut value);
        parse(&value.to_string())
    }

    fn ready() -> Ready {
        let value: Value = parse(include_str!("../../tests/fixtures/ready.json"));
        parse(&value["d"].to_string())
    }

    fn new_arc(user: model::User) -> Arc<User> {
        Arc::new(User::from_wire(user))
    }

    #[test]
    fn discriminators_parse() {
        let user = |discriminator: &str| {
            User::from_wire(edited(
                include_str!("../../tests/fixtures/user.json"),
                |value| value["discriminator"] = discriminator.into(),
            ))
        };

        assert_eq!(user("0").discriminator, 0);
        assert_eq!(user("0000").discriminator, 0);
        assert_eq!(user("1234").discriminator, 1234);
        assert_eq!(user("abc").discriminator, 0);
    }

    #[test]
    fn the_current_user_converts() {
        let current = CurrentUser::from_wire(ready().user);

        assert_eq!(current.user.id, Snowflake::new(100_000_000_000_000_001));
        assert_eq!(&*current.user.username, "akari_tester");
        assert_eq!(current.premium_type, PremiumType::None);
        assert!(current.verified);
    }

    #[test]
    fn a_ready_guild_converts() {
        let GatewayGuild::Available(wire) = ready().guilds.remove(0) else {
            panic!("expected an available guild");
        };

        let guild = Guild::from_wire(*wire);

        assert_eq!(guild.id, Snowflake::new(200_000_000_000_000_001));
        assert_eq!(&*guild.name, "Akari ✨ Lab");
        assert_eq!(
            guild.owner_id,
            Some(Snowflake::new(100_000_000_000_000_001))
        );
        assert_eq!(guild.roles.len(), 2);
        assert_eq!(guild.roles[0].id, Snowflake::new(200_000_000_000_000_001));
        assert_eq!(guild.roles[0].permissions, Permissions(104_324_673));
        assert_eq!(&*guild.roles[1].name, "Moderators");
    }

    #[test]
    fn roles_take_their_color_from_colors_first() {
        let role: model::Role = parse(
            r#"{"id": "1", "position": 1, "permissions": "0", "color": 5, "colors": {"primary_color": 7}}"#,
        );
        let legacy: model::Role =
            parse(r#"{"id": "1", "position": 1, "permissions": "0", "color": 5}"#);

        assert_eq!(Role::from_wire(role).color, 7);
        assert_eq!(Role::from_wire(legacy).color, 5);
    }

    #[test]
    fn guild_channels_get_their_guild_id() {
        let wire: model::Channel =
            parse(include_str!("../../tests/fixtures/channel_guild_text.json"));
        let mut without = wire.clone();
        without.guild_id = None;

        let guild = Snowflake::new(200_000_000_000_000_009);
        assert_eq!(
            Channel::from_wire(without, Some(guild)).guild_id,
            Some(guild)
        );
        assert_eq!(
            Channel::from_wire(wire, Some(guild)).guild_id,
            Some(Snowflake::new(200_000_000_000_000_001))
        );
    }

    #[test]
    fn a_thread_converts_with_its_metadata() {
        let thread = Channel::from_wire(
            parse(include_str!("../../tests/fixtures/channel_thread.json")),
            None,
        );

        assert_eq!(thread.kind, ChannelType::PublicThread);
        assert_eq!(
            thread.parent_id,
            Some(Snowflake::new(300_000_000_000_000_002))
        );
        let info = thread.thread.unwrap();
        assert_eq!(info.auto_archive_duration, 1440);
        assert_eq!(info.message_count, Some(12));
        assert!(!info.archived);
    }

    #[test]
    fn a_dm_keeps_recipient_ids() {
        let dm = Channel::from_wire(
            parse(include_str!("../../tests/fixtures/channel_dm.json")),
            None,
        );
        let ready_dm = Channel::from_wire(ready().private_channels.remove(1), None);

        assert_eq!(dm.kind, ChannelType::Dm);
        assert_eq!(dm.recipients.len(), 1);
        assert_eq!(
            &*ready_dm.recipients,
            [
                Snowflake::new(100_000_000_000_000_002),
                Snowflake::new(100_000_000_000_000_003)
            ]
        );
        assert_eq!(ready_dm.name.as_deref(), Some("Weekend plans"));
    }

    #[test]
    fn a_member_converts() {
        let wire = ready().merged_members.remove(0).remove(0);
        let guild = Snowflake::new(200_000_000_000_000_001);

        let member = Member::from_wire(guild, wire);

        assert_eq!(member.guild_id, guild);
        assert_eq!(member.nick.as_deref(), Some("Tester"));
        assert_eq!(&*member.roles, [Snowflake::new(500_000_000_000_000_002)]);
    }

    #[test]
    fn a_message_converts_with_reply_states() {
        let fixture = include_str!("../../tests/fixtures/message_reply.json");
        let present = Message::from_wire(parse(fixture), &mut new_arc);
        let deleted = Message::from_wire(
            edited(fixture, |value| value["referenced_message"] = Value::Null),
            &mut new_arc,
        );
        let missing = Message::from_wire(
            edited(fixture, |value| {
                value.as_object_mut().unwrap().remove("referenced_message");
            }),
            &mut new_arc,
        );

        assert_eq!(present.kind, MessageType::Reply);
        let ReferencedMessage::Message(original) = &present.referenced_message else {
            panic!("expected the referenced message");
        };
        assert_eq!(
            Some(original.id),
            present.reference.as_ref().and_then(|r| r.message_id)
        );
        assert_eq!(deleted.referenced_message, ReferencedMessage::Deleted);
        assert_eq!(missing.referenced_message, ReferencedMessage::NotIncluded);
    }

    #[test]
    fn a_message_converts_its_parts() {
        let message = Message::from_wire(
            parse(include_str!("../../tests/fixtures/message.json")),
            &mut new_arc,
        );

        assert_eq!(&*message.author.username, "mira");
        assert_eq!(
            message.author.avatar,
            Some(ImageHash::parse("0123456789abcdef0123456789abcdef"))
        );
        assert_eq!(&*message.attachments[0].filename, "screenshot.png");
        assert_eq!(message.embeds[0].title.as_deref(), Some("Akari 0.1"));
        assert_eq!(message.mentions.len(), 1);
        assert_eq!(message.stickers.len(), 1);
        assert_eq!(message.referenced_message, ReferencedMessage::NotIncluded);
    }

    #[test]
    fn authors_and_mentions_go_through_the_interner() {
        let mut seen = Vec::new();
        Message::from_wire(
            parse(include_str!("../../tests/fixtures/message.json")),
            &mut |user: model::User| {
                seen.push(user.id);
                new_arc(user)
            },
        );

        assert_eq!(
            seen,
            [
                Snowflake::new(100_000_000_000_000_002),
                Snowflake::new(100_000_000_000_000_001)
            ]
        );
    }

    fn general() -> Channel {
        Channel::from_wire(
            parse(include_str!("../../tests/fixtures/channel_guild_text.json")),
            None,
        )
    }

    #[test]
    fn a_channel_patch_keeps_missing_fields_and_clears_null_ones() {
        let channel = general();
        assert!(channel.topic.is_some());

        let kept = channel.patch(parse(r#"{"id": "300000000000000002", "name": "renamed"}"#));
        let cleared = channel.patch(parse(
            r#"{"id": "300000000000000002", "topic": null, "permission_overwrites": []}"#,
        ));

        assert_eq!(kept.name.as_deref(), Some("renamed"));
        assert_eq!(kept.topic, channel.topic);
        assert_eq!(kept.permission_overwrites, channel.permission_overwrites);
        assert_eq!(cleared.topic, None);
        assert!(cleared.permission_overwrites.is_empty());
        assert_eq!(cleared.name, channel.name);
    }

    #[test]
    fn a_thread_patch_updates_its_metadata() {
        let thread = Channel::from_wire(
            parse(include_str!("../../tests/fixtures/channel_thread.json")),
            None,
        );

        let archived = thread.patch(parse(
            r#"{"id": "300000000000000020", "thread_metadata": {"archived": true, "auto_archive_duration": 60, "archive_timestamp": "2024-03-02T08:30:00.000000+00:00", "locked": true}}"#,
        ));
        let counted = thread.patch(parse(
            r#"{"id": "300000000000000020", "message_count": 13}"#,
        ));

        let info = archived.thread.unwrap();
        assert!(info.archived && info.locked);
        assert_eq!(info.message_count, Some(12));
        assert_eq!(counted.thread.unwrap().message_count, Some(13));
    }

    #[test]
    fn a_guild_patch_keeps_roles_unless_sent() {
        let GatewayGuild::Available(wire) = ready().guilds.remove(0) else {
            panic!("expected an available guild");
        };
        let guild = Guild::from_wire(*wire);

        let renamed = guild.patch(parse(
            r#"{"id": "200000000000000001", "name": "Renamed", "icon": null}"#,
        ));
        let reroled = guild.patch(parse(
            r#"{"id": "200000000000000001", "roles": [{"id": "200000000000000001", "position": 0, "permissions": "0"}]}"#,
        ));

        assert_eq!(&*renamed.name, "Renamed");
        assert_eq!(renamed.icon, None);
        assert!(Arc::ptr_eq(&renamed.roles, &guild.roles));
        assert_eq!(renamed.member_count, guild.member_count);
        assert_eq!(reroled.roles.len(), 1);
        assert_eq!(reroled.name, guild.name);
    }

    #[test]
    fn a_member_patch_keeps_missing_fields_and_clears_null_ones() {
        let member = Member::from_wire(
            Snowflake::new(200_000_000_000_000_001),
            ready().merged_members.remove(0).remove(0),
        );
        let fixture = include_str!("../../tests/fixtures/guild_member_update.json");

        let timed_out = member.patch(parse(fixture));
        let cleared = member.patch(edited(fixture, |value| value["nick"] = Value::Null));

        assert_eq!(timed_out.nick, member.nick);
        assert!(timed_out.communication_disabled_until.is_some());
        assert_eq!(cleared.nick, None);
    }

    #[test]
    fn a_user_patch_updates_the_current_user() {
        let current = CurrentUser::from_wire(ready().user);

        let updated = current.patch(parse(include_str!("../../tests/fixtures/user_update.json")));
        let renamed = current.patch(parse(
            r#"{"id": "100000000000000001", "username": "new_name"}"#,
        ));

        assert_eq!(updated.user.global_name.as_deref(), Some("Akari"));
        assert!(
            updated
                .user
                .avatar
                .as_ref()
                .is_some_and(ImageHash::is_animated)
        );
        assert_eq!(updated.premium_type, PremiumType::Tier2);
        assert_eq!(&*renamed.user.username, "new_name");
        assert_eq!(renamed.user.global_name, current.user.global_name);
    }

    #[test]
    fn an_embeds_only_message_patch_keeps_the_content() {
        let message = Message::from_wire(
            parse(include_str!("../../tests/fixtures/message.json")),
            &mut new_arc,
        );

        let unfurled = message.patch(
            parse(
                r#"{"id": "400000000000000001", "channel_id": "300000000000000002", "embeds": []}"#,
            ),
            &mut new_arc,
        );
        let edited_message = message.patch(
            parse(include_str!("../../tests/fixtures/message_update.json")),
            &mut new_arc,
        );

        assert_eq!(unfurled.content, message.content);
        assert!(unfurled.embeds.is_empty());
        assert!(Arc::ptr_eq(&unfurled.author, &message.author));
        assert!(edited_message.content.ends_with("(fixed link)"));
        assert!(edited_message.edited_timestamp.is_some());
    }
}
