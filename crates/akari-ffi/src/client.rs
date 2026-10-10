use std::sync::Arc;

use akari_core::model::UserId;
use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs};
use tokio::runtime::Runtime;

use crate::account::Account;
use crate::errors::{ClientError, GatewayError, LoginError, LogoutError, TokenStoreError};
use crate::login::{PasswordLogin, QrLogin};
use crate::runtime::{run, runtime};
use crate::token_store::{HostStore, TokenStore};

/// What the host tells Akari about the machine; the rest comes from the build.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HostInfo {
    /// Kernel release as `uname -r` prints it, e.g. `25.0.0`.
    pub os_version: String,
    /// BCP 47 tag such as `en-US`.
    pub system_locale: String,
}

/// Where Discord lives.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Endpoints {
    /// REST base including the API version.
    pub api: String,
    pub gateway: String,
    pub remote_auth: String,
    /// The `Origin` header the remote auth gateway requires.
    pub origin: String,
}

/// Discord's production endpoints.
#[uniffi::export]
pub fn discord_endpoints() -> Endpoints {
    let endpoints = akari_core::Endpoints::default();
    Endpoints {
        api: endpoints.api,
        gateway: endpoints.gateway,
        remote_auth: endpoints.remote_auth,
        origin: endpoints.origin,
    }
}

/// A Discord token. The host can't read it; it hands it back to the client.
#[derive(uniffi::Object)]
pub struct Token(akari_core::Token);

impl Token {
    pub(crate) fn new(token: akari_core::Token) -> Arc<Self> {
        Arc::new(Self(token))
    }

    pub(crate) fn core(&self) -> &akari_core::Token {
        &self.0
    }
}

/// The shared context for talking to Discord, one per app.
#[derive(uniffi::Object)]
pub struct DiscordClient {
    core: akari_core::DiscordClient,
    runtime: &'static Runtime,
}

#[uniffi::export]
impl DiscordClient {
    /// A client for Discord's production service. The first client starts Akari's
    /// background runtime. Fails on platforms Discord has no desktop client for.
    #[uniffi::constructor]
    pub fn new(host: HostInfo, token_store: Arc<dyn TokenStore>) -> Result<Arc<Self>, ClientError> {
        Self::with_endpoints(host, token_store, discord_endpoints())
    }

    /// For canary builds and tests; every endpoint must use TLS.
    #[uniffi::constructor]
    pub fn with_endpoints(
        host: HostInfo,
        token_store: Arc<dyn TokenStore>,
        endpoints: Endpoints,
    ) -> Result<Arc<Self>, ClientError> {
        let runtime = runtime()?;
        let os = desktop_os().ok_or(ClientError::UnsupportedPlatform)?;
        let arch = Arch::current().ok_or(ClientError::UnsupportedPlatform)?;
        let host = akari_core::properties::HostInfo {
            os,
            os_version: host.os_version,
            arch,
            system_locale: host.system_locale,
        };
        let properties = ClientProperties::desktop(&host, &ClientBuild::current(os));
        let endpoints = akari_core::Endpoints {
            api: endpoints.api,
            gateway: endpoints.gateway,
            remote_auth: endpoints.remote_auth,
            origin: endpoints.origin,
            ..akari_core::Endpoints::default()
        };
        let core = akari_core::DiscordClient::with_endpoints(
            properties,
            Arc::new(HostStore(token_store)),
            endpoints,
        )?;
        Ok(Arc::new(Self { core, runtime }))
    }

    /// Starts an email/password login. Several logins can run at the same time.
    pub fn password_login(&self) -> Arc<PasswordLogin> {
        PasswordLogin::new(self.core.password_login(), self.runtime)
    }

    /// Starts a QR code login in the background.
    pub fn qr_login(&self) -> Result<Arc<QrLogin>, LoginError> {
        let _entered = self.runtime.enter();
        Ok(QrLogin::new(self.core.qr_login()?))
    }

    /// The account `token` belongs to, idle until `connect()`.
    pub fn account(&self, token: Arc<Token>) -> Result<Arc<Account>, GatewayError> {
        let _entered = self.runtime.enter();
        Ok(Account::new(
            self.core.account(token.core().clone())?,
            self.runtime,
        ))
    }

    pub async fn save_token(
        &self,
        account: UserId,
        token: Arc<Token>,
    ) -> Result<(), TokenStoreError> {
        let core = self.core.clone();
        run(self.runtime, async move {
            core.save_token(account, token.core()).await
        })
        .await
        .map_err(Into::into)
    }

    pub async fn load_token(&self, account: UserId) -> Result<Option<Arc<Token>>, TokenStoreError> {
        let core = self.core.clone();
        let token = run(self.runtime, async move { core.load_token(account).await }).await?;
        Ok(token.map(Token::new))
    }

    /// Ends the session on Discord, then deletes the stored token, even when Discord can't
    /// be reached.
    pub async fn logout(&self, account: UserId) -> Result<(), LogoutError> {
        let core = self.core.clone();
        run(self.runtime, async move { core.logout(account).await })
            .await
            .map_err(Into::into)
    }

    /// Ends the session `token` belongs to on Discord, without touching the token store. A
    /// token Discord no longer accepts counts as done.
    pub async fn end_session(&self, token: Arc<Token>) -> Result<(), LogoutError> {
        let core = self.core.clone();
        run(
            self.runtime,
            async move { core.end_session(token.core()).await },
        )
        .await
        .map_err(Into::into)
    }

    /// Deletes the stored token without a request, for a token Discord already rejected.
    pub async fn forget_token(&self, account: UserId) -> Result<(), TokenStoreError> {
        let core = self.core.clone();
        run(
            self.runtime,
            async move { core.forget_token(account).await },
        )
        .await
        .map_err(Into::into)
    }
}

impl DiscordClient {
    #[cfg(test)]
    pub(crate) fn from_core(core: akari_core::DiscordClient) -> Result<Arc<Self>, ClientError> {
        Ok(Arc::new(Self {
            core,
            runtime: runtime()?,
        }))
    }

    #[cfg(test)]
    pub(crate) fn runtime(&self) -> &'static Runtime {
        self.runtime
    }
}

fn desktop_os() -> Option<DesktopOs> {
    if cfg!(target_os = "macos") {
        Some(DesktopOs::MacOs)
    } else if cfg!(target_os = "linux") {
        Some(DesktopOs::Linux)
    } else {
        None
    }
}
