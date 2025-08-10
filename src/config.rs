use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub listeners: HashMap<String, ListenerConfig>,
    pub backends: HashMap<String, BackendConfig>,
    // New: support for backend pools
    #[serde(default)]
    pub pools: HashMap<String, PoolConfig>,
    pub routes: HashMap<String, RouteConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListenerConfig {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendConfig {
    #[serde(rename = "type")]
    pub backend_type: String,
    pub server: String,
    #[serde(default)]
    pub connection_pool: Option<ConnectionPoolConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionPoolConfig {
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_connection_timeout_secs")]
    pub connection_timeout_secs: u64,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_max_lifetime_secs")]
    pub max_lifetime_secs: u64,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            min_connections: default_min_connections(),
            max_connections: default_max_connections(),
            connection_timeout_secs: default_connection_timeout_secs(),
            idle_timeout_secs: default_idle_timeout_secs(),
            max_lifetime_secs: default_max_lifetime_secs(),
        }
    }
}

// Default values for connection pool configuration
fn default_min_connections() -> u32 {
    2
}
fn default_max_connections() -> u32 {
    10
}
fn default_connection_timeout_secs() -> u64 {
    5
}
fn default_idle_timeout_secs() -> u64 {
    300
} // 5 minutes
fn default_max_lifetime_secs() -> u64 {
    3600
} // 1 hour

// New: Pool configuration for collections of backends
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PoolConfig {
    pub backends: Vec<String>, // References to backend names
    #[serde(default)]
    pub strategy: Option<StrategyConfig>,
    #[serde(default)]
    pub side_effects: Vec<SideEffectConfig>,
}

/// Config for a side effect in YAML
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SideEffectConfig {
    #[serde(rename = "type")]
    pub effect_type: String,
    #[serde(flatten)]
    pub config: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StrategyConfig {
    #[serde(rename = "type")]
    pub strategy_type: String,
    // Strategy-specific configuration (will be used later)
    #[serde(flatten)]
    pub config: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteConfig {
    pub matcher: String,
    // Updated: can reference either a backend or a pool
    #[serde(flatten)]
    pub target: RouteTarget,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RouteTarget {
    Backend { backend: String },
    Pool { pool: String },
}

impl Config {
    /// Load configuration from a YAML file
    pub async fn from_yaml_file(path: &str) -> Result<Self, ConfigError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ConfigError::IoError(e.to_string()))?;

        let config: Config =
            serde_yaml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        // Validate the configuration
        config.validate()?;

        Ok(config)
    }

    /// Parse configuration from a YAML string (useful for testing)
    pub fn from_yaml_str(content: &str) -> Result<Self, ConfigError> {
        let config: Config =
            serde_yaml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        // Validate the configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate configuration for common errors
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Check that all backend references in pools exist
        for (pool_name, pool_config) in &self.pools {
            for backend_name in &pool_config.backends {
                if !self.backends.contains_key(backend_name) {
                    return Err(ConfigError::ValidationError(format!(
                        "Pool '{}' references unknown backend '{}'",
                        pool_name, backend_name
                    )));
                }
            }
        }

        // Check that all route targets exist
        for (route_name, route_config) in &self.routes {
            match &route_config.target {
                RouteTarget::Backend { backend } => {
                    if !self.backends.contains_key(backend) {
                        return Err(ConfigError::ValidationError(format!(
                            "Route '{}' references unknown backend '{}'",
                            route_name, backend
                        )));
                    }
                }
                RouteTarget::Pool { pool } => {
                    if !self.pools.contains_key(pool) {
                        return Err(ConfigError::ValidationError(format!(
                            "Route '{}' references unknown pool '{}'",
                            route_name, pool
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_with_pools() {
        let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"
  cache2:
    type: "memcached"
    server: "127.0.0.1:11213"

pools:
  main_pool:
    backends: ["cache1", "cache2"]
    strategy:
      type: "consistent_hash"

routes:
  default:
    matcher: "*"
    pool: "main_pool"
"#;

        let config = Config::from_yaml_str(yaml).expect("Failed to parse config");

        // Test basic structure
        assert_eq!(config.listeners.len(), 1);
        assert_eq!(config.backends.len(), 2);
        assert_eq!(config.pools.len(), 1);
        assert_eq!(config.routes.len(), 1);

        // Test pool configuration
        let pool = config.pools.get("main_pool").unwrap();
        assert_eq!(pool.backends.len(), 2);
        assert_eq!(pool.backends[0], "cache1");
        assert_eq!(pool.backends[1], "cache2");

        let strategy = pool.strategy.as_ref().unwrap();
        assert_eq!(strategy.strategy_type, "consistent_hash");

        // Test route targets pool
        let route = config.routes.get("default").unwrap();
        match &route.target {
            RouteTarget::Pool { pool } => assert_eq!(pool, "main_pool"),
            _ => panic!("Expected route to target a pool"),
        }
    }

    #[test]
    fn test_config_with_backend_target() {
        let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"

routes:
  simple:
    matcher: "*"
    backend: "cache1"
"#;

        let config = Config::from_yaml_str(yaml).expect("Failed to parse config");

        // Test route targets backend directly
        let route = config.routes.get("simple").unwrap();
        match &route.target {
            RouteTarget::Backend { backend } => assert_eq!(backend, "cache1"),
            _ => panic!("Expected route to target a backend"),
        }
    }

    #[test]
    fn test_config_validation_missing_backend() {
        let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"

pools:
  bad_pool:
    backends: ["cache1", "missing_backend"]

routes:
  default:
    matcher: "*"
    pool: "bad_pool"
"#;

        let result = Config::from_yaml_str(yaml);
        assert!(result.is_err());

        if let Err(ConfigError::ValidationError(msg)) = result {
            assert!(msg.contains("references unknown backend 'missing_backend'"));
        } else {
            panic!("Expected validation error for missing backend");
        }
    }

    #[test]
    fn test_config_validation_missing_pool() {
        let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"

routes:
  bad_route:
    matcher: "*"
    pool: "missing_pool"
"#;

        let result = Config::from_yaml_str(yaml);
        assert!(result.is_err());

        if let Err(ConfigError::ValidationError(msg)) = result {
            assert!(msg.contains("references unknown pool 'missing_pool'"));
        } else {
            panic!("Expected validation error for missing pool");
        }
    }

    #[test]
    fn test_pool_without_strategy() {
        let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  cache1:
    type: "memcached"
    server: "127.0.0.1:11212"

pools:
  simple_pool:
    backends: ["cache1"]
    # No strategy specified - should be None

routes:
  default:
    matcher: "*"
    pool: "simple_pool"
"#;

        let config = Config::from_yaml_str(yaml).expect("Failed to parse config");

        let pool = config.pools.get("simple_pool").unwrap();
        assert!(pool.strategy.is_none());
    }
}
