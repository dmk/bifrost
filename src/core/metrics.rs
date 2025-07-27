use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Core trait for backend metrics collection
pub trait BackendMetrics: Send + Sync {
    /// Record a successful operation with its latency
    fn record_success(&self, latency: Duration);

    /// Record a failed operation with its latency (if available)
    fn record_failure(&self, latency: Option<Duration>);

    /// Record a timeout
    fn record_timeout(&self);

    /// Record a connection attempt
    fn record_connection_attempt(&self);

    /// Record a successful connection
    fn record_connection_success(&self, latency: Duration);

    /// Record a connection failure
    fn record_connection_failure(&self);

    /// Get current metrics snapshot
    async fn get_snapshot(&self) -> MetricsSnapshot;

    /// Reset all metrics (useful for testing)
    fn reset(&self);

    /// Get the backend name these metrics belong to
    fn backend_name(&self) -> &str;
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub backend_name: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub timeouts: u64,
    pub connection_attempts: u64,
    pub connection_successes: u64,
    pub connection_failures: u64,
    pub average_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub current_connections: u64,
    pub success_rate: f64,
    pub last_request_time: Option<Instant>,
}

/// Thread-safe implementation of BackendMetrics
#[derive(Debug)]
pub struct AtomicBackendMetrics {
    backend_name: String,

    // Request counters
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    timeouts: AtomicU64,

    // Connection counters
    connection_attempts: AtomicU64,
    connection_successes: AtomicU64,
    connection_failures: AtomicU64,
    current_connections: AtomicU64,

    // Latency tracking
    latency_tracker: Arc<RwLock<LatencyTracker>>,

    // Last activity tracking
    last_request_time: Arc<RwLock<Option<Instant>>>,
}

impl AtomicBackendMetrics {
    pub fn new(backend_name: String) -> Self {
        Self {
            backend_name,
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            timeouts: AtomicU64::new(0),
            connection_attempts: AtomicU64::new(0),
            connection_successes: AtomicU64::new(0),
            connection_failures: AtomicU64::new(0),
            current_connections: AtomicU64::new(0),
            latency_tracker: Arc::new(RwLock::new(LatencyTracker::new())),
            last_request_time: Arc::new(RwLock::new(None)),
        }
    }

    pub fn increment_current_connections(&self) {
        self.current_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_current_connections(&self) {
        self.current_connections.fetch_sub(1, Ordering::Relaxed);
    }
}

impl BackendMetrics for AtomicBackendMetrics {
    fn record_success(&self, latency: Duration) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_requests.fetch_add(1, Ordering::Relaxed);

        // Update last request time
        let now = Instant::now();
        if let Ok(mut last_time) = self.last_request_time.try_write() {
            *last_time = Some(now);
        }

        // Record latency asynchronously
        let tracker = Arc::clone(&self.latency_tracker);
        tokio::spawn(async move {
            let mut tracker = tracker.write().await;
            tracker.record_latency(latency);
        });
    }

    fn record_failure(&self, latency: Option<Duration>) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.failed_requests.fetch_add(1, Ordering::Relaxed);

        // Update last request time
        let now = Instant::now();
        if let Ok(mut last_time) = self.last_request_time.try_write() {
            *last_time = Some(now);
        }

        // Record latency if available
        if let Some(latency) = latency {
            let tracker = Arc::clone(&self.latency_tracker);
            tokio::spawn(async move {
                let mut tracker = tracker.write().await;
                tracker.record_latency(latency);
            });
        }
    }

    fn record_timeout(&self) {
        self.timeouts.fetch_add(1, Ordering::Relaxed);
        self.record_failure(None); // Timeout is also a failure
    }

    fn record_connection_attempt(&self) {
        self.connection_attempts.fetch_add(1, Ordering::Relaxed);
    }

    fn record_connection_success(&self, latency: Duration) {
        self.connection_successes.fetch_add(1, Ordering::Relaxed);
        self.increment_current_connections();

        // Record connection latency
        let tracker = Arc::clone(&self.latency_tracker);
        tokio::spawn(async move {
            let mut tracker = tracker.write().await;
            tracker.record_connection_latency(latency);
        });
    }

    fn record_connection_failure(&self) {
        self.connection_failures.fetch_add(1, Ordering::Relaxed);
    }

    async fn get_snapshot(&self) -> MetricsSnapshot {
        let total = self.total_requests.load(Ordering::Relaxed);
        let successes = self.successful_requests.load(Ordering::Relaxed);

        let success_rate = if total > 0 {
            (successes as f64) / (total as f64) * 100.0
        } else {
            0.0
        };

        let (avg_latency, p95_latency, p99_latency) = {
            let tracker = self.latency_tracker.read().await;
            (
                tracker.average_latency_ms(),
                tracker.percentile_latency_ms(95.0),
                tracker.percentile_latency_ms(99.0),
            )
        };

        let last_request_time = {
            let time = self.last_request_time.read().await;
            *time
        };

        MetricsSnapshot {
            backend_name: self.backend_name.clone(),
            total_requests: total,
            successful_requests: successes,
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            timeouts: self.timeouts.load(Ordering::Relaxed),
            connection_attempts: self.connection_attempts.load(Ordering::Relaxed),
            connection_successes: self.connection_successes.load(Ordering::Relaxed),
            connection_failures: self.connection_failures.load(Ordering::Relaxed),
            average_latency_ms: avg_latency,
            p95_latency_ms: p95_latency,
            p99_latency_ms: p99_latency,
            current_connections: self.current_connections.load(Ordering::Relaxed),
            success_rate,
            last_request_time,
        }
    }

    fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.successful_requests.store(0, Ordering::Relaxed);
        self.failed_requests.store(0, Ordering::Relaxed);
        self.timeouts.store(0, Ordering::Relaxed);
        self.connection_attempts.store(0, Ordering::Relaxed);
        self.connection_successes.store(0, Ordering::Relaxed);
        self.connection_failures.store(0, Ordering::Relaxed);
        self.current_connections.store(0, Ordering::Relaxed);

                // Reset latency tracker asynchronously
        let tracker = Arc::clone(&self.latency_tracker);
        tokio::spawn(async move {
            let mut tracker = tracker.write().await;
            tracker.reset();
        });

        // Reset last request time
        let last_time = Arc::clone(&self.last_request_time);
        tokio::spawn(async move {
            let mut time = last_time.write().await;
            *time = None;
        });
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }
}

