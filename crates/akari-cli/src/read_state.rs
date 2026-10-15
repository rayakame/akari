use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use akari_core::gateway::{ConnectionEvent, DispatchEvent};
use akari_core::model::{ChannelId, GuildId, Permissions, RoleId, Snowflake, UserId};
use akari_core::state::{ConnectionState, Message, Store, StoreEvent};
use akari_core::{DiscordClient, MessageLoad, Token};
use serde_json::{Map, Value};
use tokio::signal::unix::{Signal, SignalKind, signal};

use crate::keychain::Accounts;
use crate::messages::utc_minute;
use crate::report;
use crate::session::{CLOSE_WAIT, Session, closed_message, stored_token};

const DISCORD_EPOCH_MILLIS: u64 = 1_420_070_400_000;
const OPT_IN_CHANNELS_ON: u64 = 1 << 14;
const OPT_IN_ENABLED: u64 = 1 << 12;
const SUPPRESS_NOTIFICATIONS: u64 = 1 << 12;
const PAGE: u8 = 100;
// A large guild's GUILD_CREATE, or reading the disk cache, can take a while.
const WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(50);
const TEXT_KEYS: [&str; 6] = [
    "name",
    "topic",
    "content",
    "nick",
    "username",
    "global_name",
];

pub struct Options {
    pub channel_id: u64,
    pub cache: Option<PathBuf>,
    pub watch: bool,
}

