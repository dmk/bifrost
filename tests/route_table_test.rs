use bifrost::config::Config;
use bifrost::core::{RouteTableBuilder};
use bifrost::core::route_table::ResolvedTarget;

#[tokio::test]
async fn test_route_table_from_working_config() {
    // Load our working pools config
    let config = Config::from_yaml_file("examples/pools.yaml").await
        .expect("Failed to load pools.yaml");

    // Build route table from config
    let route_table = RouteTableBuilder::build_from_config(&config)
        .expect("Failed to build route table");

    println!("✅ Route table built successfully!");
    println!("Number of routes: {}", route_table.routes().len());

    // Test route matching
    let test_cases = vec![
        ("direct:test", "should match direct route"),
        ("user:123", "should match balanced route"),
        ("cache:data", "should match simple route"),
        ("random:key", "should match default route"),
    ];

    for (key, description) in test_cases {
        let route = route_table.find_route(key);
        assert!(route.is_some(), "No route found for key '{}' - {}", key, description);

        let route = route.unwrap();
        println!("✅ Key '{}' matched pattern '{}' - {}", key, route.matcher.pattern(), description);

        // Check that we can identify the target type
        match &route.target {
            ResolvedTarget::Backend(backend) => {
                println!("  → Routes to backend: {}", backend.name());
            }
            ResolvedTarget::Pool(pool) => {
                println!("  → Routes to pool: {} with {} backends", pool.name(), pool.backends().len());

                // Test pool selection works
                let selected = pool.select_backend(key).await;
                assert!(selected.is_ok(), "Pool selection failed for key '{}'", key);
                let backend = selected.unwrap();
                println!("  → Pool selected backend: {}", backend.name());
            }
        }
    }
}

#[tokio::test]
async fn test_round_robin_strategy_in_route_table() {
    let config = Config::from_yaml_file("examples/pools.yaml").await
        .expect("Failed to load pools.yaml");

    let route_table = RouteTableBuilder::build_from_config(&config)
        .expect("Failed to build route table");

    // Find a route that uses the balanced pool (round robin)
    let route = route_table.find_route("user:test")
        .expect("Should find route for user:test");

    if let ResolvedTarget::Pool(pool) = &route.target {
        println!("✅ Found pool: {} with strategy: {}", pool.name(), pool.strategy().name());

        // Test that round robin actually cycles through backends
        let mut selected_backends = Vec::new();
        for i in 0..5 {
            let backend = pool.select_backend(&format!("user:test_{}", i)).await
                .expect("Pool selection should work");
            selected_backends.push(backend.name().to_string());
            println!("Request {} -> {}", i + 1, backend.name());
        }

        // Should see different backends (round robin)
        let unique_backends: std::collections::HashSet<_> = selected_backends.iter().collect();
        println!("✅ Unique backends selected: {:?}", unique_backends);

        // Should have at least 2 different backends (since we have 3 in the pool)
        assert!(unique_backends.len() >= 2, "Round robin should select different backends");
    } else {
        panic!("Expected user:test to route to a pool, not a backend");
    }
}

#[test]
fn test_route_table_config_validation() {
    // Test that our working config validates correctly
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
  test_pool:
    backends: ["cache1", "cache2"]
    strategy:
      type: "round_robin"

routes:
  default:
    matcher: "*"
    pool: "test_pool"
"#;

    let config = Config::from_yaml_str(yaml).expect("Config should parse");
    let route_table = RouteTableBuilder::build_from_config(&config);

    assert!(route_table.is_ok(), "Route table should build successfully");
    let route_table = route_table.unwrap();

    // Should have one route
    assert_eq!(route_table.routes().len(), 1);

    // Should match any key
    let route = route_table.find_route("any_key");
    assert!(route.is_some());

    println!("✅ Route table validation passed");
}