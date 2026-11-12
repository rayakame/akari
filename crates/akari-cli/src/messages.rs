use std::collections::HashSet;
use std::fmt;
use std::process::ExitCode;
use std::sync::Arc;

use akari_core::model::{ChannelId, ChannelType, GuildId, MessageId, Permissions, Snowflake};
use akari_core::state::{
    Channel, ConnectionState, Delivery, Message, Reaction, ReplyTarget, Store, StoreEvent,
    display_order,
};
use akari_core::{DiscordClient, MessageLoad, RequestError};
use clap::Subcommand;
use tokio::signal::unix::{Signal, SignalKind, signal};

use crate::keychain::Accounts;
use crate::session::{self, Session, closed_message};
use crate::{printable, report};

const TAIL_BACKLOG: u8 = 20;
const TARGET_START: usize = 60;

/// Commands that need a session.
#[derive(Subcommand)]
pub enum SessionCommand {
    /// List the guilds you're in.
    Guilds,
    /// List a guild's channels you can see, in Discord's order.
    Channels { guild_id: u64 },
    /// Print a channel's latest messages, oldest first, each with what it replies to and its
    /// reactions (`*` marks yours).
    Read {
        channel_id: u64,
        /// How many messages, 1 to 100.
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u8).range(1..=100))]
        limit: u8,
    },
    /// Send a message.
    Send {
        // Checks that Discord ignores a repeated nonce, which retrying after a 502 relies on.
        #[arg(long, hide = true)]
        repeat_nonce: bool,
        channel_id: u64,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        text: Vec<String>,
    },
    /// Print a channel's latest messages, then new, edited and deleted ones, reaction changes
    /// and changes to messages that replies answer, until Ctrl+C.
    Tail {
        // Sends op 37 without member lists, to test whether very large guilds need them.
        #[arg(long, hide = true)]
        flags_only: bool,
        channel_id: u64,
    },
}

pub async fn run<S: Accounts>(
    client: &DiscordClient,
    store: &Arc<S>,
    command: SessionCommand,
) -> ExitCode {
    let flags_only = matches!(
        command,
        SessionCommand::Tail {
            flags_only: true,
            ..
        }
    );
    let session = match session::open(client, store, flags_only).await {
        Ok(session) => session,
        Err(code) => return code,
    };
    let mut interrupts = None;
    let code = match command {
        SessionCommand::Guilds => guilds(&session),
        SessionCommand::Channels { guild_id } => channels(&session, Snowflake::new(guild_id)),
        SessionCommand::Read { channel_id, limit } => {
            read(&session, Snowflake::new(channel_id), limit).await
        }
        SessionCommand::Send {
            repeat_nonce,
            channel_id,
            text,
        } => {
            let channel = Snowflake::new(channel_id);
            if repeat_nonce {
                send_twice(&session, channel, text.join(" ")).await
            } else {
                send(&session, channel, text.join(" ")).await
            }
        }
        SessionCommand::Tail {
            flags_only,
            channel_id,
        } => match signal(SignalKind::interrupt()) {
            Ok(stream) => {
                let interrupts = interrupts.insert(stream);
                tail(&session, Snowflake::new(channel_id), flags_only, interrupts).await
            }
            Err(err) => {
                eprintln!("Couldn't watch for Ctrl+C: {err}");
                ExitCode::FAILURE
            }
        },
    };
    session.close(interrupts.as_mut()).await;
    code
}

fn guilds(session: &Session) -> ExitCode {
    let store = session.account.store();
    let mut guilds = store.guilds();
    guilds.sort_by_cached_key(|guild| guild.name.to_lowercase());
    for guild in guilds {
        println!("{}  {}", printable(&guild.name), guild.id.get());
    }
    for id in store.unavailable_guilds() {
        println!("(unavailable)  {}", id.get());
    }
    ExitCode::SUCCESS
}

