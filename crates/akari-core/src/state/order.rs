use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use super::types::{Channel, Guild};
use crate::model::{ChannelId, ChannelType, Timestamp};

/// A guild's channels in the order Discord lists them: channels outside a category first,
/// then each category followed by its channels. Text-like channels come before voice and
/// stage channels, each by position, then ID. Threads are left out.
pub fn display_order(channels: &[Arc<Channel>]) -> Vec<Arc<Channel>> {
    let categories: HashSet<ChannelId> = channels
        .iter()
        .filter(|channel| channel.kind == ChannelType::GuildCategory)
        .map(|channel| channel.id)
        .collect();
    let in_category = |channel: &Channel| {
        channel
            .parent_id
            .filter(|parent| categories.contains(parent))
    };
    let sorted = |mut list: Vec<Arc<Channel>>| {
        list.sort_by_key(|channel| (is_voice(channel), channel.position, channel.id));
        list
    };
    let listed =
        |channel: &Channel| !channel.is_thread() && channel.kind != ChannelType::GuildCategory;

    let mut ordered = sorted(
        channels
            .iter()
            .filter(|channel| listed(channel) && in_category(channel).is_none())
            .cloned()
            .collect(),
    );
    let mut category_list: Vec<&Arc<Channel>> = channels
        .iter()
        .filter(|channel| channel.kind == ChannelType::GuildCategory)
        .collect();
    category_list.sort_by_key(|category| (category.position, category.id));
    for category in category_list {
        ordered.push(category.clone());
        ordered.extend(sorted(
            channels
                .iter()
                .filter(|channel| listed(channel) && in_category(channel) == Some(category.id))
                .cloned()
                .collect(),
        ));
    }
    ordered
}

// The settings proto's order isn't read yet; Discord puts a newly joined server at the top.
pub(crate) fn guild_order(mut guilds: Vec<(Arc<Guild>, Option<Timestamp>)>) -> Vec<Arc<Guild>> {
    guilds.sort_by(|(a, a_joined), (b, b_joined)| {
        b_joined
            .is_some()
            .cmp(&a_joined.is_some())
            .then(b_joined.cmp(a_joined))
            .then(b.id.cmp(&a.id))
    });
    guilds.into_iter().map(|(guild, _)| guild).collect()
}

pub(crate) fn private_channel_order(mut channels: Vec<Arc<Channel>>) -> Vec<Arc<Channel>> {
    channels.sort_by_key(|channel| {
        let activity = channel
            .last_message_id
            .map_or(channel.id.get(), |last| last.get());
        Reverse((activity, channel.id))
    });
    channels
}

// `ordered` comes from `display_order`: channels outside a category, then each category
// followed by its channels.
pub(crate) fn visible_channels(
    ordered: Vec<Arc<Channel>>,
    can_view: impl Fn(&Channel) -> bool,
) -> Vec<Arc<Channel>> {
    let mut visible = Vec::with_capacity(ordered.len());
    let mut section = Vec::new();
    for channel in ordered {
        if channel.kind == ChannelType::GuildCategory {
            push_section(&mut visible, std::mem::take(&mut section), &can_view);
        }
        section.push(channel);
    }
    push_section(&mut visible, section, &can_view);
    visible
}

// Servers use empty categories as dividers, so those show when the category itself does.
fn push_section(
    visible: &mut Vec<Arc<Channel>>,
    section: Vec<Arc<Channel>>,
    can_view: &impl Fn(&Channel) -> bool,
) {
    let mut channels = section.into_iter().peekable();
    let category = channels.next_if(|channel| channel.kind == ChannelType::GuildCategory);
    let empty = channels.peek().is_none();
    let shown: Vec<_> = channels.filter(|channel| can_view(channel)).collect();
    if let Some(category) = category
        && (!shown.is_empty() || (empty && can_view(&category)))
    {
        visible.push(category);
    }
    visible.extend(shown);
}

