use super::*;
use crate::model::ChannelType;
use crate::state::MessageActions;

const ADD_REACTIONS: u64 = 1 << 6;
const VIEW: u64 = 1 << 10;
const SEND: u64 = 1 << 11;
const MANAGE_MESSAGES: u64 = 1 << 13;
const HISTORY: u64 = 1 << 16;
const MANAGE_THREADS: u64 = 1 << 34;
const PUBLIC_THREADS: u64 = 1 << 35;
const IN_THREADS: u64 = 1 << 38;
const PIN: u64 = 1 << 51;
// A member who reads and writes everywhere and moderates nothing.
const MEMBER: u64 = VIEW | SEND | HISTORY | IN_THREADS | ADD_REACTIONS;

const EPHEMERAL: u64 = 1 << 6;
const VOICE_MESSAGE: u64 = 1 << 13;

const ROLE: u64 = 600;
const SYSTEM: u64 = 100_000_000_000_000_009;
const SYSTEM_DM: u64 = 300_000_000_000_000_012;
const MEDIA: u64 = 300_000_000_000_000_005;
const MEDIA_POST: u64 = 300_000_000_000_000_021;
const M: u64 = 400_000_000_000_000_050;

const FORUM: u64 = 300_000_000_000_000_006;
const FORUM_POST: u64 = 300_000_000_000_000_022;
const STAGE: u64 = 300_000_000_000_000_007;

// The sets as Discord's client names them (module 448761), each type by its number and name.
const USER_MESSAGE: [(u64, &str); 9] = [
    (0, "DEFAULT"),
    (19, "REPLY"),
    (20, "CHAT_INPUT_COMMAND"),
    (23, "CONTEXT_MENU_COMMAND"),
    (26, "INTERACTION_PREMIUM_UPSELL"),
    (41, "CUSTOM_GIFT"),
    (45, "VOICE_HANGOUT_INVITE"),
    (47, "CHANGELOG"),
    (68, "MEDIA_MENTION_MESSAGE"),
];
const REPLYABLE: [(u64, &str); 21] = [
    (0, "DEFAULT"),
    (7, "USER_JOIN"),
    (19, "REPLY"),
    (20, "CHAT_INPUT_COMMAND"),
    (23, "CONTEXT_MENU_COMMAND"),
    (24, "AUTO_MODERATION_ACTION"),
    (25, "ROLE_SUBSCRIPTION_PURCHASE"),
    (32, "GUILD_APPLICATION_PREMIUM_SUBSCRIPTION"),
    (33, "PRIVATE_CHANNEL_INTEGRATION_ADDED"),
    (34, "PRIVATE_CHANNEL_INTEGRATION_REMOVED"),
    (35, "PREMIUM_REFERRAL"),
    (36, "GUILD_INCIDENT_ALERT_MODE_ENABLED"),
    (37, "GUILD_INCIDENT_ALERT_MODE_DISABLED"),
    (38, "GUILD_INCIDENT_REPORT_RAID"),
    (39, "GUILD_INCIDENT_REPORT_FALSE_ALARM"),
    (40, "GUILD_DEADCHAT_REVIVE_PROMPT"),
    (41, "CUSTOM_GIFT"),
    (42, "GUILD_GAMING_STATS_PROMPT"),
    (45, "VOICE_HANGOUT_INVITE"),
    (46, "POLL_RESULT"),
    (69, "GUILD_SPACE_MESSAGE"),
];
// What Discord's list leaves out by name: recipient add and remove, call, name and icon change,
// pin, the boosts, channel follow, thread created, thread starter, the stage lines, purchase.
const NOT_REPLYABLE: [u64; 19] = [
    1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 18, 21, 27, 28, 29, 30, 31, 44,
];
const UNDELETABLE: [(u64, &str); 11] = [
    (1, "RECIPIENT_ADD"),
    (2, "RECIPIENT_REMOVE"),
    (3, "CALL"),
    (4, "CHANNEL_NAME_CHANGE"),
    (5, "CHANNEL_ICON_CHANGE"),
    (21, "THREAD_STARTER_MESSAGE"),
    (35, "PREMIUM_REFERRAL"),
    (56, "CHAT_WALLPAPER_SET"),
    (57, "CHAT_WALLPAPER_REMOVED"),
    (64, "PREMIUM_GROUP_INVITE"),
    (68, "MEDIA_MENTION_MESSAGE"),
];
// System lines a moderator deletes: join, pin, the boosts, thread created.
const DELETABLE_LINES: [u64; 7] = [6, 7, 8, 9, 10, 11, 18];

fn listed(set: &[(u64, &str)], kind: u64) -> bool {
    set.iter().any(|(listed, _)| *listed == kind)
}