fn channels(session: &Session, guild: GuildId) -> ExitCode {
    let store = session.account.store();
    if store.guild(guild).is_none() {
        eprintln!("You're in no available guild with that ID; `akari-cli guilds` lists them.");
        return ExitCode::FAILURE;
    }
    let visible = |channel: &Channel| {
        store
            .permissions(channel.id)
            .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
    };
    let ordered = display_order(&store.guild_channels(guild));
    let mut category = None;
    for (index, channel) in ordered.iter().enumerate() {
        let kind = kind(channel);
        if kind == ChannelKind::Category {
            category = Some(channel.id);
            let has_visible = ordered[index + 1..]
                .iter()
                .take_while(|next| next.kind != ChannelType::GuildCategory)
                .any(|next| visible(next));
            if !has_visible {
                continue;
            }
        } else if !visible(channel) {
            continue;
        }
        let nested = kind != ChannelKind::Category && channel.parent_id == category;
        let name = channel.name.as_deref().unwrap_or("");
        println!("{}", channel_line(kind, name, channel.id.get(), nested));
    }
    ExitCode::SUCCESS
}

async fn read(session: &Session, channel: ChannelId, limit: u8) -> ExitCode {
    let account = &session.account;
    if let Err(err) = account
        .load_messages(channel, MessageLoad::Latest { limit })
        .await
    {
        eprintln!("Couldn't load the messages: {}", report(&err));
        return ExitCode::FAILURE;
    }
    let store = account.store();
    let messages = store
        .messages(channel)
        .map(|window| window.messages)
        .unwrap_or_default();
    if messages.is_empty() {
        println!("(no messages)");
    }
    for message in messages {
        println!("{}", line(store, &message));
    }
    ExitCode::SUCCESS
}

async fn send(session: &Session, channel: ChannelId, text: String) -> ExitCode {
    println!("Sending…");
    match session.account.send_message(channel, text, None).await {
        Ok(id) => {
            println!("Sent: {}", id.get());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Couldn't send: {}", report(&err));
            ExitCode::FAILURE
        }
    }
}

async fn send_twice(session: &Session, channel: ChannelId, text: String) -> ExitCode {
    println!("Sending, then sending again with the same nonce…");
    match session.account.send_message_twice(channel, text).await {
        Ok((first, repeat)) => {
            println!("Sent: {}", first.get());
            println!("{}", repeat_line(first, &repeat));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Couldn't send: {}", report(&err));
            ExitCode::FAILURE
        }
    }
}

fn repeat_line(first: MessageId, repeat: &Result<MessageId, RequestError>) -> String {
    match repeat {
        Ok(id) if *id == first => format!(
            "The repeat got {} back, the same message: Discord ignored the repeated nonce.",
            id.get()
        ),
        Ok(id) => format!(
            "The repeat got {} back, a second message: Discord did NOT ignore the repeated nonce.",
            id.get()
        ),
        Err(err) => format!("The repeat failed: {}", report(err)),
    }
}

async fn tail(
    session: &Session,
    channel: ChannelId,
    flags_only: bool,
    interrupts: &mut Signal,
) -> ExitCode {
    let account = &session.account;
    let store = account.store();
    let known = store.channel(channel);
    let guild = known
        .as_ref()
        .and_then(|channel| channel.guild_id)
        .and_then(|guild| store.guild(guild));
    let note = guild_note(
        known.is_some(),
        guild.as_ref().is_some_and(|guild| guild.large),
        guild.as_ref().and_then(|guild| guild.member_count),
        flags_only,
    );
    if let Some(note) = note {
        eprintln!("{note}");
    }
    let mut shown = HashSet::new();
    if let Err(code) = jump_to_present(session, channel, &mut shown, Change::Loaded).await {
        return code;
    }
    println!("-- live, Ctrl+C to stop --");
    loop {
        let event = tokio::select! {
            _ = interrupts.recv() => return ExitCode::SUCCESS,
            event = session.events.next() => event,
        };
        let Some(event) = event else {
            return ExitCode::SUCCESS;
        };
        match event {
            StoreEvent::MessageInserted(message) | StoreEvent::MessageReplaced { message, .. }
                if message.channel_id == channel =>
            {
                if shown.insert(message.id) {
                    println!("{}", marked(Change::New, &line(store, &message)));
                }
            }
            StoreEvent::MessageUpdated(message) if message.channel_id == channel => {
                println!("{}", marked(Change::Edited, &line(store, &message)));
            }
            StoreEvent::MessageReactionsChanged {
                channel_id,
                message_id,
            } if channel_id == channel => {
                let reactions = store.reactions(channel, message_id);
                println!(
                    "{}",
                    reactions_changed_line(message_id.get(), &marks(&reactions))
                );
            }
            StoreEvent::ReplyTargetChanged {
                channel_id,
                target_id,
            } if channel_id == channel => {
                println!("{}", target_change(store, channel, target_id));
            }
            StoreEvent::MessageDeleted {
                channel_id,
                message_id,
            } if channel_id == channel => {
                println!("{}", marked(Change::Deleted, &message_id.get().to_string()));
            }
            StoreEvent::MessagesLoaded {
                channel_id,
                first,
                last,
            } if channel_id == channel => {
                let messages = store
                    .messages(channel)
                    .map(|window| window.messages)
                    .unwrap_or_default();
                for message in messages
                    .iter()
                    .filter(|message| (first..=last).contains(&message.id))
                {
                    if shown.insert(message.id) {
                        println!("{}", marked(Change::New, &line(store, message)));
                    }
                }
            }
            StoreEvent::MessagesStale { channel_id } if channel_id == channel => {
                let detached = store
                    .messages(channel)
                    .is_some_and(|window| window.stale && !window.latest);
                if detached {
                    println!("(missed too much while away, showing the latest messages)");
                    if let Err(code) =
                        jump_to_present(session, channel, &mut shown, Change::New).await
                    {
                        return code;
                    }
                } else {
                    println!("(new session, catching up)");
                }
            }
            StoreEvent::MessagesRefreshed { channel_id } if channel_id == channel => {
                println!("(caught up)");
            }
            StoreEvent::Connection(ConnectionState::Connecting) => println!("(reconnecting)"),
            StoreEvent::Connection(ConnectionState::Online) => println!("(connected)"),
            StoreEvent::Connection(ConnectionState::Closed { error }) => {
                eprintln!("{}", closed_message(error.as_deref()));
                return ExitCode::FAILURE;
            }
            _ => {}
        }
    }
}

