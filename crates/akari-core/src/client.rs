use std::fmt;
use std::sync::Arc;

use reqwest::Url;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use tokio::sync::OnceCell;

use crate::auth::{LogoutError, PasswordLogin, QrLogin};
use crate::error::TransportError;
use crate::model::{Snowflake, UserMarker};
use crate::properties::ClientProperties;
use crate::rest::{BuildError, RequestExtras, RestClient};
use crate::token_store::{TokenStore, TokenStoreError};
use crate::{Token, tls};

/// The shared context for talking to Discord, one per app. Logins and connections are
/// created from it, so all of them present the same client. Cheap to clone; the async
/// methods need a Tokio runtime.
#[derive(Clone)]
pub struct DiscordClient {
    inner: Arc<Inner>,
}

struct Inner {
    tls: Arc<rustls::ClientConfig>,
    endpoints: Endpoints,
    properties: ClientProperties,
    rest: RestClient,
    fingerprint: OnceCell<String>,
    store: Arc<dyn TokenStore>,
}

/// Where Discord lives. The default is Discord's production service; tests and canary
/// builds point elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// REST base including the API version.
    pub api: String,
    pub gateway: String,
    pub remote_auth: String,
    /// The `Origin` header the remote auth gateway requires.
    pub origin: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            api: "https://discord.com/api/v9/".to_owned(),
            gateway: "wss://gateway.discord.gg/".to_owned(),
            remote_auth: "wss://remote-auth-gateway.discord.gg/?v=2".to_owned(),
            origin: "https://discord.com".to_owned(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("couldn't set up TLS")]
    Tls(#[source] TransportError),
    #[error("couldn't set up the HTTP client")]
    Http(#[source] TransportError),
    #[error("invalid {0} endpoint")]
    InvalidEndpoint(&'static str),
    #[error("a client property can't be sent as the {0} header")]
    InvalidProperties(&'static str),
}

impl DiscordClient {
    /// A client for Discord's production service.
    pub fn new(
        properties: ClientProperties,
        token_store: Arc<dyn TokenStore>,
    ) -> Result<Self, ClientError> {
        Self::with_endpoints(properties, token_store, Endpoints::default())
    }

    pub fn with_endpoints(
        properties: ClientProperties,
        token_store: Arc<dyn TokenStore>,
        endpoints: Endpoints,
    ) -> Result<Self, ClientError> {
        let mut api = endpoints.api.clone();
        // Url::join replaces the last path segment unless the base ends with a slash.
        if !api.ends_with('/') {
            api.push('/');
        }
        let api = Url::parse(&api).map_err(|_| ClientError::InvalidEndpoint("api"))?;
        Url::parse(&endpoints.gateway).map_err(|_| ClientError::InvalidEndpoint("gateway"))?;
        Url::parse(&endpoints.remote_auth)
            .map_err(|_| ClientError::InvalidEndpoint("remote auth"))?;
        HeaderValue::from_str(&endpoints.origin)
            .map_err(|_| ClientError::InvalidEndpoint("origin"))?;

        let tls = tls::client_config().map_err(ClientError::Tls)?;
        let rest = RestClient::new(&tls, api, &properties).map_err(|err| match err {
            BuildError::InvalidHeader(name) => ClientError::InvalidProperties(name),
            BuildError::Transport(err) => ClientError::Http(err),
        })?;
        Ok(Self {
            inner: Arc::new(Inner {
                tls: Arc::new(tls),
                endpoints,
                properties,
                rest,
                fingerprint: OnceCell::new(),
                store: token_store,
            }),
        })
    }

    pub fn properties(&self) -> &ClientProperties {
        &self.inner.properties
    }

    /// Starts an email/password login. Several logins can run at the same time.
    pub fn password_login(&self) -> PasswordLogin {
        PasswordLogin::new(self.clone())
    }

    /// Starts a QR code login in the background; must be called inside a Tokio runtime.
    /// It can run next to a password login.
    pub fn qr_login(&self) -> QrLogin {
        QrLogin::start(self.clone())
    }

    pub async fn save_token(
        &self,
        account: Snowflake<UserMarker>,
        token: &Token,
    ) -> Result<(), TokenStoreError> {
        let store = self.inner.store.clone();
        let token = token.clone();
        blocking(move || store.save(account, &token)).await
    }

    pub async fn load_token(
        &self,
        account: Snowflake<UserMarker>,
    ) -> Result<Option<Token>, TokenStoreError> {
        let store = self.inner.store.clone();
        blocking(move || store.load(account)).await
    }

    /// Ends the session on Discord, then deletes the stored token.
    ///
    /// The token is deleted even when Discord can't be reached; a token Discord no longer
    /// accepts counts as logged out.
    pub async fn logout(&self, account: Snowflake<UserMarker>) -> Result<(), LogoutError> {
        let token = self
            .load_token(account)
            .await
            .map_err(LogoutError::Storage)?
            .ok_or(LogoutError::NotLoggedIn)?;
        let extras = RequestExtras {
            authorization: Some(&token),
            ..RequestExtras::default()
        };
        let remote = self
            .inner
            .rest
            .post("auth/logout", &serde_json::Map::new(), &extras)
            .await;

        let store = self.inner.store.clone();
        blocking(move || store.delete(account))
            .await
            .map_err(LogoutError::Storage)?;
        match remote {
            Ok(()) => Ok(()),
            Err(err) => LogoutError::from_rest(err).map_or(Ok(()), Err),
        }
    }

    pub(crate) fn rest(&self) -> &RestClient {
        &self.inner.rest
    }

    pub(crate) fn tls(&self) -> Arc<rustls::ClientConfig> {
        self.inner.tls.clone()
    }

    pub(crate) fn endpoints(&self) -> &Endpoints {
        &self.inner.endpoints
    }

    pub(crate) async fn fingerprint(&self) -> Option<&str> {
        #[derive(Deserialize)]
        struct Experiments {
            fingerprint: Option<String>,
        }

        let fetched = self
            .inner
            .fingerprint
            .get_or_try_init(|| async {
                let experiments: Experiments = self
                    .inner
                    .rest
                    .get_json("experiments", &RequestExtras::default())
                    .await
                    .map_err(|err| err.summary())?;
                experiments
                    .fingerprint
                    .ok_or_else(|| "no fingerprint in the response".to_owned())
            })
            .await;
        match fetched {
            Ok(fingerprint) => Some(fingerprint),
            Err(err) => {
                tracing::warn!(error = %err, "couldn't get a login fingerprint");
                None
            }
        }
    }
}

impl fmt::Debug for DiscordClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscordClient")
            .field("properties", &self.inner.properties)
            .finish_non_exhaustive()
    }
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, TokenStoreError> + Send + 'static,
) -> Result<T, TokenStoreError> {
    tokio::task::spawn_blocking(job).await.unwrap_or_else(|_| {
        Err(TokenStoreError::Backend(
            "the storage call panicked".to_owned(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::model::{Snowflake, UserMarker};
    use crate::properties::{Arch, ClientBuild, DesktopOs, HostInfo};
    use crate::{Token, TokenStoreError};

    struct NoStore;

    impl TokenStore for NoStore {
        fn load(&self, _: Snowflake<UserMarker>) -> Result<Option<Token>, TokenStoreError> {
            Ok(None)
        }
        fn save(&self, _: Snowflake<UserMarker>, _: &Token) -> Result<(), TokenStoreError> {
            Ok(())
        }
        fn delete(&self, _: Snowflake<UserMarker>) -> Result<(), TokenStoreError> {
            Ok(())
        }
    }

    fn properties() -> ClientProperties {
        let host = HostInfo {
            os: DesktopOs::Linux,
            os_version: "6.8.0".to_owned(),
            arch: Arch::X64,
            system_locale: "en-US".to_owned(),
        };
        ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::Linux))
    }

    fn client(server: &MockServer) -> DiscordClient {
        let endpoints = Endpoints {
            api: format!("{}/api/v9", server.uri()),
            ..Endpoints::default()
        };
        DiscordClient::with_endpoints(properties(), Arc::new(NoStore), endpoints).unwrap()
    }

    #[tokio::test]
    async fn fingerprint_is_fetched_once() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v9/experiments"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"fingerprint": "1.abc", "assignments": []})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = client(&server);

        assert_eq!(client.fingerprint().await, Some("1.abc"));
        assert_eq!(client.clone().fingerprint().await, Some("1.abc"));
    }

    #[tokio::test]
    async fn failed_fingerprint_is_retried_next_time() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v9/experiments"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/api/v9/experiments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fingerprint": "2.def"})))
            .mount(&server)
            .await;
        let client = client(&server);

        assert_eq!(client.fingerprint().await, None);
        assert_eq!(client.fingerprint().await, Some("2.def"));
    }

    #[test]
    fn default_endpoints_are_discords() {
        let endpoints = Endpoints::default();

        assert_eq!(endpoints.api, "https://discord.com/api/v9/");
        assert_eq!(endpoints.gateway, "wss://gateway.discord.gg/");
        assert_eq!(
            endpoints.remote_auth,
            "wss://remote-auth-gateway.discord.gg/?v=2"
        );
        assert_eq!(endpoints.origin, "https://discord.com");
    }

    #[test]
    fn malformed_endpoints_are_rejected() {
        let endpoints = Endpoints {
            gateway: "not a url".to_owned(),
            ..Endpoints::default()
        };

        let err =
            DiscordClient::with_endpoints(properties(), Arc::new(NoStore), endpoints).unwrap_err();

        assert!(matches!(err, ClientError::InvalidEndpoint("gateway")));
    }

    #[test]
    fn properties_that_cant_be_headers_are_rejected() {
        let mut properties = properties();
        properties.system_locale = "en-US\n".to_owned();

        let err = DiscordClient::new(properties, Arc::new(NoStore)).unwrap_err();

        assert!(matches!(
            err,
            ClientError::InvalidProperties("x-discord-locale")
        ));
    }
}
