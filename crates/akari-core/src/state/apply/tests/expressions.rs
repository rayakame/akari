use super::*;
use crate::model::EmojiId;

const WAVE: u64 = 700_000_000_000_000_001;
const DANCE: u64 = 700_000_000_000_000_002;
const POG: u64 = 700_000_000_000_000_003;

fn emoji(id: u64, name: &str) -> Value {
    json!({"id": id.to_string(), "name": name, "animated": false, "available": true,
           "require_colons": true, "managed": false, "roles": []})
}

// ID, name, animated, available, managed, roles.
type Kept<'a> = (u64, &'a str, bool, bool, bool, Vec<u64>);

fn emoji_guild(state: &State, id: u64) -> Option<u64> {
    state.emoji_guild(EmojiId::new(id)).map(|guild| guild.get())
}

// READY's and GUILD_CREATE's guilds carry their emoji: the store knows which server an emoji is
// from, as the official client's emoji store does (its popout's "this server", "one of your
// servers").
#[test]
fn guild_emojis_say_which_server_an_emoji_is_from() {
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([emoji(DANCE, "dance"), emoji(WAVE, "wave")]);
    let mut state = state_from(&ready);

    assert_eq!(emoji_guild(&state, WAVE), Some(G1));
    assert_eq!(emoji_guild(&state, DANCE), Some(G1));
    assert_eq!(emoji_guild(&state, POG), None);

    let mut joined = guild_create(G3);
    joined["emojis"] = json!([emoji(POG, "pog")]);
    apply(&mut state, "GUILD_CREATE", joined);
    assert_eq!(emoji_guild(&state, POG), Some(G3));
}

// GUILD_EMOJIS_UPDATE replaces the list, which the picker shows; GUILD_UPDATE keeps it; leaving
// forgets it.
#[test]
fn emoji_updates_replace_the_list_and_leaving_forgets_it() {
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([emoji(WAVE, "wave")]);
    let mut state = state_from(&ready);

    let events = apply(
        &mut state,
        "GUILD_EMOJIS_UPDATE",
        json!({"guild_id": G1.to_string(), "emojis": [emoji(POG, "pog"), emoji(DANCE, "dance")]}),
    );
    assert_eq!(events, [format!("GuildEmojisChanged({G1})")]);
    assert_eq!(emoji_guild(&state, WAVE), None);
    assert_eq!(emoji_guild(&state, POG), Some(G1));

    apply(
        &mut state,
        "GUILD_UPDATE",
        json!({"id": G1.to_string(), "name": "Renamed"}),
    );
    assert_eq!(emoji_guild(&state, POG), Some(G1));

    apply(
        &mut state,
        "GUILD_EMOJIS_UPDATE",
        json!({"guild_id": "200000000000000099", "emojis": [emoji(WAVE, "wave")]}),
    );
    assert_eq!(
        emoji_guild(&state, WAVE),
        None,
        "an unknown guild's update changes nothing"
    );

    apply(&mut state, "GUILD_DELETE", json!({"id": G1.to_string()}));
    assert_eq!(emoji_guild(&state, POG), None);
}

// An emoji that doesn't decode is skipped; the guild and its other emoji stay.
#[test]
fn a_broken_emoji_is_skipped() {
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([{"id": "nope"}, emoji(WAVE, "wave"), {"name": "x"}]);
    let state = state_from(&ready);

    assert_eq!(emoji_guild(&state, WAVE), Some(G1));
    assert!(state.guild(GuildId::new(G1)).is_some());
}

// The emoji list has its own event: a READY whose servers differ only in it changes no guild.
#[test]
fn a_ready_with_only_new_emoji_reports_only_the_list() {
    let mut state = ready_state();
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([emoji(WAVE, "wave")]);

    let events = apply(&mut state, "READY", ready.clone());

    assert!(
        !events.iter().any(|event| event.starts_with("GuildUpdated")),
        "{events:?}"
    );
    assert!(
        events.contains(&format!("GuildEmojisChanged({G1})")),
        "{events:?}"
    );
    assert_eq!(emoji_guild(&state, WAVE), Some(G1));

    let again = apply(&mut state, "READY", ready);
    assert!(
        !again
            .iter()
            .any(|event| event.starts_with("GuildEmojisChanged")),
        "the same list again: {again:?}"
    );
}

