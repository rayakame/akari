use super::*;
use crate::model::MessageId;
use crate::state::settings::{ExplicitContentSettings, SpoilerSetting};
use crate::state::{MediaCover, Redaction};

const BOT: u64 = 100_000_000_000_000_050;

fn with_embed(mut message: Value, author: u64, embed: Value) -> Value {
    message["author"]["id"] = author.to_string().into();
    message["embeds"] = json!([embed]);
    message
}

fn image_embed(flags: u64, version: Option<u32>) -> Value {
    let mut embed = json!({
        "type": "image", "url": "https://example.com/a.png", "flags": flags,
        "thumbnail": {
            "url": "https://example.com/a.png",
            "proxy_url": "https://images-ext-1.discordapp.net/external/x/https/example.com/a.png",
            "width": 640, "height": 480,
        },
    });
    if let Some(version) = version {
        embed["content_scan_version"] = version.into();
    }
    embed
}

fn cover(state: &State, channel: u64, id: u64, spoilered: bool) -> Option<MediaCover> {
    let message = state
        .message(Snowflake::new(channel), Snowflake::new(id))
        .expect("the message");
    state.shown_embeds(&message, spoilered, wall_millis())[0].cover
}

fn viewing(channel: u64) -> State {
    let mut state = ready_state();
    state.view_channel(Snowflake::new(channel), &mut Vec::new());
    state
}

// Module 294520: a person's embed with media Discord hasn't scanned waits for the scan where the
// settings cover it; a bot's doesn't; the scan is asked with the attachments'.
#[test]
fn a_persons_unscanned_embed_waits_for_its_scan() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_embed(dm_message(10, DM), MIRA, image_embed(0, None)),
    );
    let mut from_bot = with_embed(dm_message(11, DM), BOT, image_embed(0, None));
    from_bot["author"]["bot"] = true.into();
    apply(&mut state, "MESSAGE_CREATE", from_bot);
    let dm = Snowflake::new(DM);

    assert_eq!(cover(&state, DM, 10, false), Some(MediaCover::Pending));
    assert_eq!(cover(&state, DM, 11, false), None, "a bot's");
    assert_eq!(state.scans_due(false), [(dm, vec![Snowflake::new(10)])]);

    state.ask_scans(&[Snowflake::new(10)]);
    let mut events = Vec::new();
    state.settle_scans(dm, &[Snowflake::new(10)], &mut events);
    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        ["MessageUpdated(10)"]
    );
    assert_eq!(cover(&state, DM, 10, false), None);
}

// An embed's own flags cover it by the user's settings, before a spoilered link does.
#[test]
fn flags_cover_by_the_settings_then_a_spoilered_link() {
    let mut state = viewing(GENERAL);
    state.entities.settings.display.explicit_content = ExplicitContentSettings {
        guilds: Redaction::Blur,
        ..ExplicitContentSettings::default()
    };
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_embed(message(10, GENERAL), MIRA, image_embed(16, Some(4))),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_embed(message(11, GENERAL), MIRA, image_embed(0, Some(4))),
    );

    assert_eq!(
        cover(&state, GENERAL, 10, true),
        Some(MediaCover::Sensitive)
    );
    assert_eq!(cover(&state, GENERAL, 11, true), Some(MediaCover::Spoiler));
    assert_eq!(cover(&state, GENERAL, 11, false), None);
    state.entities.settings.display.render_spoilers = SpoilerSetting::Always;
    assert_eq!(cover(&state, GENERAL, 11, true), None, "spoilers shown");
}

// Module 521981: one inline image for the message's one link hides the link's text.
#[test]
fn one_inline_image_hides_its_link() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_embed(message(10, GENERAL), MIRA, image_embed(0, Some(4))),
    );
    let message = state
        .message(Snowflake::new(GENERAL), Snowflake::new(10))
        .expect("the message");
    let shown = state.shown_embeds(&message, false, wall_millis());

    assert!(state.embeds_hide_link(&shown, true));
    assert!(!state.embeds_hide_link(&shown, false), "not one link");
    state.entities.settings.display.inline_embed_media = false;
    let shown = state.shown_embeds(&message, false, wall_millis());
    assert!(!state.embeds_hide_link(&shown, true), "no inline media");
    let _ = MessageId::new(1);
}

// A friendship changes a DM's embeds' covers too.
#[test]
fn a_friendship_updates_dm_embeds() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_embed(dm_message(10, DM), MIRA, image_embed(0, None)),
    );

    let events = apply(
        &mut state,
        "RELATIONSHIP_ADD",
        json!({"id": MIRA.to_string(), "type": 1, "user_ignored": false}),
    );

    assert_eq!(events, ["MessageUpdated(10)"]);
}

// Review: nothing is asked for embeds that don't show: `render_embeds` off, a system message.
#[test]
fn hidden_embeds_wait_for_no_scan() {
    let mut state = viewing(DM);
    state.entities.settings.display.render_embeds = false;
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_embed(dm_message(10, DM), MIRA, image_embed(0, None)),
    );
    assert_eq!(state.scans_due(false), []);

    state.entities.settings.display.render_embeds = true;
    let mut join = with_embed(dm_message(11, DM), MIRA, image_embed(0, None));
    join["type"] = 7.into();
    apply(&mut state, "MESSAGE_CREATE", join);
    assert_eq!(
        state.scans_due(false),
        [(Snowflake::new(DM), vec![Snowflake::new(10)])]
    );
}
