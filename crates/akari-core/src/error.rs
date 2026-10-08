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

/// Where a JSON payload failed to parse. serde's message isn't kept, because it can quote
/// values from the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{kind} error at line {line}, column {column}")]
pub struct JsonError {
    kind: JsonErrorKind,
    line: usize,
    column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum JsonErrorKind {
    /// Not valid JSON.
    Syntax,
    /// Valid JSON in an unexpected shape.
    Data,
    /// The input ended early.
    Eof,
    Io,
}

impl fmt::Display for JsonErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Syntax => "syntax",
            Self::Data => "data",
            Self::Eof => "unexpected end",
            Self::Io => "I/O",
        })
    }
}

impl JsonError {
    pub fn kind(&self) -> JsonErrorKind {
        self.kind
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }
}

impl From<&serde_json::Error> for JsonError {
    fn from(err: &serde_json::Error) -> Self {
        use serde_json::error::Category;

        Self {
            kind: match err.classify() {
                Category::Syntax => JsonErrorKind::Syntax,
                Category::Data => JsonErrorKind::Data,
                Category::Eof => JsonErrorKind::Eof,
                Category::Io => JsonErrorKind::Io,
            },
            line: err.line(),
            column: err.column(),
        }
    }
}

impl From<serde_json::Error> for JsonError {
    fn from(err: serde_json::Error) -> Self {
        Self::from(&err)
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

    pub(crate) fn from_tungstenite(err: tokio_tungstenite::tungstenite::Error) -> Self {
        use tokio_tungstenite::tungstenite::Error;

        let kind = match &err {
            Error::Io(_) | Error::ConnectionClosed | Error::AlreadyClosed => {
                TransportErrorKind::Connect
            }
            Error::Tls(_) => TransportErrorKind::Tls,
            Error::Http(_)
            | Error::HttpFormat(_)
            | Error::Protocol(_)
            | Error::Capacity(_)
            | Error::Utf8(_)
            | Error::AttackAttempt => TransportErrorKind::Protocol,
            _ => TransportErrorKind::Other,
        };
        Self::new(kind, err)
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
