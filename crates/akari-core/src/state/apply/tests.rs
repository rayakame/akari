use serde_json::{Value, json};

use super::*;
use crate::gateway::{GatewayEvent, decode};
use crate::model::{Permissions, Snowflake};

const ME: u64 = 100_000_000_000_000_001;
const MIRA: u64 = 100_000_000_000_000_002;
const G1: u64 = 200_000_000_000_000_001;
const G2: u64 = 200_000_000_000_000_002;
const G3: u64 = 200_000_000_000_000_003;
const GENERAL: u64 = 300_000_000_000_000_002;
const THREAD: u64 = 300_000_000_000_000_020;

fn dispatch(name: &str, data: &Value) -> DispatchEvent {
    let payload = json!({"op": 0, "s": 1, "t": name, "d": data});
    match decode(payload.to_string().as_bytes()) {
        Ok(GatewayEvent::Dispatch { event, .. }) => event,
        other => panic!("expected a dispatch, got {other:?}"),
    }
}

fn fixture(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn ready_value() -> Value {
    fixture(include_str!("../../../tests/fixtures/ready.json"))["d"].clone()
}

fn state_from(ready: &Value) -> State {
    let mut state = State::new();
    state.apply(dispatch("READY", ready), &mut Vec::new());
    state
}

fn ready_state() -> State {
    state_from(&ready_value())
}

fn apply(state: &mut State, name: &str, data: Value) -> Vec<String> {
    let mut events = Vec::new();
    state.apply(dispatch(name, &data), &mut events);
    events.iter().map(describe).collect()
}

fn describe(event: &StoreEvent) -> String {
    match event {
        StoreEvent::Ready => "Ready".to_owned(),
        StoreEvent::CurrentUserUpdated(user) => {
            format!("CurrentUserUpdated({})", user.user.id.get())
        }
        StoreEvent::UserUpdated(user) => format!("UserUpdated({})", user.id.get()),
        StoreEvent::GuildAdded(guild) => format!("GuildAdded({})", guild.id.get()),
        StoreEvent::GuildUpdated(guild) => format!("GuildUpdated({})", guild.id.get()),
        StoreEvent::GuildRemoved { guild_id } => format!("GuildRemoved({})", guild_id.get()),
        StoreEvent::GuildUnavailable { guild_id } => {
            format!("GuildUnavailable({})", guild_id.get())
        }
        StoreEvent::CurrentMemberUpdated(member) => {
            format!("CurrentMemberUpdated({})", member.guild_id.get())
        }
        StoreEvent::ChannelAdded(channel) => format!("ChannelAdded({})", channel.id.get()),
        StoreEvent::ChannelUpdated(channel) => format!("ChannelUpdated({})", channel.id.get()),
        StoreEvent::ChannelRemoved { channel_id, .. } => {
            format!("ChannelRemoved({})", channel_id.get())
        }
        other => format!("{other:?}"),
    }
}

fn ids<T>(items: Vec<Arc<T>>, id: impl Fn(&T) -> u64) -> Vec<u64> {
    let mut ids: Vec<u64> = items.iter().map(|item| id(item)).collect();
    ids.sort_unstable();
    ids
}

fn channel_ids(channels: Vec<Arc<Channel>>) -> Vec<u64> {
    ids(channels, |channel| channel.id.get())
}

fn guild_create(id: u64) -> Value {
    let mut guild = fixture(include_str!("../../../tests/fixtures/guild_create.json"));
    guild["id"] = id.to_string().into();
    guild["properties"]["id"] = id.to_string().into();
    guild
}

#[test]
fn the_first_ready_emits_only_ready() {
    let mut state = State::new();

    let events = apply(&mut state, "READY", ready_value());

    assert_eq!(events, ["Ready"]);
    assert_eq!(
        state.current_user().map(|user| user.user.id.get()),
        Some(ME)
    );
    assert_eq!(ids(state.guilds(), |guild| guild.id.get()), [G1]);
    assert_eq!(state.unavailable_guilds(), [Snowflake::new(G2)]);
    assert_eq!(
        channel_ids(state.guild_channels(Snowflake::new(G1))),
        [300_000_000_000_000_001, GENERAL, 300_000_000_000_000_003]
    );
    assert_eq!(channel_ids(state.threads(Snowflake::new(G1))), [THREAD]);
    assert_eq!(
        channel_ids(state.private_channels()),
        [300_000_000_000_000_010, 300_000_000_000_000_011]
    );
    assert_eq!(
        state
            .channel(Snowflake::new(GENERAL))
            .and_then(|channel| channel.guild_id),
        Some(Snowflake::new(G1))
    );
    assert_eq!(
        state
            .user(Snowflake::new(MIRA))
            .map(|user| user.username.clone()),
        Some("mira".into())
    );
    assert!(
        state
            .user(Snowflake::new(100_000_000_000_000_003))
            .is_some()
    );
    assert_eq!(
        state
            .current_member(Snowflake::new(G1))
            .and_then(|member| member.nick.clone()),
        Some("Tester".into())
    );
}

#[test]
fn a_later_ready_emits_only_the_differences() {
    let mut state = ready_state();
    let mut next = ready_value();
    next["guilds"].as_array_mut().unwrap().remove(1);
    next["merged_members"].as_array_mut().unwrap().remove(1);
    next["guilds"][0]["channels"][1]["name"] = "renamed".into();
    let mut added = next["guilds"][0]["channels"][1].clone();
    added["id"] = "300000000000000004".into();
    next["guilds"][0]["channels"]
        .as_array_mut()
        .unwrap()
        .push(added);
    next["users"][0]["global_name"] = "Mira Two".into();
    next["merged_members"][0][0]["nick"] = "New nick".into();

    let events = apply(&mut state, "READY", next);

    assert_eq!(
        events,
        [
            format!("UserUpdated({MIRA})"),
            format!("GuildRemoved({G2})"),
            format!("CurrentMemberUpdated({G1})"),
            format!("ChannelUpdated({GENERAL})"),
            "ChannelAdded(300000000000000004)".to_owned(),
            "Ready".to_owned(),
        ]
    );
    assert!(state.unavailable_guilds().is_empty());
}

#[test]
fn an_identical_ready_emits_only_ready() {
    let mut state = ready_state();

    assert_eq!(apply(&mut state, "READY", ready_value()), ["Ready"]);
}

#[test]
fn guild_create_adds_a_guild_with_its_channels_and_member() {
    let mut state = ready_state();

    let events = apply(&mut state, "GUILD_CREATE", guild_create(G3));

    assert_eq!(events, [format!("GuildAdded({G3})")]);
    assert_eq!(
        channel_ids(state.guild_channels(Snowflake::new(G3))),
        [300_000_000_000_000_031]
    );
    assert!(state.current_member(Snowflake::new(G3)).is_some());
}

#[test]
fn an_unavailable_guild_comes_back_with_guild_create() {
    let mut state = ready_state();

    let events = apply(&mut state, "GUILD_CREATE", guild_create(G2));

    assert_eq!(events, [format!("GuildAdded({G2})")]);
    assert!(state.unavailable_guilds().is_empty());
    assert!(state.guild(Snowflake::new(G2)).is_some());
}

#[test]
fn a_repeated_guild_create_replaces_the_guild() {
    let mut state = ready_state();
    apply(&mut state, "GUILD_CREATE", guild_create(G3));
    let mut changed = guild_create(G3);
    changed["properties"]["name"] = "Garden 2".into();
    changed["channels"][0]["name"] = "hello".into();

    let identical = apply(&mut state, "GUILD_CREATE", guild_create(G3));
    let events = apply(&mut state, "GUILD_CREATE", changed);

    assert!(identical.is_empty(), "{identical:?}");
    assert_eq!(
        events,
        [
            format!("GuildUpdated({G3})"),
            "ChannelUpdated(300000000000000031)".to_owned()
        ]
    );
    assert_eq!(state.guild_channels(Snowflake::new(G3)).len(), 1);
}

#[test]
fn guild_delete_tells_leaving_from_an_outage() {
    let mut state = ready_state();

    let down = apply(
        &mut state,
        "GUILD_DELETE",
        json!({"id": G1.to_string(), "unavailable": true}),
    );

    assert_eq!(down, [format!("GuildUnavailable({G1})")]);
    assert!(state.guild(Snowflake::new(G1)).is_none());
    assert!(state.channel(Snowflake::new(GENERAL)).is_none());
    assert!(state.channel(Snowflake::new(THREAD)).is_none());
    assert!(state.current_member(Snowflake::new(G1)).is_none());
    assert!(state.unavailable_guilds().contains(&Snowflake::new(G1)));

    let left = apply(&mut state, "GUILD_DELETE", json!({"id": G1.to_string()}));

    assert_eq!(left, [format!("GuildRemoved({G1})")]);
    assert!(!state.unavailable_guilds().contains(&Snowflake::new(G1)));
}

#[test]
fn leaving_an_available_guild_removes_everything_in_it() {
    let mut state = ready_state();

    let events = apply(&mut state, "GUILD_DELETE", json!({"id": G1.to_string()}));

    assert_eq!(events, [format!("GuildRemoved({G1})")]);
    assert!(state.guild_channels(Snowflake::new(G1)).is_empty());
    assert!(state.threads(Snowflake::new(G1)).is_empty());
    assert!(!state.unavailable_guilds().contains(&Snowflake::new(G1)));
}

#[test]
fn guild_update_merges_and_keeps_roles_unless_sent() {
    let mut state = ready_state();

    let renamed = apply(
        &mut state,
        "GUILD_UPDATE",
        json!({"id": G1.to_string(), "name": "New"}),
    );
    let same = apply(
        &mut state,
        "GUILD_UPDATE",
        json!({"id": G1.to_string(), "name": "New"}),
    );

    assert_eq!(renamed, [format!("GuildUpdated({G1})")]);
    assert!(same.is_empty(), "{same:?}");
    let guild = state.guild(Snowflake::new(G1)).unwrap();
    assert_eq!(&*guild.name, "New");
    assert_eq!(guild.roles.len(), 2);
}

#[test]
fn role_events_update_the_guild() {
    let mut state = ready_state();
    let role =
        |id: &str, name: &str| json!({"id": id, "name": name, "permissions": "0", "position": 2});
    let guild_id = G1.to_string();
    let roles = |state: &State| {
        state
            .guild(Snowflake::new(G1))
            .unwrap()
            .roles
            .iter()
            .map(|role| role.name.to_string())
            .collect::<Vec<_>>()
    };

    let created = apply(
        &mut state,
        "GUILD_ROLE_CREATE",
        json!({"guild_id": guild_id, "role": role("500000000000000003", "Helpers")}),
    );
    let updated = apply(
        &mut state,
        "GUILD_ROLE_UPDATE",
        json!({"guild_id": guild_id, "role": role("500000000000000002", "Mods")}),
    );
    let deleted = apply(
        &mut state,
        "GUILD_ROLE_DELETE",
        json!({"guild_id": guild_id, "role_id": "500000000000000003"}),
    );

    for events in [created, updated, deleted] {
        assert_eq!(events, [format!("GuildUpdated({G1})")]);
    }
    assert_eq!(roles(&state), ["@everyone", "Mods"]);
}

#[test]
fn channel_events_add_update_and_remove() {
    let mut state = ready_state();
    let mut channel = fixture(include_str!("../../../tests/fixtures/channel_update.json"));
    channel["id"] = "300000000000000005".into();

    let created = apply(&mut state, "CHANNEL_CREATE", channel.clone());
    channel["name"] = "other".into();
    let recreated = apply(&mut state, "CHANNEL_CREATE", channel);
    let updated = apply(
        &mut state,
        "CHANNEL_UPDATE",
        json!({"id": "300000000000000005", "topic": "news"}),
    );
    let parent_deleted = apply(
        &mut state,
        "CHANNEL_DELETE",
        json!({"id": GENERAL.to_string(), "type": 0, "guild_id": G1.to_string()}),
    );
    let dm = apply(
        &mut state,
        "CHANNEL_CREATE",
        fixture(include_str!(
            "../../../tests/fixtures/channel_create_dm.json"
        )),
    );
    let dm_deleted = apply(
        &mut state,
        "CHANNEL_DELETE",
        json!({"id": "300000000000000010", "type": 1}),
    );

    assert_eq!(created, ["ChannelAdded(300000000000000005)"]);
    assert_eq!(recreated, ["ChannelUpdated(300000000000000005)"]);
    assert_eq!(updated, ["ChannelUpdated(300000000000000005)"]);
    assert_eq!(
        state
            .channel(Snowflake::new(300_000_000_000_000_005))
            .and_then(|channel| channel.topic.clone()),
        Some("news".into())
    );
    assert_eq!(
        parent_deleted,
        [
            format!("ChannelRemoved({GENERAL})"),
            format!("ChannelRemoved({THREAD})")
        ]
    );
    assert_eq!(dm, ["ChannelAdded(300000000000000013)"]);
    assert!(
        state
            .user(Snowflake::new(100_000_000_000_000_005))
            .is_some()
    );
    assert_eq!(dm_deleted, ["ChannelRemoved(300000000000000010)"]);
}

#[test]
fn thread_events_add_update_and_remove() {
    let mut state = ready_state();
    let thread = fixture(include_str!("../../../tests/fixtures/thread_create.json"));
    let mut archived = thread.clone();
    archived["thread_metadata"]["archived"] = true.into();

    let created = apply(&mut state, "THREAD_CREATE", thread);
    let threads = channel_ids(state.threads(Snowflake::new(G1)));
    let updated = apply(&mut state, "THREAD_UPDATE", archived);
    let is_archived = state
        .channel(Snowflake::new(300_000_000_000_000_021))
        .and_then(|channel| channel.thread.as_ref().map(|info| info.archived));
    let deleted = apply(
        &mut state,
        "THREAD_DELETE",
        json!({"id": "300000000000000021", "guild_id": G1.to_string(), "parent_id": GENERAL.to_string(), "type": 11}),
    );

    assert_eq!(created, ["ChannelAdded(300000000000000021)"]);
    assert_eq!(threads, [THREAD, 300_000_000_000_000_021]);
    assert_eq!(updated, ["ChannelUpdated(300000000000000021)"]);
    assert_eq!(is_archived, Some(true));
    assert_eq!(deleted, ["ChannelRemoved(300000000000000021)"]);
}

#[test]
fn dm_recipients_are_stored_and_updated() {
    let mut state = ready_state();
    let ready = ready_value();
    let mut group = ready["private_channels"][1].clone();
    let mut mira = ready["users"][0].clone();
    mira["global_name"] = "Mira Two".into();
    group["recipients"] = json!([mira, ready["users"][1]]);
    group.as_object_mut().unwrap().remove("recipient_ids");

    let events = apply(&mut state, "CHANNEL_CREATE", group);

    assert_eq!(events, [format!("UserUpdated({MIRA})")]);
    assert_eq!(
        state
            .user(Snowflake::new(MIRA))
            .and_then(|user| user.global_name.clone()),
        Some("Mira Two".into())
    );
}

#[test]
fn guild_member_update_for_the_current_user_updates_the_member() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "GUILD_MEMBER_UPDATE",
        fixture(include_str!(
            "../../../tests/fixtures/guild_member_update.json"
        )),
    );

    assert_eq!(events, [format!("CurrentMemberUpdated({G1})")]);
    let member = state.current_member(Snowflake::new(G1)).unwrap();
    assert!(member.communication_disabled_until.is_some());
    assert_eq!(member.nick.as_deref(), Some("Tester"));
}

