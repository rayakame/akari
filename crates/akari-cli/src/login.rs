use std::io::{self, BufRead as _, IsTerminal as _, Write as _};
use std::process::ExitCode;

use akari_core::auth::{
    LoginError, LoginStep, LoginSuccess, LogoutError, MfaChallenge, MfaMethod, NewLocation,
    PasswordLogin, QrEvent,
};
use akari_core::model::{Snowflake, UserMarker};
use akari_core::{DiscordClient, Secret};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

use crate::keychain::{Accounts, KeychainStore};
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

async fn finish<S: Accounts>(client: &DiscordClient, store: &S, success: LoginSuccess) -> ExitCode {
    let previous = match store.current_account() {
        Ok(previous) => previous,
        Err(err) => {
            eprintln!(
                "Logged in, but couldn't read the keychain: {}",
                report(&err)
            );
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = client.save_token(success.user_id, &success.token).await {
        eprintln!("Logged in, but couldn't store the token: {}", report(&err));
        return ExitCode::FAILURE;
    }
    if previous != Some(success.user_id)
        && let Err(err) = store.set_current_account(success.user_id)
    {
        eprintln!(
            "Logged in, but couldn't remember the account: {}",
            report(&err)
        );
        // Without the current-account entry nothing would find this token again.
        let _ = store.delete(success.user_id);
        return ExitCode::FAILURE;
    }
    println!("Logged in as user {}.", success.user_id.get());
    if success.password_update_required {
        println!("Discord asks you to change your password in the official app.");
    }

    // Only once the new account is complete: a failure here leaves at most the old entry.
    let Some(previous) = replaced_account(previous, success.user_id) else {
        return ExitCode::SUCCESS;
    };
    let id = previous.get();
    match client.logout(previous).await {
        Ok(()) | Err(LogoutError::NotLoggedIn) => {
            println!("Logged out the previous account (user {id}).");
            ExitCode::SUCCESS
        }
        Err(LogoutError::Storage(err)) => {
            eprintln!(
                "Couldn't remove the previous account's token: {}. Delete the keychain entry \
                 \"akari-cli\" / \"{id}\" by hand.",
                report(&err)
            );
            ExitCode::FAILURE
        }
        Err(err) => {
            println!(
                "Removed the previous account's token (user {id}), but Discord didn't confirm \
                 its logout: {}",
                report(&err)
            );
            ExitCode::SUCCESS
        }
    }
}

// akari-cli keeps one account, so logging in as someone else replaces the stored one.
fn replaced_account(
    previous: Option<Snowflake<UserMarker>>,
    new: Snowflake<UserMarker>,
) -> Option<Snowflake<UserMarker>> {
    previous.filter(|previous| *previous != new)
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use akari_core::model::Snowflake;
    use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
    use akari_core::{Token, TokenStore, TokenStoreError};

    use super::*;

    #[derive(Default)]
    struct FakeAccounts {
        tokens: Mutex<HashMap<u64, String>>,
        current: Mutex<Option<u64>>,
        selecting_fails: bool,
    }

    impl FakeAccounts {
        fn token(&self, account: u64) -> Option<String> {
            self.tokens.lock().unwrap().get(&account).cloned()
        }
    }

    impl TokenStore for FakeAccounts {
        fn load(&self, account: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
            Ok(self.token(account.get()).map(Token::new))
        }
        fn save(
            &self,
            account: Snowflake<UserMarker>,
            token: &Token,
        ) -> Result<(), TokenStoreError> {
            self.tokens
                .lock()
                .unwrap()
                .insert(account.get(), token.expose().to_owned());
            Ok(())
        }
        fn delete(&self, account: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
            self.tokens.lock().unwrap().remove(&account.get());
            Ok(())
        }
    }

    impl Accounts for FakeAccounts {
        fn current_account(&self) -> Result<Option<Snowflake<UserMarker>>, TokenStoreError> {
            Ok(self.current.lock().unwrap().map(Snowflake::new))
        }
        fn set_current_account(
            &self,
            account: Snowflake<UserMarker>,
        ) -> Result<(), TokenStoreError> {
            if self.selecting_fails {
                return Err(TokenStoreError::Unavailable);
            }
            *self.current.lock().unwrap() = Some(account.get());
            Ok(())
        }
    }

    fn client(store: Arc<FakeAccounts>) -> DiscordClient {
        let host = HostInfo {
            os: DesktopOs::MacOs,
            os_version: "25.0.0".to_owned(),
            arch: Arch::Arm64,
            system_locale: "en-US".to_owned(),
        };
        let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs));
        DiscordClient::new(properties, store).unwrap()
    }

    fn login_as(account: u64) -> LoginSuccess {
        LoginSuccess {
            user_id: Snowflake::new(account),
            token: Token::new("new-token".to_owned()),
            password_update_required: false,
        }
    }

    #[tokio::test]
    async fn logging_in_again_as_the_current_account_keeps_its_token() {
        let store = Arc::new(FakeAccounts {
            current: Mutex::new(Some(1)),
            selecting_fails: true,
            ..FakeAccounts::default()
        });

        let code = finish(&client(store.clone()), &*store, login_as(1)).await;

        assert_eq!(code, ExitCode::SUCCESS);
        assert_eq!(store.token(1).as_deref(), Some("new-token"));
    }

    #[tokio::test]
    async fn a_switch_that_cant_be_selected_leaves_the_previous_account() {
        let store = Arc::new(FakeAccounts {
            tokens: Mutex::new(HashMap::from([(1, "old-token".to_owned())])),
            current: Mutex::new(Some(1)),
            selecting_fails: true,
        });

        let code = finish(&client(store.clone()), &*store, login_as(2)).await;

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(store.token(1).as_deref(), Some("old-token"));
        assert_eq!(store.token(2), None);
        assert_eq!(*store.current.lock().unwrap(), Some(1));
    }

    #[test]
    fn only_a_different_previous_account_is_replaced() {
        let old = Snowflake::new(1);
        let new = Snowflake::new(2);

        assert_eq!(replaced_account(Some(old), new), Some(old));
        assert_eq!(replaced_account(Some(new), new), None);
        assert_eq!(replaced_account(None, new), None);
    }
}
