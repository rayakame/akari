use super::*;
use crate::model::{ChannelId, GuildId, MessageId};
use crate::state::settings::ExplicitContentSettings;
use crate::state::{AgeGate, AttachmentDisplay, MediaCover, Redaction, SpoilerSetting};

const FRIEND: u64 = 100_000_000_000_000_003;
const NSFW: u64 = 300_000_000_000_000_007;
const NSFW_THREAD: u64 = 300_000_000_000_000_008;
const OTHER_STRANGER: u64 = 100_000_000_000_000_042;

fn image(flags: u64, version: Option<u32>) -> Value {
    let mut image = json!({
        "id": "600000000000000001", "filename": "photo.png", "size": 1000,
        "url": "https://cdn.discordapp.com/attachments/1/600000000000000001/photo.png",
        "proxy_url": "https://media.discordapp.net/attachments/1/600000000000000001/photo.png",
        "width": 640, "height": 480, "flags": flags,
    });
    if let Some(version) = version {
        image["content_scan_version"] = version.into();
    }
    image
}

fn with_image(mut message: Value, author: u64, image: Value) -> Value {
    message["author"]["id"] = author.to_string().into();
    message["attachments"] = json!([image]);
    message
}

fn cover(state: &State, channel: u64, id: u64) -> Option<MediaCover> {
    let message = state
        .message(Snowflake::new(channel), Snowflake::new(id))
        .expect("the message");
    state.shown_attachments(&message, wall_millis())[0].cover
}

fn viewing(channel: u64) -> State {
    let mut state = ready_state();
    state.view_channel(Snowflake::new(channel), &mut Vec::new());
    state
}

// Defaults (filter non-friends): a stranger's unscanned media in a DM waits for its scan; a
// friend's and a guild's don't.
#[test]
fn unscanned_media_waits_only_where_the_settings_cover_it() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(10, DM), MIRA, image(0, None)),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(11, DM), FRIEND, image(0, None)),
    );
    state.view_channel(Snowflake::new(GENERAL), &mut Vec::new());
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(message(12, GENERAL), MIRA, image(0, None)),
    );

    assert_eq!(cover(&state, DM, 10), Some(MediaCover::Pending));
    assert_eq!(cover(&state, DM, 11), None, "a friend's");
    assert_eq!(cover(&state, GENERAL, 12), None);
}

#[test]
fn flagged_media_is_covered_by_the_users_setting_and_never_their_own() {
    let mut state = viewing(GENERAL);
    state.entities.settings.display.explicit_content = ExplicitContentSettings {
        guilds: Redaction::Blur,
        ..ExplicitContentSettings::default()
    };
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(message(10, GENERAL), MIRA, image(16, Some(4))),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(message(11, GENERAL), ME, image(16, None)),
    );

    assert_eq!(cover(&state, GENERAL, 10), Some(MediaCover::Sensitive));
    assert_eq!(cover(&state, GENERAL, 11), None, "the user's own");
}

#[test]
fn a_moderator_sees_spoilers_with_if_moderator() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(message(10, GENERAL), MIRA, image(8, Some(4))),
    );
    let general = Snowflake::new(GENERAL);
    assert_eq!(cover(&state, GENERAL, 10), Some(MediaCover::Spoiler));

    state.entities.settings.display.render_spoilers = SpoilerSetting::IfModerator;
    let moderator = state
        .permissions(general, wall_millis())
        .is_some_and(|permissions| permissions.contains(Permissions::MANAGE_MESSAGES));
    assert_eq!(state.spoilers_shown(general, wall_millis()), moderator);
    state.entities.settings.display.render_spoilers = SpoilerSetting::Always;
    assert!(state.spoilers_shown(general, wall_millis()));
    assert_eq!(cover(&state, GENERAL, 10), None);
}

fn shown(state: &State, channel: u64, id: u64) -> Vec<(usize, AttachmentDisplay)> {
    let message = state
        .message(Snowflake::new(channel), Snowflake::new(id))
        .unwrap();
    state
        .shown_attachments(&message, wall_millis())
        .iter()
        .map(|shown| (shown.index, shown.display))
        .collect()
}

#[test]
fn inline_media_follows_the_setting() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(message(10, GENERAL), MIRA, image(0, Some(4))),
    );
    assert_eq!(shown(&state, GENERAL, 10), [(0, AttachmentDisplay::Media)]);

    state.entities.settings.display.inline_attachment_media = false;

    assert_eq!(shown(&state, GENERAL, 10), [(0, AttachmentDisplay::File)]);
}