// What the picker needs of each: the name, whether it's animated, available (the server lost the
// boosts it needs), an integration's (managed), and the roles that may use it, by ID.
#[test]
fn guild_emoji_keep_what_the_picker_needs() {
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([
        {"id": DANCE.to_string(), "name": "dance", "animated": true, "available": false,
         "managed": true, "roles": ["500000000000000002"]},
        emoji(WAVE, "wave"),
        {"id": POG.to_string(), "name": null},
    ]);
    let state = state_from(&ready);

    let guild = state.guild(GuildId::new(G1)).unwrap();
    let kept: Vec<Kept> = guild
        .emojis
        .iter()
        .map(|emoji| {
            (
                emoji.id.get(),
                &*emoji.name,
                emoji.animated,
                emoji.available,
                emoji.managed,
                emoji.roles.iter().map(|role| role.get()).collect(),
            )
        })
        .collect();
    assert_eq!(
        kept,
        [
            (WAVE, "wave", false, true, false, vec![]),
            (
                DANCE,
                "dance",
                true,
                false,
                true,
                vec![500_000_000_000_000_002]
            ),
        ],
        "by ID, a deleted (nameless) one left out"
    );
    assert_eq!(
        guild.emojis[1].image_key(),
        crate::media::ImageKey::Emoji {
            id: EmojiId::new(DANCE),
            animated: true
        }
    );
}

#[test]
fn an_unchanged_emoji_update_says_nothing() {
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([emoji(WAVE, "wave"), emoji(DANCE, "dance")]);
    let mut state = state_from(&ready);

    let events = apply(
        &mut state,
        "GUILD_EMOJIS_UPDATE",
        json!({"guild_id": G1.to_string(), "emojis": [emoji(DANCE, "dance"), emoji(WAVE, "wave")]}),
    );

    assert_eq!(events, Vec::<String>::new());
}

// A renamed emoji is a change too: the picker shows names.
#[test]
fn a_renamed_emoji_changes_the_list() {
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([emoji(WAVE, "wave")]);
    let mut state = state_from(&ready);

    let events = apply(
        &mut state,
        "GUILD_EMOJIS_UPDATE",
        json!({"guild_id": G1.to_string(), "emojis": [emoji(WAVE, "hello")]}),
    );

    assert_eq!(events, [format!("GuildEmojisChanged({G1})")]);
    let guild = state.guild(GuildId::new(G1)).unwrap();
    assert_eq!(&*guild.emojis[0].name, "hello");
}

// GUILD_CREATE for a server the store has replaces its list: an event only when it differs.
#[test]
fn a_repeated_guild_create_reports_a_new_emoji_list() {
    let mut state = ready_state();
    let mut first = guild_create(G3);
    first["emojis"] = json!([emoji(POG, "pog")]);
    apply(&mut state, "GUILD_CREATE", first.clone());

    let same = apply(&mut state, "GUILD_CREATE", first.clone());
    assert!(
        !same
            .iter()
            .any(|event| event.starts_with("GuildEmojisChanged")),
        "{same:?}"
    );

    let mut other = first;
    other["emojis"] = json!([emoji(POG, "pog"), emoji(WAVE, "wave")]);
    let changed = apply(&mut state, "GUILD_CREATE", other);
    assert!(
        changed.contains(&format!("GuildEmojisChanged({G3})")),
        "{changed:?}"
    );
    assert!(
        !changed
            .iter()
            .any(|event| event.starts_with("GuildUpdated")),
        "{changed:?}"
    );
}

// The update always carries the whole list: one without it doesn't decode, rather than emptying
// the server's emoji.
#[test]
fn an_emoji_update_without_its_list_doesnt_decode() {
    let payload = json!({"op": 0, "s": 1, "t": "GUILD_EMOJIS_UPDATE",
                         "d": {"guild_id": G1.to_string()}});

    let decoded = decode(payload.to_string().as_bytes());

    assert!(
        matches!(decoded, Err(crate::gateway::DecodeError::Dispatch { .. })),
        "{decoded:?}"
    );
}
