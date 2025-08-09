use crate::config::BackendConfig;
use crate::core::connection_pool::{MemcachedConnectionManager, MemcachedPool};
use crate::core::metrics::{AtomicBackendMetrics, BackendMetrics};
use async_trait::async_trait;
use bb8::Pool;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    async fn get_pooled_stream(&self) -> Result<bb8::PooledConnection<'_, MemcachedConnectionManager>, BackendError>;

    /// Get metrics for this backend
    fn metrics(&self) -> Arc<AtomicBackendMetrics>;
}

/// Optimize TCP socket for low latency
pub fn optimize_socket_for_latency(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    let socket_ref = socket2::SockRef::try_from(stream).unwrap();
    let _ = socket_ref.set_reuse_address(true);
    let _ = socket_ref.set_send_buffer_size(32768);
    let _ = socket_ref.set_recv_buffer_size(32768);
    // Enable TCP keepalive to avoid idle disconnects and detect dead peers
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::time::Duration;
        use socket2::TcpKeepalive;
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(30))
            .with_interval(Duration::from_secs(10))
            .with_retries(3);
        let _ = socket_ref.set_tcp_keepalive(&keepalive);
    }
}

/// Memcached backend implementation with a robust connection pool.
pub struct MemcachedBackend {
    pub name: String,
    pub server: String,
    pub connection_pool: Option<MemcachedPool>,
    pub metrics: Arc<AtomicBackendMetrics>,
}

impl MemcachedBackend {
    /// Create a new backend without connection pooling.
    pub fn new(name: String, server: String) -> Self {
        let metrics = Arc::new(AtomicBackendMetrics::new(name.clone()));
        Self {
            name,
            server,
            connection_pool: None,
            metrics,
        }
    }

    /// Create a new backend from configuration, with an optional connection pool.
    pub async fn from_config(name: String, config: &BackendConfig) -> Result<Self, BackendError> {
        let metrics = Arc::new(AtomicBackendMetrics::new(name.clone()));
        let connection_pool = if let Some(pool_config) = &config.connection_pool {
            let manager = MemcachedConnectionManager::new(config.server.clone());
            let pool = Pool::builder()
                .max_size(pool_config.max_connections)
                .min_idle(Some(pool_config.min_connections))
                .connection_timeout(Duration::from_secs(pool_config.connection_timeout_secs))
                .idle_timeout(Some(Duration::from_secs(pool_config.idle_timeout_secs)))
                .max_lifetime(Some(Duration::from_secs(pool_config.max_lifetime_secs)))
                .build(manager)
                .await
                .map_err(|e| BackendError::PoolCreationFailed(e.to_string()))?;
            Some(pool)
        } else {
            None
        };

        Ok(Self {
            name,
            server: config.server.clone(),
            connection_pool,
            metrics,
        })
    }
}

#[async_trait]
impl Backend for MemcachedBackend {
    /// Establishes a direct, non-pooled connection to the backend.
    async fn connect(&self) -> Result<TcpStream, BackendError> {
        use tokio::time::timeout;

        tracing::debug!("[{}] creating direct connection (not using pool)", self.name);
        self.metrics.record_connection_attempt();
        let start = Instant::now();
        let connect_timeout = Duration::from_millis(5000);

        let result = timeout(connect_timeout, TcpStream::connect(&self.server)).await;

        match result {
            Ok(Ok(stream)) => {
                let latency = start.elapsed();
                self.metrics.record_connection_success(latency);
                optimize_socket_for_latency(&stream);
                Ok(stream)
            }
            Ok(Err(e)) => {
                self.metrics.record_connection_failure();
                Err(BackendError::ConnectionFailed(e.to_string()))
            }
            Err(_) => {
                self.metrics.record_connection_failure();
                Err(BackendError::ConnectionFailed(format!(
                    "Connection to {} timed out after 5s",
                    self.server
                )))
            }
        }
    }

