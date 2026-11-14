use super::*;
use crate::emoji::EmojiVersion;
use crate::model::EmojiId;
use crate::state::picker::{EmojiAccess, EmojiSection, PickerEmoji};
use crate::state::reactions::ReactionEmoji;

const WAVE: u64 = 700_000_000_000_000_001;
const DANCE: u64 = 700_000_000_000_000_002;
const LOST: u64 = 700_000_000_000_000_003;
const SUBS: u64 = 700_000_000_000_000_004;
const TWITCH: u64 = 700_000_000_000_000_005;
const MINE: u64 = 700_000_000_000_000_006;
const POG: u64 = 700_000_000_000_000_011;
const GARDEN_TWITCH: u64 = 700_000_000_000_000_012;
// G3's only channel: @everyone may view it and nothing else.
const GARDEN: u64 = 300_000_000_000_000_031;
const ADMIN_ROLE: &str = "500000000000000002";
const OTHER_ROLE: &str = "500000000000000009";

fn emoji(id: u64, name: &str) -> Value {
    json!({"id": id.to_string(), "name": name})
}

// G1 (where the user is an administrator) has a plain, an animated, an unavailable, a
// role-locked, an integration's and a role's emoji the user has; G3 a plain and an
// integration's.
fn picker_state(nitro: bool) -> State {
    let mut ready = ready_value();
    ready["user"]["premium_type"] = json!(if nitro { 2 } else { 0 });
    ready["guilds"][0]["emojis"] = json!([
        emoji(WAVE, "wave"),
        {"id": DANCE.to_string(), "name": "dance", "animated": true},
        {"id": LOST.to_string(), "name": "lost", "available": false},
        {"id": SUBS.to_string(), "name": "subs", "roles": [OTHER_ROLE]},
        {"id": TWITCH.to_string(), "name": "twitch", "managed": true},
        {"id": MINE.to_string(), "name": "mine", "roles": [ADMIN_ROLE]},
    ]);
    let mut state = state_from(&ready);
    let mut garden = guild_create(G3);
    garden["emojis"] = json!([
        emoji(POG, "pog"),
        {"id": GARDEN_TWITCH.to_string(), "name": "garden_twitch", "managed": true},
    ]);
    apply(&mut state, "GUILD_CREATE", garden);
    state
}

fn access(state: &State, channel: u64, emoji: u64) -> Option<EmojiAccess> {
    state.emoji_access(ChannelId::new(channel), EmojiId::new(emoji), 0)
}

