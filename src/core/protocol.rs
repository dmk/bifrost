use async_trait::async_trait;
use tokio::net::TcpStream;

/// Core trait for protocol handlers
#[async_trait]
pub trait Protocol: Send + Sync {
    /// Handle a connection by forwarding data between client and backend
    async fn handle_connection(&self, client: TcpStream, backend: TcpStream) -> Result<(), ProtocolError>;

    /// Protocol name
    fn name(&self) -> &str;
}

/// Simple blind forward protocol - forwards raw TCP data
#[derive(Debug)]
pub struct BlindForwardProtocol {
    pub name: String,
}

impl BlindForwardProtocol {
    pub fn new() -> Self {
        Self {
            name: "blind_forward".to_string(),
        }
    }
}

impl Default for BlindForwardProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for BlindForwardProtocol {
    async fn handle_connection(&self, client: TcpStream, backend: TcpStream) -> Result<(), ProtocolError> {
        crate::server::proxy_bidirectional(client, backend)
            .await
            .map_err(|e| ProtocolError::ForwardingFailed(e.to_string()))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Forwarding failed: {0}")]
    ForwardingFailed(String),
    #[error("Protocol parse error: {0}")]
    ParseError(String),
}