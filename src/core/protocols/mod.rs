use async_trait::async_trait;
use tokio::net::TcpStream;

pub mod blind_forward;
pub mod ascii;

pub use blind_forward::BlindForwardProtocol;
pub use ascii::{AsciiProtocol, AsciiCommand, AsciiResponse};

/// Core trait for protocol handlers
#[async_trait]
pub trait Protocol: Send + Sync {
    /// Handle a connection by forwarding data between client and backend
    async fn handle_connection(&self, client: TcpStream, backend: TcpStream) -> Result<(), ProtocolError>;

    /// Protocol name
    fn name(&self) -> &str;
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Forwarding failed: {0}")]
    ForwardingFailed(String),
    #[error("Protocol parse error: {0}")]
    ParseError(String),
}