#[test]
fn user_update_updates_the_current_user() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "USER_UPDATE",
        fixture(include_str!("../../../tests/fixtures/user_update.json")),
    );

    assert_eq!(events, [format!("CurrentUserUpdated({ME})")]);
    assert_eq!(
        state
            .current_user()
            .and_then(|user| user.user.global_name.clone()),
        Some("Akari".into())
    );
}

#[test]
fn ready_supplemental_adds_lazy_private_channels() {
    let mut state = ready_state();

    let events = apply(
        &mut state,
        "READY_SUPPLEMENTAL",
        fixture(include_str!(
            "../../../tests/fixtures/ready_supplemental.json"
        )),
    );

    assert_eq!(events, ["ChannelAdded(300000000000000012)"]);
    assert!(
        state
            .user(Snowflake::new(100_000_000_000_000_004))
            .is_some()
    );
}

#[test]
fn updates_for_unknown_entities_change_nothing() {
    let mut state = ready_state();
    let mut other_member = fixture(include_str!(
        "../../../tests/fixtures/guild_member_update.json"
    ));
    other_member["user"]["id"] = MIRA.to_string().into();
    let mut foreign_channel = fixture(include_str!("../../../tests/fixtures/channel_update.json"));
    foreign_channel["id"] = "300000000000000099".into();
    foreign_channel["guild_id"] = "200000000000000099".into();
    let unknown = [
        (
            "MESSAGE_UPDATE",
            json!({"id": "400000000000000001", "channel_id": GENERAL.to_string(), "content": "x"}),
        ),
        (
            "MESSAGE_DELETE",
            json!({"id": "400000000000000001", "channel_id": GENERAL.to_string()}),
        ),
        (
            "CHANNEL_UPDATE",
            json!({"id": "300000000000000099", "name": "x"}),
        ),
        (
            "THREAD_UPDATE",
            json!({"id": "300000000000000099", "name": "x"}),
        ),
        ("CHANNEL_DELETE", json!({"id": "300000000000000099"})),
        (
            "THREAD_DELETE",
            json!({"id": "300000000000000099", "guild_id": G1.to_string(), "parent_id": GENERAL.to_string(), "type": 11}),
        ),
        ("CHANNEL_CREATE", foreign_channel),
        ("GUILD_MEMBER_UPDATE", other_member),
        (
            "GUILD_ROLE_DELETE",
            json!({"guild_id": G1.to_string(), "role_id": "500000000000000099"}),
        ),
        (
            "GUILD_ROLE_CREATE",
            json!({"guild_id": "200000000000000099", "role": {"id": "1", "permissions": "0", "position": 0}}),
        ),
        (
            "GUILD_UPDATE",
            json!({"id": "200000000000000099", "name": "x"}),
        ),
        ("GUILD_DELETE", json!({"id": "200000000000000099"})),
        (
            "USER_UPDATE",
            json!({"id": MIRA.to_string(), "username": "x"}),
        ),
    ];
    let before = ready_state();

    for (name, data) in unknown {
        let events = apply(&mut state, name, data);
        assert!(events.is_empty(), "{name}: {events:?}");
        assert!(state == before, "{name} changed the state");
    }
}

