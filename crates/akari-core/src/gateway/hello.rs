use serde::Deserialize;

/// Opcode 10, the first message on a new connection.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hello {
    /// Milliseconds between heartbeats.
    pub heartbeat_interval: u64,
    /// The gateway servers that handled the connection, for debugging.
    #[serde(rename = "_trace", default)]
    pub trace: Vec<String>,
}
