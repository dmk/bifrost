use bifrost::{config::Config, core::route_table::ResolvedTarget, core::RouteTableBuilder};

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

    println!("✅ Failover configuration loaded and route table built successfully!");
    println!(
        "✅ Strategy type: {}",
        match &critical_route.target {
            ResolvedTarget::Pool(pool) => pool.strategy().name(),
            _ => "none",
        }
    );
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

    println!("✅ Failover strategy factory working correctly!");
}