pub async fn run<S: Accounts>(
    client: &DiscordClient,
    store: &Arc<S>,
    options: Options,
) -> ExitCode {
    let (user, token) = match stored_token(client, store).await {
        Ok(found) => found,
        Err(code) => return code,
    };
    let mut interrupts = match signal(SignalKind::interrupt()) {
        Ok(interrupts) => interrupts,
        Err(err) => {
            eprintln!("Couldn't watch for Ctrl+C: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Channel {} (IDs, flags and counts only: no names, no message content)",
        options.channel_id
    );
    let wire = match capture_ready(client, token.clone(), options.channel_id, &mut interrupts).await
    {
        Ok(wire) => wire,
        Err(code) => return code,
    };
    for line in wire_lines(&wire) {
        println!("{line}");
    }
    let copy = match options
        .cache
        .as_deref()
        .map(|file| copy_cache(file, user))
        .transpose()
    {
        Ok(copy) => copy,
        Err(err) => {
            eprintln!("Couldn't copy the cache: {err}");
            return ExitCode::FAILURE;
        }
    };
    let client = match &copy {
        Some(copy) => client.clone().with_cache(copy.0.clone()),
        None => client.clone(),
    };
    let account = match client.account(token, user) {
        Ok(account) => account,
        Err(err) => {
            eprintln!("{}", closed_message(Some(&err)));
            return ExitCode::FAILURE;
        }
    };
    let events = account.store().subscribe();
    let session = Session { account, events };
    let channel = Snowflake::new(options.channel_id);
    let code = inspect(
        &session,
        channel,
        &wire,
        copy.is_some(),
        options.watch,
        &mut interrupts,
    )
    .await;
    session.close(Some(&mut interrupts)).await;
    code
}

// READY as Discord sent it, on a connection of its own: Account doesn't hand out the raw READY.
async fn capture_ready(
    client: &DiscordClient,
    token: Token,
    channel: u64,
    interrupts: &mut Signal,
) -> Result<WireFacts, ExitCode> {
    let gateway = client
        .gateway(token)
        .map_err(|err| fail(&closed_message(Some(&err))))?;
    gateway.capture_next_ready();
    gateway
        .connect()
        .map_err(|err| fail(&closed_message(Some(&err))))?;
    let mut facts = None;
    let result = loop {
        let event = tokio::select! {
            _ = interrupts.recv() => break Err(ExitCode::from(130)),
            event = gateway.next() => event,
        };
        match event {
            Ok(ConnectionEvent::CapturedReady(raw)) => match serde_json::from_slice(&raw) {
                Ok(frame) => facts = Some(wire_facts(&frame, channel)),
                Err(_) => break Err(fail("READY wasn't valid JSON.")),
            },
            Ok(ConnectionEvent::Dispatch(DispatchEvent::Ready(_))) => {
                break facts.ok_or_else(|| fail("READY came without its raw copy."));
            }
            Ok(_) => {}
            Err(err) => break Err(fail(&closed_message(Some(&err)))),
        }
    };
    gateway.close();
    let _ = tokio::time::timeout(CLOSE_WAIT, async { while gateway.next().await.is_ok() {} }).await;
    result
}

async fn inspect(
    session: &Session,
    channel: ChannelId,
    wire: &WireFacts,
    cached: bool,
    watch: bool,
    interrupts: &mut Signal,
) -> ExitCode {
    let store = session.account.store();
    if cached {
        println!();
        println!("Akari from the disk cache (a copy; READY replaces all of it)");
        if wait_until(|| store.current_user().is_some()).await {
            for line in akari_lines(store, channel, None) {
                println!("{line}");
            }
        } else {
            println!("  not restored: the file is another account's, empty or unreadable");
        }
    }
    if let Err(err) = session.account.connect() {
        return fail(&closed_message(Some(&err)));
    }
    if let Err(code) = wait_online(session, interrupts).await {
        return code;
    }
    // A guild READY sends unavailable arrives with its GUILD_CREATE.
    wait_until(|| store.channel(channel).is_some()).await;
    println!();
    println!("Akari after READY");
    for line in akari_lines(store, channel, Some(wire)) {
        println!("{line}");
    }
    println!();
    for line in history_lines(session, channel).await {
        println!("{line}");
    }
    if watch {
        return watch_channel(session, channel, interrupts).await;
    }
    ExitCode::SUCCESS
}

async fn wait_online(session: &Session, interrupts: &mut Signal) -> Result<(), ExitCode> {
    loop {
        let event = tokio::select! {
            _ = interrupts.recv() => return Err(ExitCode::from(130)),
            event = session.events.next() => event,
        };
        match event {
            Some(StoreEvent::Connection(ConnectionState::Online)) => return Ok(()),
            Some(StoreEvent::Connection(ConnectionState::Closed { error })) => {
                return Err(fail(&closed_message(error.as_deref())));
            }
            None => return Err(fail("The connection closed.")),
            Some(_) => {}
        }
    }
}

async fn wait_until(done: impl Fn() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !done() {
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
    true
}

fn akari_lines(store: &Store, channel: ChannelId, wire: Option<&WireFacts>) -> Vec<String> {
    let Some(known) = store.channel(channel) else {
        return vec!["  Akari doesn't have this channel".to_owned()];
    };
    let mut lines = vec![format!("  channel type {}", u16::from(known.kind))];
    let state = store.read_state(channel);
    let last = known.last_message_id.map(Snowflake::get);
    let joined = known
        .guild_id
        .and_then(|guild| store.current_member(guild))
        .and_then(|member| member.joined_at)
        .map(|joined| millis_snowflake(joined.unix_millis()));
    match state {
        Some(state) => {
            let acked = state.acked.map(Snowflake::get);
            let source = wire.map_or_else(String::new, |wire| {
                state_source(wire, acked, state.mentions, state.estimated)
            });
            lines.push(format!(
                "  read state: acked {}, mentions {}, estimated {}{source}",
                id_and_time(acked),
                state.mentions,
                state.estimated
            ));
        }
        None => lines.push(format!(
            "  read state: none, so the join decides (joined {})",
            id_and_time(joined)
        )),
    }
    let last_source = wire.map_or_else(String::new, |wire| last_source(wire, last));
    lines.push(format!(
        "  last_message_id: {}{last_source}",
        id_and_time(last)
    ));
    let position = state.map_or(joined, |state| state.acked.map(Snowflake::get));
    lines.push(format!(
        "  read position vs last_message_id: {}",
        position_vs_last(position, last)
    ));
    let badge = store.channel_badge(channel);
    lines.push(format!(
        "  channel badge: unread {}, mentions {}",
        badge.unread, badge.mentions
    ));
    let permissions = store.permissions(channel);
    let allowed =
        |permission| permissions.map_or("unknown", |p: Permissions| yes(p.contains(permission)));
    lines.push(format!(
        "  permissions: view channel {}, read message history {}",
        allowed(Permissions::VIEW_CHANNEL),
        allowed(Permissions::READ_MESSAGE_HISTORY)
    ));
    let Some(guild) = known.guild_id else {
        return lines;
    };
    let guild_badge = store.guild_badge(guild);
    lines.push(format!(
        "  server badge: unread {}, mentions {}",
        guild_badge.unread, guild_badge.mentions
    ));
    let settings = store.guild_settings(Some(guild));
    let picked = |id: Option<ChannelId>| {
        id.and_then(|id| settings.channels.get(&id))
            .is_some_and(|channel| channel.opted_in)
    };
    lines.push(format!(
        "  server: community {}, opt-in {}, channel picked {}, category picked {}",
        store
            .guild(guild)
            .map_or("unknown", |guild| yes(guild.community)),
        yes(settings.opt_in),
        yes(picked(Some(channel))),
        yes(picked(known.parent_id))
    ));
    lines.push(format!(
        "  muted: channel {}, category {}, server {}; suppress @everyone {}, roles {}; level {:?}",
        yes(store.channel_muted(channel)),
        yes(known
            .parent_id
            .is_some_and(|parent| store.channel_muted(parent))),
        yes(store.guild_muted(Some(guild))),
        yes(settings.suppress_everyone),
        yes(settings.suppress_roles),
        store.notification_level(channel)
    ));
    lines
}

fn state_source(wire: &WireFacts, acked: Option<u64>, mentions: u32, estimated: bool) -> String {
    match (&wire.entry, wire.mention_count) {
        (Some(_), Some(sent)) if wire.acked == acked && sent == u64::from(mentions) => {
            " (as READY sent it)".to_owned()
        }
        (Some(_), sent) => format!(
            " (READY sent acked {}, mentions {}; changed since)",
            id_and_time(wire.acked),
            sent.unwrap_or(0)
        ),
        (None, _) if estimated => " (READY sent none; Akari estimated it)".to_owned(),
        (None, _) => " (READY sent none)".to_owned(),
    }
}

fn last_source(wire: &WireFacts, last: Option<u64>) -> String {
    match &wire.channel {
        None => " (not in READY, so from GUILD_CREATE)".to_owned(),
        Some(_) if wire.last_message_id == last => " (as READY sent it)".to_owned(),
        Some(_) => format!(
            " (READY sent {}; raised since)",
            id_and_time(wire.last_message_id)
        ),
    }
}

async fn history_lines(session: &Session, channel: ChannelId) -> Vec<String> {
    let mut lines = vec![format!("REST, the newest {PAGE} messages")];
    let load = MessageLoad::Latest { limit: PAGE };
    if let Err(err) = session.account.load_messages(channel, load).await {
        lines.push(format!("  couldn't load them: {}", report(&err)));
        return lines;
    }
    let store = session.account.store();
    let Some(window) = store.messages(channel) else {
        lines.push("  no messages".to_owned());
        return lines;
    };
    let known = store.channel(channel);
    let last = known
        .as_ref()
        .and_then(|channel| channel.last_message_id)
        .map(Snowflake::get);
    let newest = window.messages.iter().map(|message| message.id.get()).max();
    lines.push(format!(
        "  newest: {}: {}",
        id_and_time(newest),
        newest_vs_last(newest, last)
    ));
    let guild = known.as_ref().and_then(|channel| channel.guild_id);
    let joined = guild
        .and_then(|guild| store.current_member(guild))
        .and_then(|member| member.joined_at)
        .map(|joined| millis_snowflake(joined.unix_millis()));
    let position = store
        .read_state(channel)
        .map_or(joined, |state| state.acked.map(Snowflake::get));
    let after: Vec<_> = window
        .messages
        .iter()
        .filter(|message| position.is_none_or(|position| message.id.get() > position))
        .collect();
    let reached = window.oldest
        || position.is_some_and(|position| {
            window
                .messages
                .iter()
                .any(|message| message.id.get() <= position)
        });
    lines.push(format!(
        "  after the read position: {}{}",
        if reached { "" } else { "at least " },
        after.len()
    ));
    let (me, roles) = me_and_roles(store, guild);
    let mut plain = 0;
    for message in after {
        let facts = facts(message, me, &roles);
        if facts.by_me || facts.direct || facts.everyone || facts.my_roles > 0 {
            lines.push(format!("    {}", message_line(&facts)));
        } else {
            plain += 1;
        }
    }
    if plain > 0 {
        lines.push(format!("    and {plain} by others without a mention"));
    }
    lines.push(
        "  Akari doesn't count history: its count starts at READY's mention_count and adds \
         messages that arrive while connected (--watch shows them)."
            .to_owned(),
    );
    lines
}

async fn watch_channel(session: &Session, channel: ChannelId, interrupts: &mut Signal) -> ExitCode {
    println!();
    println!("Watching this channel, Ctrl+C to stop");
    let store = session.account.store();
    let guild = store.channel(channel).and_then(|channel| channel.guild_id);
    loop {
        let event = tokio::select! {
            _ = interrupts.recv() => return ExitCode::SUCCESS,
            event = session.events.next() => event,
        };
        let Some(event) = event else {
            return ExitCode::SUCCESS;
        };
        match event {
            StoreEvent::MessageInserted(message) if message.channel_id == channel => {
                let (me, roles) = me_and_roles(store, guild);
                println!("+ {}", message_line(&facts(&message, me, &roles)));
            }
            StoreEvent::MessageDeleted {
                channel_id,
                message_id,
            } if channel_id == channel => println!("- {} deleted", message_id.get()),
            StoreEvent::BadgeChanged { channel_id, .. } if channel_id == channel => {
                let badge = store.channel_badge(channel);
                let state = store.read_state(channel);
                println!(
                    "  badge: unread {}, mentions {}; read state: acked {}, mentions {}, estimated {}",
                    badge.unread,
                    badge.mentions,
                    id_and_time(state.and_then(|state| state.acked).map(Snowflake::get)),
                    state.map_or(0, |state| state.mentions),
                    state.is_some_and(|state| state.estimated)
                );
            }
            StoreEvent::Connection(ConnectionState::Connecting) => println!("(reconnecting)"),
            StoreEvent::Connection(ConnectionState::Online) => println!("(connected)"),
            StoreEvent::Connection(ConnectionState::Closed { error }) => {
                return fail(&closed_message(error.as_deref()));
            }
            _ => {}
        }
    }
}

fn me_and_roles(store: &Store, guild: Option<GuildId>) -> (Option<UserId>, Vec<RoleId>) {
    let me = store.current_user().map(|current| current.user.id);
    let roles = guild
        .and_then(|guild| store.current_member(guild))
        .map(|member| member.roles.to_vec())
        .unwrap_or_default();
    (me, roles)
}

fn facts(message: &Message, me: Option<UserId>, roles: &[RoleId]) -> MessageFacts {
    MessageFacts {
        id: message.id.get(),
        kind: u16::from(message.kind),
        by_me: Some(message.author.id) == me,
        webhook: message.webhook_id.is_some(),
        direct: message.mentions.iter().any(|user| Some(user.id) == me),
        everyone: message.mention_everyone,
        my_roles: message
            .mention_roles
            .iter()
            .filter(|role| roles.contains(role))
            .count(),
        silent: message.flags & SUPPRESS_NOTIFICATIONS != 0,
    }
}

struct MessageFacts {
    id: u64,
    kind: u16,
    by_me: bool,
    webhook: bool,
    direct: bool,
    everyone: bool,
    my_roles: usize,
    silent: bool,
}

fn message_line(facts: &MessageFacts) -> String {
    let author = if facts.by_me {
        "by me"
    } else if facts.webhook {
        "by a webhook"
    } else {
        "by someone else"
    };
    let mut mentions = Vec::new();
    if facts.direct {
        mentions.push("mentions me".to_owned());
    }
    if facts.everyone {
        mentions.push("@everyone/@here".to_owned());
    }
    match facts.my_roles {
        0 => {}
        1 => mentions.push("1 role of mine".to_owned()),
        count => mentions.push(format!("{count} roles of mine")),
    }
    let mentions = if mentions.is_empty() {
        "no mention".to_owned()
    } else {
        mentions.join(", ")
    };
    let mut line = format!(
        "{}  {}  type {}  {author}  {mentions}",
        facts.id,
        snowflake_time(facts.id),
        facts.kind
    );
    if facts.silent {
        line.push_str("  silent");
    }
    line
}

#[derive(Debug, Default)]
struct WireFacts {
    read_states: String,
    entry: Option<Value>,
    acked: Option<u64>,
    mention_count: Option<u64>,
    channel: Option<Value>,
    last_message_id: Option<u64>,
    parent_id: Option<u64>,
    community: Option<bool>,
    guild_settings: Option<Value>,
    guild_flags: Option<u64>,
    channel_override: Option<Value>,
    parent_override: Option<Value>,
}

fn wire_facts(frame: &Value, channel: u64) -> WireFacts {
    let ready = frame.get("d").unwrap_or(frame);
    let id = channel.to_string();
    let read_state = &ready["read_state"];
    let read_states = entries(read_state);
    let mut facts = WireFacts {
        read_states: format!(
            "partial {}, version {}, {} entries",
            read_state["partial"],
            read_state["version"],
            read_states.len()
        ),
        ..WireFacts::default()
    };
    if let Some(entry) = read_states
        .iter()
        .find(|entry| entry["id"].as_str() == Some(&id))
    {
        facts.acked = snowflake(&entry["last_message_id"]);
        facts.mention_count = entry["mention_count"].as_u64();
        facts.entry = Some(sanitized(entry));
    }
    for guild in list(&ready["guilds"]) {
        let found = ["channels", "threads"]
            .iter()
            .flat_map(|key| list(&guild[*key]))
            .find(|candidate| candidate["id"].as_str() == Some(&id));
        let Some(found) = found else {
            continue;
        };
        facts.last_message_id = snowflake(&found["last_message_id"]);
        facts.parent_id = snowflake(&found["parent_id"]);
        facts.channel = Some(picked(
            found,
            &[
                "type",
                "parent_id",
                "last_message_id",
                "last_pin_timestamp",
                "flags",
            ],
        ));
        let features = guild
            .get("features")
            .or_else(|| guild["properties"].get("features"));
        facts.community = features.and_then(Value::as_array).map(|features| {
            features
                .iter()
                .any(|feature| feature.as_str() == Some("COMMUNITY"))
        });
        let settings = entries(&ready["user_guild_settings"])
            .iter()
            .find(|settings| settings["guild_id"] == guild["id"]);
        if let Some(settings) = settings {
            facts.guild_flags = settings["flags"].as_u64();
            let overrides = list(&settings["channel_overrides"]);
            let find = |target: Option<u64>| {
                let target = target?.to_string();
                overrides
                    .iter()
                    .find(|candidate| candidate["channel_id"].as_str() == Some(&target))
                    .map(sanitized)
            };
            facts.channel_override = find(Some(channel));
            facts.parent_override = find(facts.parent_id);
            let mut shown = sanitized(settings);
            if let Some(fields) = shown.as_object_mut() {
                fields.insert("channel_overrides".to_owned(), Value::from(overrides.len()));
            }
            facts.guild_settings = Some(shown);
        }
        break;
    }
    facts
}

fn wire_lines(facts: &WireFacts) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "READY as Discord sent it".to_owned(),
        format!("  read_state: {}", facts.read_states),
    ];
    lines.push(match &facts.entry {
        Some(entry) => format!("  entry: {entry}"),
        None => "  entry: no entry for this channel".to_owned(),
    });
    let Some(channel) = &facts.channel else {
        lines.push(
            "  channel: not in READY's guilds (its guild came unavailable, or this isn't a guild channel)"
                .to_owned(),
        );
        return lines;
    };
    lines.push(format!("  channel: {channel}"));
    lines.push(format!(
        "  server: COMMUNITY feature: {}; user guild settings flags {} (OPT_IN_CHANNELS_ON: {})",
        facts.community.map_or("unknown", yes),
        facts
            .guild_flags
            .map_or_else(|| "none".to_owned(), |flags| flags.to_string()),
        facts
            .guild_flags
            .map_or("unknown", |flags| yes(flags & OPT_IN_CHANNELS_ON != 0))
    ));
    if let Some(settings) = &facts.guild_settings {
        lines.push(format!("  server settings: {settings}"));
    }
    for (label, found) in [
        ("this channel", &facts.channel_override),
        ("its category", &facts.parent_override),
    ] {
        lines.push(match found {
            Some(found) => format!(
                "  override for {label}: {found} (OPT_IN_ENABLED: {})",
                yes(found["flags"]
                    .as_u64()
                    .is_some_and(|flags| flags & OPT_IN_ENABLED != 0))
            ),
            None => format!("  override for {label}: none"),
        });
    }
    lines
}

fn list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

// READY's versioned lists, or a plain list.
fn entries(value: &Value) -> &[Value] {
    if value.is_array() {
        list(value)
    } else {
        list(&value["entries"])
    }
}

fn snowflake(value: &Value) -> Option<u64> {
    value.as_str()?.parse().ok()
}

fn picked(value: &Value, keys: &[&str]) -> Value {
    let fields: Map<String, Value> = keys
        .iter()
        .filter_map(|key| Some(((*key).to_owned(), sanitized(value.get(*key)?))))
        .collect();
    Value::Object(fields)
}

// Keeps IDs, numbers, flags and timestamps; every other string could be a name or content.
fn sanitized(value: &Value) -> Value {
    match value {
        Value::String(text) if is_id(text) || is_timestamp(text) => value.clone(),
        Value::String(_) => Value::from("<text>"),
        Value::Array(items) => items.iter().map(sanitized).collect(),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, field)| {
                let shown = if TEXT_KEYS.contains(&key.as_str()) && field.is_string() {
                    Value::from("<text>")
                } else {
                    sanitized(field)
                };
                (key.clone(), shown)
            })
            .collect(),
        _ => value.clone(),
    }
}

