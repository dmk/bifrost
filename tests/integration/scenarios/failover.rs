//! Failover strategy integration tests
//!
//! Tests for failover behavior and configuration

use bifrost::{config::Config, core::route_table::ResolvedTarget, core::RouteTableBuilder};

// ============================================================================
// Configuration Loading Tests
// ============================================================================

#[tokio::test]
async fn test_failover_config_loading() {
    let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:22124"

backends:
  primary_cache:
    type: "memcached"
    server: "127.0.0.1:11212"
  backup_cache:
    type: "memcached"
    server: "127.0.0.1:11213"

pools:
  failover_pool:
    backends: ["primary_cache", "backup_cache"]
    strategy:
      type: "failover"

routes:
  critical:
    matcher: "critical:*"
    pool: "failover_pool"
  default:
    matcher: "*"
    pool: "failover_pool"
"#;

    let config = Config::from_yaml_str(yaml).expect("Failed to parse failover config");

    // Verify config structure
    assert_eq!(config.listeners.len(), 1);
    assert_eq!(config.backends.len(), 2);
    assert_eq!(config.pools.len(), 1);
    assert_eq!(config.routes.len(), 2);

    // Verify pool configuration
    let pool = config.pools.get("failover_pool").unwrap();
    assert_eq!(pool.backends.len(), 2);
    assert_eq!(pool.backends[0], "primary_cache");
    assert_eq!(pool.backends[1], "backup_cache");

    let strategy = pool.strategy.as_ref().unwrap();
    assert_eq!(strategy.strategy_type, "failover");

    // Build route table to verify everything works together
    let route_table = RouteTableBuilder::build_from_config(&config)
        .await
        .expect("Failed to build route table with failover strategy");

    assert_eq!(route_table.routes().len(), 2);

    // Test that routes target the failover pool
    let critical_route = route_table.find_route("critical:test").unwrap();
    match &critical_route.target {
        ResolvedTarget::Pool(pool) => {
            assert_eq!(pool.name(), "failover_pool");
            assert_eq!(pool.strategy().name(), "failover");
            assert_eq!(pool.backends().len(), 2);
        }
        _ => panic!("Expected critical route to target a pool"),
    }

    let default_route = route_table.find_route("other:test").unwrap();
    match &default_route.target {
        ResolvedTarget::Pool(pool) => {
            assert_eq!(pool.name(), "failover_pool");
            assert_eq!(pool.strategy().name(), "failover");
        }
        _ => panic!("Expected default route to target a pool"),
    }
}

#[tokio::test]
async fn test_failover_strategy_factory() {
    use bifrost::core::strategy::create_strategy;

    // Test that the strategy factory can create failover strategies
    let strategy = create_strategy("failover").expect("Failed to create failover strategy");
    assert_eq!(strategy.name(), "failover");

    // Test unknown strategy still fails
    let result = create_strategy("unknown_strategy");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_miss_failover_config_loading() {
    let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:22125"

backends:
  primary:
    type: "memcached"
    server: "127.0.0.1:11211"
  secondary:
    type: "memcached"
    server: "127.0.0.1:11212"

pools:
  miss_failover_pool:
    backends: ["primary", "secondary"]
    strategy:
      type: "miss_failover"

routes:
  all:
    matcher: "*"
    pool: "miss_failover_pool"
"#;

    let config = Config::from_yaml_str(yaml).expect("Failed to parse miss_failover config");

    // Verify pool configuration
    let pool = config.pools.get("miss_failover_pool").unwrap();
    let strategy = pool.strategy.as_ref().unwrap();
    assert_eq!(strategy.strategy_type, "miss_failover");

    // Build route table
    let route_table = RouteTableBuilder::build_from_config(&config)
        .await
        .expect("Failed to build route table with miss_failover strategy");

    // Verify route uses miss_failover pool
    let route = route_table.find_route("test_key").unwrap();
    match &route.target {
        ResolvedTarget::Pool(pool) => {
            assert_eq!(pool.name(), "miss_failover_pool");
            assert_eq!(pool.strategy().name(), "miss_failover");
            assert_eq!(pool.backends().len(), 2);
        }
        _ => panic!("Expected route to target a pool"),
    }
}

#[tokio::test]
async fn test_multi_tier_failover_config() {
    let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:22126"

backends:
  tier1:
    type: "memcached"
    server: "127.0.0.1:11211"
  tier2:
    type: "memcached"
    server: "127.0.0.1:11212"
  tier3:
    type: "memcached"
    server: "127.0.0.1:11213"

pools:
  three_tier_failover:
    backends: ["tier1", "tier2", "tier3"]
    strategy:
      type: "failover"

routes:
  default:
    matcher: "*"
    pool: "three_tier_failover"
"#;

    let config = Config::from_yaml_str(yaml).expect("Failed to parse multi-tier config");

    // Verify we can have 3+ backends in a failover pool
    let pool = config.pools.get("three_tier_failover").unwrap();
    assert_eq!(pool.backends.len(), 3);

    let route_table = RouteTableBuilder::build_from_config(&config)
        .await
        .expect("Failed to build route table");

    let route = route_table.find_route("any_key").unwrap();
    match &route.target {
        ResolvedTarget::Pool(pool) => {
            assert_eq!(pool.backends().len(), 3);
        }
        _ => panic!("Expected pool target"),
    }
}

// ============================================================================
// Failover Behavior Tests (would require running backends)
// ============================================================================

// Note: These tests would need actual mock backends to test failover behavior
// For now, we focus on configuration and setup validation