fn is_voice(channel: &Channel) -> bool {
    matches!(
        channel.kind,
        ChannelType::GuildVoice | ChannelType::GuildStageVoice
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::model::{ChannelType, Snowflake, Timestamp};

    fn channel(id: u64, kind: ChannelType, parent: Option<u64>, position: i32) -> Arc<Channel> {
        Arc::new(Channel {
            id: Snowflake::new(id),
            kind,
            guild_id: Some(Snowflake::new(1)),
            parent_id: parent.map(Snowflake::new),
            name: Some(id.to_string().into()),
            position,
            topic: None,
            nsfw: false,
            rate_limit_per_user: 0,
            permission_overwrites: Box::new([]),
            recipients: Box::new([]),
            icon: None,
            owner_id: None,
            thread: None,
            flags: 0,
            last_message_id: None,
        })
    }

    fn private(id: u64, last: Option<u64>) -> Arc<Channel> {
        let mut channel = (*channel(id, ChannelType::Dm, None, 0)).clone();
        channel.guild_id = None;
        channel.last_message_id = last.map(Snowflake::new);
        Arc::new(channel)
    }

    #[test]
    fn display_order_matches_discord() {
        use ChannelType::*;
        let channels = vec![
            channel(30, GuildCategory, None, 1),
            channel(20, GuildCategory, None, 0),
            channel(31, GuildVoice, Some(30), 0),
            channel(32, GuildText, Some(30), 5),
            channel(33, GuildText, Some(30), 5),
            channel(34, GuildForum, Some(30), 9),
            channel(21, GuildStageVoice, Some(20), 0),
            channel(22, GuildNews, Some(20), 3),
            channel(11, GuildVoice, None, 0),
            channel(10, GuildText, None, 4),
            channel(40, PublicThread, Some(10), 0),
        ];

        let ordered: Vec<u64> = display_order(&channels)
            .iter()
            .map(|channel| channel.id.get())
            .collect();

        assert_eq!(ordered, [10, 11, 20, 22, 21, 30, 32, 33, 34, 31]);
    }

    #[test]
    fn guilds_come_newest_joined_first() {
        let guild = |id: u64| {
            Arc::new(Guild {
                id: Snowflake::new(id),
                name: id.to_string().into(),
                icon: None,
                banner: None,
                owner_id: None,
                roles: Arc::new([]),
                member_count: None,
                large: false,
            })
        };
        let joined = |millis| Some(Timestamp::from_unix_millis(millis));
        let guilds = vec![
            (guild(1), joined(1_000)),
            (guild(3), None),
            (guild(2), joined(2_000)),
            (guild(4), None),
            (guild(5), joined(2_000)),
        ];

        let ordered: Vec<u64> = guild_order(guilds)
            .iter()
            .map(|guild| guild.id.get())
            .collect();

        assert_eq!(ordered, [5, 2, 1, 4, 3]);
    }

    #[test]
    fn hidden_channels_and_their_categories_are_left_out() {
        use ChannelType::*;
        let channels = vec![
            channel(10, GuildCategory, None, 0),
            channel(11, GuildText, Some(10), 0),
            channel(12, GuildText, Some(10), 1),
            channel(20, GuildCategory, None, 1),
            channel(21, GuildText, Some(20), 0),
            channel(30, GuildCategory, None, 2),
            channel(40, GuildCategory, None, 3),
            channel(1, GuildText, None, 0),
            channel(2, GuildText, None, 1),
        ];
        let hidden = [12, 21, 40, 1];

        let visible: Vec<u64> = visible_channels(display_order(&channels), |channel| {
            !hidden.contains(&channel.id.get())
        })
        .iter()
        .map(|channel| channel.id.get())
        .collect();

        assert_eq!(visible, [2, 10, 11, 30]);
    }

    #[test]
    fn channels_whose_category_is_missing_go_to_the_top() {
        let channels = vec![
            channel(20, ChannelType::GuildCategory, None, 0),
            channel(21, ChannelType::GuildText, Some(20), 0),
            channel(10, ChannelType::GuildText, Some(99), 0),
        ];

        let ordered: Vec<u64> = display_order(&channels)
            .iter()
            .map(|channel| channel.id.get())
            .collect();

        assert_eq!(ordered, [10, 20, 21]);
    }

    #[test]
    fn private_channels_come_latest_conversation_first() {
        let channels = vec![
            private(3, None),
            private(4, Some(9)),
            private(7, None),
            private(8, Some(5)),
            private(6, Some(7)),
        ];

        let ordered: Vec<u64> = private_channel_order(channels)
            .iter()
            .map(|channel| channel.id.get())
            .collect();

        assert_eq!(ordered, [4, 7, 6, 8, 3]);
    }
}
