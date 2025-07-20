use async_trait::async_trait;
use crate::core::backend::Backend;
use super::Strategy;
use std::sync::atomic::{AtomicUsize, Ordering};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::backend::MemcachedBackend;

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
}