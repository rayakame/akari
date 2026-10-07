use std::sync::Arc;

use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt as _;

use crate::error::TransportError;

// No ALPN, so connections negotiate HTTP/1.1 as WebSockets need; REST sets h2 on a copy.
pub(crate) fn client_config() -> Result<ClientConfig, TransportError> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    Ok(ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(TransportError::tls)?
        .with_platform_verifier()
        .map_err(TransportError::tls)?
        .with_no_client_auth())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_aws_lc_and_offers_no_alpn() {
        let config = client_config().unwrap();

        let suites = |provider: &rustls::crypto::CryptoProvider| {
            provider
                .cipher_suites
                .iter()
                .map(|suite| suite.suite())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            suites(config.crypto_provider()),
            suites(&rustls::crypto::aws_lc_rs::default_provider())
        );
        assert!(config.alpn_protocols.is_empty());
    }
}