    /// Retrieves a connection from the pool.
    async fn get_pooled_stream(
        &self,
    ) -> Result<bb8::PooledConnection<'_, MemcachedConnectionManager>, BackendError> {
        if let Some(pool) = &self.connection_pool {
            if tracing::enabled!(tracing::Level::DEBUG) {
                let pool_state = pool.state();
                tracing::debug!(
                    "[{}] getting connection from bb8 pool: connections={}, idle={}",
                    self.name,
                    pool_state.connections,
                    pool_state.idle_connections
                );
            }

            self.metrics.record_connection_attempt();
            let start = Instant::now();
            match pool.get().await {
                Ok(conn) => {
                    let latency = start.elapsed();
                    self.metrics.record_connection_success(latency);
                    if tracing::enabled!(tracing::Level::DEBUG) {
                        let pool_state_after = pool.state();
                        tracing::debug!(
                            "[{}] bb8 pool connection acquired in {}ms: connections={}, idle={}",
                            self.name,
                            latency.as_millis(),
                            pool_state_after.connections,
                            pool_state_after.idle_connections
                        );
                    }
                    Ok(conn)
                }
                Err(e) => {
                    self.metrics.record_connection_failure();
                    tracing::error!("[{}] bb8 pool connection failed: {}", self.name, e);
                    Err(BackendError::PoolGetFailed(e.to_string()))
                }
            }
        } else {
            tracing::warn!("[{}] no connection pool configured; should not happen", self.name);
            Err(BackendError::NoConnectionPool)
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
    /// Executes an operation with latency measurement.
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

        if result.is_ok() {
            self.metrics.record_success(latency);
        } else {
            self.metrics.record_failure(Some(latency));
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
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_backend_creation_without_pool() {
        let backend = MemcachedBackend::new("test".to_string(), "127.0.0.1:11211".to_string());
        assert_eq!(backend.name(), "test");
        assert_eq!(backend.server(), "127.0.0.1:11211");
        assert!(!backend.uses_connection_pool());
    }

    #[tokio::test]
    async fn test_backend_with_pool_config() {
        // Mock a TCP server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let config = BackendConfig {
            backend_type: "memcached".to_string(),
            server: addr.to_string(),
            connection_pool: Some(ConnectionPoolConfig {
                min_connections: 1,
                max_connections: 2,
                ..Default::default()
            }),
        };

        let backend = MemcachedBackend::from_config("test_pool".to_string(), &config)
            .await
            .unwrap();

        assert!(backend.uses_connection_pool());
        let pool = backend.connection_pool.as_ref().unwrap();
        assert!(pool.state().connections >= 1);
    }

    #[tokio::test]
    async fn test_get_pooled_stream_success() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Start a task to accept connections in the background
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    // Just keep the connection open, no need to handle the protocol
                    tokio::spawn(async move {
                        let _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                        drop(stream);
                    });
                }
            }
        });

        let config = BackendConfig {
            backend_type: "memcached".to_string(),
            server: addr.to_string(),
            connection_pool: Some(ConnectionPoolConfig {
                connection_timeout_secs: 1, // Short timeout for test
                ..Default::default()
            }),
        };

        let backend = MemcachedBackend::from_config("test_pool".to_string(), &config)
            .await
            .unwrap();

        // The first get should succeed and create a connection.
        let conn_result = backend.get_pooled_stream().await;
        if let Err(ref e) = conn_result {
            println!("Connection error: {:?}", e);
        }
        assert!(conn_result.is_ok());

        // The pool should have at least one connection (bb8 pre-populates with min_connections)
        let pool = backend.connection_pool.as_ref().unwrap();
        assert!(pool.state().connections >= 1);
    }

    #[tokio::test]
    async fn test_get_pooled_stream_failure() {
        // Use an invalid address to trigger a connection failure.
        let config = BackendConfig {
            backend_type: "memcached".to_string(),
            server: "127.0.0.1:1".to_string(), // Invalid port
            connection_pool: Some(ConnectionPoolConfig {
                connection_timeout_secs: 1,
                ..Default::default()
            }),
        };

        // We expect pool creation to fail here if it tries to connect eagerly,
        // or for `get_pooled_stream` to fail if it connects lazily.
        // `bb8` connects lazily by default.
        let backend_result = MemcachedBackend::from_config("test_fail".to_string(), &config).await;

        // Depending on bb8's build strategy, this might not fail.
        // The real test is trying to get a connection.
        if let Ok(backend) = backend_result {
            let conn_result = backend.get_pooled_stream().await;
            assert!(conn_result.is_err());
            if let Err(BackendError::PoolGetFailed(_)) = conn_result {
                // This is the expected outcome.
            } else {
                panic!("Expected PoolGetFailed error");
            }
        }
    }
}
