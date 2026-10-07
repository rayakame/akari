// The no-unwrap rule covers library crates only.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod host;
mod keychain;
mod login;

use std::error::Error;
use std::process::ExitCode;
use std::sync::Arc;

use akari_core::DiscordClient;
use akari_core::auth::LogoutError;
use akari_core::properties::{ClientBuild, ClientProperties};
use clap::{Args, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::keychain::KeychainStore;

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
        Command::Logout => logout(&client, &store).await,
    }
}

fn client(store: Arc<KeychainStore>) -> Result<DiscordClient, String> {
    let host = host::host_info()?;
    let properties = ClientProperties::desktop(&host, &ClientBuild::current(host.os));
    DiscordClient::new(properties, store).map_err(|err| report(&err))
}

async fn logout(client: &DiscordClient, store: &KeychainStore) -> ExitCode {
    let account = match store.current_account() {
        Ok(Some(account)) => account,
        Ok(None) => {
            println!("Not logged in.");
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("Couldn't read the keychain: {}", report(&err));
            return ExitCode::FAILURE;
        }
    };
    let result = client.logout(account).await;
    if let Err(LogoutError::Storage(err)) = &result {
        eprintln!("Couldn't remove the stored token: {}", report(err));
        return ExitCode::FAILURE;
    }
    if let Err(err) = store.clear_current_account() {
        eprintln!("Couldn't update the keychain: {}", report(&err));
        return ExitCode::FAILURE;
    }
    match result {
        Ok(()) => {
            println!("Logged out.");
            ExitCode::SUCCESS
        }
        Err(LogoutError::NotLoggedIn) => {
            println!("Not logged in.");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!(
                "Removed the stored token, but Discord didn't confirm the logout: {}",
                report(&err)
            );
            ExitCode::FAILURE
        }
    }
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
