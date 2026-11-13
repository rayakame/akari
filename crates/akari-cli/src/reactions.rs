use std::process::ExitCode;

use akari_core::model::{ChannelId, MessageId, Snowflake, UserId};
use akari_core::state::{ReactionEmoji, User};
use akari_core::{
    Account, MessageLoad, ReactionError, ReactionKind, ReactionLocation, ReactorsPage, RequestError,
};

use crate::{printable, report};

/// A reaction's emoji as typed: the Unicode emoji itself, or a custom one as `name:id`,
/// `a:name:id` or the way a message writes it (`<:name:id>`, `<a:name:id>`).
pub fn parse_emoji(text: &str) -> Result<ReactionEmoji, String> {
    let text = text.trim();
    let inner = text
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(text);
    if let Some((name, id)) = inner.rsplit_once(':')
        && let Ok(id) = id.parse::<u64>()
        && id != 0
    {
        let (animated, name) = match name.strip_prefix("a:") {
            Some(name) => (true, name),
            None => (false, name.strip_prefix(':').unwrap_or(name)),
        };
        let name = (!name.is_empty()).then_some(name);
        return Ok(ReactionEmoji::custom(Snowflake::new(id), name, animated));
    }
    if text.is_empty() || text.is_ascii() {
        return Err(format!(
            "`{}` is no emoji: give the Unicode emoji itself or a custom one as name:id.",
            printable(text)
        ));
    }
    Ok(ReactionEmoji::unicode(text))
}

fn reaction_error(err: &ReactionError, add: bool) -> String {
    let what = if add { "react" } else { "remove the reaction" };
    match err {
        ReactionError::Request(RequestError::InvalidRequest) => format!(
            "Couldn't {what}: Akari refused: the message isn't in this channel, or its \
             actions don't allow it (Add Reactions for a new emoji, Read Message History, a \
             timeout, a locked thread)."
        ),
        err => format!("Couldn't {what}: {}", report(err)),
    }
}

// The core acts only on a message it holds.
async fn hold(account: &Account, channel: ChannelId, message: MessageId) -> Result<(), String> {
    let around = MessageLoad::Around {
        id: message,
        limit: 1,
    };
    account
        .load_messages(channel, around)
        .await
        .map(|_| ())
        .map_err(|err| format!("Couldn't load the message: {}", report(&err)))
}

pub async fn toggle(
    account: &Account,
    channel: ChannelId,
    message: MessageId,
    emoji: &str,
    add: bool,
) -> ExitCode {
    let done = async {
        let emoji = parse_emoji(emoji)?;
        hold(account, channel, message).await?;
        let location = ReactionLocation::Message;
        let outcome = if add {
            account.react(channel, message, &emoji, location).await
        } else {
            account.unreact(channel, message, &emoji, location).await
        };
        outcome.map_err(|err| reaction_error(&err, add))
    }
    .await;
    match done {
        Ok(()) => {
            println!("{}", if add { "Reacted." } else { "Removed." });
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn reactor_line(user: &User) -> String {
    format!(
        "{} ({})  {}",
        printable(user.display_name()),
        printable(&user.username),
        user.id.get()
    )
}

pub(crate) fn reactor_lines(page: &ReactorsPage) -> Vec<String> {
    let mut lines: Vec<String> = page.users.iter().map(|user| reactor_line(user)).collect();
    if lines.is_empty() {
        lines.push("(nobody)".to_owned());
    }
    if let Some(last) = page.users.last().filter(|_| page.has_more) {
        lines.push(format!("(more: --after {})", last.id.get()));
    }
    lines
}

pub struct ReactorsArgs {
    pub emoji: String,
    pub burst: bool,
    pub after: Option<u64>,
    pub limit: u8,
}

pub async fn reactors(
    account: &Account,
    channel: ChannelId,
    message: MessageId,
    args: ReactorsArgs,
) -> ExitCode {
    let page = async {
        let emoji = parse_emoji(&args.emoji)?;
        let kind = if args.burst {
            ReactionKind::Burst
        } else {
            ReactionKind::Normal
        };
        let after = args.after.map(UserId::new);
        account
            .reactors(channel, message, &emoji, kind, after, args.limit)
            .await
            .map_err(|err| format!("Couldn't load the reactions' users: {}", report(&err)))
    }
    .await;
    match page {
        Ok(page) => {
            for line in reactor_lines(&page) {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(text: &str) -> (Option<u64>, Option<String>, bool) {
        let emoji = parse_emoji(text).unwrap();
        (
            emoji.id.map(Snowflake::get),
            emoji.name.map(String::from),
            emoji.animated,
        )
    }

    #[test]
    fn a_unicode_emoji_is_taken_as_typed() {
        for emoji in ["👍", "❤\u{fe0f}", "❤", "👍🏽", "🇩🇪", "#\u{fe0f}\u{20e3}"] {
            assert_eq!(parts(emoji), (None, Some(emoji.to_owned()), false));
        }
        assert_eq!(parts(" 👍 "), (None, Some("👍".to_owned()), false));
    }

    #[test]
    fn a_custom_emoji_is_its_name_and_id() {
        let wave = (Some(7), Some("wave".to_owned()), false);
        let party = (Some(8), Some("party".to_owned()), true);

        assert_eq!(parts("wave:7"), wave);
        assert_eq!(parts(":wave:7"), wave);
        assert_eq!(parts("<:wave:7>"), wave);
        assert_eq!(parts("a:party:8"), party);
        assert_eq!(parts("<a:party:8>"), party);
        assert_eq!(parts(":9"), (Some(9), None, false), "a deleted one");
        assert_eq!(parts("a:9"), (Some(9), Some("a".to_owned()), false));
    }

    #[test]
    fn plain_text_is_no_emoji() {
        for text in ["", "  ", "wave", "wave:", "wave:x", "wave:0", ":thumbsup:"] {
            let err = parse_emoji(text).unwrap_err();
            assert!(err.contains("name:id"), "{text}: {err}");
        }
        assert!(!parse_emoji("a\u{1b}b").unwrap_err().contains('\u{1b}'));
    }

    #[test]
    fn a_refusal_before_asking_says_what_to_check() {
        let refused = ReactionError::Request(RequestError::InvalidRequest);

        assert!(reaction_error(&refused, true).starts_with("Couldn't react: Akari refused"));
        assert!(
            reaction_error(&refused, false)
                .starts_with("Couldn't remove the reaction: Akari refused")
        );
        assert_eq!(
            reaction_error(&ReactionError::TooMany, true),
            "Couldn't react: the message has too many reactions"
        );
    }
}