fn places(ready: &mut Value) {
    ready["users"].as_array_mut().unwrap().push(json!({
        "id": SYSTEM.to_string(), "username": "system", "discriminator": "0000",
        "avatar": null, "system": true, "public_flags": 0
    }));
    ready["private_channels"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": SYSTEM_DM.to_string(), "type": 1, "flags": 0,
            "recipient_ids": [SYSTEM.to_string()]
        }));
    let guild = &mut ready["guilds"][0];
    guild["channels"].as_array_mut().unwrap().push(json!({
        "id": MEDIA.to_string(), "type": 16, "name": "media", "position": 3,
        "permission_overwrites": []
    }));
    guild["channels"].as_array_mut().unwrap().push(json!({
        "id": FORUM.to_string(), "type": 15, "name": "forum", "position": 4,
        "permission_overwrites": []
    }));
    guild["channels"].as_array_mut().unwrap().push(json!({
        "id": STAGE.to_string(), "type": 13, "name": "stage", "position": 5,
        "permission_overwrites": []
    }));
    for (id, parent) in [(MEDIA_POST, MEDIA), (FORUM_POST, FORUM)] {
        let mut post = guild["threads"][0].clone();
        post["id"] = id.to_string().into();
        post["parent_id"] = parent.to_string().into();
        guild["threads"].as_array_mut().unwrap().push(post);
    }
}

// The fixture's user owns the guild and has every permission.
fn owner(edit: impl FnOnce(&mut Value)) -> State {
    let mut ready = ready_value();
    places(&mut ready);
    edit(&mut ready);
    state_from(&ready)
}

// A plain member whose one role allows `allow`.
fn member(allow: u64, edit: impl FnOnce(&mut Value)) -> State {
    owner(|ready| {
        let guild = &mut ready["guilds"][0];
        guild["properties"]["owner_id"] = "100000000000000099".into();
        guild["roles"][0]["permissions"] = "0".into();
        guild["roles"].as_array_mut().unwrap().push(json!({
            "id": ROLE.to_string(), "name": "r", "permissions": allow.to_string(), "position": 2
        }));
        ready["merged_members"][0][0]["roles"] = json!([ROLE.to_string()]);
        edit(ready);
    })
}

fn plain(_: &mut Value) {}

fn archived(locked: bool) -> impl FnOnce(&mut Value) {
    move |ready| {
        let metadata = &mut ready["guilds"][0]["threads"][0]["thread_metadata"];
        metadata["archived"] = true.into();
        metadata["locked"] = locked.into();
    }
}

fn locked(ready: &mut Value) {
    ready["guilds"][0]["threads"][0]["thread_metadata"]["locked"] = true.into();
}

fn timeout(ready: &mut Value) {
    ready["merged_members"][0][0]["communication_disabled_until"] =
        "2999-01-01T00:00:00+00:00".into();
}

fn quarantine(ready: &mut Value) {
    ready["merged_members"][0][0]["flags"] = (1 << 7).into();
}

fn load(mut state: State, channel: u64, messages: &[Value]) -> State {
    let id = Snowflake::new(channel);
    let mut events = Vec::new();
    state.view_channel(id, &mut events);
    let ticket = state
        .begin_load(id, LoadKind::Latest, 1, &mut events)
        .unwrap();
    let page = messages
        .iter()
        .map(|message| serde_json::from_str(&message.to_string()).unwrap())
        .collect();
    state.finish_load(ticket, page, true, &mut events);
    state
}

// Message `M` of `channel` by `author` as Discord sends it.
fn sent(channel: u64, author: u64, kind: u64) -> Value {
    let mut message = if matches!(channel, DM | GROUP | SYSTEM_DM) {
        dm_message(M, channel)
    } else {
        message(M, channel)
    };
    message["author"]["id"] = author.to_string().into();
    message["type"] = kind.into();
    message
}

fn with(mut message: Value, edit: impl FnOnce(&mut Value)) -> Value {
    edit(&mut message);
    message
}

fn of(state: &State, channel: u64) -> MessageActions {
    state.message_actions(
        Snowflake::new(channel),
        Snowflake::new(M),
        state.now_millis(),
    )
}

// What the user may do with `message`, loaded into its channel.
fn actions(state: State, message: Value) -> MessageActions {
    let channel = message["channel_id"].as_str().unwrap().parse().unwrap();
    of(&load(state, channel, &[message]), channel)
}

fn queued(state: &mut State, channel: u64) {
    let me = state.current_user().unwrap();
    let now = state.now_millis();
    state.queue(
        Snowflake::new(channel),
        Arc::new(Message::pending(
            Snowflake::new(M),
            Snowflake::new(channel),
            Arc::new(me.user.clone()),
            "hi".to_owned(),
            0,
        )),
        now,
        &mut Vec::new(),
    );
}

#[test]
fn the_fixture_has_its_places() {
    let state = owner(archived(true));
    let thread = state.channel(Snowflake::new(THREAD)).unwrap();
    let info = thread.thread.as_deref().unwrap();
    assert!(info.archived && info.locked);
    assert!(state.channel(Snowflake::new(MEDIA)).unwrap().is_forum());
    assert!(
        state
            .channel(Snowflake::new(MEDIA_POST))
            .unwrap()
            .is_thread()
    );
    assert_eq!(
        state.channel(Snowflake::new(FORUM)).unwrap().kind,
        ChannelType::GuildForum
    );
    assert_eq!(
        state.channel(Snowflake::new(FORUM_POST)).unwrap().parent_id,
        Some(Snowflake::new(FORUM))
    );
    assert_eq!(
        state.channel(Snowflake::new(STAGE)).unwrap().kind,
        ChannelType::GuildStageVoice
    );
    assert!(state.user(Snowflake::new(SYSTEM)).unwrap().system);
    assert!(state.channel(Snowflake::new(SYSTEM_DM)).is_some());
}

