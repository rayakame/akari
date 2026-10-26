use super::*;
use crate::model::EmojiId;

const WAVE: u64 = 700_000_000_000_000_001;
const DANCE: u64 = 700_000_000_000_000_002;
const POG: u64 = 700_000_000_000_000_003;

fn emoji(id: u64, name: &str) -> Value {
    json!({"id": id.to_string(), "name": name, "animated": false, "available": true,
           "require_colons": true, "managed": false, "roles": []})
}

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

// GUILD_EMOJIS_UPDATE replaces the list; GUILD_UPDATE keeps it; leaving forgets it. None of it
// is shown, so no event.
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
    assert_eq!(events, Vec::<String>::new());
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

// Nothing shows a server's emoji list: a READY whose servers differ only in it changes nothing
// the host sees.
#[test]
fn a_ready_with_only_new_emoji_reports_no_guild_change() {
    let mut state = ready_state();
    let mut ready = ready_value();
    ready["guilds"][0]["emojis"] = json!([emoji(WAVE, "wave")]);

    let events = apply(&mut state, "READY", ready);

    assert!(
        !events.iter().any(|event| event.starts_with("GuildUpdated")),
        "{events:?}"
    );
    assert_eq!(emoji_guild(&state, WAVE), Some(G1));
}
