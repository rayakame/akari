// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

pub mod gateway;
pub mod remote_auth;

use std::sync::Arc;

use akari_core::model::{Snowflake, UserMarker};
use akari_core::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
use akari_core::{DiscordClient, Endpoints, Token, TokenStore, TokenStoreError};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct NoStore;

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

pub fn properties() -> ClientProperties {
    let host = HostInfo {
        os: DesktopOs::MacOs,
        os_version: "25.0.0".to_owned(),
        arch: Arch::Arm64,
        system_locale: "en-US".to_owned(),
    };
    ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs))
}

pub async fn rest_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v9/experiments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fingerprint": "fp.1"})))
        .mount(&server)
        .await;
    server
}

pub fn client(rest: &MockServer, remote_auth: &remote_auth::RemoteAuthServer) -> DiscordClient {
    let endpoints = Endpoints {
        api: format!("{}/api/v9/", rest.uri()),
        remote_auth: remote_auth.url(),
        allow_plaintext: true,
        ..Endpoints::default()
    };
    DiscordClient::with_endpoints(properties(), Arc::new(NoStore), endpoints)
        .unwrap_or_else(|err| panic!("client setup failed: {err}"))
}