// Asked once per message; a settled scan shows the media and says so.
#[test]
fn scans_are_asked_once_and_settle_with_an_update() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(10, DM), MIRA, image(0, None)),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(11, DM), MIRA, image(0, Some(4))),
    );
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(12, DM), ME, image(0, None)),
    );
    let dm = Snowflake::new(DM);

    assert_eq!(state.scans_due(false), [(dm, vec![Snowflake::new(10)])]);
    state.ask_scans(&[Snowflake::new(10)]);
    assert_eq!(state.scans_due(false), []);

    let mut events = Vec::new();
    state.settle_scans(dm, &[Snowflake::new(10)], &mut events);
    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        ["MessageUpdated(10)"]
    );
    assert_eq!(cover(&state, DM, 10), None);
    let mut again = Vec::new();
    state.settle_scans(dm, &[Snowflake::new(10)], &mut again);
    assert!(again.is_empty(), "settled once");
    let mut scanned = Vec::new();
    state.settle_scans(dm, &[Snowflake::new(11)], &mut scanned);
    assert!(scanned.is_empty(), "a scanned message didn't wait");
}

// Discord's answer is a MESSAGE_UPDATE with the scanned attachment.
#[test]
fn a_scan_result_replaces_the_wait() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(10, DM), MIRA, image(0, None)),
    );
    state.ask_scans(&[Snowflake::new(10)]);

    apply(
        &mut state,
        "MESSAGE_UPDATE",
        with_image(dm_message(10, DM), MIRA, image(16, Some(4))),
    );

    assert_eq!(cover(&state, DM, 10), Some(MediaCover::Sensitive));
}

#[test]
fn nothing_is_due_where_nothing_is_covered() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(message(10, GENERAL), MIRA, image(0, None)),
    );

    assert_eq!(state.scans_due(false), []);
}

fn nsfw_channels(state: &mut State) {
    let mut nsfw = fixture(include_str!(
        "../../../../tests/fixtures/channel_guild_text.json"
    ));
    nsfw["id"] = NSFW.to_string().into();
    nsfw["nsfw"] = true.into();
    apply(state, "CHANNEL_CREATE", nsfw);
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/channel_thread.json"
    ));
    thread["id"] = NSFW_THREAD.to_string().into();
    thread["parent_id"] = NSFW.to_string().into();
    apply(state, "THREAD_CREATE", thread);
}

fn gate(state: &State, channel: u64) -> AgeGate {
    state.age_gate(Snowflake::new(channel))
}

// Module 323073: a restricted channel, its threads, every channel of a restricted server;
// never a DM. One agreement opens the server's channels.
#[test]
fn age_restricted_channels_ask_until_the_user_agrees_for_the_server() {
    let mut state = ready_state();
    nsfw_channels(&mut state);

    assert_eq!(gate(&state, NSFW), AgeGate::Ask { server: false });
    assert_eq!(gate(&state, NSFW_THREAD), AgeGate::Ask { server: false });
    assert_eq!(gate(&state, GENERAL), AgeGate::Open);
    assert_eq!(gate(&state, DM), AgeGate::Open);

    let mut events = Vec::new();
    state.agree_to_age_restricted(GuildId::new(G1), &mut events);

    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        [format!("AgeGateChanged({G1})")]
    );
    assert_eq!(gate(&state, NSFW), AgeGate::Open);
    assert_eq!(gate(&state, NSFW_THREAD), AgeGate::Open);
    let mut again = Vec::new();
    state.agree_to_age_restricted(GuildId::new(G1), &mut again);
    assert!(again.is_empty());
}

#[test]
fn a_restricted_server_gates_every_channel_and_a_minor_never_passes() {
    let mut ready = ready_value();
    ready["guilds"][0]["properties"]["owner_configured_content_level"] = 3.into();
    ready["user"]["nsfw_allowed"] = false.into();
    let mut state = state_from(&ready);

    assert_eq!(gate(&state, GENERAL), AgeGate::TooYoung { server: true });
    state.agree_to_age_restricted(GuildId::new(G1), &mut Vec::new());
    assert_eq!(gate(&state, GENERAL), AgeGate::TooYoung { server: true });
}

#[test]
fn the_agreement_outlives_a_new_session() {
    let mut state = ready_state();
    nsfw_channels(&mut state);
    state.agree_to_age_restricted(GuildId::new(G1), &mut Vec::new());

    state.apply(dispatch("READY", &ready_value()), &mut Vec::new());
    nsfw_channels(&mut state);

    assert_eq!(gate(&state, NSFW), AgeGate::Open);
    let _ = (ChannelId::new(NSFW), MessageId::new(1));
}

