use super::Strategy;
use crate::core::backend::Backend;
use crate::core::metrics::BackendMetrics;
use async_trait::async_trait;

/// Least latency strategy - selects the backend with the lowest recent p95 latency
/// Falls back to average latency if p95 is unavailable (no samples yet)
pub struct LeastLatencyStrategy {
    pub name: String,
}

impl LeastLatencyStrategy {
    pub fn new() -> Self {
        Self {
            name: "least_latency".to_string(),
        }
    }
}

impl Default for LeastLatencyStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for LeastLatencyStrategy {
    async fn select_backend<'a>(
        &self,
        backends: &'a [Box<dyn Backend>],
    ) -> Option<&'a dyn Backend> {
        if backends.is_empty() {
            return None;
        }

        // Choose backend with the lowest p95 latency; if equal or zero, use average as tie-breaker
        let mut best_index: usize = 0;
        let mut best_p95: f64 = f64::INFINITY;
        let mut best_avg: f64 = f64::INFINITY;

        // Snapshot metrics concurrently while ensuring the Arc lives inside the future
        let mut snapshot_futures = Vec::with_capacity(backends.len());
        for backend in backends.iter() {
            let metrics = backend.metrics();
            snapshot_futures.push(async move { metrics.get_snapshot().await });
        }

        let snapshots = futures::future::join_all(snapshot_futures).await;

        for (idx, snapshot) in snapshots.iter().enumerate() {
            let p95_raw = snapshot.p95_latency_ms;
            let avg = snapshot.average_latency_ms;
            // If we have no samples yet, treat as 0 to prefer warming up fairly
            let p95 = if p95_raw <= 0.0 {
                avg.max(0.0)
            } else {
                p95_raw
            };

            if p95 < best_p95 || (p95 == best_p95 && avg < best_avg) {
                best_index = idx;
                best_p95 = p95;
                best_avg = avg;
            }
        }

        backends.get(best_index).map(|b| b.as_ref())
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
    async fn test_least_latency_with_empty() {
        let strategy = LeastLatencyStrategy::new();
        let backends: Vec<Box<dyn Backend>> = vec![];
        assert!(strategy.select_backend(&backends).await.is_none());
    }

    #[tokio::test]
    async fn test_least_latency_picks_first_when_equal() {
        let backend1 = Box::new(MemcachedBackend::new(
            "b1".to_string(),
            "127.0.0.1:11211".to_string(),
        )) as Box<dyn Backend>;
        let backend2 = Box::new(MemcachedBackend::new(
            "b2".to_string(),
            "127.0.0.1:11212".to_string(),
        )) as Box<dyn Backend>;
        let backends = vec![backend1, backend2];

        let strategy = LeastLatencyStrategy::new();
        // With no samples, both p95 and avg are 0; should pick index 0 by tie-breaker
        let selected = strategy.select_backend(&backends).await.unwrap();
        assert_eq!(selected.name(), "b1");
    }
}
