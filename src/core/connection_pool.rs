use crate::config::ConnectionPoolConfig;
use async_trait::async_trait;
use bb8::{Pool, PooledConnection};
use std::time::Duration;
use tokio::net::TcpStream;

/// Connection manager for bb8 that creates TCP connections to memcached servers
#[derive(Debug, Clone)]
pub struct MemcachedConnectionManager {
    server_address: String,
}

impl MemcachedConnectionManager {
    pub fn new(server_address: String) -> Self {
        Self { server_address }
    }
}

#[async_trait]
impl bb8::ManageConnection for MemcachedConnectionManager {
    type Connection = TcpStream;
    type Error = ConnectionError;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let stream = TcpStream::connect(&self.server_address)
            .await
            .map_err(|e| ConnectionError::ConnectionFailed(e.to_string()))?;

        // Apply socket optimizations
        optimize_socket_for_latency(&stream);

        Ok(stream)
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        // For TCP connections, we can check if the socket is still readable/writable
        // This is a simple check - in a real implementation you might want to send a ping
        if conn.readable().await.is_ok() {
            Ok(())
        } else {
            Err(ConnectionError::ConnectionInvalid)
        }
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        // For TCP connections, bb8 will handle broken connections through is_valid
        false
    }
}

/// Optimize TCP socket for low latency (moved from backend.rs)
fn optimize_socket_for_latency(stream: &TcpStream) {
    // Disable Nagle's algorithm for lower latency
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

/// Type alias for our connection pool
pub type MemcachedPool = Pool<MemcachedConnectionManager>;

/// Type alias for a pooled connection (without static lifetime constraint)
pub type PooledMemcachedConnection<'a> = PooledConnection<'a, MemcachedConnectionManager>;

/// Builder for creating connection pools
pub struct ConnectionPoolBuilder;

impl ConnectionPoolBuilder {
    /// Create a new connection pool with the given configuration
    pub async fn build_pool(
        server_address: String,
        config: &ConnectionPoolConfig,
    ) -> Result<MemcachedPool, ConnectionError> {
        let manager = MemcachedConnectionManager::new(server_address);

        let pool = Pool::builder()
            .min_idle(Some(config.min_connections))
            .max_size(config.max_connections)
            .connection_timeout(Duration::from_secs(config.connection_timeout_secs))
            .idle_timeout(Some(Duration::from_secs(config.idle_timeout_secs)))
            .max_lifetime(Some(Duration::from_secs(config.max_lifetime_secs)))
            .build(manager)
            .await
            .map_err(|e| ConnectionError::PoolCreationFailed(e.to_string()))?;

        Ok(pool)
    }

    /// Create a pool with default configuration
    pub async fn build_default_pool(
        server_address: String,
    ) -> Result<MemcachedPool, ConnectionError> {
        let config = ConnectionPoolConfig::default();
        Self::build_pool(server_address, &config).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Connection pool creation failed: {0}")]
    PoolCreationFailed(String),
    #[error("Failed to get connection from pool: {0}")]
    PoolGetFailed(String),
    #[error("Connection is invalid")]
    ConnectionInvalid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_pool_config_defaults() {
        let config = ConnectionPoolConfig::default();
        assert_eq!(config.min_connections, 2);
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.connection_timeout_secs, 5);
        assert_eq!(config.idle_timeout_secs, 300);
        assert_eq!(config.max_lifetime_secs, 3600);
    }

    #[tokio::test]
    async fn test_connection_manager_creation() {
        let manager = MemcachedConnectionManager::new("127.0.0.1:11211".to_string());
        assert_eq!(manager.server_address, "127.0.0.1:11211");
    }

    // Note: Additional integration tests would require a running memcached server
}
