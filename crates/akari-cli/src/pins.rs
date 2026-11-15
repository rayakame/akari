use std::process::ExitCode;

use akari_core::model::{ChannelId, MessageId};
use akari_core::state::PinsState;
use akari_core::{Account, PinError, RequestError};

use crate::messages::{line, utc_minute};
use crate::reactions::hold;
use crate::report;

// 250 pins at 25 a page.
const MAX_PAGES: usize = 10;

fn pin_line(pinned_at_millis: i64, message: &str) -> String {
    format!("pinned {}  {message}", utc_minute(pinned_at_millis))
}

fn pin_error(err: &PinError, pin: bool) -> String {
    let what = if pin { "pin" } else { "unpin" };
    match err {
        PinError::Request(RequestError::InvalidRequest) => format!(
            "Couldn't {what}: Akari refused: the message isn't in this channel, {}, or its \
             actions don't allow it (Pin Messages and Read Message History; system messages, \
             voice and stage chats, media posts and locked threads can't be pinned).",
            if pin {
                "it is pinned already"
            } else {
                "it isn't pinned"
            }
        ),
        PinError::TooMany { limit } => {
            format!("Couldn't {what}: the channel has as many pins as Discord allows ({limit}).")
        }
        err => format!("Couldn't {what}: {}", report(err)),
    }
}

// Every page, the newest pin first.
async fn lines(account: &Account, channel: ChannelId) -> Result<Vec<String>, String> {
    let failed = |err: RequestError| format!("Couldn't load the pins: {}", report(&err));
    account.load_pins(channel).await.map_err(failed)?;
    let store = account.store();
    for _ in 1..MAX_PAGES {
        if !store.pins(channel).has_more {
            break;
        }
        account.load_more_pins(channel).await.map_err(failed)?;
    }
    let page = store.pins(channel);
    if page.state != PinsState::Loaded {
        return Err("Couldn't load the pins: the channel isn't known.".to_owned());
    }
    if page.items.is_empty() {
        return Ok(vec!["(no pins)".to_owned()]);
    }
    Ok(page
        .items
        .iter()
        .map(|item| pin_line(item.pinned_at.unix_millis(), &line(store, &item.message)))
        .collect())
}

pub async fn list(account: &Account, channel: ChannelId) -> ExitCode {
    match lines(account, channel).await {
        Ok(lines) => {
            for line in lines {
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

pub async fn change(
    account: &Account,
    channel: ChannelId,
    message: MessageId,
    pin: bool,
) -> ExitCode {
    let done = async {
        hold(account, channel, message).await?;
        let changed = if pin {
            account.pin(channel, message).await
        } else {
            account.unpin(channel, message).await
        };
        changed.map_err(|err| pin_error(&err, pin))
    }
    .await;
    match done {
        Ok(()) => {
            println!("{}", if pin { "Pinned." } else { "Unpinned." });
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

    #[test]
    fn a_pin_line_puts_the_pin_time_before_the_message() {
        assert_eq!(
            pin_line(1_700_000_000_000, "2023-11-01 10:00  Mira: hi"),
            "pinned 2023-11-14 22:13  2023-11-01 10:00  Mira: hi"
        );
    }

    #[test]
    fn a_refusal_before_asking_says_what_to_check() {
        let refused = PinError::Request(RequestError::InvalidRequest);

        assert!(pin_error(&refused, true).starts_with("Couldn't pin: Akari refused"));
        assert!(pin_error(&refused, true).contains("pinned already"));
        assert!(pin_error(&refused, false).contains("it isn't pinned"));
    }

    #[test]
    fn discords_refusals_say_why() {
        assert_eq!(
            pin_error(&PinError::TooMany { limit: 250 }, true),
            "Couldn't pin: the channel has as many pins as Discord allows (250)."
        );
        assert_eq!(
            pin_error(&PinError::Archived, false),
            "Couldn't unpin: the thread is archived"
        );
        assert_eq!(
            pin_error(
                &PinError::Request(RequestError::Discord {
                    status: 400,
                    code: 12345,
                    message: "Something\u{1b} new".to_owned(),
                }),
                true
            ),
            "Couldn't pin: Discord error 12345: Something\\u{1b} new"
        );
    }
}