async fn jump_to_present(
    session: &Session,
    channel: ChannelId,
    shown: &mut HashSet<MessageId>,
    change: Change,
) -> Result<(), ExitCode> {
    let load = MessageLoad::Latest {
        limit: TAIL_BACKLOG,
    };
    if let Err(err) = session.account.load_messages(channel, load).await {
        eprintln!("Couldn't load the messages: {}", report(&err));
        return Err(ExitCode::FAILURE);
    }
    let store = session.account.store();
    let messages = store
        .messages(channel)
        .map(|window| window.messages)
        .unwrap_or_default();
    for message in messages {
        if shown.insert(message.id) {
            println!("{}", marked(change, &line(store, &message)));
        }
    }
    Ok(())
}

fn guild_note(
    known: bool,
    large: bool,
    member_count: Option<u32>,
    flags_only: bool,
) -> Option<String> {
    if !known {
        return Some(
            "(this channel isn't in Akari's state, so its guild can't be subscribed: \
             live messages may not arrive)"
                .to_owned(),
        );
    }
    if !large {
        return None;
    }
    let size = member_count.map_or_else(String::new, |count| format!(", {count} members"));
    let subscription = if flags_only {
        "op 37 sends the guild flags only, no member list"
    } else {
        "Akari subscribes this channel's member list"
    };
    Some(format!("(a large server{size}: {subscription})"))
}

fn line(store: &Store, message: &Message) -> String {
    let text = MessageLine {
        time: utc_minute(message.timestamp.unix_millis()),
        author: message.author.display_name(),
        content: &message.content,
        edited: message.edited_timestamp.is_some(),
        attachments: message.attachments.len(),
        embeds: message.embeds.len(),
    };
    let line = match message.delivery {
        Delivery::Pending => format!("{text} (sending)"),
        Delivery::Failed => format!("{text} (failed)"),
        _ => text.to_string(),
    };
    let target = message
        .replied_to()
        .map(|(channel, target)| store.reply_target(channel, target));
    let target = target
        .as_ref()
        .map(|target| shown_target(target, Some(message)));
    with_extras(line, target.as_ref(), &marks(message.reactions()))
}

fn with_extras(
    mut line: String,
    target: Option<&Target<'_>>,
    reactions: &[ReactionMark],
) -> String {
    if let Some(target) = target {
        line.push('\n');
        line.push_str(&target_line(target));
    }
    if !reactions.is_empty() {
        line.push_str("\n    reactions: ");
        line.push_str(&reactions_text(reactions));
    }
    line
}

struct ReactionMark {
    emoji: String,
    count: u32,
    burst: u32,
    mine: bool,
}

