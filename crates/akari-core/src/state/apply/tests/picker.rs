use super::*;
use crate::emoji::EmojiVersion;
use crate::model::EmojiId;
use crate::state::picker::{EmojiAccess, EmojiSection, PickerEmoji};
use crate::state::reactions::ReactionEmoji;
use crate::state::settings::FolderEntry;

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

fn picker_state(nitro: bool) -> State {
    picker_state_with_premium(if nitro { 2 } else { 0 })
}

// G1 (where the user is an administrator) has a plain, an animated, an unavailable, a
// role-locked, an integration's and a role's emoji the user has; G3 a plain and an
// integration's.
fn picker_state_with_premium(premium_type: u8) -> State {
    let mut ready = ready_value();
    ready["user"]["premium_type"] = json!(premium_type);
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

// Nitro Classic and Nitro Basic count as Nitro does.
#[test]
fn every_nitro_tier_unlocks_emoji() {
    for tier in [1, 2, 3] {
        let state = picker_state_with_premium(tier);

        assert_eq!(
            access(&state, GENERAL, DANCE),
            Some(EmojiAccess::Usable),
            "{tier}"
        );
        assert_eq!(
            access(&state, GENERAL, POG),
            Some(EmojiAccess::Usable),
            "{tier}"
        );
    }
}

#[test]
fn one_of_the_users_roles_is_enough_for_a_role_locked_emoji() {
    let mut state = picker_state(false);
    apply(
        &mut state,
        "GUILD_MEMBER_UPDATE",
        json!({
            "guild_id": G1.to_string(),
            "user": {"id": ME.to_string(), "username": "akari_tester"},
            "roles": [ADMIN_ROLE, "500000000000000003"],
        }),
    );

    assert_eq!(access(&state, GENERAL, MINE), Some(EmojiAccess::Usable));
    assert_eq!(access(&state, GENERAL, SUBS), Some(EmojiAccess::RoleLocked));
}

// Roles the store doesn't know lock rather than unlock.
#[test]
fn without_the_users_member_a_role_locked_emoji_stays_locked() {
    let mut state = state_from(&ready_value());
    let mut garden = guild_create(G3);
    garden["members"] = json!([]);
    garden["emojis"] = json!([{"id": POG.to_string(), "name": "pog", "roles": [OTHER_ROLE]}]);
    apply(&mut state, "GUILD_CREATE", garden);

    assert_eq!(access(&state, GARDEN, POG), Some(EmojiAccess::RoleLocked));
}

// The picker hides an emoji for other roles; that it's unavailable too doesn't show it.
#[test]
fn an_unavailable_emoji_for_other_roles_stays_hidden() {
    let mut state = picker_state(false);
    apply(
        &mut state,
        "GUILD_EMOJIS_UPDATE",
        json!({"guild_id": G1.to_string(), "emojis": [
            emoji(WAVE, "wave"),
            {"id": SUBS.to_string(), "name": "subs", "available": false, "roles": [OTHER_ROLE]},
        ]}),
    );

    assert_eq!(access(&state, GENERAL, SUBS), Some(EmojiAccess::RoleLocked));
    assert_eq!(
        sections(&state, GENERAL)[0],
        (G1, vec![(WAVE, EmojiAccess::Usable)])
    );
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

const GARDEN_THREAD: u64 = 300_000_000_000_000_032;

fn garden_thread() -> Value {
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread["id"] = GARDEN_THREAD.to_string().into();
    thread["guild_id"] = G3.to_string().into();
    thread["parent_id"] = GARDEN.to_string().into();
    thread["member"]["id"] = GARDEN_THREAD.to_string().into();
    thread
}

// A thread has no overwrites of its own: its parent's decide whether other servers' emoji may be
// used there.
#[test]
fn in_a_thread_the_parents_overwrites_decide() {
    let mut state = picker_state(false);
    apply(&mut state, "THREAD_CREATE", garden_thread());
    assert_eq!(
        access(&state, GARDEN_THREAD, WAVE),
        Some(EmojiAccess::External)
    );

    let mut garden = guild_create(G3)["channels"][0].clone();
    garden["guild_id"] = G3.to_string().into();
    garden["permission_overwrites"] = json!([{
        "id": G3.to_string(), "type": 0, "deny": "0",
        "allow": Permissions::USE_EXTERNAL_EMOJIS.0.to_string(),
    }]);
    apply(&mut state, "CHANNEL_UPDATE", garden);

    assert_eq!(
        access(&state, GARDEN_THREAD, WAVE),
        Some(EmojiAccess::NitroLocked)
    );
    assert_eq!(
        access(&state, GARDEN_THREAD, TWITCH),
        Some(EmojiAccess::Usable)
    );
    let order: Vec<u64> = sections(&state, GARDEN_THREAD)
        .iter()
        .map(|(guild, _)| *guild)
        .collect();
    assert_eq!(order, [G3, G1], "the thread's own server first");
}

// One entry per `(folder ID, servers)`; `None` places the servers outside a folder.
fn in_folders(state: &mut State, entries: &[(Option<i64>, &[u64])]) {
    let entries: Vec<FolderEntry> = entries
        .iter()
        .map(|(id, guilds)| FolderEntry {
            id: *id,
            name: None,
            color: None,
            guilds: guilds.iter().copied().map(GuildId::new).collect(),
        })
        .collect();
    state.entities.settings.folders = Some(entries.into());
}

fn section_order(state: &State, channel: u64) -> Vec<u64> {
    sections(state, channel)
        .iter()
        .map(|(guild, _)| *guild)
        .collect()
}

// Outside a server the rail's order alone.
#[test]
fn in_a_dm_sections_follow_the_rail() {
    let mut state = picker_state(true);
    assert_eq!(
        section_order(&state, DM),
        [G3, G1],
        "the server joined last first"
    );

    in_folders(&mut state, &[(None, &[G1]), (Some(7), &[G3])]);

    assert_eq!(section_order(&state, DM), [G1, G3]);
}

#[test]
fn a_folders_servers_are_in_the_picker() {
    let mut state = picker_state(true);
    in_folders(&mut state, &[(Some(7), &[G3])]);

    assert_eq!(section_order(&state, GENERAL), [G1, G3]);
    assert_eq!(
        sections(&state, GENERAL)[1],
        (
            G3,
            vec![
                (POG, EmojiAccess::Usable),
                (GARDEN_TWITCH, EmojiAccess::Usable)
            ]
        )
    );
    assert_eq!(section_order(&state, DM), [G1, G3]);
    assert_eq!(search(&state, GENERAL, "pog"), ["pog"]);
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
    assert_eq!(
        quick(&state, GENERAL, EmojiVersion::new(16, 0))[0],
        "face_with_bags_under_eyes",
        "the host's own version"
    );
}

// 💯, 😆 and 💖 came with Emoji 0.6.
#[test]
fn the_three_defaults_keep_to_the_hosts_version_too() {
    let state = picker_state(false);

    assert!(quick(&state, GENERAL, EmojiVersion::new(0, 5)).is_empty());
    assert_eq!(
        quick(&state, GENERAL, EmojiVersion::new(0, 6)),
        ["100", "laughing", "sparkling_heart"]
    );
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

fn search(state: &State, channel: u64, query: &str) -> Vec<String> {
    shown(state.search_emoji(ChannelId::new(channel), query, ALL, 500, NOW))
}

fn position(found: &[String], name: &str) -> usize {
    found
        .iter()
        .position(|found| found == name)
        .unwrap_or_else(|| panic!("{name} not in {found:?}"))
}

// An exact name first, then names that start with the query, then a word that does, then any
// name containing it.
#[test]
fn search_ranks_exact_then_prefix_then_word_prefix_then_contains() {
    let state = picker_state(true);

    let found = search(&state, GENERAL, "art");

    assert_eq!(found[0], "art");
    let prefix = position(&found, "articulated_lorry");
    let word = position(&found, "performing_arts");
    let contains = position(&found, "heart");
    assert!(prefix < word && word < contains, "{found:?}");
}

#[test]
fn search_reads_other_short_codes_and_unicodes_names() {
    let state = picker_state(false);

    assert_eq!(search(&state, GENERAL, "+1")[0], "thumbsup");
    assert_eq!(search(&state, GENERAL, "satisfied")[0], "laughing");
    assert_eq!(search(&state, GENERAL, "thumbs up")[0], "thumbsup");
    assert_eq!(search(&state, GENERAL, "grinning squinting")[0], "laughing");
    assert_eq!(search(&state, GENERAL, "flag: germany")[0], "flag_de");
    assert_eq!(
        search(&state, GENERAL, "smiling face with heart eyes")[0],
        "heart_eyes",
        "a hyphen in Unicode's name"
    );
}

#[test]
fn search_ignores_case_colons_and_spaces_around() {
    let state = picker_state(false);

    assert_eq!(search(&state, GENERAL, " :Smile: ")[0], "smile");
    assert_eq!(search(&state, GENERAL, "THUMBSUP")[0], "thumbsup");
    assert_eq!(search(&state, GENERAL, ":+1:")[0], "thumbsup");
    assert!(search(&state, GENERAL, "").is_empty());
    assert!(search(&state, GENERAL, " :: ").is_empty());
    assert!(search(&state, GENERAL, "?").is_empty(), "no letters");
}

// Within a rank: the channel's own server, then Unicode, then the other servers.
#[test]
fn search_puts_the_channels_server_first() {
    let mut state = picker_state(true);
    apply(
        &mut state,
        "GUILD_EMOJIS_UPDATE",
        json!({"guild_id": G3.to_string(), "emojis": [emoji(POG, "wave")]}),
    );
    let wave = |found: &PickerEmoji| match found {
        PickerEmoji::Custom(custom) => custom.guild_id.get().to_string(),
        PickerEmoji::Unicode(emoji) => emoji.as_str().to_owned(),
    };
    let first_three = |channel: u64| -> Vec<String> {
        state
            .search_emoji(ChannelId::new(channel), "wave", ALL, 3, NOW)
            .iter()
            .map(wave)
            .collect()
    };

    assert_eq!(
        first_three(GENERAL),
        [G1.to_string(), "👋".to_owned(), G3.to_string()]
    );
    assert_eq!(
        first_three(DM),
        ["👋".to_owned(), G3.to_string(), G1.to_string()],
        "the rail's order, the server joined last first"
    );
}

// Discord's picker lists what the user may use first; a locked exact match comes after a usable
// prefix.
#[test]
fn usable_matches_come_before_locked_ones() {
    let state = picker_state(false);

    let found = state.search_emoji(ChannelId::new(GENERAL), "dance", ALL, 500, NOW);

    let locked = found
        .iter()
        .position(|emoji| emoji.name() == "dance")
        .unwrap();
    assert_eq!(found[locked].access(), EmojiAccess::NitroLocked);
    assert!(found[..locked].iter().all(|emoji| emoji.access().usable()));
    assert!(locked > 0, "{:?}", shown(found.clone()));
}

#[test]
fn search_leaves_out_what_the_picker_hides_and_what_the_host_cant_draw() {
    let state = picker_state(false);

    assert!(!search(&state, GENERAL, "subs").contains(&"subs".to_owned()));
    let garden = state.search_emoji(ChannelId::new(GARDEN), "wave", ALL, 500, NOW);
    assert!(
        garden
            .iter()
            .all(|emoji| matches!(emoji, PickerEmoji::Unicode(_))),
        "G1's are external there"
    );
    let newer = state.search_emoji(
        ChannelId::new(GENERAL),
        "face_with_bags",
        EmojiVersion::new(15, 1),
        10,
        NOW,
    );
    assert!(newer.is_empty());
    assert_eq!(
        search(&state, GENERAL, "face_with_bags"),
        ["face_with_bags_under_eyes"]
    );
    let own = state.search_emoji(
        ChannelId::new(GENERAL),
        "face_with_bags",
        EmojiVersion::new(16, 0),
        10,
        NOW,
    );
    assert_eq!(
        shown(own),
        ["face_with_bags_under_eyes"],
        "the host's own version"
    );
}

#[test]
fn a_custom_names_case_doesnt_matter() {
    let mut state = picker_state(true);
    apply(
        &mut state,
        "GUILD_EMOJIS_UPDATE",
        json!({"guild_id": G3.to_string(), "emojis": [emoji(POG, "PartyBlob")]}),
    );

    assert_eq!(search(&state, GENERAL, "partyb"), ["PartyBlob"]);
    assert_eq!(search(&state, GENERAL, "BLOB"), ["PartyBlob"]);
}

#[test]
fn search_gives_at_most_the_limit() {
    let state = picker_state(false);

    let found = state.search_emoji(ChannelId::new(GENERAL), "a", ALL, 5, NOW);

    assert_eq!(found.len(), 5);
}

// An unknown channel still finds Unicode emoji.
#[test]
fn search_in_an_unknown_channel_finds_unicode_only() {
    let state = picker_state(true);

    let found = state.search_emoji(
        ChannelId::new(300_000_000_000_000_099),
        "wave",
        ALL,
        50,
        NOW,
    );

    assert!(!found.is_empty());
    assert!(
        found
            .iter()
            .all(|emoji| matches!(emoji, PickerEmoji::Unicode(_)))
    );
}
