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

/// Optimize TCP socket for low latency
fn optimize_socket_for_latency(stream: &TcpStream) {
    // Disable Nagle's algorithm for lower latency (available on tokio TcpStream)
    let _ = stream.set_nodelay(true);

    // Additional socket optimizations using socket2
    let socket_ref = socket2::SockRef::try_from(stream).unwrap();
    // Set socket to reuse address for faster reconnection
    let _ = socket_ref.set_reuse_address(true);

    // Optimize send/receive buffer sizes for cache workloads
    // 32KB buffers balance latency vs throughput for cache operations
    let _ = socket_ref.set_send_buffer_size(32768);
    let _ = socket_ref.set_recv_buffer_size(32768);
}

/// Simple memcached backend implementation with TCP optimizations
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
        let stream = TcpStream::connect(&self.server)
            .await
            .map_err(|e| BackendError::ConnectionFailed(e.to_string()))?;

        // Apply socket optimizations (best-effort)
        optimize_socket_for_latency(&stream);

        Ok(stream)
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