#[test]
fn a_message_the_store_doesnt_have_offers_nothing() {
    let state = load(owner(plain), GENERAL, &[]);
    assert_eq!(of(&state, GENERAL), MessageActions::default());
    assert_eq!(of(&state, DM), MessageActions::default());
}

#[test]
fn a_sending_message_offers_nothing_and_a_failed_one_only_delete() {
    for channel in [GENERAL, DM] {
        let mut state = load(owner(plain), channel, &[]);
        queued(&mut state, channel);
        assert_eq!(of(&state, channel), MessageActions::default(), "{channel}");

        state.fail(Snowflake::new(channel), Snowflake::new(M), &mut Vec::new());
        let failed = of(&state, channel);
        assert!(failed.delete, "{channel}");
        assert_eq!(
            MessageActions {
                delete: false,
                ..failed
            },
            MessageActions::default(),
            "{channel}"
        );
    }
}

#[test]
fn an_ephemeral_message_offers_nothing() {
    for author in [ME, MIRA] {
        let ephemeral = with(sent(GENERAL, author, 0), |message| {
            message["flags"] = EPHEMERAL.into();
        });
        assert_eq!(
            actions(owner(plain), ephemeral),
            MessageActions::default(),
            "{author}"
        );
    }
}

#[test]
fn a_message_of_a_channel_the_user_cant_view_offers_nothing() {
    let hidden = member(MEMBER & !VIEW, plain);
    assert_eq!(
        actions(hidden, sent(GENERAL, MIRA, 0)),
        MessageActions::default()
    );
}

