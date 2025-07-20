use super::backend::Backend;
use super::pool::{BasicPool, Pool};
use super::strategy::create_strategy;
use crate::config::Config;
use std::sync::Arc;

/// Fast immutable route table for thread-safe routing
pub struct RouteTable {
    routes: Vec<Route>,
}

/// A single route with matcher and target
pub struct Route {
    pub matcher: Box<dyn Matcher>,
    pub target: ResolvedTarget,
}

/// Target for a route - either a backend or a pool
#[derive(Clone)]
pub enum ResolvedTarget {
    Backend(Arc<dyn Backend>),
    Pool(Arc<dyn Pool>),
}

/// Core trait for pattern matching (glob, regex, etc.)
pub trait Matcher: Send + Sync {
    /// Check if a key matches this pattern
    fn matches(&self, key: &str) -> bool;

    /// Get the pattern string
    fn pattern(&self) -> &str;
}

/// Simple glob matcher that supports * wildcards
pub struct GlobMatcher {
    pattern: String,
}

impl GlobMatcher {
    pub fn new(pattern: String) -> Self {
        Self { pattern }
    }
}

impl Matcher for GlobMatcher {
    fn matches(&self, key: &str) -> bool {
        if self.pattern == "*" {
            return true;
        }

        // Simple prefix matching for patterns like "user:*"
        if self.pattern.ends_with('*') {
            let prefix = &self.pattern[..self.pattern.len() - 1];
            return key.starts_with(prefix);
        }

        // Exact match
        key == self.pattern
    }

    fn pattern(&self) -> &str {
        &self.pattern
    }
}

impl RouteTable {
    pub fn new(routes: Vec<Route>) -> Self {
        Self { routes }
    }

    /// Find the first matching route for a key
    pub fn find_route(&self, key: &str) -> Option<&Route> {
        // Linear search through routes (will be fast for <20 routes)
        for route in &self.routes {
            if route.matcher.matches(key) {
                return Some(route);
            }
        }
        None
    }

    /// Get all routes (for debugging)
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }
}

/// Builder for creating route tables from configuration
pub struct RouteTableBuilder;

impl RouteTableBuilder {
    /// Build a route table from configuration
    pub async fn build_from_config(config: &Config) -> Result<Arc<RouteTable>, RouteTableError> {
        let mut routes = Vec::new();

        // Build backends
        let mut backends = std::collections::HashMap::new();
        for (name, backend_config) in &config.backends {
            // Create backends with connection pooling support
            let backend =
                crate::core::backend::MemcachedBackend::from_config(name.clone(), backend_config)
                    .await
                    .map_err(|e| RouteTableError::BackendCreationFailed(e.to_string()))?;

            backends.insert(name.clone(), Arc::new(backend) as Arc<dyn Backend>);
        }

        // Build pools
        let mut pools = std::collections::HashMap::new();
        for (name, pool_config) in &config.pools {
            // Collect backends for this pool
            let mut pool_backends = Vec::new();
            for backend_name in &pool_config.backends {
                let backend = backends
                    .get(backend_name)
                    .ok_or_else(|| RouteTableError::BackendNotFound(backend_name.clone()))?;

                // Clone the backend (this clones the Arc, not the underlying object)
                let backend_clone = Arc::clone(backend);
                pool_backends
                    .push(Box::new(BackendWrapper::new(backend_clone)) as Box<dyn Backend>);
            }

            // Create strategy
            let strategy = if let Some(strategy_config) = &pool_config.strategy {
                create_strategy(&strategy_config.strategy_type)
                    .map_err(|e| RouteTableError::StrategyError(e.to_string()))?
            } else {
                // Default to blind forward if no strategy specified
                create_strategy("blind_forward")
                    .map_err(|e| RouteTableError::StrategyError(e.to_string()))?
            };

            // Create pool - use ConcurrentPool for miss_failover strategy
            let pool: Arc<dyn Pool> = if strategy.name() == "miss_failover" {
                // Create ConcurrentPool for miss failover strategy
                tracing::info!(
                    "🔄 Creating ConcurrentPool for miss failover strategy: {}",
                    name
                );
                Arc::new(super::pool::ConcurrentPool::new(
                    name.clone(),
                    pool_backends,
                    strategy,
                ))
            } else {
                // Create regular BasicPool for other strategies
                tracing::info!(
                    "📦 Creating BasicPool for strategy {}: {}",
                    strategy.name(),
                    name
                );
                Arc::new(BasicPool::new(name.clone(), pool_backends, strategy))
            };
            pools.insert(name.clone(), pool);
        }

        // Build routes
        for (_route_name, route_config) in &config.routes {
            let matcher =
                Box::new(GlobMatcher::new(route_config.matcher.clone())) as Box<dyn Matcher>;

            let target = match &route_config.target {
                crate::config::RouteTarget::Backend { backend } => {
                    let backend_ref = backends
                        .get(backend)
                        .ok_or_else(|| RouteTableError::BackendNotFound(backend.clone()))?;
                    ResolvedTarget::Backend(Arc::clone(backend_ref))
                }
                crate::config::RouteTarget::Pool { pool } => {
                    let pool_ref = pools
                        .get(pool)
                        .ok_or_else(|| RouteTableError::PoolNotFound(pool.clone()))?;
                    ResolvedTarget::Pool(Arc::clone(pool_ref))
                }
            };

            routes.push(Route { matcher, target });
        }

        Ok(Arc::new(RouteTable::new(routes)))
    }
}

/// Wrapper to convert Arc<dyn Backend> to Box<dyn Backend>
pub struct BackendWrapper {
    backend: Arc<dyn Backend>,
}

impl BackendWrapper {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }
}

#[async_trait::async_trait]
impl Backend for BackendWrapper {
    async fn connect(&self) -> Result<tokio::net::TcpStream, crate::core::backend::BackendError> {
        self.backend.connect().await
    }

    fn name(&self) -> &str {
        self.backend.name()
    }

    fn server(&self) -> &str {
        self.backend.server()
    }

    fn uses_connection_pool(&self) -> bool {
        self.backend.uses_connection_pool()
    }

    async fn get_pooled_stream(
        &self,
    ) -> Result<tokio::net::TcpStream, crate::core::backend::BackendError> {
        self.backend.get_pooled_stream().await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RouteTableError {
    #[error("Backend not found: {0}")]
    BackendNotFound(String),
    #[error("Pool not found: {0}")]
    PoolNotFound(String),
    #[error("Strategy error: {0}")]
    StrategyError(String),
    #[error("Backend creation failed: {0}")]
    BackendCreationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matcher() {
        let matcher = GlobMatcher::new("user:*".to_string());
        assert!(matcher.matches("user:123"));
        assert!(matcher.matches("user:abc"));
        assert!(!matcher.matches("cache:123"));

        let matcher = GlobMatcher::new("*".to_string());
        assert!(matcher.matches("anything"));

        let matcher = GlobMatcher::new("exact".to_string());
        assert!(matcher.matches("exact"));
        assert!(!matcher.matches("exactish"));
    }

    #[test]
    fn test_route_table_lookup() {
        let routes = vec![Route {
            matcher: Box::new(GlobMatcher::new("user:*".to_string())),
            target: ResolvedTarget::Backend(Arc::new(crate::core::backend::MemcachedBackend::new(
                "test".to_string(),
                "127.0.0.1:11211".to_string(),
            ))),
        }];

        let table = RouteTable::new(routes);

        let route = table.find_route("user:123");
        assert!(route.is_some());
        assert_eq!(route.unwrap().matcher.pattern(), "user:*");

        let route = table.find_route("cache:123");
        assert!(route.is_none());
    }
}
