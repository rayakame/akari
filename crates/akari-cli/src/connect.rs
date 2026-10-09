use std::fs::{self, DirBuilder, OpenOptions, Permissions};
use std::io::{self, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use akari_core::DiscordClient;
use akari_core::gateway::{
    ConnectionEvent, DisconnectReason, DispatchEvent, Gateway, GatewayCommand, GatewayError,
    GatewayGuild, PresenceStatus, Ready,
};

use tokio::signal::unix::{Signal, SignalKind, signal};

use crate::keychain::Accounts;
use crate::session::{CLOSE_WAIT, LOG_IN, stored_token};
use crate::{printable, report};

const CAPTURES: &str = "captures";
const CAPTURE_WARNING: &str = "It contains personal data and secrets such as the analytics \
    token: don't commit or share it, and delete it when you're done.";

pub struct Options {
    pub keep_open: bool,
    pub capture: bool,
    pub status: Option<Status>,
}

/// A status for `--status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Status {
    Online,
    Idle,
    Dnd,
    Invisible,
}

impl Status {
    fn presence(self) -> PresenceStatus {
        match self {
            Self::Online => PresenceStatus::Online,
            Self::Idle => PresenceStatus::Idle,
            Self::Dnd => PresenceStatus::DoNotDisturb,
            Self::Invisible => PresenceStatus::Invisible,
        }
    }
}

