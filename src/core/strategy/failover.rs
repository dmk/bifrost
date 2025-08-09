use super::Strategy;
use crate::core::backend::Backend;
use async_trait::async_trait;

/// Failover strategy - selects primary first without probing
/// Actual failover is handled at request time if the primary fails
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

        // Do not probe/connect on selection; pick the first (primary),
        // the caller will handle failures and move to next as needed.
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
    async fn test_failover_strategy_selects_primary_without_probe() {
        // Backends may be unhealthy; selection should still return the first (primary)
        let backend1 = Box::new(MemcachedBackend::new(
            "primary".to_string(),
            "127.0.0.1:1".to_string(),
        )) as Box<dyn Backend>;
        let backend2 = Box::new(MemcachedBackend::new(
            "secondary".to_string(),
            "127.0.0.1:2".to_string(),
        )) as Box<dyn Backend>;

        let strategy = FailoverStrategy::new();

        let backends = vec![backend1, backend2];
        let selected = strategy.select_backend(&backends).await;
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().name(), "primary");

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
