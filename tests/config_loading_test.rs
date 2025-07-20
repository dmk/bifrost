use bifrost::config::Config;

#[tokio::test]
async fn test_working_pools_config_loads() {
    let config = Config::from_yaml_file("examples/pools.yaml").await;

    match config {
        Ok(config) => {
            println!("✅ pools.yaml loaded successfully!");

            // Verify basic structure
            assert!(!config.listeners.is_empty(), "Should have listeners");
            assert!(!config.backends.is_empty(), "Should have backends");
            assert!(!config.pools.is_empty(), "Should have pools");
            assert!(!config.routes.is_empty(), "Should have routes");

            // Check specific pools exist
            assert!(
                config.pools.contains_key("balanced_pool"),
                "Should have balanced_pool"
            );
            assert!(
                config.pools.contains_key("simple_pool"),
                "Should have simple_pool"
            );

            // Check pool configurations use implemented strategies
            let balanced_pool = &config.pools["balanced_pool"];
            assert_eq!(balanced_pool.backends.len(), 3);
            assert!(balanced_pool.strategy.is_some());
            assert_eq!(
                balanced_pool.strategy.as_ref().unwrap().strategy_type,
                "round_robin"
            );

            let simple_pool = &config.pools["simple_pool"];
            assert_eq!(simple_pool.backends.len(), 2);
            assert!(simple_pool.strategy.is_some());
            assert_eq!(
                simple_pool.strategy.as_ref().unwrap().strategy_type,
                "blind_forward"
            );

            println!("✅ All pool configurations use implemented strategies!");
        }
        Err(e) => {
            panic!("❌ Failed to load pools.yaml: {}", e);
        }
    }
}

#[tokio::test]
async fn test_pools_demo_config_loads() {
    let config = Config::from_yaml_file("examples/pools.yaml").await;

    match config {
        Ok(config) => {
            println!("✅ pools.yaml loaded successfully!");

            // Verify basic structure
            assert!(!config.listeners.is_empty(), "Should have listeners");
            assert!(!config.backends.is_empty(), "Should have backends");
            assert!(!config.pools.is_empty(), "Should have pools");
            assert!(!config.routes.is_empty(), "Should have routes");

            // Check specific pools exist
            assert!(
                config.pools.contains_key("balanced_pool"),
                "Should have balanced_pool"
            );
            assert!(
                config.pools.contains_key("simple_pool"),
                "Should have simple_pool"
            );

            // Verify pool configurations
            let balanced_pool = &config.pools["balanced_pool"];
            assert_eq!(
                balanced_pool.backends.len(),
                3,
                "balanced_pool should have 3 backends"
            );
            assert!(
                balanced_pool.strategy.is_some(),
                "balanced_pool should have strategy"
            );
            assert_eq!(
                balanced_pool.strategy.as_ref().unwrap().strategy_type,
                "round_robin"
            );

            let simple_pool = &config.pools["simple_pool"];
            assert_eq!(
                simple_pool.backends.len(),
                2,
                "simple_pool should have 2 backends"
            );
            assert!(
                simple_pool.strategy.is_some(),
                "simple_pool should have strategy"
            );
            assert_eq!(
                simple_pool.strategy.as_ref().unwrap().strategy_type,
                "blind_forward"
            );

            // Verify backends
            assert!(config.backends.contains_key("cache1"), "Should have cache1");
            assert!(config.backends.contains_key("cache2"), "Should have cache2");
            assert!(config.backends.contains_key("cache3"), "Should have cache3");

            // Verify routes
            assert!(
                config.routes.contains_key("direct"),
                "Should have direct route"
            );
            assert!(
                config.routes.contains_key("balanced"),
                "Should have balanced route"
            );
            assert!(
                config.routes.contains_key("simple"),
                "Should have simple route"
            );
            assert!(
                config.routes.contains_key("default"),
                "Should have default route"
            );

            // Test config validation
            match config.validate() {
                Ok(_) => println!("✅ pools.yaml validation passed!"),
                Err(e) => panic!("❌ pools.yaml validation failed: {}", e),
            }

            println!("🎯 All pools.yaml tests passed!");
        }
        Err(e) => {
            panic!("❌ Failed to load pools.yaml: {}", e);
        }
    }
}

#[tokio::test]
async fn test_simple_config_still_works() {
    let config = Config::from_yaml_file("examples/simple.yaml").await;

    match config {
        Ok(config) => {
            println!("✅ simple.yaml still loads correctly!");

            // Should work with old format (no pools)
            assert!(
                config.pools.is_empty(),
                "Simple config shouldn't have pools"
            );
            assert!(!config.backends.is_empty(), "Should still have backends");
            assert!(!config.routes.is_empty(), "Should still have routes");

            println!("✅ Backward compatibility maintained!");
        }
        Err(e) => {
            panic!("❌ Failed to load simple.yaml: {}", e);
        }
    }
}