fn guild_media_waits(state: &mut State) {
    state.entities.settings.display.explicit_content = ExplicitContentSettings {
        guilds: Redaction::Blur,
        ..ExplicitContentSettings::default()
    };
}

fn starter(id: u64, origin: Value) -> Value {
    let mut starter = message(id, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({
        "message_id": origin["id"].clone(), "channel_id": GENERAL.to_string(),
    });
    starter["referenced_message"] = origin;
    starter
}

// Review: a viewed thread shows its starter from the row, not a window; its media is asked for
// in the parent, and the settle updates the row that shows it.
#[test]
fn a_viewed_threads_starter_is_scanned_and_its_row_updated() {
    let mut state = viewing(THREAD);
    guild_media_waits(&mut state);
    let origin = with_image(message(19, GENERAL), MIRA, image(0, None));
    apply(&mut state, "MESSAGE_CREATE", starter(20, origin));
    let general = Snowflake::new(GENERAL);

    assert_eq!(
        state.scans_due(false),
        [(general, vec![Snowflake::new(19)])]
    );
    state.ask_scans(&[Snowflake::new(19)]);
    assert_eq!(state.scans_due(false), []);

    let mut events = Vec::new();
    state.settle_scans(general, &[Snowflake::new(19)], &mut events);
    assert_eq!(
        events.iter().map(describe).collect::<Vec<_>>(),
        ["MessageUpdated(20)"]
    );
}

// Shown in its channel's window and as a viewed thread's starter: asked once, both rows updated.
#[test]
fn a_starter_shown_twice_is_asked_once() {
    let mut state = viewing(THREAD);
    guild_media_waits(&mut state);
    let origin = with_image(message(19, GENERAL), MIRA, image(0, None));
    apply(&mut state, "MESSAGE_CREATE", starter(20, origin.clone()));
    let general = Snowflake::new(GENERAL);
    state.view_channel(general, &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", origin);

    assert_eq!(
        state.scans_due(false),
        [(general, vec![Snowflake::new(19)])]
    );
    state.ask_scans(&[Snowflake::new(19)]);
    let mut events = Vec::new();
    state.settle_scans(general, &[Snowflake::new(19)], &mut events);
    let mut updated: Vec<String> = events.iter().map(describe).collect();
    updated.sort();
    assert_eq!(updated, ["MessageUpdated(19)", "MessageUpdated(20)"]);
}

// Review: an ask that failed for a while (network, 5xx, rate limit) is asked again when the
// account reloads, until Discord has scanned the media; the wait still ends meanwhile.
#[test]
fn a_failed_ask_is_asked_again_on_a_reload_until_scanned() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(10, DM), MIRA, image(0, None)),
    );
    let dm = Snowflake::new(DM);
    let ten = [Snowflake::new(10)];
    state.ask_scans(&ten);
    state.fail_scans(&ten);
    state.settle_scans(dm, &ten, &mut Vec::new());
    assert_eq!(cover(&state, DM, 10), None);

    assert_eq!(state.scans_due(false), [], "not for any event");
    assert_eq!(state.scans_due(true), [(dm, ten.to_vec())]);
    state.ask_scans(&ten);
    assert_eq!(state.scans_due(true), [], "asked again once");

    state.fail_scans(&ten);
    apply(
        &mut state,
        "MESSAGE_UPDATE",
        with_image(dm_message(10, DM), MIRA, image(0, Some(4))),
    );
    assert_eq!(state.scans_due(true), [], "scanned meanwhile");
}

// Review: a READY forgets the scans of messages no window or starter shows any more; what
// still shows stays settled.
#[test]
fn a_ready_forgets_the_scans_of_messages_gone() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(10, DM), MIRA, image(0, None)),
    );
    state.view_channel(Snowflake::new(THREAD), &mut Vec::new());
    let origin = with_image(message(19, GENERAL), MIRA, image(0, None));
    apply(&mut state, "MESSAGE_CREATE", starter(20, origin));
    let dm = Snowflake::new(DM);
    let asked = [Snowflake::new(10), Snowflake::new(99)];
    state.ask_scans(&asked);
    state.fail_scans(&asked[1..]);
    state.settle_scans(dm, &asked, &mut Vec::new());
    state.ask_scans(&[Snowflake::new(19)]);
    state.settle_scans(
        Snowflake::new(GENERAL),
        &[Snowflake::new(19)],
        &mut Vec::new(),
    );

    state.apply(dispatch("READY", &ready_value()), &mut Vec::new());

    assert_eq!(cover(&state, DM, 10), None, "still settled");
    let shown = HashSet::from([Snowflake::new(10), Snowflake::new(19)]);
    assert_eq!(state.scans.asked, shown);
    assert_eq!(state.scans.settled, shown, "the thread's starter too");
    assert!(state.scans.failed.is_empty());
}