/// Latency tracking with sliding window for percentiles
#[derive(Debug)]
struct LatencyTracker {
    latencies: Vec<Duration>,
    connection_latencies: Vec<Duration>,
    max_samples: usize,
    next_index: usize,
}

impl LatencyTracker {
    fn new() -> Self {
        Self {
            latencies: Vec::with_capacity(1000),
            connection_latencies: Vec::with_capacity(1000),
            max_samples: 1000, // Keep last 1000 samples for percentile calculation
            next_index: 0,
        }
    }

    fn record_latency(&mut self, latency: Duration) {
        if self.latencies.len() < self.max_samples {
            self.latencies.push(latency);
        } else {
            self.latencies[self.next_index] = latency;
            self.next_index = (self.next_index + 1) % self.max_samples;
        }
    }

    fn record_connection_latency(&mut self, latency: Duration) {
        if self.connection_latencies.len() < self.max_samples {
            self.connection_latencies.push(latency);
        } else {
            // Use simple ring buffer for connection latencies too
            let index = self.connection_latencies.len() % self.max_samples;
            if index < self.connection_latencies.len() {
                self.connection_latencies[index] = latency;
            }
        }
    }

    fn average_latency_ms(&self) -> f64 {
        if self.latencies.is_empty() {
            return 0.0;
        }

        let total_ms: f64 = self.latencies
            .iter()
            .map(|d| d.as_secs_f64() * 1000.0)
            .sum();

        total_ms / self.latencies.len() as f64
    }

    fn percentile_latency_ms(&self, percentile: f64) -> f64 {
        if self.latencies.is_empty() {
            return 0.0;
        }

        let mut sorted_latencies = self.latencies.clone();
        sorted_latencies.sort();

        let index = ((percentile / 100.0) * sorted_latencies.len() as f64) as usize;
        let index = index.min(sorted_latencies.len() - 1);

        sorted_latencies[index].as_secs_f64() * 1000.0
    }

    fn reset(&mut self) {
        self.latencies.clear();
        self.connection_latencies.clear();
        self.next_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_backend_metrics_basic() {
        let metrics = AtomicBackendMetrics::new("test_backend".to_string());

        // Record some operations
        metrics.record_success(Duration::from_millis(10));
        metrics.record_success(Duration::from_millis(20));
        metrics.record_failure(Some(Duration::from_millis(100)));
        metrics.record_timeout();

        // Get snapshot
        let snapshot = metrics.get_snapshot().await;

        assert_eq!(snapshot.backend_name, "test_backend");
        assert_eq!(snapshot.total_requests, 4);
        assert_eq!(snapshot.successful_requests, 2);
        assert_eq!(snapshot.failed_requests, 2);
        assert_eq!(snapshot.timeouts, 1);
        assert_eq!(snapshot.success_rate, 50.0);
    }

    #[tokio::test]
    async fn test_connection_metrics() {
        let metrics = AtomicBackendMetrics::new("test_backend".to_string());

        metrics.record_connection_attempt();
        metrics.record_connection_success(Duration::from_millis(5));
        metrics.record_connection_attempt();
        metrics.record_connection_failure();

        let snapshot = metrics.get_snapshot().await;

        assert_eq!(snapshot.connection_attempts, 2);
        assert_eq!(snapshot.connection_successes, 1);
        assert_eq!(snapshot.connection_failures, 1);
        assert_eq!(snapshot.current_connections, 1);
    }

    #[test]
    fn test_latency_tracker_percentiles() {
        let mut tracker = LatencyTracker::new();

        // Add some sample latencies
        for i in 1..=100 {
            tracker.record_latency(Duration::from_millis(i));
        }

        let avg = tracker.average_latency_ms();
        let p95 = tracker.percentile_latency_ms(95.0);
        let p99 = tracker.percentile_latency_ms(99.0);

        assert!((avg - 50.5).abs() < 1.0); // Should be around 50.5ms
        assert!(p95 >= 90.0 && p95 <= 100.0); // Should be around 95ms
        assert!(p99 >= 95.0 && p99 <= 100.0); // Should be around 99ms
    }
}