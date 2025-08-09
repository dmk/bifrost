use super::backend::{Backend, BackendError};
use super::pool::{BasicPool, Pool};
use super::strategy::create_strategy;
use crate::config::Config;
use crate::core::connection_pool::MemcachedConnectionManager;
use std::sync::Arc;
use tokio::net::TcpStream;

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
    fn matches(&self, key: &str) -> bool;
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
        if self.pattern.ends_with('*') {
            let prefix = &self.pattern[..self.pattern.len() - 1];
            return key.starts_with(prefix);
        }
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
        self.routes.iter().find(|route| route.matcher.matches(key))
    }

    pub fn routes(&self) -> &[Route] {
        &self.routes
    }
}

/// Builder for creating route tables from configuration
pub struct RouteTableBuilder;

impl RouteTableBuilder {
    pub async fn build_from_config(config: &Config) -> Result<Arc<RouteTable>, RouteTableError> {
        let mut routes = Vec::new();
        let mut backends = std::collections::HashMap::new();

        for (name, backend_config) in &config.backends {
            let backend =
                crate::core::backend::MemcachedBackend::from_config(name.clone(), backend_config)
                    .await
                    .map_err(|e| RouteTableError::BackendCreationFailed(e.to_string()))?;
            backends.insert(name.clone(), Arc::new(backend) as Arc<dyn Backend>);
        }

        let mut pools = std::collections::HashMap::new();
        for (name, pool_config) in &config.pools {
            let mut pool_backends = Vec::new();
            for backend_name in &pool_config.backends {
                let backend = backends
                    .get(backend_name)
                    .ok_or_else(|| RouteTableError::BackendNotFound(backend_name.clone()))?;
                pool_backends
                    .push(Box::new(BackendWrapper::new(backend.clone())) as Box<dyn Backend>);
            }

            let strategy = if let Some(strategy_config) = &pool_config.strategy {
                create_strategy(&strategy_config.strategy_type)
                    .map_err(|e| RouteTableError::StrategyError(e.to_string()))?
            } else {
                create_strategy("blind_forward")
                    .map_err(|e| RouteTableError::StrategyError(e.to_string()))?
            };

            let pool: Arc<dyn Pool> = if strategy.name() == "miss_failover" {
                Arc::new(super::pool::ConcurrentPool::new(
                    name.clone(),
                    pool_backends,
                    strategy,
                ))
            } else {
                Arc::new(BasicPool::new(name.clone(), pool_backends, strategy))
            };
            pools.insert(name.clone(), pool);
        }

        for route_config in config.routes.values() {
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
    async fn connect(&self) -> Result<TcpStream, BackendError> {
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
    ) -> Result<bb8::PooledConnection<'_, MemcachedConnectionManager>, BackendError> {
        self.backend.get_pooled_stream().await
    }

    fn metrics(&self) -> Arc<crate::core::metrics::AtomicBackendMetrics> {
        self.backend.metrics()
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
    use crate::config::{
        BackendConfig, Config, ListenerConfig, PoolConfig, RouteConfig, RouteTarget,
    };
    use crate::core::backend::MemcachedBackend;
    use crate::core::connection_pool::MemcachedConnectionManager;
    use crate::core::metrics::BackendMetrics;
    use std::collections::HashMap;

    #[test]
    fn test_glob_matcher() {
        let matcher = GlobMatcher::new("user:*".to_string());
        assert!(matcher.matches("user:123"));
        assert!(!matcher.matches("cache:123"));
    }

    #[test]
    fn test_glob_matcher_star_and_exact() {
        // '*' matches everything
        let any = GlobMatcher::new("*".to_string());
        assert!(any.matches("anything"));
        assert!(any.matches(""));
        // exact match only
        let exact = GlobMatcher::new("abc".to_string());
        assert!(exact.matches("abc"));
        assert!(!exact.matches("abcd"));
        assert!(!exact.matches("xabc"));
        // prefix match 'ab*'
        let prefix = GlobMatcher::new("ab*".to_string());
        assert!(prefix.matches("abc"));
        assert!(prefix.matches("ab"));
        assert!(!prefix.matches("aXc"));
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
        let route = table.find_route("cache:123");
        assert!(route.is_none());
    }

    #[test]
    fn test_find_route_returns_first_match_in_order() {
        let routes = vec![
            Route {
                matcher: Box::new(GlobMatcher::new("user:*".to_string())),
                target: ResolvedTarget::Backend(Arc::new(
                    crate::core::backend::MemcachedBackend::new(
                        "a".to_string(),
                        "127.0.0.1:11211".to_string(),
                    ),
                )),
            },
            Route {
                matcher: Box::new(GlobMatcher::new("*".to_string())),
                target: ResolvedTarget::Backend(Arc::new(
                    crate::core::backend::MemcachedBackend::new(
                        "b".to_string(),
                        "127.0.0.1:11212".to_string(),
                    ),
                )),
            },
        ];
        let table = RouteTable::new(routes);
        let route = table.find_route("user:xyz").unwrap();
        assert_eq!(route.matcher.pattern(), "user:*");
    }

    #[test]
    fn test_empty_route_table_has_no_routes() {
        let table = RouteTable::new(Vec::new());
        assert_eq!(table.routes().len(), 0);
        assert!(table.find_route("anything").is_none());
    }

    #[tokio::test]
    async fn test_build_from_config_success_backend_and_pool() {
        let mut listeners = HashMap::new();
        listeners.insert(
            "main".to_string(),
            ListenerConfig {
                bind: "127.0.0.1:0".to_string(),
            },
        );

        let mut backends = HashMap::new();
        backends.insert(
            "b1".to_string(),
            BackendConfig {
                backend_type: "memcached".to_string(),
                server: "127.0.0.1:11211".to_string(),
                connection_pool: None,
            },
        );

        let mut pools = HashMap::new();
        pools.insert(
            "p1".to_string(),
            PoolConfig {
                backends: vec!["b1".into()],
                strategy: None,
            },
        );

        let mut routes = HashMap::new();
        routes.insert(
            "r1".to_string(),
            RouteConfig {
                matcher: "*".into(),
                target: RouteTarget::Backend {
                    backend: "b1".into(),
                },
            },
        );
        routes.insert(
            "r2".to_string(),
            RouteConfig {
                matcher: "*".into(),
                target: RouteTarget::Pool { pool: "p1".into() },
            },
        );

        let cfg = Config {
            listeners,
            backends,
            pools,
            routes,
        };
        let table = RouteTableBuilder::build_from_config(&cfg).await.unwrap();
        assert!(table.routes().len() >= 2);
    }

    #[tokio::test]
    async fn test_build_from_config_miss_failover_uses_concurrent_pool() {
        use crate::config::{BackendConfig, PoolConfig, StrategyConfig};
        use std::collections::HashMap;

        let listeners = HashMap::new();

        let mut backends = HashMap::new();
        backends.insert(
            "b1".to_string(),
            BackendConfig {
                backend_type: "memcached".to_string(),
                server: "127.0.0.1:11211".to_string(),
                connection_pool: None,
            },
        );

        let mut pools = HashMap::new();
        pools.insert(
            "p1".to_string(),
            PoolConfig {
                backends: vec!["b1".into()],
                strategy: Some(StrategyConfig {
                    strategy_type: "miss_failover".to_string(),
                    config: HashMap::new(),
                }),
            },
        );

        let mut routes = HashMap::new();
        routes.insert(
            "r1".to_string(),
            crate::config::RouteConfig {
                matcher: "*".into(),
                target: crate::config::RouteTarget::Pool { pool: "p1".into() },
            },
        );

        let cfg = crate::config::Config {
            listeners,
            backends,
            pools,
            routes,
        };

        let table = RouteTableBuilder::build_from_config(&cfg).await.unwrap();
        // Find the pool route and assert concurrent support
        let pool_route = table
            .routes()
            .iter()
            .find(|r| matches!(r.target, ResolvedTarget::Pool(_)))
            .expect("pool route present");
        match &pool_route.target {
            ResolvedTarget::Pool(p) => assert!(p.supports_concurrent_requests()),
            _ => panic!("expected pool target"),
        }
    }

    #[tokio::test]
    async fn test_build_from_config_unknown_strategy_errors() {
        use crate::config::{BackendConfig, PoolConfig, StrategyConfig};
        use std::collections::HashMap;

        let listeners = HashMap::new();

        let mut backends = HashMap::new();
        backends.insert(
            "b1".to_string(),
            BackendConfig {
                backend_type: "memcached".to_string(),
                server: "127.0.0.1:11211".to_string(),
                connection_pool: None,
            },
        );

        let mut pools = HashMap::new();
        pools.insert(
            "p1".to_string(),
            PoolConfig {
                backends: vec!["b1".into()],
                strategy: Some(StrategyConfig {
                    strategy_type: "unknown_strategy".to_string(),
                    config: HashMap::new(),
                }),
            },
        );

        let routes = HashMap::new();
        let cfg = crate::config::Config {
            listeners,
            backends,
            pools,
            routes,
        };

        let err = RouteTableBuilder::build_from_config(&cfg)
            .await
            .err()
            .unwrap();
        match err {
            RouteTableError::StrategyError(_) => (),
            _ => panic!("expected StrategyError for unknown strategy"),
        }
    }

    #[tokio::test]
    async fn test_build_from_config_unknown_backend_in_pool() {
        let mut listeners = HashMap::new();
        listeners.insert(
            "main".to_string(),
            ListenerConfig {
                bind: "127.0.0.1:0".to_string(),
            },
        );

        let backends = HashMap::new();
        let mut pools = HashMap::new();
        pools.insert(
            "p1".to_string(),
            PoolConfig {
                backends: vec!["missing".into()],
                strategy: None,
            },
        );
        let routes = HashMap::new();
        let cfg = Config {
            listeners,
            backends,
            pools,
            routes,
        };
        let err = RouteTableBuilder::build_from_config(&cfg)
            .await
            .err()
            .unwrap();
        match err {
            RouteTableError::BackendCreationFailed(_) | RouteTableError::BackendNotFound(_) => (),
            _ => panic!("expected backend-related error"),
        }
    }

    #[tokio::test]
    async fn test_build_from_config_unknown_pool_in_route() {
        let mut listeners = HashMap::new();
        listeners.insert(
            "main".to_string(),
            ListenerConfig {
                bind: "127.0.0.1:0".to_string(),
            },
        );

        let mut backends = HashMap::new();
        backends.insert(
            "b1".to_string(),
            BackendConfig {
                backend_type: "memcached".to_string(),
                server: "127.0.0.1:11211".to_string(),
                connection_pool: None,
            },
        );
        let pools = HashMap::new();
        let mut routes = HashMap::new();
        routes.insert(
            "r1".to_string(),
            RouteConfig {
                matcher: "*".into(),
                target: RouteTarget::Pool {
                    pool: "missing".into(),
                },
            },
        );
        let cfg = Config {
            listeners,
            backends,
            pools,
            routes,
        };
        let err = RouteTableBuilder::build_from_config(&cfg)
            .await
            .err()
            .unwrap();
        match err {
            RouteTableError::PoolNotFound(_) => (),
            _ => panic!("expected pool not found"),
        }
    }

    #[tokio::test]
    async fn test_build_from_config_backend_not_found_in_route() {
        let mut listeners = HashMap::new();
        listeners.insert(
            "main".to_string(),
            ListenerConfig {
                bind: "127.0.0.1:0".to_string(),
            },
        );

        let backends = HashMap::new();
        let pools = HashMap::new();
        let mut routes = HashMap::new();
        routes.insert(
            "r1".to_string(),
            RouteConfig {
                matcher: "*".into(),
                target: RouteTarget::Backend {
                    backend: "missing".into(),
                },
            },
        );

        let cfg = Config {
            listeners,
            backends,
            pools,
            routes,
        };

        let err = RouteTableBuilder::build_from_config(&cfg)
            .await
            .err()
            .unwrap();
        match err {
            RouteTableError::BackendNotFound(_) => (),
            _ => panic!("expected BackendNotFound"),
        }
    }

    struct MockBackend {
        name: String,
        addr: String,
        use_pool: bool,
        metrics: Arc<crate::core::metrics::AtomicBackendMetrics>,
    }

    #[async_trait::async_trait]
    impl Backend for MockBackend {
        async fn connect(&self) -> Result<TcpStream, BackendError> {
            let s = TcpStream::connect(&self.addr)
                .await
                .map_err(|e| BackendError::ConnectionFailed(e.to_string()))?;
            Ok(s)
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn server(&self) -> &str {
            &self.addr
        }
        fn uses_connection_pool(&self) -> bool {
            self.use_pool
        }
        async fn get_pooled_stream(
            &self,
        ) -> Result<bb8::PooledConnection<'_, MemcachedConnectionManager>, BackendError> {
            Err(BackendError::NoConnectionPool)
        }
        fn metrics(&self) -> Arc<crate::core::metrics::AtomicBackendMetrics> {
            Arc::clone(&self.metrics)
        }
    }

    #[tokio::test]
    async fn test_backend_wrapper_delegates_identity_and_connect() {
        // Listener to accept a single connection
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let accept = tokio::spawn(async move {
            let _ = listener.accept().await.unwrap();
        });

        let inner = Arc::new(MockBackend {
            name: "mock".into(),
            addr: addr.clone(),
            use_pool: false,
            metrics: Arc::new(crate::core::metrics::AtomicBackendMetrics::new(
                "mock".into(),
            )),
        }) as Arc<dyn Backend>;
        let wrapper = BackendWrapper::new(inner.clone());

        // Delegated identity
        assert_eq!(wrapper.name(), "mock");
        assert_eq!(wrapper.server(), addr);
        assert!(!wrapper.uses_connection_pool());
        assert_eq!(wrapper.metrics().backend_name(), "mock");

        // Delegated connect
        let mut stream = wrapper.connect().await.unwrap();
        use tokio::io::AsyncWriteExt;
        stream.write_all(b"hi").await.unwrap();
        let _ = accept.await;
    }

    #[tokio::test]
    async fn test_backend_wrapper_get_pooled_stream_err_for_no_pool() {
        let inner = Arc::new(MockBackend {
            name: "mock".into(),
            addr: "127.0.0.1:9".into(),
            use_pool: false,
            metrics: Arc::new(crate::core::metrics::AtomicBackendMetrics::new(
                "mock".into(),
            )),
        }) as Arc<dyn Backend>;
        let wrapper = BackendWrapper::new(inner);
        let err = wrapper.get_pooled_stream().await.err().unwrap();
        match err {
            BackendError::NoConnectionPool => (),
            _ => panic!("expected NoConnectionPool"),
        }
    }

    #[tokio::test]
    async fn test_backend_wrapper_get_pooled_stream_success_with_memcached_backend() {
        // Spin up a simple TCP server to accept pool connections
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    // keep it open briefly
                    tokio::spawn(async move {
                        let _ = tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                        drop(stream);
                    });
                }
            }
        });

        let cfg = BackendConfig {
            backend_type: "memcached".into(),
            server: addr,
            connection_pool: Some(crate::config::ConnectionPoolConfig {
                min_connections: 1,
                max_connections: 2,
                connection_timeout_secs: 1,
                ..Default::default()
            }),
        };
        let mem = MemcachedBackend::from_config("mem".into(), &cfg)
            .await
            .unwrap();
        let wrapper = BackendWrapper::new(Arc::new(mem));
        let pooled = wrapper.get_pooled_stream().await;
        assert!(pooled.is_ok());
    }
}
