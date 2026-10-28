use akari_core::model::{ChannelId, EmojiId, TagId};
use serde_json::json;

use super::fake::{FakeGateway, events_until, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::images::ImageKey;
use crate::records::{
    ForumLayout, ForumSort, ForumTag, ForumView, OlderThreads, TagEmoji, TagMatching,
};
use crate::subscription::StoreEvent;

const FORUM: ChannelId = ChannelId::new(300_000_000_000_000_050);
const POST: ChannelId = ChannelId::new(300_000_000_000_000_051);
const BUG: TagId = TagId::new(500_000_000_000_000_001);

// The forum's settings, a post's tags and pin, and its page cross as records (task 171).
#[tokio::test]
async fn a_forum_and_its_posts_cross_as_records() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "CHANNEL_CREATE", "d": {
            "id": FORUM.get().to_string(), "type": 15, "guild_id": "200000000000000001",
            "name": "help", "position": 3, "flags": 0, "permission_overwrites": [],
            "default_sort_order": 1, "default_forum_layout": 2,
            "available_tags": [{"id": BUG.get().to_string(), "name": "Bug", "moderated": true, "emoji_id": "600000000000000001"}],
        }}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 3, "t": "THREAD_CREATE", "d": {
            "id": POST.get().to_string(), "type": 11, "guild_id": "200000000000000001",
            "parent_id": FORUM.get().to_string(), "name": "a question", "flags": 2,
            "owner_id": "100000000000000002", "applied_tags": [BUG.get().to_string()],
            "total_message_sent": 4, "message_count": 4, "member_count": 1,
            "thread_metadata": {"archived": false, "auto_archive_duration": 1440,
                "archive_timestamp": "2024-03-02T08:30:00.250000+00:00", "locked": false},
        }}),
    )
    .await;
    events_until(
        &subscription,
        |event| matches!(event, StoreEvent::ChannelAdded { channel_id, .. } if *channel_id == POST),
    )
    .await;
    let store = account.store();

    let forum = store
        .channel(FORUM)
        .and_then(|forum| forum.forum)
        .expect("settings");
    assert_eq!(forum.sort, ForumSort::CreationDate);
    assert_eq!(forum.layout, ForumLayout::Gallery);
    assert_eq!(forum.tag_matching, TagMatching::Some);
    let emoji = EmojiId::new(600_000_000_000_000_001);
    assert_eq!(
        forum.tags,
        [ForumTag {
            id: BUG,
            name: "Bug".to_owned(),
            moderated: true,
            emoji: Some(TagEmoji::Custom {
                id: emoji,
                key: ImageKey::Emoji {
                    emoji_id: emoji,
                    animated: false
                },
            }),
        }]
    );
    let post = store
        .channel(POST)
        .and_then(|post| post.thread)
        .expect("a post");
    assert_eq!(post.applied_tags, [BUG]);
    assert_eq!(post.total_message_sent, Some(4));
    assert!(post.pinned);

    account.open_forum(Some(FORUM));
    events_until(
        &subscription,
        |event| matches!(event, StoreEvent::ForumPostsChanged { forum_id } if *forum_id == FORUM),
    )
    .await;
    let view = ForumView {
        tags: vec![BUG],
        ..ForumView::default()
    };
    let posts = store.forum_posts(FORUM, view);
    assert_eq!(posts.active, [POST]);
    assert_eq!(posts.older, OlderThreads::NotLoaded);
    account.close();
}
