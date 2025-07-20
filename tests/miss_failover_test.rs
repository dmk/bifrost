use bifrost::{config::Config, core::route_table::ResolvedTarget, core::RouteTableBuilder};

#[tokio::test]
async fn test_miss_failover_config_loading() {
    let yaml = r#"
listeners:
  main:
    bind: "127.0.0.1:22125"

backends:
  primary_cache:
    type: "memcached"
    server: "127.0.0.1:11216"
  backup_cache:
    type: "memcached"
    server: "127.0.0.1:11217"

pools:
  miss_failover_pool:
    backends: ["primary_cache", "backup_cache"]
    strategy:
      type: "miss_failover"

routes:
  critical:
    matcher: "critical:*"
    pool: "miss_failover_pool"
  default:
    matcher: "*"
    pool: "miss_failover_pool"
"#;

    let config = Config::from_yaml_str(yaml).expect("Failed to parse miss_failover config");

    // Verify config structure
    assert_eq!(config.listeners.len(), 1);
    assert_eq!(config.backends.len(), 2);
    assert_eq!(config.pools.len(), 1);
    assert_eq!(config.routes.len(), 2);

    // Verify pool configuration
    let pool = config.pools.get("miss_failover_pool").unwrap();
    assert_eq!(pool.backends.len(), 2);
    assert_eq!(pool.backends[0], "primary_cache");
    assert_eq!(pool.backends[1], "backup_cache");

    let strategy = pool.strategy.as_ref().unwrap();
    assert_eq!(strategy.strategy_type, "miss_failover");

    // Build route table to verify everything works together
    let route_table = RouteTableBuilder::build_from_config(&config)
        .await
        .expect("Failed to build route table with miss_failover strategy");

    assert_eq!(route_table.routes().len(), 2);

    // Test that routes target the miss_failover pool
    let critical_route = route_table.find_route("critical:test").unwrap();
    match &critical_route.target {
        ResolvedTarget::Pool(pool) => {
            assert_eq!(pool.name(), "miss_failover_pool");
            assert_eq!(pool.strategy().name(), "miss_failover");
            assert_eq!(pool.backends().len(), 2);
        }
        _ => panic!("Expected critical route to target a pool"),
    }

    let default_route = route_table.find_route("other:test").unwrap();
    match &default_route.target {
        ResolvedTarget::Pool(pool) => {
            assert_eq!(pool.name(), "miss_failover_pool");
            assert_eq!(pool.strategy().name(), "miss_failover");
        }
        _ => panic!("Expected default route to target a pool"),
    }

    println!("✅ Miss failover configuration loaded and route table built successfully!");
}

#[tokio::test]
async fn test_miss_failover_strategy_factory() {
    use bifrost::core::strategy::create_strategy;

    // Test that the strategy factory can create miss_failover strategies
    let strategy =
        create_strategy("miss_failover").expect("Failed to create miss_failover strategy");
    assert_eq!(strategy.name(), "miss_failover");

    // Test unknown strategy still fails
    let result = create_strategy("unknown_strategy");
    assert!(result.is_err());

    println!("✅ Miss failover strategy factory working correctly!");
}
