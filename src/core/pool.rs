use super::backend::{Backend, BackendError};
use super::strategy::Strategy;
use async_trait::async_trait;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;

/// Core trait for backend pools - collections of backends with selection strategies
#[async_trait]
pub trait Pool: Send + Sync {
    /// Select a backend from this pool for the given key
    async fn select_backend(&self, key: &str) -> Result<&dyn Backend, PoolError>;

    /// Get pool name/identifier
    fn name(&self) -> &str;

    /// Get all backends in this pool
    fn backends(&self) -> &[Box<dyn Backend>];

    /// Get the strategy used by this pool
    fn strategy(&self) -> &dyn Strategy;

    /// Check if pool has any healthy backends
    fn has_healthy_backends(&self) -> bool;

    /// Check if this pool supports concurrent requests (miss failover)
    fn supports_concurrent_requests(&self) -> bool {
        false // Default: most pools don't support concurrent requests
    }

    /// Handle concurrent requests (optional, default implementation delegates to select_backend)
    async fn handle_concurrent_request(
        &self,
        key: &str,
        _request_data: &[u8],
    ) -> Result<Vec<u8>, PoolError> {
        // Default implementation: just select one backend and handle normally
        let _backend = self.select_backend(key).await?;
        // This would need to be implemented by the protocol layer
        // For now, return an error to indicate this pool doesn't support concurrent requests
        Err(PoolError::ConcurrentRequestsNotSupported)
    }
}

/// Basic pool implementation that combines backends with a strategy
pub struct BasicPool {
    pub name: String,
    pub backends: Vec<Box<dyn Backend>>,
    pub strategy: Box<dyn Strategy>,
}

impl BasicPool {
    pub fn new(name: String, backends: Vec<Box<dyn Backend>>, strategy: Box<dyn Strategy>) -> Self {
        Self {
            name,
            backends,
            strategy,
        }
    }
}

#[async_trait]
impl Pool for BasicPool {
    async fn select_backend(&self, _key: &str) -> Result<&dyn Backend, PoolError> {
        // Use the strategy to select from available backends
        let selected = self
            .strategy
            .select_backend(&self.backends)
            .await
            .ok_or(PoolError::NoBackendsAvailable)?;

        Ok(selected.as_ref())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn backends(&self) -> &[Box<dyn Backend>] {
        &self.backends
    }

    fn strategy(&self) -> &dyn Strategy {
        self.strategy.as_ref()
    }

    fn has_healthy_backends(&self) -> bool {
        // For now, assume all backends are healthy
        // We'll add health checking in the next step
        !self.backends.is_empty()
    }
}

/// Concurrent pool implementation that sends requests to all backends simultaneously
/// and returns the first non-empty response, prioritizing by backend order
pub struct ConcurrentPool {
    pub name: String,
    pub backends: Vec<Box<dyn Backend>>,
    pub strategy: Box<dyn Strategy>,
    pub timeout_ms: u64,
}

impl ConcurrentPool {
    pub fn new(name: String, backends: Vec<Box<dyn Backend>>, strategy: Box<dyn Strategy>) -> Self {
        Self {
            name,
            backends,
            strategy,
            timeout_ms: 1000, // 1 second default timeout
        }
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Send a request to all backends concurrently and return the first non-empty response
    async fn send_concurrent_requests(&self, request_data: &[u8]) -> Result<Vec<u8>, PoolError> {
        if self.backends.is_empty() {
            return Err(PoolError::NoBackendsAvailable);
        }

        use futures::future::FutureExt;
        use tokio::time::timeout;

        // Create a vector to hold all the backend request futures
        let mut futures = Vec::new();

        for (index, backend) in self.backends.iter().enumerate() {
            let backend_request = async move {
                // Connect to backend
                let mut stream = match backend.get_pooled_stream().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::warn!("Failed to connect to backend {}: {}", backend.name(), e);
                        return Err(PoolError::BackendError(e));
                    }
                };

                // Send request
                use tokio::io::AsyncWriteExt;

                if let Err(e) = stream.write_all(request_data).await {
                    tracing::warn!(
                        "Failed to send request to backend {}: {}",
                        backend.name(),
                        e
                    );
                    return Err(PoolError::IoError(e.to_string()));
                }

                if let Err(e) = stream.flush().await {
                    tracing::warn!(
                        "Failed to flush request to backend {}: {}",
                        backend.name(),
                        e
                    );
                    return Err(PoolError::IoError(e.to_string()));
                }

                // Read response using proper memcached protocol parsing
                match read_memcached_response(&mut stream).await {
                    Ok(response) => {
                        // Check if this is a cache hit (contains "VALUE") or miss ("END" only)
                        let response_str = String::from_utf8_lossy(&response);
                        let is_cache_hit = response_str.contains("VALUE");

                        if is_cache_hit {
                            tracing::debug!(
                                "Backend {} returned cache HIT ({} bytes)",
                                backend.name(),
                                response.len()
                            );
                            Ok((index, response))
                        } else {
                            tracing::debug!("Backend {} returned cache MISS", backend.name());
                            Err(PoolError::EmptyResponse)
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read response from backend {}: {}",
                            backend.name(),
                            e
                        );
                        Err(PoolError::IoError(e.to_string()))
                    }
                }
            };

            futures.push(backend_request.boxed());
        }