fn marks(reactions: &[Reaction]) -> Vec<ReactionMark> {
    reactions
        .iter()
        .map(|reaction| ReactionMark {
            emoji: emoji_text(
                reaction.emoji.id.map(Snowflake::get),
                reaction.emoji.name.as_deref(),
            ),
            count: reaction.normal.saturating_add(reaction.burst),
            burst: reaction.burst,
            mine: reaction.me || reaction.me_burst,
        })
        .collect()
}

fn emoji_text(id: Option<u64>, name: Option<&str>) -> String {
    match (id, name) {
        (Some(_), Some(name)) => format!(":{}:", printable(name)),
        (Some(id), None) => format!(":{id}:"),
        (None, name) => printable(name.unwrap_or("?")).into_owned(),
    }
}

fn reactions_text(reactions: &[ReactionMark]) -> String {
    if reactions.is_empty() {
        return "none".to_owned();
    }
    let shown: Vec<String> = reactions
        .iter()
        .map(|reaction| {
            let mine = if reaction.mine { "*" } else { "" };
            let burst = match reaction.burst {
                0 => String::new(),
                burst => format!(" ({burst} super)"),
            };
            format!("{} {}{mine}{burst}", reaction.emoji, reaction.count)
        })
        .collect();
    shown.join("  ")
}

fn reactions_changed_line(message: u64, reactions: &[ReactionMark]) -> String {
    format!("* {message} reactions: {}", reactions_text(reactions))
}

enum Target<'a> {
    Loaded {
        author: &'a str,
        content: &'a str,
        pinged: bool,
        attachments: bool,
        sticker: bool,
        embeds: bool,
    },
    Deleted,
    NotLoaded,
}

fn shown_target<'a>(target: &'a ReplyTarget, reply: Option<&Message>) -> Target<'a> {
    match target {
        ReplyTarget::Loaded(preview) => Target::Loaded {
            author: preview.author.display_name(),
            content: &preview.content,
            pinged: reply.is_some_and(|reply| reply.mentions_user(preview.author.id)),
            attachments: preview.has_attachments,
            sticker: preview.has_sticker,
            embeds: preview.has_embeds,
        },
        ReplyTarget::Deleted => Target::Deleted,
        ReplyTarget::NotLoaded => Target::NotLoaded,
    }
}

fn target_text(target: &Target<'_>) -> String {
    let Target::Loaded {
        author,
        content,
        attachments,
        sticker,
        embeds,
        ..
    } = target
    else {
        return String::new();
    };
    let start: String = content
        .chars()
        .take(TARGET_START)
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect();
    let more = if content.chars().nth(TARGET_START).is_some() {
        "…"
    } else {
        ""
    };
    let start = match (start.is_empty(), attachments, sticker, embeds) {
        (false, ..) => printable(&start).into_owned(),
        (true, true, ..) => "[attachment]".to_owned(),
        (true, false, true, _) => "[sticker]".to_owned(),
        (true, false, false, true) => "[embed]".to_owned(),
        (true, false, false, false) => "[no text]".to_owned(),
    };
    format!("{}: {start}{more}", printable(author))
}

fn target_line(target: &Target<'_>) -> String {
    match target {
        Target::Loaded { pinged, .. } => {
            let at = if *pinged { "@" } else { "" };
            format!("    ↳ replying to {at}{}", target_text(target))
        }
        Target::Deleted => "    ↳ replying to a deleted message".to_owned(),
        Target::NotLoaded => "    ↳ replying to a message that is not loaded".to_owned(),
    }
}

fn target_change(store: &Store, channel: ChannelId, target: MessageId) -> String {
    let now = store.reply_target(channel, target);
    target_changed_line(target.get(), &shown_target(&now, None))
}

fn target_changed_line(id: u64, target: &Target<'_>) -> String {
    let now = match target {
        Target::Loaded { .. } => target_text(target),
        Target::Deleted => "deleted".to_owned(),
        Target::NotLoaded => "not loaded".to_owned(),
    };
    format!("* {id} answered by a reply here: {now}")
}

struct MessageLine<'a> {
    time: String,
    author: &'a str,
    content: &'a str,
    edited: bool,
    attachments: usize,
    embeds: usize,
}

