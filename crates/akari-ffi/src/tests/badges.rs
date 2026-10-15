use akari_core::model::{ChannelId, GuildId, MessageId};
use serde_json::{Value, json};

use super::fake::{FakeGateway, GENERAL, GUILD, VOICE, events_until, fixture, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::records::{Badge, ChannelBadge, GuildBadge, ReadState};
use crate::subscription::StoreEvent;

const DM: ChannelId = ChannelId::new(300_000_000_000_000_010);
const GROUP: ChannelId = ChannelId::new(300_000_000_000_000_011);

fn dm_message(id: u64, channel: ChannelId, mentions_me: bool) -> Value {
    let mut message = fixture(include_str!(
        "../../../akari-core/tests/fixtures/message_create.json"
    ));
    message["id"] = id.to_string().into();
    message["channel_id"] = channel.get().to_string().into();
    let fields = message.as_object_mut().unwrap();
    fields.remove("guild_id");
    fields.remove("member");
    if !mentions_me {
        message["mentions"] = json!([]);
    }
    message
}

#[tokio::test]
async fn badges_cross_in_one_read_each() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();
    let unread = Badge {
        unread: true,
        mentions: 2,
    };
    let none = Badge {
        unread: false,
        mentions: 0,
    };

    let channels = store.channel_badges(vec![GENERAL, VOICE, ChannelId::new(1)]);
    let guilds = store.guild_badges(vec![GUILD, GuildId::new(1)]);

    assert_eq!(
        channels,
        [
            ChannelBadge {
                channel_id: GENERAL,
                badge: unread,
            },
            ChannelBadge {
                channel_id: VOICE,
                badge: none,
            },
            ChannelBadge {
                channel_id: ChannelId::new(1),
                badge: none,
            },
        ]
    );
    assert_eq!(
        guilds,
        [
            GuildBadge {
                guild_id: GUILD,
                badge: unread,
            },
            GuildBadge {
                guild_id: GuildId::new(1),
                badge: none,
            },
        ]
    );
    account.close();
}

#[tokio::test]
async fn the_read_state_crosses() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();

    assert_eq!(
        store.read_state(GENERAL),
        Some(ReadState {
            acked: Some(MessageId::new(400_000_000_000_000_002)),
            mentions: 2,
            estimated: false,
        })
    );
    assert_eq!(store.read_state(GROUP), None);
    account.close();
}

#[tokio::test]
async fn badge_events_carry_ids_only() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_ACK", "d": {
            "channel_id": GENERAL.get().to_string(),
            "message_id": "400000000000000003",
            "version": 2,
        }}),
    )
    .await;
    let events = events_until(&subscription, |event| {
        matches!(event, StoreEvent::GuildBadgeChanged { .. })
    })
    .await;

    let badges: Vec<&StoreEvent> = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                StoreEvent::BadgeChanged { .. } | StoreEvent::GuildBadgeChanged { .. }
            )
        })
        .collect();
    assert_eq!(
        badges,
        [
            &StoreEvent::BadgeChanged {
                channel_id: GENERAL,
                guild_id: Some(GUILD),
            },
            &StoreEvent::GuildBadgeChanged { guild_id: GUILD },
        ]
    );
    account.close();
}

#[tokio::test]
async fn unread_private_channels_cross_in_order() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();
    assert!(store.unread_private_channels().is_empty());

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": dm_message(400_000_000_000_000_022, GROUP, false)}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 3, "t": "MESSAGE_CREATE", "d": dm_message(400_000_000_000_000_023, DM, true)}),
    )
    .await;
    events_until(&subscription, |event| {
        *event
            == StoreEvent::BadgeChanged {
                channel_id: DM,
                guild_id: None,
            }
    })
    .await;

    assert_eq!(store.unread_private_channels(), [DM, GROUP]);
    account.close();
}
