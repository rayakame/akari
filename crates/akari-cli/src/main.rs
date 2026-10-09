// The no-unwrap rule covers library crates only.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod connect;
mod host;
mod keychain;
mod login;
mod messages;
mod session;

use std::error::Error;
use std::process::ExitCode;
use std::sync::Arc;

use akari_core::DiscordClient;
use akari_core::properties::{ClientBuild, ClientProperties};
use clap::{Args, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::keychain::KeychainStore;
use crate::messages::SessionCommand;

/// Terminal test client for akari-core.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Log in and keep the token in the OS keychain.
    Login(LoginArgs),
    /// End the session on Discord and remove the stored token.
    Logout,
    /// Connect to the gateway with the stored login and print a summary of READY.
    Connect(ConnectArgs),
    #[command(flatten)]
    Session(SessionCommand),
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct LoginArgs {
    /// Show a QR code to scan with the Discord mobile app.
    #[arg(long)]
    qr: bool,
    /// Log in with email or phone number and password.
    #[arg(long)]
    password: bool,
}

#[derive(Args)]
struct ConnectArgs {
    /// Stay connected and report reconnects until Ctrl+C.
    #[arg(long)]
    keep_open: bool,
    /// Save the raw READY payload to captures/. It contains personal data.
    #[arg(long)]
    capture: bool,
    /// Set this session's status after READY.
    #[arg(long, value_enum)]
    status: Option<connect::Status>,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();

    let store = Arc::new(KeychainStore);
    let client = match client(store.clone()) {
        Ok(client) => client,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    match cli.command {
        Command::Login(args) if args.qr => login::qr(&client, &store).await,
        Command::Login(_) => login::password(&client, &store).await,
        Command::Logout => login::logout(&client, &store).await,
        Command::Connect(args) => {
            let options = connect::Options {
                keep_open: args.keep_open,
                capture: args.capture,
                status: args.status,
            };
            connect::run(&client, &store, options).await
        }
        Command::Session(command) => messages::run(&client, &store, command).await,
    }
}

fn client(store: Arc<KeychainStore>) -> Result<DiscordClient, String> {
    let host = host::host_info()?;
    let properties = ClientProperties::desktop(&host, &ClientBuild::current(host.os));
    DiscordClient::new(properties, store).map_err(|err| report(&err))
}

fn report(err: &dyn Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::*;

    #[test]
    fn limits_outside_1_to_100_are_refused() {
        for limit in ["0", "101"] {
            assert!(Cli::try_parse_from(["akari-cli", "read", "1", "--limit", limit]).is_err());
        }
        assert!(Cli::try_parse_from(["akari-cli", "read", "1", "--limit", "100"]).is_ok());
    }

    #[test]
    fn repeat_nonce_is_a_hidden_send_option() {
        let cli =
            Cli::try_parse_from(["akari-cli", "send", "--repeat-nonce", "1", "hello"]).unwrap();
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("send")
            .unwrap()
            .render_help()
            .to_string();

        let Command::Session(SessionCommand::Send {
            repeat_nonce, text, ..
        }) = cli.command
        else {
            panic!("expected send");
        };
        assert!(repeat_nonce);
        assert_eq!(text, ["hello"]);
        assert!(!help.contains("repeat"), "{help}");
    }

    #[test]
    fn flags_only_is_a_hidden_tail_option() {
        let cli = Cli::try_parse_from(["akari-cli", "tail", "--flags-only", "1"]).unwrap();
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("tail")
            .unwrap()
            .render_help()
            .to_string();

        let Command::Session(SessionCommand::Tail {
            flags_only,
            channel_id,
        }) = cli.command
        else {
            panic!("expected tail");
        };
        assert!(flags_only);
        assert_eq!(channel_id, 1);
        assert!(!help.contains("flags"), "{help}");
    }

    #[test]
    fn send_joins_the_words_of_its_text() {
        let cli = Cli::try_parse_from(["akari-cli", "send", "1", "hello", "there"]).unwrap();

        let Command::Session(SessionCommand::Send { text, .. }) = cli.command else {
            panic!("expected send");
        };
        assert_eq!(text.join(" "), "hello there");
    }
}
