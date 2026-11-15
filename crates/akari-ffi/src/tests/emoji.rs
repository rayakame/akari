use akari_core::emoji as core_emoji;
use akari_core::model::EmojiId;
use serde_json::json;

use super::fake::{FakeGateway, GENERAL, GUILD, events_until, online};
use super::support::{USER, token};
use crate::emoji::{
    CustomEmoji, EmojiAccess, EmojiGroup, EmojiVersion, PickerEmoji, newest_emoji_version,
    picker_reaction, unicode_emoji,
};
use crate::formatted::emoji_shortcode;
use crate::images::ImageKey;
use crate::records::ReactionEmoji;
use crate::subscription::StoreEvent;

const WAVE: u64 = 700_000_000_000_000_001;
const DANCE: u64 = 700_000_000_000_000_002;
const LOST: u64 = 700_000_000_000_000_003;
const SUBS: u64 = 700_000_000_000_000_004;
const NEWEST: EmojiVersion = EmojiVersion {
    major: 18,
    minor: 0,
};

// The picker's names and the tooltips' come from two crates that can't share the rule:
// akari-core doesn't depend on akari-markdown. Every Unicode emoji, skin tones too, gets the
// same name from both, but ©, ® and ™, which Discord leaves as text in a message.
#[test]
fn the_picker_and_the_tooltips_name_every_emoji_the_same() {
    let mut checked = 0;
    for group in core_emoji::unicode_groups(core_emoji::EmojiVersion::NEWEST) {
        for emoji in group.emojis {
            let tones: Vec<&str> = emojis_of(emoji.as_str());
            for text in tones {
                let picker = core_emoji::UnicodeEmoji::get(text).unwrap().name();
                match emoji_shortcode(text.to_owned()) {
                    Some(tooltip) => assert_eq!(picker, tooltip, "{text}"),
                    None => assert!(
                        matches!(picker.as_ref(), "copyright" | "registered" | "tm"),
                        "{text} ({picker}) has no tooltip name"
                    ),
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 3_900, "{checked}");
}

// The emoji and its skin-toned variants.
fn emojis_of(emoji: &str) -> Vec<&'static str> {
    let found = emojis::get(emoji).unwrap();
    match found.skin_tones() {
        Some(tones) => tones.map(emojis::Emoji::as_str).collect(),
        None => vec![found.as_str()],
    }
}

#[test]
fn the_catalog_crosses_in_its_groups() {
    let groups = unicode_emoji(NEWEST);

    let order: Vec<EmojiGroup> = groups.iter().map(|group| group.group).collect();
    assert_eq!(
        order,
        [
            EmojiGroup::SmileysAndEmotion,
            EmojiGroup::PeopleAndBody,
            EmojiGroup::AnimalsAndNature,
            EmojiGroup::FoodAndDrink,
            EmojiGroup::TravelAndPlaces,
            EmojiGroup::Activities,
            EmojiGroup::Objects,
            EmojiGroup::Symbols,
            EmojiGroup::Flags,
        ]
    );
    let up = groups[1]
        .emojis
        .iter()
        .find(|emoji| emoji.emoji == "👍")
        .unwrap();
    assert_eq!(up.name, "thumbsup");
    assert_eq!(up.shortcodes, ["+1"]);
    assert_eq!(up.cldr_name, "thumbs up");
    assert!(up.skin_tones);
    assert_eq!(up.version, EmojiVersion { major: 0, minor: 6 });
    assert_eq!(newest_emoji_version(), NEWEST);

    let older = unicode_emoji(EmojiVersion {
        major: 15,
        minor: 0,
    });
    let count = |groups: &[crate::emoji::UnicodeEmojiGroup]| -> usize {
        groups.iter().map(|group| group.emojis.len()).sum()
    };
    assert!(count(&older) < count(&groups));
}

#[test]
fn a_picked_emoji_reacts_as_the_core_says() {
    let unicode = unicode_emoji(NEWEST)[0].emojis[0].clone();
    assert_eq!(
        picker_reaction(PickerEmoji::Unicode { emoji: unicode }),
        ReactionEmoji {
            id: None,
            name: Some("😀".to_owned()),
            animated: false,
            key: None,
        }
    );
    let custom = CustomEmoji {
        id: EmojiId::new(DANCE),
        guild_id: GUILD,
        name: "dance".to_owned(),
        animated: true,
        key: ImageKey::Emoji {
            emoji_id: EmojiId::new(DANCE),
            animated: true,
        },
        access: EmojiAccess::Usable,
    };
    assert_eq!(
        picker_reaction(PickerEmoji::Custom { emoji: custom }),
        ReactionEmoji {
            id: Some(EmojiId::new(DANCE)),
            name: Some("dance".to_owned()),
            animated: true,
            key: Some(ImageKey::Emoji {
                emoji_id: EmojiId::new(DANCE),
                animated: true,
            }),
        }
    );
}

fn names(emojis: &[PickerEmoji]) -> Vec<String> {
    emojis
        .iter()
        .map(|emoji| match emoji {
            PickerEmoji::Unicode { emoji } => emoji.name.clone(),
            PickerEmoji::Custom { emoji } => emoji.name.clone(),
        })
        .collect()
}

#[tokio::test]
async fn the_pickers_reads_cross() {
    let gateway = FakeGateway::start().await;
    let account = gateway.client().account(token("t"), USER).unwrap();
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut ws = gateway
        .serve_ready_with(|ready| {
            ready["guilds"][0]["emojis"] = json!([
                {"id": WAVE.to_string(), "name": "wave"},
                {"id": DANCE.to_string(), "name": "dance", "animated": true},
                {"id": LOST.to_string(), "name": "lost", "available": false},
                {"id": SUBS.to_string(), "name": "subs", "roles": ["500000000000000009"]},
            ]);
        })
        .await;
    events_until(&subscription, online).await;
    let store = account.store();

    let sections = store.custom_emoji_sections(GENERAL);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].guild_id, GUILD);
    let custom = |id: u64, name: &str, animated: bool, access: EmojiAccess| CustomEmoji {
        id: EmojiId::new(id),
        guild_id: GUILD,
        name: name.to_owned(),
        animated,
        key: ImageKey::Emoji {
            emoji_id: EmojiId::new(id),
            animated,
        },
        access,
    };
    assert_eq!(
        sections[0].emojis,
        [
            custom(WAVE, "wave", false, EmojiAccess::Usable),
            custom(DANCE, "dance", true, EmojiAccess::NitroLocked),
            custom(LOST, "lost", false, EmojiAccess::Unavailable),
        ]
    );
    assert_eq!(
        store.emoji_access(GENERAL, EmojiId::new(WAVE)),
        Some(EmojiAccess::Usable)
    );
    assert_eq!(
        store.emoji_access(GENERAL, EmojiId::new(SUBS)),
        Some(EmojiAccess::RoleLocked)
    );
    assert_eq!(store.emoji_access(GENERAL, EmojiId::new(9)), None);
    assert_eq!(
        names(&store.quick_reactions(GENERAL, NEWEST)),
        ["100", "laughing", "sparkling_heart"]
    );
    assert!(store.recent_reactions(GENERAL, NEWEST).is_empty());

    let found = store
        .search_emoji(GENERAL, ":wave".to_owned(), NEWEST, 2)
        .await;
    assert_eq!(names(&found), ["wave", "wave"]);
    assert!(matches!(found[0], PickerEmoji::Custom { .. }), "{found:?}");

    super::gateway::send(
        &mut ws,
        json!({"op": 0, "s": 2, "t": "GUILD_EMOJIS_UPDATE",
               "d": {"guild_id": GUILD.get().to_string(), "emojis": []}}),
    )
    .await;
    events_until(&subscription, |event| {
        *event == StoreEvent::GuildEmojisChanged { guild_id: GUILD }
    })
    .await;
    assert!(store.custom_emoji_sections(GENERAL).is_empty());
}
