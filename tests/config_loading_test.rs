use bifrost::config::Config;

#[tokio::test]
async fn test_working_pools_config_loads() {
    let config = Config::from_yaml_file("examples/working_pools.yaml").await;

    match config {
        Ok(config) => {
            println!("✅ working_pools.yaml loaded successfully!");

            // Verify basic structure
            assert!(!config.listeners.is_empty(), "Should have listeners");
            assert!(!config.backends.is_empty(), "Should have backends");
            assert!(!config.pools.is_empty(), "Should have pools");
            assert!(!config.routes.is_empty(), "Should have routes");

            // Check specific pools exist
            assert!(config.pools.contains_key("balanced_pool"), "Should have balanced_pool");
            assert!(config.pools.contains_key("simple_pool"), "Should have simple_pool");

            // Check pool configurations use implemented strategies
            let balanced_pool = &config.pools["balanced_pool"];
            assert_eq!(balanced_pool.backends.len(), 3);
            assert!(balanced_pool.strategy.is_some());
            assert_eq!(balanced_pool.strategy.as_ref().unwrap().strategy_type, "round_robin");

            let simple_pool = &config.pools["simple_pool"];
            assert_eq!(simple_pool.backends.len(), 2);
            assert!(simple_pool.strategy.is_some());
            assert_eq!(simple_pool.strategy.as_ref().unwrap().strategy_type, "blind_forward");

            println!("✅ All pool configurations use implemented strategies!");
        }
        Err(e) => {
            panic!("❌ Failed to load working_pools.yaml: {}", e);
        }
    }
}

#[tokio::test]
async fn test_pools_demo_config_loads() {
    let config = Config::from_yaml_file("examples/pools_demo.yaml").await;

    match config {
        Ok(config) => {
            println!("✅ pools_demo.yaml loaded successfully!");

            // Verify basic structure
            assert!(!config.listeners.is_empty(), "Should have listeners");
            assert!(!config.backends.is_empty(), "Should have backends");
            assert!(!config.pools.is_empty(), "Should have pools");
            assert!(!config.routes.is_empty(), "Should have routes");

            // Check specific pools exist
            assert!(config.pools.contains_key("main_pool"), "Should have main_pool");
            assert!(config.pools.contains_key("failover_pool"), "Should have failover_pool");
            assert!(config.pools.contains_key("round_robin_pool"), "Should have round_robin_pool");

            // Check pool configurations
            let main_pool = &config.pools["main_pool"];
            assert_eq!(main_pool.backends.len(), 3);
            assert!(main_pool.strategy.is_some());
            assert_eq!(main_pool.strategy.as_ref().unwrap().strategy_type, "consistent_hash");

            let failover_pool = &config.pools["failover_pool"];
            assert_eq!(failover_pool.backends.len(), 2);
            assert!(failover_pool.strategy.is_some());
            assert_eq!(failover_pool.strategy.as_ref().unwrap().strategy_type, "failover");

            println!("✅ All pool configurations look good!");
        }
        Err(e) => {
            panic!("❌ Failed to load pools_demo.yaml: {}", e);
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
            assert!(config.pools.is_empty(), "Simple config shouldn't have pools");
            assert!(!config.backends.is_empty(), "Should still have backends");
            assert!(!config.routes.is_empty(), "Should still have routes");

            println!("✅ Backward compatibility maintained!");
        }
        Err(e) => {
            panic!("❌ Failed to load simple.yaml: {}", e);
        }
    }
}