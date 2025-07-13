use async_trait::async_trait;
use tokio::net::TcpStream;
use super::{Protocol, ProtocolError};

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