use std::collections::HashSet;
use std::fmt;
use std::process::ExitCode;
use std::sync::Arc;

use akari_core::model::{ChannelId, ChannelType, GuildId, MessageId, Permissions, Snowflake};
use akari_core::state::{Channel, ConnectionState, Delivery, Message, StoreEvent, display_order};
use akari_core::{DiscordClient, MessageLoad, RequestError};
use clap::Subcommand;
use tokio::signal::unix::{Signal, SignalKind, signal};

use crate::keychain::Accounts;
use crate::report;
use crate::session::{self, Session, closed_message};

const TAIL_BACKLOG: u8 = 20;

/// Commands that need a session.
#[derive(Subcommand)]
pub enum SessionCommand {
    /// List the guilds you're in.
    Guilds,
    /// List a guild's channels you can see, in Discord's order.
    Channels { guild_id: u64 },
    /// Print a channel's latest messages, oldest first.
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
    /// Print a channel's latest messages, then new, edited and deleted ones until Ctrl+C.
    Tail { channel_id: u64 },
}

pub async fn run<S: Accounts>(
    client: &DiscordClient,
    store: &Arc<S>,
    command: SessionCommand,
) -> ExitCode {
    let session = match session::open(client, store).await {
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
        SessionCommand::Tail { channel_id } => match signal(SignalKind::interrupt()) {
            Ok(stream) => {
                let interrupts = interrupts.insert(stream);
                tail(&session, Snowflake::new(channel_id), interrupts).await
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
        println!("{}  {}", guild.name, guild.id.get());
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
    let messages = account
        .store()
        .messages(channel)
        .map(|window| window.messages)
        .unwrap_or_default();
    if messages.is_empty() {
        println!("(no messages)");
    }
    for message in messages {
        println!("{}", line(&message));
    }
    ExitCode::SUCCESS
}

async fn send(session: &Session, channel: ChannelId, text: String) -> ExitCode {
    println!("Sending…");
    match session.account.send_message(channel, text).await {
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

async fn tail(session: &Session, channel: ChannelId, interrupts: &mut Signal) -> ExitCode {
    let account = &session.account;
    let store = account.store();
    let large = store
        .channel(channel)
        .and_then(|channel| channel.guild_id)
        .and_then(|guild| store.guild(guild))
        .is_some_and(|guild| guild.large);
    if large {
        eprintln!("(a large server: live messages arrive through Akari's guild subscription)");
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
                    println!("{}", marked(Change::New, &line(&message)));
                }
            }
            StoreEvent::MessageUpdated(message) if message.channel_id == channel => {
                println!("{}", marked(Change::Edited, &line(&message)));
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
                        println!("{}", marked(Change::New, &line(message)));
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
    let messages = session
        .account
        .store()
        .messages(channel)
        .map(|window| window.messages)
        .unwrap_or_default();
    for message in messages {
        if shown.insert(message.id) {
            println!("{}", marked(change, &line(&message)));
        }
    }
    Ok(())
}

fn line(message: &Message) -> String {
    let text = MessageLine {
        time: utc_minute(message.timestamp.unix_millis()),
        author: message.author.display_name(),
        content: &message.content,
        edited: message.edited_timestamp.is_some(),
        attachments: message.attachments.len(),
        embeds: message.embeds.len(),
    };
    match message.delivery {
        Delivery::Pending => format!("{text} (sending)"),
        Delivery::Failed => format!("{text} (failed)"),
        _ => text.to_string(),
    }
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
        write!(f, "{}  {}: {}", self.time, self.author, self.content)?;
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
    match kind {
        ChannelKind::Category => format!("{}  {id}", name.to_uppercase()),
        ChannelKind::Text => format!("{indent}#{name}  {id}"),
        ChannelKind::Voice => format!("{indent}(voice) {name}  {id}"),
    }
}

// Howard Hinnant's days-to-civil-date algorithm, so the CLI needs no date crate.
fn utc_minute(unix_millis: i64) -> String {
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
    use super::*;

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
