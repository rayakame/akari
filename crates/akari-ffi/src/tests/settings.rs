use std::time::{Duration, UNIX_EPOCH};

use akari_core::model::{ChannelId, GuildId};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::fake::{FakeGateway, GENERAL, GUILD, VOICE, events_until, online};
use super::gateway::send;
use super::support::{USER, token};
use crate::records::{
    DisplaySettings, ExplicitContentFilter, ExplicitContentSettings, Folder, GuildSettings,
    Redaction, ServerListItem, SpoilerSetting, StickerAnimation,
};
use crate::subscription::StoreEvent;

const DM: ChannelId = ChannelId::new(300_000_000_000_000_010);
const LATER: &str = "2099-01-01T00:00:00.000000+00:00";
const LATER_SECONDS: u64 = 4_070_908_800;

fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

fn bytes_field(number: u64, payload: &[u8]) -> Vec<u8> {
    [
        varint(number << 3 | 2),
        varint(payload.len() as u64),
        payload.to_vec(),
    ]
    .concat()
}

fn varint_field(number: u64, value: u64) -> Vec<u8> {
    [varint(number << 3), varint(value)].concat()
}

// The settings proto with one folder: field 14, its folders list, the folder's fields.
fn folder_proto(id: i64, name: &str, color: u64, guilds: &[GuildId]) -> String {
    let ids: Vec<u8> = guilds
        .iter()
        .flat_map(|id| id.get().to_le_bytes())
        .collect();
    let folder = [
        bytes_field(1, &ids),
        bytes_field(2, &varint_field(1, id as u64)),
        bytes_field(3, &bytes_field(1, name.as_bytes())),
        bytes_field(4, &varint_field(1, color)),
    ]
    .concat();
    STANDARD.encode(bytes_field(14, &bytes_field(1, &folder)))
}

fn muted_guild(guild: GuildId) -> Value {
    json!({
        "guild_id": guild.get().to_string(),
        "muted": true,
        "mute_config": {"end_time": LATER, "selected_time_window": 3600},
        "message_notifications": 3,
        "suppress_everyone": false,
        "suppress_roles": false,
        "hide_muted_channels": false,
        "flags": 0,
        "channel_overrides": [],
    })
}

#[tokio::test]
async fn the_server_list_crosses_with_its_folders() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway
        .serve_ready_with(|ready| {
            ready["user_settings_proto"] = folder_proto(7, "Games", 0x5865f2, &[GUILD]).into();
        })
        .await;
    events_until(&subscription, online).await;

    assert_eq!(
        account.store().server_list(),
        [ServerListItem::Folder {
            folder: Folder {
                id: 7,
                name: Some("Games".to_owned()),
                color: Some(0x5865f2),
                guild_ids: vec![GUILD],
            }
        }]
    );
    assert_eq!(account.store().guild_ids(), [GUILD]);
    account.close();
}

#[tokio::test]
async fn guild_settings_cross_for_guilds_and_dms() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();

    assert_eq!(
        store.guild_settings(Some(GUILD)),
        GuildSettings {
            muted: false,
            mute_ends_at: None,
            suppress_everyone: true,
            suppress_roles: false,
            hide_muted_channels: true,
        }
    );
    assert_eq!(
        store.guild_settings(None),
        GuildSettings {
            muted: false,
            mute_ends_at: None,
            suppress_everyone: false,
            suppress_roles: false,
            hide_muted_channels: false,
        }
    );

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "USER_GUILD_SETTINGS_UPDATE", "d": muted_guild(GUILD)}),
    )
    .await;
    events_until(&subscription, |event| {
        matches!(event, StoreEvent::GuildSettingsChanged { .. })
    })
    .await;

    let lab = store.guild_settings(Some(GUILD));
    assert!(lab.muted);
    assert_eq!(
        lab.mute_ends_at,
        Some(UNIX_EPOCH + Duration::from_secs(LATER_SECONDS))
    );
    account.close();
}

#[tokio::test]
async fn muted_channels_reads_a_batch() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let _ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    // The fixture mutes the DM until it is unmuted, and GENERAL until a time long past.
    let muted = account
        .store()
        .muted_channels(vec![GENERAL, DM, VOICE, ChannelId::new(1)]);

    assert_eq!(muted, [DM]);
    account.close();
}

#[tokio::test]
async fn the_settings_events_carry_ids_only() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;

    let mut dms = muted_guild(GUILD);
    dms["guild_id"] = Value::Null;
    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "USER_GUILD_SETTINGS_UPDATE", "d": muted_guild(GUILD)}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 3, "t": "USER_GUILD_SETTINGS_UPDATE", "d": dms}),
    )
    .await;
    send(
        &mut ws,
        json!({"op": 0, "s": 4, "t": "USER_SETTINGS_PROTO_UPDATE", "d": {
            "settings": {"type": 1, "proto": folder_proto(7, "Games", 0, &[GUILD])},
            "partial": false,
        }}),
    )
    .await;
    let events = events_until(&subscription, |event| {
        *event == StoreEvent::ServerListChanged
    })
    .await;

    assert!(events.contains(&StoreEvent::GuildSettingsChanged {
        guild_id: Some(GUILD)
    }));
    assert!(events.contains(&StoreEvent::GuildSettingsChanged { guild_id: None }));
    account.close();
}

// text_and_images (6): gif_auto_play (11) off, animate_emoji (14) off, animate_stickers (15)
// never.
fn quiet_display_proto() -> String {
    let gif = bytes_field(11, &varint_field(1, 0));
    let emoji = bytes_field(14, &varint_field(1, 0));
    let stickers = bytes_field(15, &varint_field(1, 2));
    STANDARD.encode(bytes_field(6, &[gif, emoji, stickers].concat()))
}

#[tokio::test]
async fn display_settings_cross_with_their_event() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway.serve_ready().await;
    events_until(&subscription, online).await;
    let store = account.store();

    assert_eq!(
        store.display_settings(),
        DisplaySettings {
            render_spoilers: SpoilerSetting::OnClick,
            view_image_descriptions: false,
            inline_attachment_media: true,
            inline_embed_media: true,
            gif_auto_play: true,
            render_embeds: true,
            animate_emoji: true,
            animate_stickers: StickerAnimation::Always,
            explicit_content_filter: ExplicitContentFilter::NonFriends,
            explicit_content: ExplicitContentSettings {
                guilds: Redaction::Unset,
                friend_dm: Redaction::Unset,
                non_friend_dm: Redaction::Unset,
            },
            gore: ExplicitContentSettings {
                guilds: Redaction::Unset,
                friend_dm: Redaction::Unset,
                non_friend_dm: Redaction::Unset,
            },
            self_harm: ExplicitContentSettings {
                guilds: Redaction::Unset,
                friend_dm: Redaction::Unset,
                non_friend_dm: Redaction::Unset,
            },
        }
    );

    send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "USER_SETTINGS_PROTO_UPDATE", "d": {
            "settings": {"type": 1, "proto": quiet_display_proto()},
            "partial": true,
        }}),
    )
    .await;
    events_until(&subscription, |event| {
        *event == StoreEvent::DisplaySettingsChanged
    })
    .await;

    let display = store.display_settings();
    assert!(!display.gif_auto_play);
    assert!(!display.animate_emoji);
    assert_eq!(display.animate_stickers, StickerAnimation::Never);
    assert!(display.render_embeds);
    account.close();
}