        // Race all the futures with a timeout
        let timeout_duration = Duration::from_millis(self.timeout_ms);

        // Wait for backends to respond (or timeout); return on first cache hit
        let mut remaining_futures = futures;

        // Race and return immediately on first successful cache hit response
        while !remaining_futures.is_empty() {
            match timeout(
                timeout_duration,
                futures::future::select_all(remaining_futures),
            )
            .await
            {
                Ok((result, _index, remaining)) => {
                    remaining_futures = remaining;

                    match result {
                        Ok((backend_index, response)) => {
                            tracing::debug!(
                                "concurrent pool: cache hit from backend {} (index {})",
                                self.backends[backend_index].name(),
                                backend_index
                            );
                            // Drop remaining futures to cancel them via drop
                            return Ok(response);
                        }
                        Err(_) => {
                            // This backend failed or miss; continue waiting
                            continue;
                        }
                    }
                }
                Err(_) => {
                    // Timeout reached for the next batch; return miss
                    tracing::debug!(
                        "concurrent pool: timeout ({}ms) reached, returning miss",
                        self.timeout_ms
                    );
                    return Ok(b"END\r\n".to_vec());
                }
            }
        }

        // No futures left; return miss
        Ok(b"END\r\n".to_vec())
    }
}

#[async_trait]
impl Pool for ConcurrentPool {
    async fn select_backend(&self, _key: &str) -> Result<&dyn Backend, PoolError> {
        // For concurrent pool, we don't really "select" a single backend
        // But we need to implement this for compatibility
        // Return the first backend as a fallback
        self.backends
            .first()
            .map(|b| b.as_ref())
            .ok_or(PoolError::NoBackendsAvailable)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn backends(&self) -> &[Box<dyn Backend>] {
        &self.backends
    }

    fn strategy(&self) -> &dyn Strategy {
        self.strategy.as_ref()
    }

    fn has_healthy_backends(&self) -> bool {
        !self.backends.is_empty()
    }

    fn supports_concurrent_requests(&self) -> bool {
        true // ConcurrentPool supports concurrent requests
    }

    async fn handle_concurrent_request(
        &self,
        _key: &str,
        request_data: &[u8],
    ) -> Result<Vec<u8>, PoolError> {
        self.send_concurrent_requests(request_data).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("No backends available in pool")]
    NoBackendsAvailable,
    #[error("All backends are unhealthy")]
    AllBackendsUnhealthy,
    #[error("Backend error: {0}")]
    BackendError(#[from] BackendError),
    #[error("Backend returned empty response")]
    EmptyResponse,
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Concurrent requests not supported by this pool type")]
    ConcurrentRequestsNotSupported,
}

/// Read a complete memcached response (handles GET responses properly)
async fn read_memcached_response(stream: &mut TcpStream) -> Result<Vec<u8>, std::io::Error> {
    let mut reader = BufReader::new(stream);
    let mut response = Vec::new();
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            break; // EOF
        }

        response.extend_from_slice(line.as_bytes());

        // Check if this is the end of the response
        let trimmed = line.trim();
        if trimmed == "END"
            || trimmed == "STORED"
            || trimmed == "NOT_STORED"
            || trimmed == "EXISTS"
            || trimmed == "NOT_FOUND"
            || trimmed == "DELETED"
            || trimmed.starts_with("ERROR")
            || trimmed.starts_with("CLIENT_ERROR")
            || trimmed.starts_with("SERVER_ERROR")
        {
            break;
        }

        // For VALUE responses, we need to read the data line too
        if trimmed.starts_with("VALUE") {
            // Parse: VALUE key flags bytes
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 4 {
                if let Ok(_data_bytes) = parts[3].parse::<usize>() {
                    // Read the data line (contains the actual cached value)
                    let mut data_line = String::new();
                    reader.read_line(&mut data_line).await?;
                    response.extend_from_slice(data_line.as_bytes());
                }
            }
        }
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::backend::MemcachedBackend;
    use crate::core::strategy::BlindForwardStrategy;

    #[tokio::test]
    async fn test_basic_pool_creation() {
        // Create some test backends
        let backend1 = Box::new(MemcachedBackend::new(
            "test1".to_string(),
            "127.0.0.1:11211".to_string(),
        )) as Box<dyn Backend>;

        let backend2 = Box::new(MemcachedBackend::new(
            "test2".to_string(),
            "127.0.0.1:11212".to_string(),
        )) as Box<dyn Backend>;

        let backends = vec![backend1, backend2];
        let strategy = Box::new(BlindForwardStrategy::new()) as Box<dyn Strategy>;

        // Create pool
        let pool = BasicPool::new("test_pool".to_string(), backends, strategy);

        // Test basic properties
        assert_eq!(pool.name(), "test_pool");
        assert_eq!(pool.backends().len(), 2);
        assert!(pool.has_healthy_backends());
        assert_eq!(pool.strategy().name(), "blind_forward");
    }

    #[tokio::test]
    async fn test_pool_backend_selection() {
        // Create a backend
        let backend = Box::new(MemcachedBackend::new(
            "test1".to_string(),
            "127.0.0.1:11211".to_string(),
        )) as Box<dyn Backend>;

        let backends = vec![backend];
        let strategy = Box::new(BlindForwardStrategy::new()) as Box<dyn Strategy>;

        // Create pool
        let pool = BasicPool::new("test_pool".to_string(), backends, strategy);

        // Test backend selection
        let selected = pool.select_backend("test_key").await;
        assert!(selected.is_ok());

        let backend = selected.unwrap();
        assert_eq!(backend.name(), "test1");
        assert_eq!(backend.server(), "127.0.0.1:11211");
    }

    #[tokio::test]
    async fn test_empty_pool() {
        let backends = vec![];
        let strategy = Box::new(BlindForwardStrategy::new()) as Box<dyn Strategy>;

        // Create empty pool
        let pool = BasicPool::new("empty_pool".to_string(), backends, strategy);

        // Test properties
        assert_eq!(pool.name(), "empty_pool");
        assert_eq!(pool.backends().len(), 0);
        assert!(!pool.has_healthy_backends());

        // Test backend selection fails
        let result = pool.select_backend("test_key").await;
        assert!(result.is_err());

        if let Err(PoolError::NoBackendsAvailable) = result {
            // Expected error
        } else {
            panic!("Expected NoBackendsAvailable error");
        }
    }

    #[tokio::test]
    async fn test_concurrent_pool_creation() {
        use crate::core::strategy::MissFailoverStrategy;

        // Create some test backends
        let backend1 = Box::new(MemcachedBackend::new(
            "test1".to_string(),
            "127.0.0.1:11211".to_string(),
        )) as Box<dyn Backend>;

        let backend2 = Box::new(MemcachedBackend::new(
            "test2".to_string(),
            "127.0.0.1:11212".to_string(),
        )) as Box<dyn Backend>;

        let backends = vec![backend1, backend2];
        let strategy = Box::new(MissFailoverStrategy::new()) as Box<dyn Strategy>;

        // Create concurrent pool
        let pool = ConcurrentPool::new("concurrent_pool".to_string(), backends, strategy)
            .with_timeout(500);

        // Test basic properties
        assert_eq!(pool.name(), "concurrent_pool");
        assert_eq!(pool.backends().len(), 2);
        assert!(pool.has_healthy_backends());
        assert_eq!(pool.strategy().name(), "miss_failover");
        assert_eq!(pool.timeout_ms, 500);
    }
}
