pub mod backend;
pub mod strategy;
pub mod protocols;
pub mod pool;
pub mod route_table;
pub mod connection_pool;

// Re-export core traits
pub use backend::Backend;
pub use strategy::{Strategy, BlindForwardStrategy, RoundRobinStrategy};
pub use protocols::Protocol;
pub use pool::{Pool, BasicPool};
pub use route_table::{RouteTable, RouteTableBuilder, Matcher, GlobMatcher};
pub use connection_pool::{MemcachedPool, ConnectionPoolBuilder};