fn is_id(text: &str) -> bool {
    !text.is_empty() && text.len() <= 20 && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 19
        && bytes.get(4) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || b"-:T.+Z".contains(byte))
}

fn snowflake_time(id: u64) -> String {
    let millis = (id >> 22) + DISCORD_EPOCH_MILLIS;
    utc_minute(i64::try_from(millis).unwrap_or(i64::MAX))
}

fn millis_snowflake(millis: i64) -> u64 {
    u64::try_from(millis)
        .unwrap_or(0)
        .saturating_sub(DISCORD_EPOCH_MILLIS)
        << 22
}

fn id_and_time(id: Option<u64>) -> String {
    id.map_or_else(
        || "none".to_owned(),
        |id| format!("{id} ({} UTC)", snowflake_time(id)),
    )
}

fn newest_vs_last(newest: Option<u64>, last: Option<u64>) -> &'static str {
    match (newest, last) {
        (_, None) => "Akari has no last_message_id",
        (None, Some(_)) => "no readable messages",
        (Some(newest), Some(last)) if newest == last => {
            "the newest readable message is last_message_id"
        }
        (Some(newest), Some(last)) if newest < last => {
            "older than last_message_id: no readable message has that ID (deleted or hidden)"
        }
        _ => "newer than last_message_id: Akari's last_message_id is behind",
    }
}

