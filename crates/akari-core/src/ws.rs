use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

use crate::error::{TransportError, TransportErrorKind};

pub(crate) type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn connect(
    url: &str,
    headers: &[(&'static str, &str)],
    tls: Arc<rustls::ClientConfig>,
    max_message: usize,
) -> Result<WsStream, TransportError> {
    let mut request = url
        .into_client_request()
        .map_err(TransportError::from_tungstenite)?;
    for (name, value) in headers {
        let value = HeaderValue::from_str(value)
            .map_err(|err| TransportError::new(TransportErrorKind::Other, err))?;
        request
            .headers_mut()
            .insert(HeaderName::from_static(name), value);
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(max_message))
        .max_frame_size(Some(max_message));
    let connect = tokio_tungstenite::connect_async_tls_with_config(
        request,
        Some(config),
        false,
        Some(Connector::Rustls(tls)),
    );
    match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Ok(Ok((stream, _))) => Ok(stream),
        Ok(Err(err)) => Err(TransportError::from_tungstenite(err)),
        Err(elapsed) => Err(TransportError::new(TransportErrorKind::Timeout, elapsed)),
    }
}
