use std::error::Error as StdError;
use std::fmt;

type BoxError = Box<dyn StdError + Send + Sync>;

/// A network failure below the Discord protocol: connecting, TLS, or a broken connection.
///
/// The underlying error stays available through [`std::error::Error::source`] for logging.
/// It never contains a request URL.
#[derive(Debug, thiserror::Error)]
#[error("{kind}")]
pub struct TransportError {
    kind: TransportErrorKind,
    #[source]
    source: Option<BoxError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    Connect,
    Timeout,
    Tls,
    /// The other side broke the HTTP or WebSocket protocol.
    Protocol,
    Other,
}

impl fmt::Display for TransportErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Connect => "couldn't connect",
            Self::Timeout => "timed out",
            Self::Tls => "TLS failed",
            Self::Protocol => "protocol error",
            Self::Other => "network error",
        })
    }
}

impl TransportError {
    pub fn kind(&self) -> TransportErrorKind {
        self.kind
    }

    pub(crate) fn new(kind: TransportErrorKind, source: impl Into<BoxError>) -> Self {
        Self {
            kind,
            source: Some(source.into()),
        }
    }

    pub(crate) fn from_reqwest(err: reqwest::Error) -> Self {
        let kind = if err.is_timeout() {
            TransportErrorKind::Timeout
        } else if err.is_connect() {
            TransportErrorKind::Connect
        } else if err.is_body() || err.is_decode() {
            TransportErrorKind::Protocol
        } else {
            TransportErrorKind::Other
        };
        Self::new(kind, err.without_url())
    }

    pub(crate) fn tls(err: rustls::Error) -> Self {
        Self::new(TransportErrorKind::Tls, err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reqwest_failures_drop_the_url() {
        let err = reqwest::Client::new()
            .get("http://127.0.0.1:1/auth?ticket=secret-value")
            .send()
            .await
            .unwrap_err();

        let err = TransportError::from_reqwest(err);

        assert_eq!(err.kind(), TransportErrorKind::Connect);
        assert!(!format!("{err:?}").contains("secret-value"), "{err:?}");
        assert!(!err.to_string().contains("secret-value"));
    }

    #[test]
    fn display_names_the_kind() {
        let err = TransportError::new(TransportErrorKind::Tls, "bad certificate");

        assert_eq!(err.to_string(), "TLS failed");
        assert_eq!(
            std::error::Error::source(&err).map(ToString::to_string),
            Some("bad certificate".to_owned())
        );
    }
}
