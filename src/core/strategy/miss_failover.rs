use super::Strategy;
use crate::core::backend::Backend;
use async_trait::async_trait;

/// Miss Failover strategy - used with ConcurrentPool to handle cache misses
/// When used with ConcurrentPool, requests are sent to all backends simultaneously
/// and the first non-empty response is returned, with priority given to earlier backends
/// This provides failover for cache misses, not just backend failures
pub struct MissFailoverStrategy {
    pub name: String,
}

impl MissFailoverStrategy {
    pub fn new() -> Self {
        Self {
            name: "miss_failover".to_string(),
        }
    }
}

impl Default for MissFailoverStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for MissFailoverStrategy {
    async fn select_backend<'a>(
        &self,
        backends: &'a [Box<dyn Backend>],
    ) -> Option<&'a Box<dyn Backend>> {
        // For compatibility with regular pools, just return the first backend
        // The real concurrent logic is implemented in ConcurrentPool
        backends.first()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::backend::MemcachedBackend;

    #[tokio::test]
    async fn test_miss_failover_strategy() {
        let backend1 = Box::new(MemcachedBackend::new(
            "test1".to_string(),
            "127.0.0.1:11211".to_string(),
        )) as Box<dyn Backend>;
        let backend2 = Box::new(MemcachedBackend::new(
            "test2".to_string(),
            "127.0.0.1:11212".to_string(),
        )) as Box<dyn Backend>;
        let backends = vec![backend1, backend2];

        let strategy = MissFailoverStrategy::new();

        // Should return first backend for compatibility
        let selected = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected.name(), "test1");

        // Test strategy name
        assert_eq!(strategy.name(), "miss_failover");
    }

    #[tokio::test]
    async fn test_miss_failover_with_empty_backends() {
        let backends = vec![];
        let strategy = MissFailoverStrategy::new();

        let result = strategy.select_backend(&backends).await;
        assert!(result.is_none());
    }
}