// Review: whether the author is a friend decides a DM's covers; a new or ended friendship
// updates the author's messages with media, and only those.
#[test]
fn a_friendship_updates_the_covers_of_the_authors_dm_media() {
    let mut state = viewing(DM);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(10, DM), MIRA, image(0, None)),
    );
    let mut text = dm_message(11, DM);
    text["author"]["id"] = MIRA.to_string().into();
    apply(&mut state, "MESSAGE_CREATE", text);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_image(dm_message(12, DM), OTHER_STRANGER, image(0, None)),
    );
    assert_eq!(cover(&state, DM, 10), Some(MediaCover::Pending));
    let relationship =
        |kind: u8| json!({"id": MIRA.to_string(), "type": kind, "user_ignored": false});

    let events = apply(&mut state, "RELATIONSHIP_ADD", relationship(1));
    assert_eq!(events, ["MessageUpdated(10)"]);
    assert_eq!(cover(&state, DM, 10), None, "a friend's");

    let events = apply(&mut state, "RELATIONSHIP_ADD", relationship(1));
    assert!(events.is_empty(), "no change");

    let events = apply(
        &mut state,
        "RELATIONSHIP_REMOVE",
        json!({"id": MIRA.to_string(), "type": 1}),
    );
    assert_eq!(events, ["MessageUpdated(10)"]);
    assert_eq!(cover(&state, DM, 10), Some(MediaCover::Pending));
}

// Module 207133: HIDE_MEDIA_DOWNLOAD_OPTIONS (1 << 15) on a forum or media channel; Discord reads
// it for the channel's posts, from their parent, and for nothing else.
#[test]
fn a_media_channels_posts_hide_download_options() {
    const MEDIA: u64 = 300_000_000_000_000_040;
    const POST: u64 = 300_000_000_000_000_041;
    const FORUM: u64 = 300_000_000_000_000_042;
    const FORUM_POST: u64 = 300_000_000_000_000_043;
    const OPEN_MEDIA: u64 = 300_000_000_000_000_044;
    const OPEN_POST: u64 = 300_000_000_000_000_045;
    const FLAGGED_TEXT: u64 = 300_000_000_000_000_046;
    const TEXT_THREAD: u64 = 300_000_000_000_000_047;
    const ORPHAN: u64 = 300_000_000_000_000_048;
    let mut state = ready_state();
    let channel = |id: u64, kind: u8, flags: u64| {
        let mut channel = fixture(include_str!(
            "../../../../tests/fixtures/channel_guild_text.json"
        ));
        channel["id"] = id.to_string().into();
        channel["type"] = kind.into();
        channel["flags"] = flags.into();
        channel
    };
    let thread = |id: u64, parent: u64| {
        let mut thread = fixture(include_str!(
            "../../../../tests/fixtures/channel_thread.json"
        ));
        thread["id"] = id.to_string().into();
        thread["parent_id"] = parent.to_string().into();
        thread
    };
    for (parent, kind, flags, post) in [
        (MEDIA, 16, 1u64 << 15, POST),
        (FORUM, 15, 1 << 15, FORUM_POST),
        (OPEN_MEDIA, 16, 0, OPEN_POST),
        (FLAGGED_TEXT, 0, 1 << 15, TEXT_THREAD),
    ] {
        apply(&mut state, "CHANNEL_CREATE", channel(parent, kind, flags));
        apply(&mut state, "THREAD_CREATE", thread(post, parent));
    }
    apply(
        &mut state,
        "THREAD_CREATE",
        thread(ORPHAN, 300_000_000_000_000_099),
    );

    let hides = |state: &State, id: u64| state.hides_media_downloads(Snowflake::new(id));
    assert!(hides(&state, POST));
    assert!(hides(&state, FORUM_POST));
    assert!(!hides(&state, OPEN_POST), "the option is on");
    assert!(
        !hides(&state, MEDIA),
        "the channel itself shows no messages"
    );
    assert!(
        !hides(&state, TEXT_THREAD),
        "a text channel's thread isn't a post"
    );
    assert!(!hides(&state, ORPHAN), "its parent isn't known");
    assert!(!hides(&state, GENERAL));

    apply(&mut state, "CHANNEL_UPDATE", channel(MEDIA, 16, 0));
    assert!(
        !hides(&state, POST),
        "the channel turned the option back on"
    );
}
