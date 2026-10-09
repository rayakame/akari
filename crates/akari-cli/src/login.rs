use std::io::{self, BufRead as _, IsTerminal as _, Write as _};
use std::process::ExitCode;
use std::sync::Arc;

use akari_core::auth::{
    LoginError, LoginStep, LoginSuccess, LogoutError, MfaChallenge, MfaMethod, NewLocation,
    PasswordLogin, QrEvent,
};
use akari_core::model::{Snowflake, UserMarker};
use akari_core::{DiscordClient, Secret};
use akari_core::{Token, TokenStoreError};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

use crate::keychain::{Accounts, KeychainStore, off_runtime};
use crate::{printable, report};

const CAPTCHA_HELP: &str = "Discord wants a captcha for this login, which akari-cli can't show.";
const MFA_TRIES: usize = 3;

pub async fn password(client: &DiscordClient, store: &Arc<KeychainStore>) -> ExitCode {
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

pub async fn qr(client: &DiscordClient, store: &Arc<KeychainStore>) -> ExitCode {
    let flow = match client.qr_login() {
        Ok(flow) => flow,
        Err(err) => {
            eprintln!("Login failed: {}", report(&err));
            return ExitCode::FAILURE;
        }
    };
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
                    printable(&user.username)
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

async fn finish<S: Accounts>(
    client: &DiscordClient,
    store: &Arc<S>,
    success: LoginSuccess,
) -> ExitCode {
    let id = success.user_id;
    // Until the token is stored and selected, a failure would leave a live session that
    // nothing can find, so each early return ends the new session first.
    let previous = match off_runtime(store, |store| store.current_account()).await {
        Ok(previous) => previous,
        Err(err) => {
            eprintln!(
                "Logged in, but couldn't read the keychain: {}",
                report(&err)
            );
            end_new_session(client, &success.token).await;
            return ExitCode::FAILURE;
        }
    };
    let replaced = replaced_session(client, previous, &success).await;
    if let Err(err) = client.save_token(id, &success.token).await {
        eprintln!("Logged in, but couldn't store the token: {}", report(&err));
        end_new_session(client, &success.token).await;
        return ExitCode::FAILURE;
    }
    if previous != Some(id)
        && let Err(err) = off_runtime(store, move |store| store.set_current_account(id)).await
    {
        eprintln!(
            "Logged in, but couldn't remember the account: {}",
            report(&err)
        );
        // Without the current-account entry nothing would find this token again.
        discard(client, store, &success).await;
        return ExitCode::FAILURE;
    }
    println!("Logged in as user {}.", id.get());
    if success.password_update_required {
        println!("Discord asks you to change your password in the official app.");
    }
    match replaced {
        Ok(Some(old)) => {
            if let Err(err) = client.end_session(&old).await {
                eprintln!(
                    "Couldn't end this account's previous session. {}",
                    session_may_be_active(&err)
                );
            }
        }
        Ok(None) => {}
        Err(err) => eprintln!(
            "Couldn't read this account's previous token ({}), so its session couldn't be \
             ended and may still be active. End it in Discord under User Settings → Devices.",
            report(&err)
        ),
    }

    // Only once the new account is complete: a failure here leaves at most the old entry.
    let Some(previous) = replaced_account(previous, id) else {
        return ExitCode::SUCCESS;
    };
    let previous_id = previous.get();
    match client.logout(previous).await {
        Ok(()) | Err(LogoutError::NotLoggedIn) => {
            println!("Logged out the previous account (user {previous_id}).");
            ExitCode::SUCCESS
        }
        Err(LogoutError::Storage(err)) => {
            eprintln!(
                "Couldn't remove the previous account's token: {}. Delete the keychain entry \
                 \"akari-cli\" / \"{previous_id}\" by hand.",
                report(&err)
            );
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!(
                "Removed the previous account's token (user {previous_id}). {}",
                session_may_be_active(&err)
            );
            ExitCode::SUCCESS
        }
    }
}

// Logging in again as the same account replaces its token, whose session then has to end.
// An unreadable old token is an error, not "none": its session may still be active.
async fn replaced_session(
    client: &DiscordClient,
    previous: Option<Snowflake<UserMarker>>,
    success: &LoginSuccess,
) -> Result<Option<Token>, TokenStoreError> {
    if previous != Some(success.user_id) {
        return Ok(None);
    }
    Ok(client
        .load_token(success.user_id)
        .await?
        .filter(|token| *token != success.token))
}

// Undoes a login that can't be kept: ends its session first, so no live session is left
// behind a deleted token.
async fn discard<S: Accounts>(client: &DiscordClient, store: &Arc<S>, success: &LoginSuccess) {
    end_new_session(client, &success.token).await;
    let id = success.user_id;
    if let Err(err) = off_runtime(store, move |store| store.delete(id)).await {
        eprintln!(
            "Couldn't remove the new token: {}. Delete the keychain entry \"akari-cli\" / \
             \"{}\" by hand.",
            report(&err),
            id.get()
        );
    }
}

async fn end_new_session(client: &DiscordClient, token: &Token) {
    if let Err(err) = client.end_session(token).await {
        eprintln!(
            "Couldn't end the new session. {}",
            session_may_be_active(&err)
        );
    }
}

pub async fn logout<S: Accounts>(client: &DiscordClient, store: &Arc<S>) -> ExitCode {
    let account = match off_runtime(store, |store| store.current_account()).await {
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
    if let Err(err) = off_runtime(store, |store| store.clear_current_account()).await {
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
            eprintln!("Removed the stored token. {}", session_may_be_active(&err));
            ExitCode::FAILURE
        }
    }
}

fn session_may_be_active(err: &LogoutError) -> String {
    format!(
        "Discord didn't confirm the logout ({}), so the session may still be active. End it in \
         Discord under User Settings → Devices.",
        report(err)
    )
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
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use akari_core::model::Snowflake;
    use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
    use akari_core::{Endpoints, Token, TokenStore, TokenStoreError};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[derive(Default)]
    struct FakeAccounts {
        tokens: Mutex<HashMap<u64, String>>,
        current: Mutex<Option<u64>>,
        reading_fails: bool,
        loading_fails: bool,
        saving_fails: bool,
        selecting_fails: bool,
        deleting_fails: bool,
        // When set, current_account waits for a signal another task sends.
        signal: Option<Mutex<mpsc::Receiver<()>>>,
    }

    impl FakeAccounts {
        fn with(account: u64, token: &str) -> Self {
            Self {
                tokens: Mutex::new(HashMap::from([(account, token.to_owned())])),
                current: Mutex::new(Some(account)),
                ..Self::default()
            }
        }

        fn token(&self, account: u64) -> Option<String> {
            self.tokens.lock().unwrap().get(&account).cloned()
        }
    }

    impl TokenStore for FakeAccounts {
        fn load(&self, account: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
            if self.loading_fails {
                return Err(TokenStoreError::Unavailable);
            }
            Ok(self.token(account.get()).map(Token::new))
        }
        fn save(
            &self,
            account: Snowflake<UserMarker>,
            token: &Token,
        ) -> Result<(), TokenStoreError> {
            if self.saving_fails {
                return Err(TokenStoreError::Unavailable);
            }
            self.tokens
                .lock()
                .unwrap()
                .insert(account.get(), token.expose().to_owned());
            Ok(())
        }
        fn delete(&self, account: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
            if self.deleting_fails {
                return Err(TokenStoreError::Unavailable);
            }
            self.tokens.lock().unwrap().remove(&account.get());
            Ok(())
        }
    }

    impl Accounts for FakeAccounts {
        fn current_account(&self) -> Result<Option<Snowflake<UserMarker>>, TokenStoreError> {
            if let Some(signal) = &self.signal {
                signal
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(|_| TokenStoreError::Backend("blocked the runtime".to_owned()))?;
            }
            if self.reading_fails {
                return Err(TokenStoreError::Unavailable);
            }
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
        fn clear_current_account(&self) -> Result<(), TokenStoreError> {
            *self.current.lock().unwrap() = None;
            Ok(())
        }
    }

    fn client(server: &MockServer, store: Arc<FakeAccounts>) -> DiscordClient {
        let host = HostInfo {
            os: DesktopOs::MacOs,
            os_version: "25.0.0".to_owned(),
            arch: Arch::Arm64,
            system_locale: "en-US".to_owned(),
        };
        let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs));
        let endpoints = Endpoints {
            api: format!("{}/api/v9/", server.uri()),
            allow_plaintext: true,
            ..Endpoints::default()
        };
        DiscordClient::with_endpoints(properties, store, endpoints).unwrap()
    }

    async fn expect_logout(server: &MockServer, token: &str) {
        Mock::given(method("POST"))
            .and(path("/api/v9/auth/logout"))
            .and(header("authorization", token))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(server)
            .await;
    }

    fn login_as(account: u64) -> LoginSuccess {
        LoginSuccess {
            user_id: Snowflake::new(account),
            token: Token::new("new-token".to_owned()),
            password_update_required: false,
        }
    }

    #[test]
    fn only_a_different_previous_account_is_replaced() {
        let old = Snowflake::new(1);
        let new = Snowflake::new(2);

        assert_eq!(replaced_account(Some(old), new), Some(old));
        assert_eq!(replaced_account(Some(new), new), None);
        assert_eq!(replaced_account(None, new), None);
    }

    #[tokio::test]
    async fn logging_in_again_ends_the_old_session_and_keeps_the_new_token() {
        let server = MockServer::start().await;
        expect_logout(&server, "old-token").await;
        let store = Arc::new(FakeAccounts {
            selecting_fails: true,
            ..FakeAccounts::with(1, "old-token")
        });

        let code = finish(&client(&server, store.clone()), &store, login_as(1)).await;

        assert_eq!(code, ExitCode::SUCCESS);
        assert_eq!(store.token(1).as_deref(), Some("new-token"));
    }

    #[tokio::test]
    async fn a_switch_that_cant_be_selected_ends_the_new_session() {
        let server = MockServer::start().await;
        expect_logout(&server, "new-token").await;
        let store = Arc::new(FakeAccounts {
            selecting_fails: true,
            ..FakeAccounts::with(1, "old-token")
        });

        let code = finish(&client(&server, store.clone()), &store, login_as(2)).await;

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(store.token(1).as_deref(), Some("old-token"));
        assert_eq!(store.token(2), None);
        assert_eq!(*store.current.lock().unwrap(), Some(1));
    }

    #[tokio::test]
    async fn the_new_session_ends_even_when_its_token_cant_be_deleted() {
        let server = MockServer::start().await;
        expect_logout(&server, "new-token").await;
        let store = Arc::new(FakeAccounts {
            selecting_fails: true,
            deleting_fails: true,
            ..FakeAccounts::with(1, "old-token")
        });

        let code = finish(&client(&server, store.clone()), &store, login_as(2)).await;

        assert_eq!(code, ExitCode::FAILURE);
    }

    #[tokio::test]
    async fn an_unreadable_keychain_ends_the_new_session() {
        let server = MockServer::start().await;
        expect_logout(&server, "new-token").await;
        let store = Arc::new(FakeAccounts {
            reading_fails: true,
            ..FakeAccounts::default()
        });

        let code = finish(&client(&server, store.clone()), &store, login_as(1)).await;

        assert_eq!(code, ExitCode::FAILURE);
    }

    #[tokio::test]
    async fn a_token_that_cant_be_saved_ends_its_session() {
        let server = MockServer::start().await;
        expect_logout(&server, "new-token").await;
        let store = Arc::new(FakeAccounts {
            saving_fails: true,
            ..FakeAccounts::default()
        });

        let code = finish(&client(&server, store.clone()), &store, login_as(1)).await;

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(store.token(1), None);
    }

    #[tokio::test]
    async fn an_unreadable_old_token_is_reported_not_ignored() {
        let server = MockServer::start().await;
        let store = Arc::new(FakeAccounts {
            loading_fails: true,
            ..FakeAccounts::with(1, "old-token")
        });
        let client = client(&server, store.clone());

        let replaced = replaced_session(&client, Some(Snowflake::new(1)), &login_as(1)).await;

        assert!(replaced.is_err());
        assert!(matches!(
            replaced_session(&client, Some(Snowflake::new(2)), &login_as(1)).await,
            Ok(None)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn keychain_calls_run_off_the_runtime_thread() {
        let server = MockServer::start().await;
        let (send, receive) = mpsc::channel();
        let store = Arc::new(FakeAccounts {
            signal: Some(Mutex::new(receive)),
            ..FakeAccounts::default()
        });
        let signal = tokio::spawn(async move { send.send(()) });

        let code = finish(&client(&server, store.clone()), &store, login_as(1)).await;

        assert_eq!(code, ExitCode::SUCCESS);
        assert!(signal.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn logout_without_discord_deletes_locally_and_warns() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v9/auth/logout"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let store = Arc::new(FakeAccounts::with(1, "old-token"));

        let code = logout(&client(&server, store.clone()), &store).await;

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(store.token(1), None);
        assert_eq!(*store.current.lock().unwrap(), None);
    }

    #[test]
    fn the_warning_says_where_to_end_the_session() {
        let warning = session_may_be_active(&LogoutError::UnexpectedResponse);

        assert!(warning.contains("may still be active"), "{warning}");
        assert!(warning.contains("Devices"), "{warning}");
    }
}
