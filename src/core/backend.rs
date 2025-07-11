use async_trait::async_trait;
use tokio::net::TcpStream;

/// Core trait for cache backends (memcached servers)
#[async_trait]
pub trait Backend: Send + Sync {
    /// Connect to this backend
    async fn connect(&self) -> Result<TcpStream, BackendError>;

    /// Get backend identifier/name
    fn name(&self) -> &str;

    /// Get the server address
    fn server(&self) -> &str;
}

/// Simple memcached backend implementation
#[derive(Debug, Clone)]
pub struct MemcachedBackend {
    pub name: String,
    pub server: String,
}

impl MemcachedBackend {
    pub fn new(name: String, server: String) -> Self {
        Self { name, server }
    }
}

#[async_trait]
impl Backend for MemcachedBackend {
    async fn connect(&self) -> Result<TcpStream, BackendError> {
        TcpStream::connect(&self.server)
            .await
            .map_err(|e| BackendError::ConnectionFailed(e.to_string()))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn server(&self) -> &str {
        &self.server
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Backend unavailable")]
    Unavailable,
}