fn position_vs_last(position: Option<u64>, last: Option<u64>) -> &'static str {
    match (position, last) {
        (_, None) => "no last_message_id: read",
        (None, Some(_)) => "no read position: unread",
        (Some(position), Some(last)) if position < last => "before it: unread",
        (Some(position), Some(last)) if position == last => "at it: read",
        _ => "past it: read",
    }
}

fn yes(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn copy_cache(file: &Path, user: UserId) -> io::Result<CacheCopy> {
    let directory =
        std::env::temp_dir().join(format!("akari-cli-read-state-{}", std::process::id()));
    DirBuilder::new().mode(0o700).create(&directory)?;
    let copy = CacheCopy(directory);
    let target = copy.0.join(format!("{}.sqlite", user.get()));
    fs::copy(file, &target)?;
    let wal = suffixed(file, "-wal");
    if wal.exists() {
        fs::copy(&wal, suffixed(&target, "-wal"))?;
    }
    Ok(copy)
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

// A copy, so the app's own file is never opened, migrated or flushed to.
struct CacheCopy(PathBuf);

impl Drop for CacheCopy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const CHANNEL: u64 = 200_000_000_000_000_002;
    const CATEGORY: &str = "200000000000000009";
    const GUILD: &str = "100000000000000001";

    fn ready() -> Value {
        json!({
            "op": 0,
            "t": "READY",
            "d": {
                "read_state": {
                    "version": 7,
                    "partial": false,
                    "entries": [
                        {"id": "200000000000000001", "last_message_id": "1", "mention_count": 0},
                        {
                            "id": CHANNEL.to_string(),
                            "last_message_id": "300000000000000000",
                            "mention_count": 2,
                            "flags": 1,
                            "last_pin_timestamp": "2024-05-01T10:00:00.000000+00:00",
                            "last_viewed": 3400
                        }
                    ]
                },
                "guilds": [{
                    "id": GUILD,
                    "properties": {"name": "Secret Server", "features": ["COMMUNITY", "NEWS"]},
                    "channels": [
                        {"id": CATEGORY, "type": 4, "name": "Secret Category"},
                        {
                            "id": CHANNEL.to_string(),
                            "type": 0,
                            "name": "secret-channel",
                            "topic": "private words",
                            "parent_id": CATEGORY,
                            "last_message_id": "300000000000000005",
                            "flags": 0
                        }
                    ],
                    "threads": []
                }],
                "user_guild_settings": {
                    "version": 3,
                    "partial": false,
                    "entries": [{
                        "guild_id": GUILD,
                        "flags": 16384,
                        "muted": false,
                        "message_notifications": 1,
                        "channel_overrides": [
                            {"channel_id": CHANNEL.to_string(), "flags": 4096, "muted": false},
                            {"channel_id": CATEGORY, "flags": 0, "muted": true, "collapsed": true}
                        ]
                    }]
                }
            }
        })
    }

    #[test]
    fn the_wire_facts_find_the_channel_in_ready() {
        let facts = wire_facts(&ready(), CHANNEL);

        assert_eq!(facts.read_states, "partial false, version 7, 2 entries");
        assert_eq!(
            facts
                .entry
                .as_ref()
                .map(|entry| entry["mention_count"].clone()),
            Some(json!(2))
        );
        assert_eq!(facts.acked, Some(300_000_000_000_000_000));
        assert_eq!(facts.mention_count, Some(2));
        assert_eq!(facts.last_message_id, Some(300_000_000_000_000_005));
        assert_eq!(facts.parent_id, Some(200_000_000_000_000_009));
        assert_eq!(facts.community, Some(true));
        assert_eq!(facts.guild_flags, Some(16_384));
        assert_eq!(
            facts.channel_override.as_ref().map(|o| o["flags"].clone()),
            Some(json!(4096))
        );
        assert_eq!(
            facts.parent_override.as_ref().map(|o| o["muted"].clone()),
            Some(json!(true))
        );
    }

    #[test]
    fn the_wire_lines_carry_no_names_or_text() {
        let text = wire_lines(&wire_facts(&ready(), CHANNEL)).join("\n");

        for secret in ["Secret", "secret", "private words"] {
            assert!(!text.contains(secret), "{text}");
        }
        assert!(text.contains("300000000000000005"), "{text}");
        assert!(text.contains("\"mention_count\":2"), "{text}");
        assert!(text.contains("COMMUNITY feature: yes"), "{text}");
        assert!(text.contains("OPT_IN_CHANNELS_ON: yes"), "{text}");
    }

    #[test]
    fn features_can_sit_at_the_top_of_the_guild() {
        let mut ready = ready();
        let guild = &mut ready["d"]["guilds"][0];
        guild["features"] = json!([]);
        guild["properties"] = json!({"name": "Secret Server"});

        assert_eq!(wire_facts(&ready, CHANNEL).community, Some(false));
    }

    #[test]
    fn a_channel_ready_leaves_out_says_so() {
        let facts = wire_facts(&ready(), 999);
        let text = wire_lines(&facts).join("\n");

        assert_eq!(facts.entry, None);
        assert_eq!(facts.last_message_id, None);
        assert!(text.contains("no entry for this channel"), "{text}");
        assert!(text.contains("not in READY's guilds"), "{text}");
    }

    #[test]
    fn sanitizing_keeps_ids_numbers_and_timestamps_only() {
        let raw = json!({
            "id": "123",
            "name": "Mira",
            "label": "Mira's notes",
            "count": 3,
            "at": "2024-05-01T10:00:00.000000+00:00",
            "on": true,
            "none": null,
            "nested": [{"topic": "words"}]
        });

        assert_eq!(
            sanitized(&raw),
            json!({
                "id": "123",
                "name": "<text>",
                "label": "<text>",
                "count": 3,
                "at": "2024-05-01T10:00:00.000000+00:00",
                "on": true,
                "none": null,
                "nested": [{"topic": "<text>"}]
            })
        );
    }

    #[test]
    fn snowflake_times_are_utc() {
        // The example snowflake of Discord's API reference.
        assert_eq!(snowflake_time(175_928_847_299_117_063), "2016-04-30 11:18");
    }

    #[test]
    fn the_newest_message_is_compared_with_last_message_id() {
        assert_eq!(
            newest_vs_last(Some(5), Some(5)),
            "the newest readable message is last_message_id"
        );
        assert_eq!(
            newest_vs_last(Some(4), Some(5)),
            "older than last_message_id: no readable message has that ID (deleted or hidden)"
        );
        assert_eq!(
            newest_vs_last(Some(6), Some(5)),
            "newer than last_message_id: Akari's last_message_id is behind"
        );
        assert_eq!(newest_vs_last(None, Some(5)), "no readable messages");
        assert_eq!(
            newest_vs_last(Some(5), None),
            "Akari has no last_message_id"
        );
    }

    #[test]
    fn a_message_line_names_what_it_carries_without_content() {
        let mut facts = MessageFacts {
            id: 175_928_847_299_117_063,
            kind: 19,
            by_me: false,
            webhook: false,
            direct: true,
            everyone: true,
            my_roles: 2,
            silent: true,
        };

        assert_eq!(
            message_line(&facts),
            "175928847299117063  2016-04-30 11:18  type 19  by someone else  \
             mentions me, @everyone/@here, 2 roles of mine  silent"
        );
        facts.direct = false;
        facts.everyone = false;
        facts.my_roles = 0;
        facts.silent = false;
        facts.by_me = true;
        assert_eq!(
            message_line(&facts),
            "175928847299117063  2016-04-30 11:18  type 19  by me  no mention"
        );
        facts.by_me = false;
        facts.webhook = true;
        facts.my_roles = 1;
        assert_eq!(
            message_line(&facts),
            "175928847299117063  2016-04-30 11:18  type 19  by a webhook  1 role of mine"
        );
    }
}
