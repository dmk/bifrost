use super::Strategy;
use crate::core::backend::Backend;
use async_trait::async_trait;

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
    pub fn select_backend_sync<'a>(
        &self,
        backends: &'a [Box<dyn Backend>],
    ) -> Option<&'a dyn Backend> {
        backends.first().map(|b| b.as_ref())
    }
}

impl Default for BlindForwardStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for BlindForwardStrategy {
    async fn select_backend<'a>(
        &self,
        backends: &'a [Box<dyn Backend>],
    ) -> Option<&'a dyn Backend> {
        // Use the fast sync version to avoid async overhead
        self.select_backend_sync(backends)
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
    async fn test_blind_forward_strategy() {
        let backend1 = Box::new(MemcachedBackend::new(
            "test1".to_string(),
            "127.0.0.1:11211".to_string(),
        )) as Box<dyn Backend>;
        let backend2 = Box::new(MemcachedBackend::new(
            "test2".to_string(),
            "127.0.0.1:11212".to_string(),
        )) as Box<dyn Backend>;
        let backends = vec![backend1, backend2];

        let strategy = BlindForwardStrategy::new();

        // Should always return first backend
        let selected = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected.name(), "test1");

        // Should still return first backend on subsequent calls
        let selected = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected.name(), "test1");
    }
}
