pub mod backend;
pub mod connection_pool;
pub mod metrics;
pub mod pool;
pub mod protocols;
pub mod route_table;
pub mod side_effect;
pub mod strategy;

// Re-export core traits
pub use backend::Backend;
pub use connection_pool::MemcachedPool;
pub use metrics::{AtomicBackendMetrics, BackendMetrics, MetricsSnapshot};
pub use pool::{BasicPool, ConcurrentPool, Pool};
pub use protocols::Protocol;
pub use route_table::{GlobMatcher, Matcher, RouteTable, RouteTableBuilder};
pub use side_effect::{create_side_effect, BackendInfo, ConcurrentHitEvent, SideEffect};
pub use strategy::{
    BlindForwardStrategy, FailoverStrategy, MissFailoverStrategy, RoundRobinStrategy, Strategy,
};
