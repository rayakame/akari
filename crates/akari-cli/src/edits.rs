use std::process::ExitCode;

use akari_core::model::{ChannelId, MessageId};
use akari_core::{Account, EditError, RequestError};

use crate::reactions::hold;
use crate::report;

fn edit_error(err: &EditError) -> String {
    match err {
        EditError::Request(RequestError::InvalidRequest) => "Couldn't edit: Akari refused: the \
             message isn't yours or isn't in this channel, blank text needs an attachment or a \
             sticker to stay (delete the message instead), or its actions don't allow it (Send \
             Messages, a timeout, a locked thread)."
            .to_owned(),
        EditError::MessageBlocked { reason } => {
            format!(
                "Couldn't edit: AutoMod refused it: {}",
                crate::printable(reason)
            )
        }
        EditError::TooManyEdits {
            retry_after: Some(wait),
        } => format!(
            "Couldn't edit: a published message may be edited 3 times an hour; try again in {} \
             minute(s).",
            wait.as_secs().div_ceil(60)
        ),
        err => format!("Couldn't edit: {}", report(err)),
    }
}

fn delete_error(err: &RequestError) -> String {
    match err {
        RequestError::InvalidRequest => "Couldn't delete: Akari refused: the message isn't in \
             this channel, or its actions don't allow it (your own, or Manage Messages; some \
             system messages can't be deleted)."
            .to_owned(),
        err => format!("Couldn't delete: {}", report(err)),
    }
}

pub async fn edit(
    account: &Account,
    channel: ChannelId,
    message: MessageId,
    text: &str,
) -> ExitCode {
    let done = async {
        hold(account, channel, message).await?;
        account
            .edit_message(channel, message, text)
            .await
            .map_err(|err| edit_error(&err))
    }
    .await;
    finish(done, "Edited.")
}

pub async fn delete(account: &Account, channel: ChannelId, message: MessageId) -> ExitCode {
    let done = async {
        hold(account, channel, message).await?;
        account
            .delete_message(channel, message)
            .await
            .map_err(|err| delete_error(&err))
    }
    .await;
    finish(done, "Deleted.")
}

fn finish(done: Result<(), String>, said: &str) -> ExitCode {
    match done {
        Ok(()) => {
            println!("{said}");
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
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_refusal_before_asking_says_what_to_check() {
        let refused = EditError::Request(RequestError::InvalidRequest);

        assert!(edit_error(&refused).starts_with("Couldn't edit: Akari refused"));
        assert!(
            delete_error(&RequestError::InvalidRequest)
                .starts_with("Couldn't delete: Akari refused")
        );
    }

    #[test]
    fn automod_and_the_published_limit_say_why() {
        let blocked = EditError::MessageBlocked {
            reason: "Not here\u{1b}".to_owned(),
        };
        let limited = EditError::TooManyEdits {
            retry_after: Some(Duration::from_secs(61)),
        };

        assert_eq!(
            edit_error(&blocked),
            "Couldn't edit: AutoMod refused it: Not here\\u{1b}"
        );
        assert!(edit_error(&limited).ends_with("try again in 2 minute(s)."));
        assert_eq!(
            edit_error(&EditError::Forbidden),
            "Couldn't edit: editing this message isn't allowed"
        );
        assert_eq!(
            delete_error(&RequestError::Discord {
                status: 403,
                code: 50013,
                message: "Missing Permissions".to_owned(),
            }),
            "Couldn't delete: Discord error 50013: Missing Permissions"
        );
    }
}
