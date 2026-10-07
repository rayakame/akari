use std::io::{self, BufRead as _, IsTerminal as _, Write as _};
use std::process::ExitCode;

use akari_core::auth::{
    LoginError, LoginStep, LoginSuccess, MfaChallenge, MfaMethod, NewLocation, PasswordLogin,
    QrEvent,
};
use akari_core::{DiscordClient, Secret};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

use crate::keychain::KeychainStore;
use crate::report;

const CAPTCHA_HELP: &str = "Discord wants a captcha for this login, which akari-cli can't show.";
const MFA_TRIES: usize = 3;

pub async fn password(client: &DiscordClient, store: &KeychainStore) -> ExitCode {
    let flow = client.password_login();
    let Some(login) = prompt("Email or phone number: ") else {
        return ExitCode::FAILURE;
    };
    let password = match rpassword::prompt_password("Password: ") {
        Ok(password) => Secret::new(password),
        Err(err) => {
            eprintln!("Couldn't read the password: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut step = flow.submit(&login, password).await;
    loop {
        step = match step {
            Ok(LoginStep::Done(success)) => return finish(client, store, success).await,
            Ok(LoginStep::Captcha(_)) => {
                eprintln!("{CAPTCHA_HELP} Run `akari-cli login --qr` instead.");
                return ExitCode::FAILURE;
            }
            Ok(LoginStep::Mfa(challenge)) => match mfa(&flow, &challenge).await {
                Some(result) => result,
                None => return ExitCode::FAILURE,
            },
            Ok(LoginStep::NewLocation(NewLocation::Email)) => {
                println!(
                    "Discord sent you an email to confirm this login location. Open the link \
                     in a browser and paste the address it ends up on (it contains #token=)."
                );
                match prompt("Address: ") {
                    Some(link) => flow.confirm_new_location(&link).await,
                    None => return ExitCode::FAILURE,
                }
            }
            Ok(LoginStep::NewLocation(NewLocation::Phone)) => {
                println!("Discord texted you a code to confirm this login location.");
                match prompt("Code: ") {
                    Some(code) => flow.verify_phone(Secret::new(code)).await,
                    None => return ExitCode::FAILURE,
                }
            }
            Err(err) => {
                eprintln!("Login failed: {}", report(&err));
                return ExitCode::FAILURE;
            }
        };
    }
}

// None: the user can't continue here, and has been told why.
async fn mfa(
    flow: &PasswordLogin,
    challenge: &MfaChallenge,
) -> Option<Result<LoginStep, LoginError>> {
    if !challenge.methods.contains(&MfaMethod::Totp) {
        let methods: Vec<_> = challenge
            .methods
            .iter()
            .map(|method| method_name(*method))
            .collect();
        eprintln!(
            "This account confirms logins with {}, which akari-cli doesn't support. Run \
             `akari-cli login --qr` instead.",
            methods.join(" or ")
        );
        return None;
    }
    for attempt in 1..=MFA_TRIES {
        let code = prompt("Two-factor code (authenticator app or backup code): ")?;
        match flow.submit_mfa(MfaMethod::Totp, Secret::new(code)).await {
            Err(LoginError::InvalidMfaCode) if attempt < MFA_TRIES => {
                eprintln!("That code didn't work, try again.");
            }
            result => return Some(result),
        }
    }
    Some(Err(LoginError::InvalidMfaCode))
}

fn method_name(method: MfaMethod) -> &'static str {
    match method {
        MfaMethod::Totp => "an authenticator app",
        MfaMethod::Sms => "SMS",
        MfaMethod::Backup => "backup codes",
        MfaMethod::WebAuthn => "a security key",
    }
}

pub async fn qr(client: &DiscordClient, store: &KeychainStore) -> ExitCode {
    let flow = client.qr_login();
    loop {
        let event = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                flow.cancel();
                eprintln!("Cancelled.");
                return ExitCode::from(130);
            }
            event = flow.next() => event,
        };
        match event {
            Ok(QrEvent::Code { url }) => show_code(&url),
            Ok(QrEvent::Scanned(user)) => {
                println!(
                    "Scanned by {}. Confirm the login on your phone.",
                    user.username
                );
            }
            Ok(QrEvent::CancelledOnPhone) => println!("Cancelled on the phone."),
            Ok(QrEvent::Captcha(_)) => {
                eprintln!("{CAPTCHA_HELP}");
                return ExitCode::FAILURE;
            }
            Ok(QrEvent::Done(success)) => return finish(client, store, success).await,
            Err(err) => {
                eprintln!("Login failed: {}", report(&err));
                return ExitCode::FAILURE;
            }
        }
    }
}

fn show_code(url: &str) {
    let Ok(code) = QrCode::new(url.as_bytes()) else {
        eprintln!("Couldn't draw the QR code.");
        return;
    };
    let image = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .build();
    if io::stdout().is_terminal() {
        print!("\x1b[2J\x1b[H");
    }
    println!("{image}\n\nScan this with the Discord mobile app (Settings → Scan QR Code).");
    println!("The code renews itself every few minutes. Ctrl+C cancels.");
}

async fn finish(client: &DiscordClient, store: &KeychainStore, success: LoginSuccess) -> ExitCode {
    if let Err(err) = client.save_token(success.user_id, &success.token).await {
        eprintln!("Logged in, but couldn't store the token: {}", report(&err));
        return ExitCode::FAILURE;
    }
    if let Err(err) = store.set_current_account(success.user_id) {
        eprintln!(
            "Logged in, but couldn't remember the account: {}",
            report(&err)
        );
        return ExitCode::FAILURE;
    }
    println!("Logged in as user {}.", success.user_id.get());
    if success.password_update_required {
        println!("Discord asks you to change your password in the official app.");
    }
    ExitCode::SUCCESS
}

fn prompt(label: &str) -> Option<String> {
    print!("{label}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line.trim().to_owned()),
        Err(err) => {
            eprintln!("Couldn't read the input: {err}");
            None
        }
    }
}