#[test]
fn replying_follows_discords_rule() {
    let cases: Vec<(&str, State, Value, bool)> = vec![
        (
            "send and history",
            member(VIEW | SEND | HISTORY, plain),
            sent(GENERAL, MIRA, 0),
            true,
        ),
        (
            "no send",
            member(VIEW | HISTORY, plain),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        (
            "no history",
            member(VIEW | SEND, plain),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        (
            "timed out",
            member(MEMBER, timeout),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        (
            "quarantined",
            member(MEMBER, quarantine),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        ("own", member(MEMBER, plain), sent(GENERAL, ME, 0), true),
        ("dm", member(0, plain), sent(DM, MIRA, 0), true),
        ("group dm", member(0, plain), sent(GROUP, MIRA, 0), true),
        ("system dm", owner(plain), sent(SYSTEM_DM, SYSTEM, 0), false),
        (
            "thread",
            member(VIEW | HISTORY | IN_THREADS, plain),
            sent(THREAD, MIRA, 0),
            true,
        ),
        (
            "thread, only send",
            member(VIEW | HISTORY | SEND, plain),
            sent(THREAD, MIRA, 0),
            false,
        ),
        (
            "archived",
            member(MEMBER, archived(false)),
            sent(THREAD, MIRA, 0),
            true,
        ),
        (
            "archived, no send",
            member(VIEW | HISTORY, archived(false)),
            sent(THREAD, MIRA, 0),
            false,
        ),
        (
            "locked",
            member(MEMBER, locked),
            sent(THREAD, MIRA, 0),
            false,
        ),
        (
            "locked, moderator",
            member(MEMBER | MANAGE_THREADS, locked),
            sent(THREAD, MIRA, 0),
            true,
        ),
    ];
    for (name, state, message, expected) in cases {
        assert_eq!(actions(state, message).reply, expected, "{name}");
    }
}

#[test]
fn only_discords_replyable_types_take_a_reply() {
    for kind in 0..=70 {
        assert_eq!(
            actions(owner(plain), sent(GENERAL, MIRA, kind)).reply,
            listed(&REPLYABLE, kind),
            "type {kind}"
        );
    }
    for kind in NOT_REPLYABLE {
        assert!(
            !actions(owner(plain), sent(GENERAL, MIRA, kind)).reply,
            "type {kind}"
        );
    }
}

#[test]
fn the_replys_ping_and_its_switch() {
    let cases = [
        ("another's", GENERAL, MIRA, true, true),
        ("own", GENERAL, ME, false, false),
        ("another's in a dm", DM, MIRA, true, false),
        ("own in a dm", DM, ME, false, false),
        ("another's in a group dm", GROUP, MIRA, true, false),
    ];
    for (name, channel, author, pings, toggle) in cases {
        let actions = actions(owner(plain), sent(channel, author, 0));
        assert!(actions.reply, "{name}");
        assert_eq!(actions.reply_pings, pings, "{name}");
        assert_eq!(actions.mention_toggle, toggle, "{name}");
    }

    let none = actions(member(VIEW | HISTORY, plain), sent(GENERAL, MIRA, 0));
    assert!(!none.reply && !none.reply_pings && !none.mention_toggle);
}

fn sticker(message: &mut Value) {
    message["sticker_items"] =
        json!([{"id": "700000000000000001", "name": "wave", "format_type": 1}]);
}

fn forward(message: &mut Value) {
    message["message_reference"] = json!({
        "type": 1, "channel_id": VOICE.to_string(), "message_id": "400000000000000001"
    });
}

#[test]
fn editing_follows_discords_rule() {
    let own = || sent(GENERAL, ME, 0);
    let cases: Vec<(&str, State, Value, bool)> = vec![
        ("own", member(MEMBER, plain), own(), true),
        ("another's", owner(plain), sent(GENERAL, MIRA, 0), false),
        (
            "own reply",
            member(MEMBER, plain),
            sent(GENERAL, ME, 19),
            true,
        ),
        (
            "no text",
            member(MEMBER, plain),
            with(own(), |m| m["content"] = "".into()),
            true,
        ),
        (
            "sticker with text",
            member(MEMBER, plain),
            with(own(), sticker),
            true,
        ),
        (
            "sticker alone",
            member(MEMBER, plain),
            with(own(), |m| {
                sticker(m);
                m["content"] = "".into();
            }),
            false,
        ),
        (
            "voice message",
            member(MEMBER, plain),
            with(own(), |m| m["flags"] = VOICE_MESSAGE.into()),
            false,
        ),
        (
            "forward",
            member(MEMBER, plain),
            with(own(), forward),
            false,
        ),
        ("no send", member(VIEW | HISTORY, plain), own(), false),
        ("timed out", member(MEMBER, timeout), own(), false),
        ("dm", member(0, plain), sent(DM, ME, 0), true),
        (
            "another's in a dm",
            member(0, plain),
            sent(DM, MIRA, 0),
            false,
        ),
        ("group dm", member(0, plain), sent(GROUP, ME, 0), true),
        ("system dm", owner(plain), sent(SYSTEM_DM, ME, 0), false),
        ("thread", member(MEMBER, plain), sent(THREAD, ME, 0), true),
        (
            "archived",
            member(MEMBER, archived(false)),
            sent(THREAD, ME, 0),
            true,
        ),
        (
            "archived, no send",
            member(VIEW | HISTORY, archived(false)),
            sent(THREAD, ME, 0),
            false,
        ),
        (
            "archived and locked",
            owner(archived(true)),
            sent(THREAD, ME, 0),
            false,
        ),
        ("locked", member(MEMBER, locked), sent(THREAD, ME, 0), false),
        (
            "locked, moderator",
            member(MEMBER | MANAGE_THREADS, locked),
            sent(THREAD, ME, 0),
            true,
        ),
    ];
    for (name, state, message, expected) in cases {
        assert_eq!(actions(state, message).edit, expected, "{name}");
    }
}

#[test]
fn only_a_user_message_is_edited_and_never_a_media_mention() {
    for kind in 0..=70 {
        assert_eq!(
            actions(owner(plain), sent(GENERAL, ME, kind)).edit,
            listed(&USER_MESSAGE, kind) && kind != 68,
            "type {kind}"
        );
    }
}

#[test]
fn deleting_follows_discords_rule() {
    let moderator = MEMBER | MANAGE_MESSAGES;
    let cases: Vec<(&str, State, Value, bool)> = vec![
        ("own", member(MEMBER, plain), sent(GENERAL, ME, 0), true),
        (
            "another's",
            member(MEMBER, plain),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        (
            "another's, moderator",
            member(moderator, plain),
            sent(GENERAL, MIRA, 0),
            true,
        ),
        (
            "own while timed out",
            member(MEMBER, timeout),
            sent(GENERAL, ME, 0),
            true,
        ),
        (
            "a join line",
            member(MEMBER, plain),
            sent(GENERAL, MIRA, 7),
            false,
        ),
        (
            "a join line, moderator",
            member(moderator, plain),
            sent(GENERAL, MIRA, 7),
            true,
        ),
        (
            "a pin line, moderator",
            member(moderator, plain),
            sent(GENERAL, MIRA, 6),
            true,
        ),
        (
            "automod's, own",
            member(MEMBER, plain),
            sent(GENERAL, ME, 24),
            false,
        ),
        (
            "automod's, moderator",
            member(moderator, plain),
            sent(GENERAL, ME, 24),
            true,
        ),
        ("dm, own", member(0, plain), sent(DM, ME, 0), true),
        ("dm, another's", owner(plain), sent(DM, MIRA, 0), false),
        (
            "own group dm, another's",
            owner(plain),
            sent(GROUP, MIRA, 0),
            false,
        ),
        ("group dm, own", owner(plain), sent(GROUP, ME, 0), true),
        ("system dm", owner(plain), sent(SYSTEM_DM, SYSTEM, 0), false),
        (
            "archived",
            member(MEMBER, archived(false)),
            sent(THREAD, ME, 0),
            true,
        ),
        (
            "archived, no send",
            member(VIEW | HISTORY, archived(false)),
            sent(THREAD, ME, 0),
            false,
        ),
        (
            "archived and locked",
            owner(archived(true)),
            sent(THREAD, ME, 0),
            false,
        ),
        ("locked", member(MEMBER, locked), sent(THREAD, ME, 0), true),
    ];
    for (name, state, message, expected) in cases {
        assert_eq!(actions(state, message).delete, expected, "{name}");
    }
}

#[test]
fn discords_undeletable_types_are_never_deleted() {
    for kind in 0..=70 {
        assert_eq!(
            actions(owner(plain), sent(GENERAL, ME, kind)).delete,
            !listed(&UNDELETABLE, kind),
            "type {kind}"
        );
    }
    for kind in DELETABLE_LINES {
        assert!(
            actions(owner(plain), sent(GENERAL, MIRA, kind)).delete,
            "type {kind}"
        );
    }
}

#[test]
fn pinning_follows_discords_rule() {
    let cases: Vec<(&str, State, Value, bool)> = vec![
        (
            "pin and history",
            member(VIEW | PIN | HISTORY, plain),
            sent(GENERAL, MIRA, 0),
            true,
        ),
        (
            "no history",
            member(VIEW | PIN, plain),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        (
            "manage messages alone",
            member(MEMBER | MANAGE_MESSAGES, plain),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        (
            "timed out",
            member(MEMBER | PIN, timeout),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        ("dm", member(0, plain), sent(DM, MIRA, 0), true),
        ("group dm", member(0, plain), sent(GROUP, ME, 0), true),
        ("system dm", owner(plain), sent(SYSTEM_DM, SYSTEM, 0), false),
        ("voice chat", owner(plain), sent(VOICE, MIRA, 0), false),
        ("stage chat", owner(plain), sent(STAGE, MIRA, 0), false),
        ("media post", owner(plain), sent(MEDIA_POST, MIRA, 0), false),
        ("forum post", owner(plain), sent(FORUM_POST, MIRA, 0), true),
        (
            "thread",
            member(MEMBER | PIN, plain),
            sent(THREAD, MIRA, 0),
            true,
        ),
        (
            "archived",
            member(MEMBER | PIN, archived(false)),
            sent(THREAD, MIRA, 0),
            true,
        ),
        (
            "archived, no send",
            member(VIEW | HISTORY | PIN, archived(false)),
            sent(THREAD, MIRA, 0),
            false,
        ),
        (
            "archived and locked",
            owner(archived(true)),
            sent(THREAD, MIRA, 0),
            false,
        ),
        (
            "locked",
            member(MEMBER | PIN, locked),
            sent(THREAD, MIRA, 0),
            true,
        ),
    ];
    for (name, state, message, expected) in cases {
        let unpinned = actions(state, message);
        assert_eq!(unpinned.pin, expected, "{name}");
        assert!(!unpinned.unpin, "{name}");
    }
}

#[test]
fn a_pinned_message_is_unpinned_by_the_same_rule() {
    let pinned = |channel| with(sent(channel, MIRA, 0), |m| m["pinned"] = true.into());

    let may = actions(member(VIEW | PIN | HISTORY, plain), pinned(GENERAL));
    assert!(may.unpin && !may.pin);
    let dm = actions(member(0, plain), pinned(DM));
    assert!(dm.unpin && !dm.pin);
    let may_not = actions(member(MEMBER, plain), pinned(GENERAL));
    assert!(!may_not.unpin && !may_not.pin);
}

#[test]
fn only_a_user_message_is_pinned() {
    for kind in 0..=70 {
        assert_eq!(
            actions(owner(plain), sent(GENERAL, MIRA, kind)).pin,
            listed(&USER_MESSAGE, kind),
            "type {kind}"
        );
    }
}

#[test]
fn reacting_follows_discords_rules() {
    let no_add = MEMBER & !ADD_REACTIONS;
    // (new emoji, an emoji already there)
    let cases: Vec<(&str, State, Value, (bool, bool))> = vec![
        (
            "add reactions",
            member(MEMBER, plain),
            sent(GENERAL, MIRA, 0),
            (true, true),
        ),
        (
            "without",
            member(no_add, plain),
            sent(GENERAL, MIRA, 0),
            (false, true),
        ),
        (
            "no history",
            member(MEMBER & !HISTORY, plain),
            sent(GENERAL, MIRA, 0),
            (false, false),
        ),
        (
            "timed out",
            member(MEMBER, timeout),
            sent(GENERAL, MIRA, 0),
            (false, false),
        ),
        (
            "quarantined",
            member(MEMBER, quarantine),
            sent(GENERAL, MIRA, 0),
            (false, false),
        ),
        (
            "the owner's timeout",
            owner(timeout),
            sent(GENERAL, MIRA, 0),
            (true, true),
        ),
        (
            "a join line",
            member(MEMBER, plain),
            sent(GENERAL, MIRA, 7),
            (true, true),
        ),
        (
            "own",
            member(MEMBER, plain),
            sent(GENERAL, ME, 0),
            (true, true),
        ),
        ("dm", member(0, plain), sent(DM, MIRA, 0), (true, true)),
        (
            "group dm",
            member(0, plain),
            sent(GROUP, MIRA, 0),
            (true, true),
        ),
        (
            "system dm",
            owner(plain),
            sent(SYSTEM_DM, SYSTEM, 0),
            (false, false),
        ),
        (
            "media post",
            owner(plain),
            sent(MEDIA_POST, MIRA, 0),
            (false, false),
        ),
        (
            "forum post",
            owner(plain),
            sent(FORUM_POST, MIRA, 0),
            (true, true),
        ),
        (
            "voice chat",
            owner(plain),
            sent(VOICE, MIRA, 0),
            (true, true),
        ),
        (
            "thread",
            member(MEMBER, plain),
            sent(THREAD, MIRA, 0),
            (true, true),
        ),
        (
            "archived",
            member(MEMBER, archived(false)),
            sent(THREAD, MIRA, 0),
            (true, true),
        ),
        (
            "archived, no send",
            member(VIEW | HISTORY | ADD_REACTIONS, archived(false)),
            sent(THREAD, MIRA, 0),
            (false, false),
        ),
        (
            "archived and locked",
            owner(archived(true)),
            sent(THREAD, MIRA, 0),
            (false, false),
        ),
        (
            "locked",
            member(MEMBER, locked),
            sent(THREAD, MIRA, 0),
            (true, true),
        ),
    ];
    for (name, state, message, expected) in cases {
        let actions = actions(state, message);
        assert_eq!((actions.react, actions.toggle_reaction), expected, "{name}");
    }
}

#[test]
fn removing_others_reactions_needs_manage_messages() {
    let moderator = MEMBER | MANAGE_MESSAGES;
    let cases: Vec<(&str, State, Value, bool)> = vec![
        (
            "moderator",
            member(moderator, plain),
            sent(GENERAL, MIRA, 0),
            true,
        ),
        ("member", member(MEMBER, plain), sent(GENERAL, ME, 0), false),
        ("dm", owner(plain), sent(DM, ME, 0), false),
        ("own group dm", owner(plain), sent(GROUP, ME, 0), false),
        (
            "archived",
            member(moderator, archived(false)),
            sent(THREAD, MIRA, 0),
            true,
        ),
        (
            "archived, no send",
            member(VIEW | HISTORY | MANAGE_MESSAGES, archived(false)),
            sent(THREAD, MIRA, 0),
            false,
        ),
        (
            "archived and locked",
            owner(archived(true)),
            sent(THREAD, MIRA, 0),
            false,
        ),
    ];
    for (name, state, message, expected) in cases {
        assert_eq!(actions(state, message).remove_reactions, expected, "{name}");
    }
}

#[test]
fn marking_unread_and_copying_the_link() {
    // (mark unread, copy link)
    let cases: Vec<(&str, State, Value, (bool, bool))> = vec![
        (
            "channel",
            member(VIEW, plain),
            sent(GENERAL, MIRA, 0),
            (true, true),
        ),
        (
            "a join line",
            member(VIEW, plain),
            sent(GENERAL, MIRA, 7),
            (true, true),
        ),
        ("dm", member(0, plain), sent(DM, MIRA, 0), (true, true)),
        (
            "system dm",
            owner(plain),
            sent(SYSTEM_DM, SYSTEM, 0),
            (true, true),
        ),
        (
            "timed out",
            member(MEMBER, timeout),
            sent(GENERAL, MIRA, 0),
            (true, true),
        ),
        (
            "archived",
            member(MEMBER, archived(false)),
            sent(THREAD, MIRA, 0),
            (true, true),
        ),
        (
            "archived, no send",
            member(VIEW, archived(false)),
            sent(THREAD, MIRA, 0),
            (false, true),
        ),
        (
            "archived and locked",
            owner(archived(true)),
            sent(THREAD, MIRA, 0),
            (false, true),
        ),
        (
            "locked",
            member(VIEW, locked),
            sent(THREAD, MIRA, 0),
            (true, true),
        ),
    ];
    for (name, state, message, expected) in cases {
        let actions = actions(state, message);
        assert_eq!((actions.mark_unread, actions.copy_link), expected, "{name}");
    }
}

#[test]
fn creating_a_thread_is_can_start_threads_answer() {
    let creator = VIEW | HISTORY | PUBLIC_THREADS | IN_THREADS;
    let cases: Vec<(&str, State, Value, bool)> = vec![
        (
            "creator",
            member(creator, plain),
            sent(GENERAL, MIRA, 0),
            true,
        ),
        (
            "member",
            member(MEMBER, plain),
            sent(GENERAL, MIRA, 0),
            false,
        ),
        ("a join line", owner(plain), sent(GENERAL, MIRA, 7), false),
        ("in a thread", owner(plain), sent(THREAD, MIRA, 0), false),
        ("dm", owner(plain), sent(DM, MIRA, 0), false),
        (
            "has a thread",
            owner(plain),
            with(sent(GENERAL, MIRA, 0), |m| m["flags"] = (1 << 5).into()),
            false,
        ),
    ];
    for (name, state, message, expected) in cases {
        let state = load(state, GENERAL, &[]);
        let channel = message["channel_id"].as_str().unwrap().parse().unwrap();
        let state = load(state, channel, &[message]);
        let actions = of(&state, channel);
        assert_eq!(actions.create_thread, expected, "{name}");
        assert_eq!(
            actions.create_thread,
            state.can_start_thread(
                Snowflake::new(channel),
                Snowflake::new(M),
                state.now_millis()
            ),
            "{name}"
        );
    }
}

fn thread_of_m(parent: u64) -> Value {
    let mut thread = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    thread["id"] = M.to_string().into();
    thread["parent_id"] = parent.to_string().into();
    thread["member"]["id"] = M.to_string().into();
    thread
}

#[test]
fn viewing_a_thread_needs_the_thread() {
    let flagged = || with(sent(GENERAL, MIRA, 0), |m| m["flags"] = (1 << 5).into());

    let unknown = actions(owner(plain), flagged());
    assert!(!unknown.view_thread && !unknown.create_thread);

    let mut state = load(owner(plain), GENERAL, &[flagged()]);
    apply(&mut state, "THREAD_CREATE", thread_of_m(GENERAL));
    let known = of(&state, GENERAL);
    assert!(known.view_thread && !known.create_thread);

    // Someone's thread from it, before the message's update says so.
    let mut state = load(owner(plain), GENERAL, &[sent(GENERAL, MIRA, 0)]);
    apply(&mut state, "THREAD_CREATE", thread_of_m(GENERAL));
    let unflagged = of(&state, GENERAL);
    assert!(unflagged.view_thread && !unflagged.create_thread);
}

#[test]
fn viewing_a_thread_needs_view_channel_on_the_thread_itself() {
    let flagged = with(sent(GENERAL, MIRA, 0), |m| m["flags"] = (1 << 5).into());
    let hidden_voice = member(MEMBER, |ready| {
        ready["guilds"][0]["channels"][2]["permission_overwrites"] =
            json!([{"id": ROLE.to_string(), "type": 0, "allow": "0", "deny": VIEW.to_string()}]);
    });
    let mut state = load(hidden_voice, GENERAL, &[flagged]);
    apply(&mut state, "THREAD_CREATE", thread_of_m(VOICE));
    assert!(state.channel(Snowflake::new(M)).is_some());

    let actions = of(&state, GENERAL);

    assert!(actions.reply && actions.copy_link);
    assert!(!actions.view_thread);
}

#[test]
fn a_forum_posts_message_is_pinned_and_reacted_to() {
    let forum = actions(owner(plain), sent(FORUM_POST, ME, 0));
    assert!(forum.pin && forum.react && forum.toggle_reaction);

    let media = actions(owner(plain), sent(MEDIA_POST, ME, 0));
    assert!(!media.pin && !media.react && !media.toggle_reaction);
    assert!(media.reply && media.edit);
}

#[test]
fn a_message_in_a_guild_without_my_member_offers_nothing() {
    let memberless = owner(|ready| ready["merged_members"][0] = json!([]));
    assert!(memberless.current_member(Snowflake::new(G1)).is_none());

    assert_eq!(
        actions(memberless, sent(GENERAL, ME, 0)),
        MessageActions::default()
    );
}

// A forum post's first message has the post's ID and sits in the post.
#[test]
fn a_posts_first_message_views_no_thread() {
    let mut first = sent(MEDIA_POST, MIRA, 0);
    first["id"] = MEDIA_POST.to_string().into();
    let state = load(owner(plain), MEDIA_POST, &[first]);

    let actions = state.message_actions(
        Snowflake::new(MEDIA_POST),
        Snowflake::new(MEDIA_POST),
        state.now_millis(),
    );
    assert!(actions.copy_link && !actions.view_thread);
}

#[test]
fn a_message_the_window_let_go_of_is_judged_the_same() {
    let loaded = load(
        member(MEMBER | PIN, plain),
        GENERAL,
        &[sent(GENERAL, ME, 0)],
    );
    let message = loaded
        .message(Snowflake::new(GENERAL), Snowflake::new(M))
        .unwrap();
    let elsewhere = member(MEMBER | PIN, plain);

    let actions = elsewhere.message_actions_of(&message, elsewhere.now_millis());
    assert_eq!(actions, of(&loaded, GENERAL));
    assert!(actions.edit && actions.delete && actions.pin && actions.reply);
}

// A thread's starter is the parent's message, shown in the thread while the parent isn't open.
#[test]
fn a_shown_starter_message_is_found() {
    let mut state = owner(plain);
    apply(&mut state, "THREAD_CREATE", thread_of_m(GENERAL));
    let starter = with(sent(M, MIRA, 21), |m| {
        m["id"] = (M + 1).to_string().into();
        m["message_reference"] = json!({
            "channel_id": GENERAL.to_string(), "message_id": M.to_string()
        });
        m["referenced_message"] = sent(GENERAL, MIRA, 0);
    });
    let state = load(state, M, &[starter]);
    assert!(matches!(
        state.starter_message(Snowflake::new(M)),
        Some(crate::state::StarterMessage::Loaded(_))
    ));

    let actions = of(&state, GENERAL);
    assert!(actions.copy_link && actions.view_thread);
}

const ROW: u64 = 400_000_000_000_000_021;

// THREAD's first row, pointing back at the message of GENERAL it started from.
fn starter(origin: Option<Value>) -> Value {
    let mut row = message(ROW, THREAD);
    row["type"] = 21.into();
    row["message_reference"] = json!({
        "message_id": THREAD.to_string(), "channel_id": GENERAL.to_string(),
    });
    if let Some(origin) = origin {
        row["referenced_message"] = origin;
    }
    row
}

fn of_row(state: &State) -> MessageActions {
    state.message_actions(
        Snowflake::new(THREAD),
        Snowflake::new(ROW),
        state.now_millis(),
    )
}

// The official client renders the row as the original message; it lives in the parent, so the
// thread's own state doesn't count.
#[test]
fn a_starter_row_answers_for_the_message_it_shows() {
    let state = load(
        owner(archived(true)),
        THREAD,
        &[starter(Some(message(THREAD, GENERAL)))],
    );
    let Some(crate::state::StarterMessage::Loaded(original)) =
        state.starter_message(Snowflake::new(THREAD))
    else {
        panic!("expected the starter");
    };
    let in_parent = state.message_actions_of(&original, state.now_millis());
    assert!(in_parent.reply && in_parent.react && in_parent.delete && in_parent.view_thread);

    let actions = of_row(&state);

    assert_eq!(
        actions,
        MessageActions {
            view_thread: false,
            mark_unread: false,
            ..in_parent
        },
        "the thread it would view and the thread a mark would mark are the one the row is in"
    );
    assert_eq!(
        state.shown_message(Snowflake::new(THREAD), Snowflake::new(ROW)),
        Some((Snowflake::new(GENERAL), Snowflake::new(THREAD)))
    );
}

fn link(state: &State, channel: u64, row: u64) -> String {
    state.message_link(Snowflake::new(channel), Snowflake::new(row))
}

#[test]
fn a_message_link_names_the_guild_or_the_dms() {
    let state = load(owner(plain), GENERAL, &[sent(GENERAL, MIRA, 0)]);
    assert_eq!(
        link(&state, GENERAL, M),
        format!("https://discord.com/channels/{G1}/{GENERAL}/{M}")
    );

    let state = load(owner(plain), DM, &[sent(DM, MIRA, 0)]);
    assert_eq!(
        link(&state, DM, M),
        format!("https://discord.com/channels/@me/{DM}/{M}")
    );
}

// As the official client's, which copies the link of the message the row renders.
#[test]
fn a_starter_rows_link_is_the_originals() {
    let shown = load(
        owner(plain),
        THREAD,
        &[starter(Some(message(THREAD, GENERAL)))],
    );
    assert_eq!(
        link(&shown, THREAD, ROW),
        format!("https://discord.com/channels/{G1}/{GENERAL}/{THREAD}")
    );

    let missing = load(owner(plain), THREAD, &[starter(None)]);
    assert_eq!(
        link(&missing, THREAD, ROW),
        format!("https://discord.com/channels/{G1}/{THREAD}/{ROW}"),
        "the row itself while its message isn't loaded"
    );
}

// Marking unread from the row marks the thread, so the thread says whether it may be.
#[test]
fn a_starter_rows_mark_unread_is_its_threads() {
    let origin = || starter(Some(message(THREAD, GENERAL)));
    let mut events = Vec::new();

    let mut closed = load(owner(archived(true)), THREAD, &[origin()]);
    let now = closed.now_millis();
    assert!(!of_row(&closed).mark_unread);
    assert_eq!(
        closed.mark_unread(
            Snowflake::new(THREAD),
            Snowflake::new(ROW),
            now,
            &mut events
        ),
        None
    );

    let mut open = load(owner(plain), THREAD, &[origin()]);
    let now = open.now_millis();
    assert!(of_row(&open).mark_unread);
    let marked = open
        .mark_unread(
            Snowflake::new(THREAD),
            Snowflake::new(ROW),
            now,
            &mut events,
        )
        .unwrap();
    let row: MessageId = Snowflake::new(ROW);
    assert_eq!(
        marked.position,
        MessageId::from_unix_millis(row.unix_millis() - 1, 0),
        "from the thread's start"
    );
}

#[test]
fn a_starter_row_without_its_message_answers_for_itself() {
    let state = load(owner(plain), THREAD, &[starter(None)]);

    let actions = of_row(&state);

    assert!(!actions.reply && !actions.delete && !actions.edit);
    assert!(actions.react && actions.copy_link);
    let row = (Snowflake::new(THREAD), Snowflake::new(ROW));
    assert_eq!(state.shown_message(row.0, row.1), Some(row));
    assert_eq!(state.shown_message(row.0, Snowflake::new(M)), None);
}
