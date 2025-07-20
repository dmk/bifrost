use async_trait::async_trait;
use super::backend::{Backend, BackendError};
use super::strategy::Strategy;

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
        let selected = self.strategy.select_backend(&self.backends).await
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

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("No backends available in pool")]
    NoBackendsAvailable,
    #[error("All backends are unhealthy")]
    AllBackendsUnhealthy,
    #[error("Backend error: {0}")]
    BackendError(#[from] BackendError),
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
            "127.0.0.1:11211".to_string()
        )) as Box<dyn Backend>;

        let backend2 = Box::new(MemcachedBackend::new(
            "test2".to_string(),
            "127.0.0.1:11212".to_string()
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
            "127.0.0.1:11211".to_string()
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
}