pub async fn run<S: Accounts>(
    client: &DiscordClient,
    store: &Arc<S>,
    options: Options,
) -> ExitCode {
    let token = match stored_token(client, store).await {
        Ok(token) => token,
        Err(code) => return code,
    };
    let gateway = match client.gateway(token) {
        Ok(gateway) => gateway,
        Err(err) => {
            eprintln!("{}", failure_message(&err));
            return ExitCode::FAILURE;
        }
    };
    if options.capture {
        gateway.capture_next_ready();
    }
    // One stream for the whole run: an interrupt arriving between two selects isn't lost.
    let mut interrupts = match signal(SignalKind::interrupt()) {
        Ok(interrupts) => interrupts,
        Err(err) => {
            eprintln!("Couldn't watch for Ctrl+C: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = gateway.connect() {
        eprintln!("{}", failure_message(&err));
        return ExitCode::FAILURE;
    }
    let code = events(&gateway, &options, &mut interrupts).await;
    gateway.close();
    let drained = tokio::time::timeout(CLOSE_WAIT, async { while gateway.next().await.is_ok() {} });
    tokio::select! {
        _ = drained => {}
        _ = interrupts.recv() => {}
    }
    code
}

async fn events(gateway: &Gateway, options: &Options, interrupts: &mut Signal) -> ExitCode {
    let mut connected = false;
    loop {
        let event = tokio::select! {
            _ = interrupts.recv() => {
                if connected {
                    return ExitCode::SUCCESS;
                }
                eprintln!("Cancelled.");
                return ExitCode::from(130);
            }
            event = gateway.next() => event,
        };
        match event {
            Ok(ConnectionEvent::CapturedReady(raw)) => {
                if let Err(code) = save_and_report(Path::new(CAPTURES), &raw) {
                    return code;
                }
            }
            Ok(ConnectionEvent::Dispatch(DispatchEvent::Ready(ready))) => {
                println!("{}", ready_summary(&ready));
                // A new session starts with Identify's "unknown" status, so it is set again.
                if let Some(status) = options.status {
                    set_status(gateway, status).await;
                }
                if !options.keep_open {
                    return ExitCode::SUCCESS;
                }
                connected = true;
            }
            Ok(ConnectionEvent::Dispatch(DispatchEvent::Resumed)) => println!("Resumed."),
            Ok(ConnectionEvent::Reconnecting {
                resume,
                delay,
                reason,
            }) => println!("{}", reconnecting_line(resume, delay, &reason)),
            Ok(_) => {}
            Err(err) => {
                eprintln!("{}", failure_message(&err));
                return ExitCode::FAILURE;
            }
        }
    }
}

async fn set_status(gateway: &Gateway, status: Status) {
    let command = GatewayCommand::UpdatePresence {
        status: status.presence(),
    };
    match gateway.send(command).await {
        Ok(()) => println!("Status set to {}.", status_name(status)),
        Err(err) => eprintln!("Couldn't set the status: {err}"),
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Online => "online",
        Status::Idle => "idle",
        Status::Dnd => "do not disturb",
        Status::Invisible => "invisible",
    }
}

fn ready_summary(ready: &Ready) -> String {
    let unavailable = ready
        .guilds
        .iter()
        .filter(|guild| matches!(guild, GatewayGuild::Unavailable(_)))
        .count();
    format!(
        "Connected as {}: {} guilds ({unavailable} unavailable), {} private channels.",
        printable(&ready.user.user.username),
        ready.guilds.len(),
        ready.private_channels.len()
    )
}

fn reconnecting_line(resume: bool, delay: Duration, reason: &DisconnectReason) -> String {
    let next = if resume {
        "resuming"
    } else {
        "starting a new session"
    };
    format!(
        "Disconnected ({}); {next} in {:.1} s.",
        reason_text(reason),
        delay.as_secs_f64()
    )
}

fn reason_text(reason: &DisconnectReason) -> String {
    match reason {
        DisconnectReason::Transport(err) => report(err),
        DisconnectReason::ClosedByDiscord { code } => {
            format!("Discord closed the connection with {code}")
        }
        DisconnectReason::Requested => "Discord asked for a reconnect".to_owned(),
        DisconnectReason::InvalidSession => "Discord invalidated the session".to_owned(),
        DisconnectReason::Zombie => "no heartbeat acknowledgement".to_owned(),
        DisconnectReason::NoHello => "Discord didn't say hello".to_owned(),
        DisconnectReason::Decompress => "the compressed stream broke".to_owned(),
        _ => "unknown reason".to_owned(),
    }
}

fn failure_message(err: &GatewayError) -> String {
    match err {
        GatewayError::AuthenticationFailed => {
            format!("Discord rejected the stored token (4004). {LOG_IN} to log in again.")
        }
        err => format!("The gateway connection failed: {}", report(err)),
    }
}

fn save_and_report(dir: &Path, json: &[u8]) -> Result<(), ExitCode> {
    match save_capture(dir, json) {
        Ok(path) => {
            eprintln!("Saved READY to {}. {CAPTURE_WARNING}", path.display());
            Ok(())
        }
        Err(err) => {
            eprintln!("Couldn't save READY: {err}");
            Err(ExitCode::FAILURE)
        }
    }
}

fn save_capture(dir: &Path, json: &[u8]) -> io::Result<PathBuf> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    // The mode above only applies to a directory that didn't exist yet.
    fs::set_permissions(dir, Permissions::from_mode(0o700))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let path = dir.join(format!("ready-{stamp}.json"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(json)?;
    // The ignored READY test runs from crates/akari-core, so a relative path wouldn't work there.
    std::path::absolute(&path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use akari_core::gateway::{DispatchEvent, GatewayEvent, decode};

    use super::*;

    fn fixture() -> Ready {
        let json = include_bytes!("../../akari-core/tests/fixtures/ready.json");
        match decode(json).unwrap() {
            GatewayEvent::Dispatch {
                event: DispatchEvent::Ready(ready),
                ..
            } => *ready,
            other => panic!("not READY: {other:?}"),
        }
    }

    #[test]
    fn the_summary_counts_guilds_and_private_channels() {
        assert_eq!(
            ready_summary(&fixture()),
            "Connected as akari_tester: 2 guilds (1 unavailable), 2 private channels."
        );
    }

    #[test]
    fn a_rejected_token_says_how_to_log_in_again() {
        let message = failure_message(&GatewayError::AuthenticationFailed);

        assert!(message.contains("akari-cli login"), "{message}");
    }

    #[test]
    fn reconnect_lines_say_what_happens_next() {
        assert_eq!(
            reconnecting_line(true, Duration::from_millis(1500), &DisconnectReason::Zombie),
            "Disconnected (no heartbeat acknowledgement); resuming in 1.5 s."
        );
        assert_eq!(
            reconnecting_line(
                false,
                Duration::from_secs(3),
                &DisconnectReason::ClosedByDiscord { code: 4009 }
            ),
            "Disconnected (Discord closed the connection with 4009); starting a new session in 3.0 s."
        );
    }

    #[test]
    fn captures_are_private_files() {
        let dir = std::env::temp_dir().join(format!("akari-cli-capture-{}", std::process::id()));

        let path = save_capture(&dir, b"{\"op\":0}").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"{\"op\":0}");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn captures_report_an_absolute_path() {
        let parent = Path::new(CAPTURES);
        let dir = parent.join(format!("akari-cli-test-{}", std::process::id()));

        let path = save_capture(&dir, b"{}");

        fs::remove_dir_all(&dir).unwrap();
        let _ = fs::remove_dir(parent);
        let path = path.unwrap();
        assert!(path.is_absolute(), "{}", path.display());
    }

    #[test]
    fn an_undecodable_ready_never_quotes_its_values() {
        let err =
            decode(br#"{"op": 0, "s": 1, "t": "READY", "d": {"v": "secret-value"}}"#).unwrap_err();

        let message = failure_message(&GatewayError::InvalidReady(err));

        assert!(!message.contains("secret-value"), "{message}");
        assert!(message.contains("READY couldn't be decoded"), "{message}");
        assert!(message.contains("line 1"), "{message}");
    }

    #[test]
    fn a_failed_capture_exits_non_zero() {
        let file = std::env::temp_dir().join(format!("akari-cli-not-a-dir-{}", std::process::id()));
        fs::write(&file, b"").unwrap();

        let result = save_and_report(&file.join("captures"), b"{}");

        fs::remove_file(&file).unwrap();
        assert_eq!(result, Err(ExitCode::FAILURE));
    }

    #[test]
    fn an_existing_captures_directory_becomes_private() {
        let dir = std::env::temp_dir().join(format!("akari-cli-open-dir-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

        save_capture(&dir, b"{}").unwrap();

        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn status_flags_map_to_presence_statuses() {
        assert_eq!(Status::Online.presence(), PresenceStatus::Online);
        assert_eq!(Status::Idle.presence(), PresenceStatus::Idle);
        assert_eq!(Status::Dnd.presence(), PresenceStatus::DoNotDisturb);
        assert_eq!(Status::Invisible.presence(), PresenceStatus::Invisible);
    }
}