impl fmt::Display for MessageLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}  {}: {}",
            self.time,
            printable(self.author),
            printable(self.content)
        )?;
        if self.edited {
            f.write_str(" (edited)")?;
        }
        for (count, noun) in [(self.attachments, "attachment"), (self.embeds, "embed")] {
            match count {
                0 => {}
                1 => write!(f, " [1 {noun}]")?,
                count => write!(f, " [{count} {noun}s]")?,
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Loaded,
    New,
    Edited,
    Deleted,
}

fn marked(change: Change, line: &str) -> String {
    let mark = match change {
        Change::Loaded => return line.to_owned(),
        Change::New => '+',
        Change::Edited => '~',
        Change::Deleted => '-',
    };
    format!("{mark} {line}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelKind {
    Category,
    Text,
    Voice,
}

fn kind(channel: &Channel) -> ChannelKind {
    match channel.kind {
        ChannelType::GuildCategory => ChannelKind::Category,
        ChannelType::GuildVoice | ChannelType::GuildStageVoice => ChannelKind::Voice,
        _ => ChannelKind::Text,
    }
}

fn channel_line(kind: ChannelKind, name: &str, id: u64, nested: bool) -> String {
    let indent = if nested { "  " } else { "" };
    let name = printable(name);
    match kind {
        ChannelKind::Category => format!("{}  {id}", name.to_uppercase()),
        ChannelKind::Text => format!("{indent}#{name}  {id}"),
        ChannelKind::Voice => format!("{indent}(voice) {name}  {id}"),
    }
}

// Howard Hinnant's days-to-civil-date algorithm, so the CLI needs no date crate.
pub(crate) fn utc_minute(unix_millis: i64) -> String {
    let seconds = unix_millis.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let minute = seconds.rem_euclid(86_400) / 60;
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        minute / 60,
        minute % 60
    )
}

#[cfg(test)]
mod tests {
    use akari_core::model::UserId;
    use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
    use akari_core::{Account, Endpoints, Token, TokenStore, TokenStoreError};
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const GENERAL: u64 = 300_000_000_000_000_002;
    // message_reply.json: the user's reply and the message of Mira's it answers.
    const REPLY: u64 = 400_000_000_000_000_002;
    const TARGET: u64 = 400_000_000_000_000_001;

    struct NoTokens;

    impl TokenStore for NoTokens {
        fn load(&self, _: UserId) -> Result<Option<Token>, TokenStoreError> {
            Ok(None)
        }

        fn save(&self, _: UserId, _: &Token) -> Result<(), TokenStoreError> {
            Ok(())
        }

        fn delete(&self, _: UserId) -> Result<(), TokenStoreError> {
            Ok(())
        }
    }

    fn reply() -> Value {
        serde_json::from_str(include_str!(
            "../../akari-core/tests/fixtures/message_reply.json"
        ))
        .unwrap()
    }

    // A reply like the fixture's, to another message.
    fn reply_to(id: u64, target: u64, sent_along: Option<Value>) -> Value {
        let mut message = reply();
        message["id"] = id.to_string().into();
        message["message_reference"]["message_id"] = target.to_string().into();
        match sent_along {
            Some(target) => message["referenced_message"] = target,
            None => {
                message
                    .as_object_mut()
                    .unwrap()
                    .remove("referenced_message");
            }
        }
        message
    }

    // An account that never connects, with `page` loaded into #general from a local server, as
    // `read` loads it.
    async fn loaded(page: Vec<Value>) -> (MockServer, Account) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v9/channels/{GENERAL}/messages")))
            .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(page)))
            .mount(&server)
            .await;
        let host = HostInfo {
            os: DesktopOs::MacOs,
            os_version: "25.0.0".to_owned(),
            arch: Arch::Arm64,
            system_locale: "en-US".to_owned(),
        };
        let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs));
        let endpoints = Endpoints {
            api: format!("{}/api/v9/", server.uri()),
            allow_plaintext: true,
            ..Endpoints::default()
        };
        let client =
            DiscordClient::with_endpoints(properties, Arc::new(NoTokens), endpoints).unwrap();
        let account = client
            .account(Token::new("t".to_owned()), UserId::new(1))
            .unwrap();
        account
            .load_messages(Snowflake::new(GENERAL), MessageLoad::Latest { limit: 50 })
            .await
            .unwrap();
        (server, account)
    }

    fn row(account: &Account, id: u64) -> Arc<Message> {
        let window = account.store().messages(Snowflake::new(GENERAL)).unwrap();
        window
            .messages
            .iter()
            .find(|message| message.id.get() == id)
            .cloned()
            .unwrap()
    }

    #[tokio::test]
    async fn marks_add_bursts_and_mark_a_super_reaction_as_mine() {
        let mut message = reply();
        message["reactions"] = json!([
            {"emoji": {"id": null, "name": "👍"}, "count": 3,
             "count_details": {"normal": 2, "burst": 1}, "me": false, "me_burst": true},
            {"emoji": {"id": "800000000000000001", "name": "party"}, "count": 1,
             "count_details": {"normal": 1, "burst": 0}, "me": false, "me_burst": false},
            {"emoji": {"id": null, "name": "🔥"}, "count": 2,
             "count_details": {"normal": 2, "burst": 0}, "me": true, "me_burst": false},
        ]);
        let (_server, account) = loaded(vec![message]).await;

        let marks = marks(row(&account, REPLY).reactions());

        assert_eq!(reactions_text(&marks), "👍 3* (1 super)  :party: 1  🔥 2*");
        account.close();
    }

    #[tokio::test]
    async fn shown_target_maps_each_state_and_the_ping() {
        let mut sticker = reply()["referenced_message"].clone();
        sticker["id"] = "400000000000000011".into();
        sticker["content"] = "".into();
        sticker["sticker_items"] =
            json!([{"id": "700000000000000001", "name": "wave", "format_type": 1}]);
        let mut file = sticker.clone();
        file["id"] = "400000000000000012".into();
        file["sticker_items"] = json!([]);
        file["attachments"] = json!([
            {"id": "600000000000000001", "filename": "notes.txt", "size": 12,
             "url": "https://cdn.discordapp.com/attachments/1/2/notes.txt",
             "proxy_url": "https://media.discordapp.net/attachments/1/2/notes.txt"},
        ]);
        let mut silent = reply_to(400_000_000_000_000_025, TARGET, None);
        silent["mentions"] = json!([]);
        let (_server, account) = loaded(vec![
            silent,
            reply_to(400_000_000_000_000_024, 400_000_000_000_000_012, Some(file)),
            reply_to(
                400_000_000_000_000_023,
                400_000_000_000_000_011,
                Some(sticker),
            ),
            reply_to(400_000_000_000_000_022, 400_000_000_000_000_009, None),
            reply_to(
                400_000_000_000_000_021,
                400_000_000_000_000_008,
                Some(Value::Null),
            ),
            reply(),
        ])
        .await;
        let store = account.store();
        let shown = |id: u64| {
            let reply = row(&account, id);
            let (channel, target) = reply.replied_to().unwrap();
            target_line(&shown_target(
                &store.reply_target(channel, target),
                Some(&reply),
            ))
        };

        assert_eq!(
            shown(REPLY),
            "    ↳ replying to @Mira: Release notes are up"
        );
        assert_eq!(
            shown(400_000_000_000_000_025),
            "    ↳ replying to Mira: Release notes are up",
            "the reply didn't ping"
        );
        assert_eq!(
            shown(400_000_000_000_000_021),
            "    ↳ replying to a deleted message"
        );
        assert_eq!(
            shown(400_000_000_000_000_022),
            "    ↳ replying to a message that is not loaded"
        );
        assert_eq!(
            shown(400_000_000_000_000_023),
            "    ↳ replying to @Mira: [sticker]"
        );
        assert_eq!(
            shown(400_000_000_000_000_024),
            "    ↳ replying to @Mira: [attachment]"
        );
        account.close();
    }

    #[tokio::test]
    async fn a_reply_with_reactions_prints_all_three_lines() {
        let mut message = reply();
        message["reactions"] = json!([
            {"emoji": {"id": null, "name": "👍"}, "count": 1,
             "count_details": {"normal": 1, "burst": 0}, "me": false, "me_burst": false},
        ]);
        let (_server, account) = loaded(vec![message]).await;

        let printed = line(account.store(), &row(&account, REPLY));

        assert_eq!(
            printed,
            "2024-03-01 08:31  Akari Tester: Looks good! (edited)\n    \
             ↳ replying to @Mira: Release notes are up\n    \
             reactions: 👍 1"
        );
        account.close();
    }

    #[tokio::test]
    async fn tail_names_a_changed_target_by_its_own_id() {
        let (_server, account) = loaded(vec![reply()]).await;

        let printed = target_change(
            account.store(),
            Snowflake::new(GENERAL),
            Snowflake::new(TARGET),
        );

        assert_eq!(
            printed,
            format!("* {TARGET} answered by a reply here: Mira: Release notes are up")
        );
        account.close();
    }

    #[test]
    fn times_are_shown_in_utc_to_the_minute() {
        assert_eq!(utc_minute(0), "1970-01-01 00:00");
        assert_eq!(utc_minute(1_700_000_000_000), "2023-11-14 22:13");
        assert_eq!(utc_minute(951_782_400_000), "2000-02-29 00:00");
    }

    #[test]
    fn a_message_line_shows_time_author_and_content() {
        let line = MessageLine {
            time: "2026-10-08 12:00".to_owned(),
            author: "Mira",
            content: "hello",
            edited: false,
            attachments: 0,
            embeds: 0,
        };

        assert_eq!(line.to_string(), "2026-10-08 12:00  Mira: hello");
    }

    #[test]
    fn edits_attachments_and_embeds_are_marked() {
        let line = MessageLine {
            time: "2026-10-08 12:00".to_owned(),
            author: "Mira",
            content: "look",
            edited: true,
            attachments: 2,
            embeds: 1,
        };

        assert_eq!(
            line.to_string(),
            "2026-10-08 12:00  Mira: look (edited) [2 attachments] [1 embed]"
        );
    }

    #[test]
    fn message_lines_escape_control_characters() {
        let line = MessageLine {
            time: "2026-10-08 12:00".to_owned(),
            author: "Mi\u{1b}[31mra",
            content: "first\rsecond\u{1b}[2K\nthird\tfourth",
            edited: false,
            attachments: 0,
            embeds: 0,
        };

        assert_eq!(
            line.to_string(),
            "2026-10-08 12:00  Mi\\u{1b}[31mra: first\\rsecond\\u{1b}[2K\nthird\tfourth"
        );
        assert_eq!(
            channel_line(ChannelKind::Text, "gen\u{7}eral", 2, false),
            "#gen\\u{7}eral  2"
        );
    }

    fn mark(emoji: &str, count: u32, burst: u32, mine: bool) -> ReactionMark {
        ReactionMark {
            emoji: emoji.to_owned(),
            count,
            burst,
            mine,
        }
    }

    #[test]
    fn reactions_show_emoji_count_and_a_mark_for_mine() {
        let marks = [
            mark("👍", 3, 0, true),
            mark(":wave:", 1, 0, false),
            mark("🔥", 4, 2, true),
        ];

        assert_eq!(reactions_text(&marks), "👍 3*  :wave: 1  🔥 4* (2 super)");
        assert_eq!(reactions_text(&[]), "none");
    }

    #[test]
    fn a_custom_emoji_shows_by_name_and_a_deleted_one_by_id() {
        assert_eq!(emoji_text(None, Some("👍")), "👍");
        assert_eq!(emoji_text(Some(7), Some("wave")), ":wave:");
        assert_eq!(emoji_text(Some(7), None), ":7:");
        assert_eq!(emoji_text(Some(7), Some("wa\u{1b}ve")), ":wa\\u{1b}ve:");
    }

    #[test]
    fn a_replys_target_line_shows_the_author_and_the_start_of_the_content() {
        let loaded = |content: &'static str, pinged| Target::Loaded {
            author: "Mira",
            content,
            pinged,
            attachments: false,
            sticker: false,
            embeds: false,
        };

        assert_eq!(
            target_line(&loaded("Release notes are up", false)),
            "    ↳ replying to Mira: Release notes are up"
        );
        assert_eq!(
            target_line(&loaded("first\nsecond\u{1b}[2K", true)),
            "    ↳ replying to @Mira: first second\\u{1b}[2K"
        );
        let long = "a".repeat(80);
        assert_eq!(
            target_text(&Target::Loaded {
                author: "Mira",
                content: &long,
                pinged: false,
                attachments: false,
                sticker: false,
                embeds: false,
            }),
            format!("Mira: {}…", "a".repeat(60))
        );
        assert_eq!(
            target_line(&Target::Deleted),
            "    ↳ replying to a deleted message"
        );
        assert_eq!(
            target_line(&Target::NotLoaded),
            "    ↳ replying to a message that is not loaded"
        );
    }

    #[test]
    fn a_target_without_text_says_what_it_carries() {
        let carrying = |attachments, sticker, embeds| Target::Loaded {
            author: "Mira",
            content: "",
            pinged: false,
            attachments,
            sticker,
            embeds,
        };

        assert_eq!(
            target_text(&carrying(true, true, true)),
            "Mira: [attachment]"
        );
        assert_eq!(target_text(&carrying(false, true, true)), "Mira: [sticker]");
        assert_eq!(target_text(&carrying(false, false, true)), "Mira: [embed]");
        assert_eq!(
            target_text(&carrying(false, false, false)),
            "Mira: [no text]"
        );
    }

    #[test]
    fn tail_says_which_messages_reactions_or_target_changed() {
        assert_eq!(
            reactions_changed_line(42, &[mark("👍", 1, 0, true)]),
            "* 42 reactions: 👍 1*"
        );
        assert_eq!(reactions_changed_line(42, &[]), "* 42 reactions: none");
        assert_eq!(
            target_changed_line(41, &Target::Deleted),
            "* 41 answered by a reply here: deleted"
        );
        assert_eq!(
            target_changed_line(41, &Target::NotLoaded),
            "* 41 answered by a reply here: not loaded"
        );
        assert_eq!(
            target_changed_line(
                41,
                &Target::Loaded {
                    author: "Mira",
                    content: "edited text",
                    pinged: true,
                    attachments: false,
                    sticker: false,
                    embeds: false,
                }
            ),
            "* 41 answered by a reply here: Mira: edited text"
        );
    }

    #[test]
    fn a_messages_extra_lines_follow_it() {
        assert_eq!(with_extras("a line".to_owned(), None, &[]), "a line");
        assert_eq!(
            with_extras(
                "a line".to_owned(),
                Some(&Target::Deleted),
                &[mark("👍", 2, 0, false)]
            ),
            "a line\n    ↳ replying to a deleted message\n    reactions: 👍 2"
        );
    }

    #[test]
    fn tail_lines_mark_insert_edit_and_delete() {
        assert_eq!(marked(Change::Loaded, "a line"), "a line");
        assert_eq!(marked(Change::New, "a line"), "+ a line");
        assert_eq!(marked(Change::Edited, "a line"), "~ a line");
        assert_eq!(
            marked(Change::Deleted, "400000000000000001"),
            "- 400000000000000001"
        );
    }

    #[test]
    fn the_repeat_line_says_whether_discord_ignored_the_nonce() {
        let first = Snowflake::new(42);

        assert_eq!(
            repeat_line(first, &Ok(Snowflake::new(42))),
            "The repeat got 42 back, the same message: Discord ignored the repeated nonce."
        );
        assert_eq!(
            repeat_line(first, &Ok(Snowflake::new(43))),
            "The repeat got 43 back, a second message: Discord did NOT ignore the repeated nonce."
        );
        assert_eq!(
            repeat_line(first, &Err(RequestError::UnexpectedResponse)),
            "The repeat failed: unexpected response from Discord"
        );
    }

    #[test]
    fn tail_says_how_live_messages_will_arrive() {
        assert_eq!(
            guild_note(false, false, None, false).as_deref(),
            Some(
                "(this channel isn't in Akari's state, so its guild can't be subscribed: \
                 live messages may not arrive)"
            )
        );
        assert_eq!(
            guild_note(true, true, Some(3_000_000), false).as_deref(),
            Some("(a large server, 3000000 members: Akari subscribes this channel's member list)")
        );
        assert_eq!(
            guild_note(true, true, None, false).as_deref(),
            Some("(a large server: Akari subscribes this channel's member list)")
        );
        assert_eq!(
            guild_note(true, true, Some(3_000_000), true).as_deref(),
            Some(
                "(a large server, 3000000 members: op 37 sends the guild flags only, no member list)"
            )
        );
        assert_eq!(guild_note(true, false, Some(12), false), None);
    }

    #[test]
    fn channel_lines_show_kind_and_nesting() {
        assert_eq!(
            channel_line(ChannelKind::Category, "Text Channels", 1, false),
            "TEXT CHANNELS  1"
        );
        assert_eq!(
            channel_line(ChannelKind::Text, "general", 2, true),
            "  #general  2"
        );
        assert_eq!(
            channel_line(ChannelKind::Voice, "Lounge", 3, false),
            "(voice) Lounge  3"
        );
    }
}
