use super::Strategy;
use crate::core::backend::Backend;
use async_trait::async_trait;

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
    async fn select_backend<'a>(
        &self,
        backends: &'a [Box<dyn Backend>],
    ) -> Option<&'a Box<dyn Backend>> {
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
                    tracing::warn!(
                        "Failover strategy: backend '{}' failed: {}",
                        backend.name(),
                        e
                    );
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

    #[tokio::test]
    async fn test_failover_strategy() {
        // Create backends with invalid addresses that will fail to connect
        let bad_backend1 = Box::new(MemcachedBackend::new(
            "bad1".to_string(),
            "127.0.0.1:1".to_string(),
        )) as Box<dyn Backend>;
        let bad_backend2 = Box::new(MemcachedBackend::new(
            "bad2".to_string(),
            "127.0.0.1:2".to_string(),
        )) as Box<dyn Backend>;

        let strategy = FailoverStrategy::new();

        // Test with all failing backends
        let failing_backends = vec![bad_backend1, bad_backend2];
        let result = strategy.select_backend(&failing_backends).await;
        assert!(
            result.is_none(),
            "Should return None when all backends fail"
        );

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
