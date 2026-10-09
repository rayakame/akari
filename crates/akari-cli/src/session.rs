use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use akari_core::gateway::GatewayError;
use akari_core::state::{ConnectionState, StoreEvent, Subscription};
use akari_core::{Account, DiscordClient, Token};
use tokio::signal::unix::Signal;

use crate::keychain::{Accounts, off_runtime};
use crate::report;

// Long enough for the close frame, so Discord ends the session instead of timing it out.
pub const CLOSE_WAIT: Duration = Duration::from_secs(5);
pub const LOG_IN: &str = "Run `akari-cli login --qr` or `akari-cli login --password`";

/// An online account and its change events, for one command.
pub struct Session {
    pub account: Account,
    pub events: Subscription,
}

/// The current account's token, or the reason there is none, already printed.
pub async fn stored_token<S: Accounts>(
    client: &DiscordClient,
    store: &Arc<S>,
) -> Result<Token, ExitCode> {
    let id = match off_runtime(store, |store| store.current_account()).await {
        Ok(Some(id)) => id,
        Ok(None) => return Err(fail(&format!("Not logged in. {LOG_IN} first."))),
        Err(err) => {
            return Err(fail(&format!(
                "Couldn't read the keychain: {}",
                report(&err)
            )));
        }
    };
    match client.load_token(id).await {
        Ok(Some(token)) => Ok(token),
        Ok(None) => Err(fail(&format!(
            "No token is stored for this account. {LOG_IN}."
        ))),
        Err(err) => Err(fail(&format!("Couldn't read the token: {}", report(&err)))),
    }
}

/// `flags_only` leaves member lists out of op 37, for testing.
pub async fn open<S: Accounts>(
    client: &DiscordClient,
    store: &Arc<S>,
    flags_only: bool,
) -> Result<Session, ExitCode> {
    let token = stored_token(client, store).await?;
    let account = client
        .account(token)
        .map_err(|err| fail(&closed_message(Some(&err))))?;
    if flags_only {
        account.subscribe_flags_only();
    }
    let events = account.store().subscribe();
    account
        .connect()
        .map_err(|err| fail(&closed_message(Some(&err))))?;
    while let Some(event) = events.next().await {
        match event {
            StoreEvent::Connection(ConnectionState::Online) => {
                return Ok(Session { account, events });
            }
            StoreEvent::Connection(ConnectionState::Closed { error }) => {
                return Err(fail(&closed_message(error.as_deref())));
            }
            _ => {}
        }
    }
    Err(fail("The connection closed."))
}

impl Session {
    /// Closes the session and waits for the close frame, or for Ctrl+C on `interrupts`.
    pub async fn close(self, interrupts: Option<&mut Signal>) {
        self.account.close();
        let drained = tokio::time::timeout(CLOSE_WAIT, async {
            while self.events.next().await.is_some() {}
        });
        match interrupts {
            Some(interrupts) => {
                tokio::select! {
                    _ = drained => {}
                    _ = interrupts.recv() => {}
                }
            }
            None => {
                let _ = drained.await;
            }
        }
    }
}

pub fn closed_message(error: Option<&GatewayError>) -> String {
    match error {
        Some(GatewayError::AuthenticationFailed) => {
            format!("Discord no longer accepts the stored token. {LOG_IN} to log in again.")
        }
        Some(err) => format!("The connection failed: {}", report(err)),
        None => "The connection closed.".to_owned(),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}
