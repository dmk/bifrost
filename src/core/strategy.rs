use async_trait::async_trait;
use crate::core::backend::Backend;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Core trait for routing strategies
#[async_trait]
pub trait Strategy: Send + Sync {
    /// Select a backend for the given request
    async fn select_backend<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>>;

    /// Strategy name
    fn name(&self) -> &str;
}

/// Strategy factory for creating strategies from configuration
pub fn create_strategy(strategy_type: &str) -> Result<Box<dyn Strategy>, StrategyError> {
    match strategy_type {
        "blind_forward" => Ok(Box::new(BlindForwardStrategy::new())),
        "round_robin" => Ok(Box::new(RoundRobinStrategy::new())),
        "failover" => Ok(Box::new(FailoverStrategy::new())),
        _ => Err(StrategyError::UnknownStrategy(strategy_type.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    #[error("Unknown strategy type: {0}")]
    UnknownStrategy(String),
}

/// Simple blind forward strategy - always uses the first backend
/// This is optimized for minimal latency by avoiding async overhead
#[derive(Debug)]
pub struct BlindForwardStrategy {
    pub name: String,
}

impl BlindForwardStrategy {
    pub fn new() -> Self {
        Self {
            name: "blind_forward".to_string(),
        }
    }

    /// Fast synchronous backend selection (no async overhead)
    pub fn select_backend_sync<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>> {
        backends.first()
    }
}

impl Default for BlindForwardStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for BlindForwardStrategy {
    async fn select_backend<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>> {
        // Use the fast sync version to avoid async overhead
        self.select_backend_sync(backends)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Simple round robin strategy - cycles through backends in order
/// Perfect for load balancing across multiple servers
pub struct RoundRobinStrategy {
    pub name: String,
    counter: AtomicUsize,
}

impl RoundRobinStrategy {
    pub fn new() -> Self {
        Self {
            name: "round_robin".to_string(),
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobinStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for RoundRobinStrategy {
    async fn select_backend<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>> {
        if backends.is_empty() {
            return None;
        }

        // Get next backend using round robin
        let index = self.counter.fetch_add(1, Ordering::Relaxed) % backends.len();
        backends.get(index)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Failover strategy - tries backends sequentially until one succeeds
/// Perfect for primary/backup configurations where you want high availability
pub struct FailoverStrategy {
    pub name: String,
}

impl FailoverStrategy {
    pub fn new() -> Self {
        Self {
            name: "failover".to_string(),
        }
    }
}

impl Default for FailoverStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for FailoverStrategy {
    async fn select_backend<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>> {
        if backends.is_empty() {
            return None;
        }

        // Try each backend in order until one connects successfully
        for backend in backends {
            match backend.connect().await {
                Ok(_stream) => {
                    // Connection successful, return this backend
                    tracing::debug!("Failover strategy: backend '{}' is healthy", backend.name());
                    return Some(backend);
                }
                Err(e) => {
                    // Connection failed, try next backend
                    tracing::warn!("Failover strategy: backend '{}' failed: {}", backend.name(), e);
                    continue;
                }
            }
        }

        // All backends failed
        tracing::error!("Failover strategy: all {} backends failed", backends.len());
        None
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::backend::MemcachedBackend;

    #[test]
    fn test_strategy_factory() {
        // Test creating known strategies
        let blind_forward = create_strategy("blind_forward").unwrap();
        assert_eq!(blind_forward.name(), "blind_forward");

        let round_robin = create_strategy("round_robin").unwrap();
        assert_eq!(round_robin.name(), "round_robin");

        // Test unknown strategy
        let result = create_strategy("unknown_strategy");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_blind_forward_strategy() {
        let backend1 = Box::new(MemcachedBackend::new("test1".to_string(), "127.0.0.1:11211".to_string())) as Box<dyn Backend>;
        let backend2 = Box::new(MemcachedBackend::new("test2".to_string(), "127.0.0.1:11212".to_string())) as Box<dyn Backend>;
        let backends = vec![backend1, backend2];

        let strategy = BlindForwardStrategy::new();

        // Should always return first backend
        let selected = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected.name(), "test1");

        // Should still return first backend on subsequent calls
        let selected = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected.name(), "test1");
    }

    #[tokio::test]
    async fn test_round_robin_strategy() {
        let backend1 = Box::new(MemcachedBackend::new("test1".to_string(), "127.0.0.1:11211".to_string())) as Box<dyn Backend>;
        let backend2 = Box::new(MemcachedBackend::new("test2".to_string(), "127.0.0.1:11212".to_string())) as Box<dyn Backend>;
        let backend3 = Box::new(MemcachedBackend::new("test3".to_string(), "127.0.0.1:11213".to_string())) as Box<dyn Backend>;
        let backends = vec![backend1, backend2, backend3];

        let strategy = RoundRobinStrategy::new();

        // Should cycle through backends
        let selected1 = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected1.name(), "test1");

        let selected2 = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected2.name(), "test2");

        let selected3 = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected3.name(), "test3");

        // Should wrap around to first backend
        let selected4 = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected4.name(), "test1");
    }

    #[tokio::test]
    async fn test_round_robin_with_empty_backends() {
        let backends = vec![];
        let strategy = RoundRobinStrategy::new();

        let result = strategy.select_backend(&backends).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_round_robin_with_single_backend() {
        let backend1 = Box::new(MemcachedBackend::new("test1".to_string(), "127.0.0.1:11211".to_string())) as Box<dyn Backend>;
        let backends = vec![backend1];

        let strategy = RoundRobinStrategy::new();

        // Should always return the same backend
        let selected1 = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected1.name(), "test1");

        let selected2 = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected2.name(), "test1");
    }

    #[tokio::test]
    async fn test_failover_strategy() {
        // Create backends with invalid addresses that will fail to connect
        let bad_backend1 = Box::new(MemcachedBackend::new("bad1".to_string(), "127.0.0.1:1".to_string())) as Box<dyn Backend>;
        let bad_backend2 = Box::new(MemcachedBackend::new("bad2".to_string(), "127.0.0.1:2".to_string())) as Box<dyn Backend>;

        let strategy = FailoverStrategy::new();

        // Test with all failing backends
        let failing_backends = vec![bad_backend1, bad_backend2];
        let result = strategy.select_backend(&failing_backends).await;
        assert!(result.is_none(), "Should return None when all backends fail");

        // Test strategy name
        assert_eq!(strategy.name(), "failover");
    }

    #[tokio::test]
    async fn test_failover_with_empty_backends() {
        let backends = vec![];
        let strategy = FailoverStrategy::new();

        let result = strategy.select_backend(&backends).await;
        assert!(result.is_none());
    }
}