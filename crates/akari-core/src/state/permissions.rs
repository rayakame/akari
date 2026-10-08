use super::types::{Channel, Guild, Member};
use crate::model::{Permissions, UserId};

// AUTOMOD_QUARANTINED_NAME and AUTOMOD_QUARANTINED_GUILD_TAG.
const QUARANTINED: u64 = (1 << 7) | (1 << 10);
const SEND_EXTRAS: Permissions = Permissions(
    Permissions::MENTION_EVERYONE.0
        | Permissions::SEND_TTS_MESSAGES.0
        | Permissions::ATTACH_FILES.0
        | Permissions::EMBED_LINKS.0,
);

// A thread passes its parent, whose overwrites it inherits. In a thread, SEND_MESSAGES
// stands for SEND_MESSAGES_IN_THREADS, so callers check one bit everywhere.
pub(crate) fn compute(
    guild: &Guild,
    user: UserId,
    member: &Member,
    channel: &Channel,
    parent: Option<&Channel>,
    now_millis: i64,
) -> Permissions {
    if guild.owner_id == Some(user) {
        return Permissions::ALL;
    }
    let role = |id| guild.roles.iter().find(|role| role.id == id);
    let mut permissions = role(guild.id.cast()).map_or(Permissions::NONE, |role| role.permissions);
    for id in &member.roles {
        if let Some(role) = role(*id) {
            permissions |= role.permissions;
        }
    }
    if permissions.contains(Permissions::ADMINISTRATOR) {
        return Permissions::ALL;
    }

    let overwrites = &parent.unwrap_or(channel).permission_overwrites;
    let overwrite = |id: u64| overwrites.iter().find(|overwrite| overwrite.id.get() == id);
    if let Some(everyone) = overwrite(guild.id.get()) {
        permissions &= !everyone.deny;
        permissions |= everyone.allow;
    }
    let (mut allow, mut deny) = (Permissions::NONE, Permissions::NONE);
    for id in &member.roles {
        if let Some(role) = overwrite(id.get()) {
            allow |= role.allow;
            deny |= role.deny;
        }
    }
    permissions &= !deny;
    permissions |= allow;
    if let Some(own) = overwrite(user.get()) {
        permissions &= !own.deny;
        permissions |= own.allow;
    }

    let timed_out = member
        .communication_disabled_until
        .is_some_and(|until| until.unix_millis() > now_millis);
    if member.flags & QUARANTINED != 0 {
        permissions &= Permissions::VIEW_CHANNEL
            | Permissions::READ_MESSAGE_HISTORY
            | Permissions::CHANGE_NICKNAME;
    } else if timed_out {
        permissions &= Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    }

    if channel.thread.is_some() {
        permissions &= !Permissions::SEND_MESSAGES;
        if permissions.contains(Permissions::SEND_MESSAGES_IN_THREADS) {
            permissions |= Permissions::SEND_MESSAGES;
        }
    }
    if !permissions.contains(Permissions::VIEW_CHANNEL) {
        return Permissions::NONE;
    }
    if !permissions.contains(Permissions::SEND_MESSAGES) {
        permissions &= !SEND_EXTRAS;
    }
    permissions
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::model::{ChannelType, OverwriteType, Snowflake, Timestamp};
    use crate::state::types::{Channel, Guild, Member, PermissionOverwrite, Role, ThreadInfo};

    const GUILD: u64 = 1;
    const USER: u64 = 10;
    const ROLE_A: u64 = 2;
    const ROLE_B: u64 = 3;
    const NOON: &str = "\"2026-10-08T12:00:00+00:00\"";
    const HOUR: i64 = 3_600_000;

    const VIEW: Permissions = Permissions::VIEW_CHANNEL;
    const SEND: Permissions = Permissions::SEND_MESSAGES;
    const HISTORY: Permissions = Permissions::READ_MESSAGE_HISTORY;

    fn noon() -> Timestamp {
        serde_json::from_str(NOON).unwrap()
    }

    fn role(id: u64, permissions: Permissions) -> Role {
        Role {
            id: Snowflake::new(id),
            name: "role".into(),
            position: 0,
            permissions,
            color: 0,
            hoist: false,
        }
    }

    fn guild(everyone: Permissions, roles: &[(u64, Permissions)]) -> Guild {
        let mut all = vec![role(GUILD, everyone)];
        all.extend(roles.iter().map(|&(id, permissions)| role(id, permissions)));
        Guild {
            id: Snowflake::new(GUILD),
            name: "guild".into(),
            icon: None,
            banner: None,
            owner_id: Some(Snowflake::new(99)),
            roles: Arc::from(all),
            member_count: None,
            large: false,
        }
    }

    fn member(roles: &[u64]) -> Member {
        Member {
            guild_id: Snowflake::new(GUILD),
            nick: None,
            avatar: None,
            roles: roles.iter().copied().map(Snowflake::new).collect(),
            joined_at: None,
            communication_disabled_until: None,
            flags: 0,
            pending: false,
        }
    }

    fn overwrite(
        id: u64,
        kind: OverwriteType,
        allow: Permissions,
        deny: Permissions,
    ) -> PermissionOverwrite {
        PermissionOverwrite {
            id: Snowflake::new(id),
            kind,
            allow,
            deny,
        }
    }

    fn channel(overwrites: Vec<PermissionOverwrite>) -> Channel {
        Channel {
            id: Snowflake::new(100),
            kind: ChannelType::GuildText,
            guild_id: Some(Snowflake::new(GUILD)),
            parent_id: None,
            name: Some("general".into()),
            position: 0,
            topic: None,
            nsfw: false,
            rate_limit_per_user: 0,
            permission_overwrites: overwrites.into_boxed_slice(),
            recipients: Box::new([]),
            icon: None,
            owner_id: None,
            thread: None,
            flags: 0,
        }
    }

    fn thread() -> Channel {
        Channel {
            id: Snowflake::new(101),
            kind: ChannelType::PublicThread,
            parent_id: Some(Snowflake::new(100)),
            thread: Some(Box::new(ThreadInfo {
                archived: false,
                locked: false,
                auto_archive_duration: 60,
                archive_timestamp: noon(),
                create_timestamp: None,
                message_count: None,
                member_count: None,
            })),
            ..channel(Vec::new())
        }
    }

    fn of(guild: &Guild, member: &Member, channel: &Channel) -> Permissions {
        compute(
            guild,
            Snowflake::new(USER),
            member,
            channel,
            None,
            noon().unix_millis(),
        )
    }

    #[test]
    fn the_owner_gets_everything() {
        let mut guild = guild(Permissions::NONE, &[]);
        guild.owner_id = Some(Snowflake::new(USER));
        let denied = channel(vec![overwrite(
            GUILD,
            OverwriteType::Role,
            Permissions::NONE,
            VIEW,
        )]);

        assert_eq!(of(&guild, &member(&[]), &denied), Permissions::ALL);
    }

    #[test]
    fn administrator_bypasses_overwrites() {
        let guild = guild(VIEW, &[(ROLE_A, Permissions::ADMINISTRATOR)]);
        let denied = channel(vec![
            overwrite(GUILD, OverwriteType::Role, Permissions::NONE, VIEW),
            overwrite(USER, OverwriteType::Member, Permissions::NONE, VIEW),
        ]);

        assert_eq!(of(&guild, &member(&[ROLE_A]), &denied), Permissions::ALL);
    }

    #[test]
    fn everyone_and_member_roles_combine() {
        let guild = guild(VIEW, &[(ROLE_A, SEND), (ROLE_B, HISTORY)]);

        assert_eq!(
            of(&guild, &member(&[ROLE_A]), &channel(Vec::new())),
            VIEW | SEND
        );
        assert_eq!(
            of(&guild, &member(&[ROLE_A, ROLE_B]), &channel(Vec::new())),
            VIEW | SEND | HISTORY
        );
    }

    #[test]
    fn overwrites_apply_everyone_then_roles_then_member() {
        let guild = guild(
            VIEW | SEND,
            &[(ROLE_A, Permissions::NONE), (ROLE_B, Permissions::NONE)],
        );
        let roles_disagree = vec![
            overwrite(GUILD, OverwriteType::Role, Permissions::NONE, VIEW),
            overwrite(ROLE_A, OverwriteType::Role, Permissions::NONE, VIEW),
            overwrite(ROLE_B, OverwriteType::Role, VIEW, Permissions::NONE),
        ];
        let mut member_denies = roles_disagree.clone();
        member_denies.push(overwrite(
            USER,
            OverwriteType::Member,
            Permissions::NONE,
            VIEW,
        ));
        let role_beats_everyone = vec![
            overwrite(GUILD, OverwriteType::Role, HISTORY, Permissions::NONE),
            overwrite(
                ROLE_A,
                OverwriteType::Role,
                Permissions::NONE,
                HISTORY | SEND,
            ),
        ];

        let both = member(&[ROLE_A, ROLE_B]);
        assert_eq!(of(&guild, &both, &channel(roles_disagree)), VIEW | SEND);
        assert_eq!(
            of(&guild, &both, &channel(member_denies)),
            Permissions::NONE
        );
        assert_eq!(
            of(&guild, &member(&[ROLE_A]), &channel(role_beats_everyone)),
            VIEW
        );
    }

    #[test]
    fn overwrites_of_other_roles_and_members_dont_apply() {
        let guild = guild(VIEW, &[(ROLE_A, Permissions::NONE)]);
        let others = channel(vec![
            overwrite(ROLE_A, OverwriteType::Role, Permissions::NONE, VIEW),
            overwrite(USER + 1, OverwriteType::Member, Permissions::NONE, VIEW),
        ]);

        assert_eq!(of(&guild, &member(&[]), &others), VIEW);
    }

    #[test]
    fn no_view_channel_means_nothing() {
        let guild = guild(SEND | HISTORY, &[]);

        assert_eq!(
            of(&guild, &member(&[]), &channel(Vec::new())),
            Permissions::NONE
        );
    }

    #[test]
    fn no_send_messages_drops_the_send_extras() {
        let extras = Permissions::EMBED_LINKS
            | Permissions::ATTACH_FILES
            | Permissions::MENTION_EVERYONE
            | Permissions::SEND_TTS_MESSAGES;
        let guild = guild(VIEW | HISTORY | extras, &[]);

        assert_eq!(
            of(&guild, &member(&[]), &channel(Vec::new())),
            VIEW | HISTORY
        );
    }

    #[test]
    fn threads_use_the_parent_and_send_in_threads() {
        let in_threads = Permissions::SEND_MESSAGES_IN_THREADS;
        let parent = channel(Vec::new());
        let hidden_parent = channel(vec![overwrite(
            GUILD,
            OverwriteType::Role,
            Permissions::NONE,
            VIEW,
        )]);
        let thread_of = |guild: &Guild, parent: &Channel| {
            compute(
                guild,
                Snowflake::new(USER),
                &member(&[]),
                &thread(),
                Some(parent),
                noon().unix_millis(),
            )
        };

        assert_eq!(thread_of(&guild(VIEW | SEND, &[]), &parent), VIEW);
        assert_eq!(
            thread_of(&guild(VIEW | in_threads, &[]), &parent),
            VIEW | SEND | in_threads
        );
        assert_eq!(
            thread_of(&guild(VIEW | in_threads, &[]), &hidden_parent),
            Permissions::NONE
        );
    }

    #[test]
    fn a_timeout_leaves_view_and_history() {
        let guild = guild(VIEW | SEND | HISTORY | Permissions::CHANGE_NICKNAME, &[]);
        let mut timed_out = member(&[]);
        timed_out.communication_disabled_until = Some(noon());

        let during = compute(
            &guild,
            Snowflake::new(USER),
            &timed_out,
            &channel(Vec::new()),
            None,
            noon().unix_millis() - HOUR,
        );
        let after = compute(
            &guild,
            Snowflake::new(USER),
            &timed_out,
            &channel(Vec::new()),
            None,
            noon().unix_millis() + HOUR,
        );

        assert_eq!(during, VIEW | HISTORY);
        assert_eq!(after, VIEW | SEND | HISTORY | Permissions::CHANGE_NICKNAME);
    }

    #[test]
    fn admins_ignore_timeouts() {
        let guild = guild(VIEW, &[(ROLE_A, Permissions::ADMINISTRATOR)]);
        let mut timed_out = member(&[ROLE_A]);
        timed_out.communication_disabled_until = Some(noon());

        let permissions = compute(
            &guild,
            Snowflake::new(USER),
            &timed_out,
            &channel(Vec::new()),
            None,
            noon().unix_millis() - HOUR,
        );

        assert_eq!(permissions, Permissions::ALL);
    }

    #[test]
    fn quarantine_also_keeps_change_nickname() {
        let guild = guild(VIEW | SEND | HISTORY | Permissions::CHANGE_NICKNAME, &[]);
        for flag in [1 << 7, 1 << 10] {
            let mut quarantined = member(&[]);
            quarantined.flags = flag;

            assert_eq!(
                of(&guild, &quarantined, &channel(Vec::new())),
                VIEW | HISTORY | Permissions::CHANGE_NICKNAME
            );
        }
    }

    #[test]
    fn a_missing_everyone_role_counts_as_no_permissions() {
        let mut guild = guild(VIEW, &[(ROLE_A, SEND)]);
        guild.roles = Arc::from(vec![role(ROLE_A, VIEW | SEND)]);

        assert_eq!(
            of(&guild, &member(&[]), &channel(Vec::new())),
            Permissions::NONE
        );
        assert_eq!(
            of(&guild, &member(&[ROLE_A]), &channel(Vec::new())),
            VIEW | SEND
        );
    }
}