#[test]
fn resumed_and_other_dispatches_change_nothing() {
    let mut state = ready_state();
    let before = ready_state();

    assert!(apply(&mut state, "RESUMED", json!({})).is_empty());
    assert!(
        apply(
            &mut state,
            "TYPING_START",
            json!({"channel_id": GENERAL.to_string()})
        )
        .is_empty()
    );
    assert!(state == before);
}

#[test]
fn permissions_follow_role_and_member_changes() {
    let mut ready = ready_value();
    ready["guilds"][0]["properties"]["owner_id"] = MIRA.to_string().into();
    let mut state = state_from(&ready);
    let now = 0;
    let general = Snowflake::new(GENERAL);

    let as_moderator = state.permissions(general, now);
    apply(
        &mut state,
        "GUILD_MEMBER_UPDATE",
        json!({"guild_id": G1.to_string(), "user": {"id": ME.to_string(), "username": "akari_tester"}, "roles": []}),
    );
    let as_everyone = state.permissions(general, now);
    apply(
        &mut state,
        "GUILD_ROLE_UPDATE",
        json!({"guild_id": G1.to_string(), "role": {"id": G1.to_string(), "name": "@everyone", "permissions": "0", "position": 0}}),
    );
    let as_nobody = state.permissions(general, now);

    assert_eq!(as_moderator, Some(Permissions::ALL));
    let everyone = as_everyone.unwrap();
    assert!(everyone.contains(Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES));
    assert!(!everyone.contains(Permissions::ADMINISTRATOR));
    assert_eq!(as_nobody, Some(Permissions::NONE));
    assert_eq!(
        state.permissions(Snowflake::new(THREAD), now),
        Some(Permissions::NONE)
    );
    assert_eq!(
        state.permissions(Snowflake::new(300_000_000_000_000_010), now),
        None
    );
    assert_eq!(
        state.permissions(Snowflake::new(300_000_000_000_000_099), now),
        None
    );
}

#[test]
fn a_later_ready_implies_the_channels_of_guilds_that_go_away() {
    let mut gone = ready_state();
    let mut down = ready_state();
    let mut without = ready_value();
    without["guilds"].as_array_mut().unwrap().remove(0);
    without["merged_members"].as_array_mut().unwrap().remove(0);
    let mut unavailable = ready_value();
    unavailable["guilds"][0] = json!({"id": G1.to_string(), "unavailable": true});

    let gone = apply(&mut gone, "READY", without);
    let down = apply(&mut down, "READY", unavailable);

    assert_eq!(gone, [format!("GuildRemoved({G1})"), "Ready".to_owned()]);
    assert_eq!(
        down,
        [format!("GuildUnavailable({G1})"), "Ready".to_owned()]
    );
}
