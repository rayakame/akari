use std::collections::HashSet;
use std::sync::Arc;

use super::types::Channel;
use crate::model::{ChannelId, ChannelType};

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
    use crate::model::{ChannelType, Snowflake};

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
        })
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
}