fn sections(state: &State, channel: u64) -> Vec<(u64, Vec<(u64, EmojiAccess)>)> {
    state
        .custom_emoji_sections(ChannelId::new(channel), 0)
        .into_iter()
        .map(|EmojiSection { guild_id, emojis }| {
            (
                guild_id.get(),
                emojis
                    .into_iter()
                    .map(|custom| {
                        assert_eq!(custom.guild_id, guild_id);
                        (custom.emoji.id.get(), custom.access)
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn without_nitro_a_servers_own_plain_emoji_are_usable_there() {
    let state = picker_state(false);

    assert_eq!(access(&state, GENERAL, WAVE), Some(EmojiAccess::Usable));
    assert_eq!(access(&state, GENERAL, TWITCH), Some(EmojiAccess::Usable));
    assert_eq!(access(&state, GENERAL, MINE), Some(EmojiAccess::Usable));
    assert_eq!(
        access(&state, GENERAL, DANCE),
        Some(EmojiAccess::NitroLocked),
        "animated"
    );
    assert_eq!(
        access(&state, GENERAL, LOST),
        Some(EmojiAccess::Unavailable)
    );
    assert_eq!(access(&state, GENERAL, SUBS), Some(EmojiAccess::RoleLocked));
}

#[test]
fn without_nitro_another_servers_emoji_are_locked_unless_an_integrations() {
    let state = picker_state(false);

    assert_eq!(access(&state, GENERAL, POG), Some(EmojiAccess::NitroLocked));
    assert_eq!(
        access(&state, GENERAL, GARDEN_TWITCH),
        Some(EmojiAccess::Usable)
    );
}

#[test]
fn nitro_unlocks_animated_and_other_servers_emoji() {
    let state = picker_state(true);

    assert_eq!(access(&state, GENERAL, DANCE), Some(EmojiAccess::Usable));
    assert_eq!(access(&state, GENERAL, POG), Some(EmojiAccess::Usable));
    assert_eq!(
        access(&state, GENERAL, LOST),
        Some(EmojiAccess::Unavailable)
    );
    assert_eq!(access(&state, GENERAL, SUBS), Some(EmojiAccess::RoleLocked));
}

// G3's @everyone lacks Use External Emojis: other servers' emoji are out there, with Nitro too,
// integrations' too.
#[test]
fn a_channel_without_external_emoji_refuses_other_servers_emoji() {
    for nitro in [false, true] {
        let state = picker_state(nitro);
        assert_eq!(access(&state, GARDEN, WAVE), Some(EmojiAccess::External));
        assert_eq!(access(&state, GARDEN, TWITCH), Some(EmojiAccess::External));
        assert_eq!(
            access(&state, GARDEN, LOST),
            Some(EmojiAccess::External),
            "hidden there, so not shown as unavailable"
        );
        assert_eq!(access(&state, GARDEN, POG), Some(EmojiAccess::Usable));
    }
}

// A DM has no server of its own: every server's emoji is another's, so Nitro decides, and no
// permission does.
#[test]
fn in_a_dm_every_servers_emoji_needs_nitro_unless_an_integrations() {
    let state = picker_state(false);
    for channel in [DM, GROUP] {
        assert_eq!(
            access(&state, channel, WAVE),
            Some(EmojiAccess::NitroLocked)
        );
        assert_eq!(access(&state, channel, POG), Some(EmojiAccess::NitroLocked));
        assert_eq!(access(&state, channel, TWITCH), Some(EmojiAccess::Usable));
        assert_eq!(
            access(&state, channel, LOST),
            Some(EmojiAccess::Unavailable)
        );
    }

    let nitro = picker_state(true);
    assert_eq!(access(&nitro, DM, WAVE), Some(EmojiAccess::Usable));
    assert_eq!(access(&nitro, DM, DANCE), Some(EmojiAccess::Usable));
}

#[test]
fn an_unknown_emoji_or_channel_has_no_answer() {
    let state = picker_state(false);

    assert_eq!(access(&state, GENERAL, 700_000_000_000_000_099), None);
    assert_eq!(access(&state, 300_000_000_000_000_099, WAVE), None);
    assert!(sections(&state, 300_000_000_000_000_099).is_empty());
}

// The channel's own server first, each server's emoji by ID; what Discord's picker leaves out
// (role-locked, external) isn't there; locked and unavailable ones are, with why.
#[test]
fn sections_put_the_channels_server_first_and_leave_out_what_the_picker_hides() {
    let state = picker_state(false);

    assert_eq!(
        sections(&state, GENERAL),
        [
            (
                G1,
                vec![
                    (WAVE, EmojiAccess::Usable),
                    (DANCE, EmojiAccess::NitroLocked),
                    (LOST, EmojiAccess::Unavailable),
                    (TWITCH, EmojiAccess::Usable),
                    (MINE, EmojiAccess::Usable),
                ]
            ),
            (
                G3,
                vec![
                    (POG, EmojiAccess::NitroLocked),
                    (GARDEN_TWITCH, EmojiAccess::Usable),
                ]
            ),
        ]
    );
    assert_eq!(
        sections(&state, GARDEN),
        [(
            G3,
            vec![
                (POG, EmojiAccess::Usable),
                (GARDEN_TWITCH, EmojiAccess::Usable),
            ]
        )],
        "G1 has nothing left here"
    );
}

// Outside a server the rail's order alone.
#[test]
fn in_a_dm_sections_follow_the_rail() {
    let state = picker_state(true);
    let rail: Vec<u64> = state
        .server_list()
        .into_iter()
        .flat_map(|item| match item {
            ServerListItem::Guild(id) => vec![id],
            ServerListItem::Folder(folder) => folder.guilds.to_vec(),
        })
        .filter(|id| {
            state
                .guild(*id)
                .is_some_and(|guild| !guild.emojis.is_empty())
        })
        .map(|id| id.get())
        .collect();
    assert_eq!(rail.len(), 2);

    let order: Vec<u64> = sections(&state, DM)
        .iter()
        .map(|(guild, _)| *guild)
        .collect();

    assert_eq!(order, rail);
}

const NOW: i64 = 1_800_000_000_000;
const DAY: i64 = 86_400_000;

fn note(state: &mut State, emoji: &ReactionEmoji, times: usize, at: i64) {
    for _ in 0..times {
        state.note_reaction(emoji, at);
    }
}

fn unicode(emoji: &str) -> ReactionEmoji {
    ReactionEmoji::unicode(emoji)
}

fn custom(id: u64) -> ReactionEmoji {
    ReactionEmoji::custom(EmojiId::new(id), None, false)
}

fn shown(emojis: Vec<PickerEmoji>) -> Vec<String> {
    emojis
        .iter()
        .map(|emoji| emoji.name().into_owned())
        .collect()
}

fn quick(state: &State, channel: u64, newest: EmojiVersion) -> Vec<String> {
    shown(state.quick_reactions(ChannelId::new(channel), newest, NOW))
}

fn recent(state: &State, channel: u64, newest: EmojiVersion) -> Vec<String> {
    shown(state.recent_reactions(ChannelId::new(channel), newest, NOW))
}

const ALL: EmojiVersion = EmojiVersion::NEWEST;

// Research §3.8: without frequently used reactions, the official client's three.
#[test]
fn quick_reactions_without_uses_are_discords_three() {
    let state = picker_state(false);

    assert_eq!(
        quick(&state, GENERAL, ALL),
        ["100", "laughing", "sparkling_heart"]
    );
    assert_eq!(
        quick(&state, DM, ALL),
        ["100", "laughing", "sparkling_heart"]
    );
}

#[test]
fn used_emoji_come_first_and_the_three_fill_the_rest_once_each() {
    let mut state = picker_state(false);
    note(&mut state, &unicode("👍"), 1, NOW);
    assert_eq!(quick(&state, GENERAL, ALL), ["thumbsup", "100", "laughing"]);

    let mut state = picker_state(false);
    note(&mut state, &unicode("😆"), 1, NOW);
    assert_eq!(
        quick(&state, GENERAL, ALL),
        ["laughing", "100", "sparkling_heart"],
        "no emoji twice"
    );
}

#[test]
fn the_three_most_used_are_the_quick_reactions() {
    let mut state = picker_state(false);
    note(&mut state, &unicode("😂"), 1, NOW);
    note(&mut state, &unicode("👍"), 3, NOW);
    note(&mut state, &unicode("🔥"), 2, NOW);
    note(&mut state, &unicode("🎉"), 1, NOW - 10 * DAY);

    assert_eq!(quick(&state, GENERAL, ALL), ["thumbsup", "fire", "joy"]);
}

// The official client leaves out what its emoji filter refuses here; Akari keeps only what the
// user may use (a locked one would refuse the click).
#[test]
fn quick_reactions_skip_what_cant_be_used_here() {
    let mut state = picker_state(false);
    note(&mut state, &custom(POG), 5, NOW);
    note(&mut state, &custom(DANCE), 4, NOW);
    note(&mut state, &custom(WAVE), 3, NOW);

    assert_eq!(quick(&state, GENERAL, ALL), ["wave", "100", "laughing"]);
    assert_eq!(
        quick(&state, GARDEN, ALL),
        ["pog", "100", "laughing"],
        "its own server's"
    );
    assert_eq!(
        quick(&state, DM, ALL),
        ["100", "laughing", "sparkling_heart"]
    );
}

#[test]
fn quick_reactions_skip_emoji_newer_than_the_host_draws() {
    let mut state = picker_state(false);
    note(&mut state, &unicode("🫩"), 3, NOW);

    assert_eq!(
        quick(&state, GENERAL, EmojiVersion::new(15, 1)),
        ["100", "laughing", "sparkling_heart"]
    );
    assert_eq!(quick(&state, GENERAL, ALL)[0], "face_with_bags_under_eyes");
}

// The picker's Frequently Used: best first, locked ones too (dimmed), but nothing the picker
// hides here, no emoji the store doesn't know and none the host can't draw.
#[test]
fn recent_reactions_list_what_the_picker_shows_here() {
    let mut state = picker_state(false);
    note(&mut state, &unicode("👍"), 6, NOW);
    note(&mut state, &custom(POG), 5, NOW);
    note(&mut state, &custom(SUBS), 4, NOW);
    note(&mut state, &custom(700_000_000_000_000_099), 3, NOW);
    note(&mut state, &unicode("🫩"), 2, NOW);
    note(&mut state, &custom(WAVE), 1, NOW);

    let listed = state.recent_reactions(ChannelId::new(GENERAL), EmojiVersion::new(15, 1), NOW);

    assert_eq!(shown(listed.clone()), ["thumbsup", "pog", "wave"]);
    let access: Vec<EmojiAccess> = listed.iter().map(PickerEmoji::access).collect();
    assert_eq!(
        access,
        [
            EmojiAccess::Usable,
            EmojiAccess::NitroLocked,
            EmojiAccess::Usable
        ]
    );
    assert_eq!(
        recent(&state, GARDEN, ALL),
        ["thumbsup", "pog", "face_with_bags_under_eyes"],
        "G1's are external there"
    );
}

// What a pick sends: a Unicode emoji as it is, a custom one by ID with its name.
#[test]
fn a_picked_emoji_reacts_with_its_id_and_name() {
    let mut state = picker_state(true);
    note(&mut state, &custom(DANCE), 1, NOW);
    note(&mut state, &unicode("❤"), 1, NOW - DAY);

    let listed = state.recent_reactions(ChannelId::new(GENERAL), ALL, NOW);

    assert_eq!(
        listed[0].reaction(),
        ReactionEmoji::custom(EmojiId::new(DANCE), Some("dance"), true)
    );
    assert_eq!(listed[1].reaction(), ReactionEmoji::unicode("❤\u{fe0f}"));
}
