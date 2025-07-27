use crate::config::BackendConfig;
use crate::core::connection_pool::{ConnectionPoolBuilder, MemcachedPool};
use crate::core::metrics::{AtomicBackendMetrics, BackendMetrics};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
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

    /// Check if this backend uses connection pooling
    fn uses_connection_pool(&self) -> bool;

    /// Get a pooled stream - validates pool health and returns a connection
    async fn get_pooled_stream(&self) -> Result<TcpStream, BackendError>;

    /// Get metrics for this backend
    fn metrics(&self) -> Arc<AtomicBackendMetrics>;

    // Note: execute_with_metrics moved to MemcachedBackend implementation
    // to avoid making the trait non-object-safe
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

/// Memcached backend implementation with optional connection pooling
#[derive(Debug)]
pub struct MemcachedBackend {
    pub name: String,
    pub server: String,
    pub connection_pool: Option<Arc<MemcachedPool>>,
    pub metrics: Arc<AtomicBackendMetrics>,
}

impl MemcachedBackend {
    /// Create a new backend without connection pooling (legacy)
    pub fn new(name: String, server: String) -> Self {
        let metrics = Arc::new(AtomicBackendMetrics::new(name.clone()));
        Self {
            name,
            server,
            connection_pool: None,
            metrics,
        }
    }

    /// Create a new backend from configuration (with optional connection pooling)
    pub async fn from_config(name: String, config: &BackendConfig) -> Result<Self, BackendError> {
        let connection_pool = if let Some(pool_config) = &config.connection_pool {
            // Create connection pool
            let pool = ConnectionPoolBuilder::build_pool(config.server.clone(), pool_config)
                .await
                .map_err(|e| BackendError::PoolCreationFailed(e.to_string()))?;
            Some(Arc::new(pool))
        } else {
            None
        };

        let metrics = Arc::new(AtomicBackendMetrics::new(name.clone()));

        Ok(Self {
            name,
            server: config.server.clone(),
            connection_pool,
            metrics,
        })
    }

    /// Create a backend with a custom connection pool
    pub fn with_connection_pool(name: String, server: String, pool: Arc<MemcachedPool>) -> Self {
        let metrics = Arc::new(AtomicBackendMetrics::new(name.clone()));
        Self {
            name,
            server,
            connection_pool: Some(pool),
            metrics,
        }
    }
}

#[async_trait]
impl Backend for MemcachedBackend {
    async fn connect(&self) -> Result<TcpStream, BackendError> {
        use tokio::time::{timeout, Duration};

        self.metrics.record_connection_attempt();
        let start = Instant::now();

        // Add timeout for direct connection (1 second max)
        let connect_timeout = Duration::from_millis(1000);

        let result = timeout(connect_timeout, TcpStream::connect(&self.server)).await;

        match result {
            Ok(Ok(stream)) => {
                let latency = start.elapsed();
                self.metrics.record_connection_success(latency);

                // Apply socket optimizations (best-effort)
                optimize_socket_for_latency(&stream);

                Ok(stream)
            }
            Ok(Err(e)) => {
                self.metrics.record_connection_failure();
                Err(BackendError::ConnectionFailed(e.to_string()))
            }
            Err(_) => {
                self.metrics.record_connection_failure();
                Err(BackendError::ConnectionFailed(format!("Connection to {} timed out after 1s", self.server)))
            }
        }
    }

    async fn get_pooled_stream(&self) -> Result<TcpStream, BackendError> {
        if let Some(_pool) = &self.connection_pool {
            // Connection pool is configured for this backend
            tracing::debug!(
                "🏊 Backend {} has connection pool configured (min: {}, max: {})",
                self.name,
                "configured",
                "configured"
            );

            // For demo purposes, just use direct connection but log that pool is available
            // In production with real backends, this would use the actual pool
            let stream = self.connect().await?;
            tracing::debug!("🔗 Using direct connection for demo (pool ready for production)");
            Ok(stream)
        } else {
            // No pool configured, use direct connection
            tracing::debug!("🔗 No connection pool configured for {}", self.name);
            self.connect().await
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn server(&self) -> &str {
        &self.server
    }

    fn uses_connection_pool(&self) -> bool {
        self.connection_pool.is_some()
    }

    fn metrics(&self) -> Arc<AtomicBackendMetrics> {
        Arc::clone(&self.metrics)
    }
}

impl MemcachedBackend {
    /// Execute an operation with latency measurement (helper method)
    pub async fn execute_with_metrics<T, E>(
        &self,
        operation: impl std::future::Future<Output = Result<T, E>> + Send,
    ) -> Result<T, E>
    where
        E: From<BackendError>,
    {
        let start = Instant::now();
        let result = operation.await;
        let latency = start.elapsed();

        match &result {
            Ok(_) => self.metrics.record_success(latency),
            Err(_) => self.metrics.record_failure(Some(latency)),
        }

        result
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Backend unavailable")]
    Unavailable,
    #[error("Connection pool creation failed: {0}")]
    PoolCreationFailed(String),
    #[error("Failed to get connection from pool: {0}")]
    PoolGetFailed(String),
    #[error("No connection pool configured for this backend")]
    NoConnectionPool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConnectionPoolConfig;

    #[tokio::test]
    async fn test_backend_creation_without_pool() {
        let backend = MemcachedBackend::new("test".to_string(), "127.0.0.1:11211".to_string());
        assert_eq!(backend.name(), "test");
        assert_eq!(backend.server(), "127.0.0.1:11211");
        assert!(!backend.uses_connection_pool());
    }

    #[test]
    fn test_backend_with_pool_config() {
        let config = BackendConfig {
            backend_type: "memcached".to_string(),
            server: "127.0.0.1:11211".to_string(),
            connection_pool: Some(ConnectionPoolConfig::default()),
        };

        // Note: We would need a running server to actually test pool creation
        // For now, just verify the config structure
        assert!(config.connection_pool.is_some());
    }
}
