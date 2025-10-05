//! Pool and strategy integration tests
//!
//! Tests for pool selection strategies and backend management

use bifrost::core::backend::MemcachedBackend;
use bifrost::core::strategy::create_strategy;
use bifrost::core::{Backend, BasicPool, Pool};
use rstest::rstest;

// ============================================================================
// Strategy Factory Tests
// ============================================================================

#[test]
fn test_strategy_factory_all_types() {
    // Test that we can create all implemented strategies
    let strategies = vec!["round_robin", "blind_forward", "failover"];

    for strategy_type in strategies {
        let strategy = create_strategy(strategy_type);
        assert!(
            strategy.is_ok(),
            "Failed to create strategy: {}",
            strategy_type
        );

        let strategy = strategy.unwrap();
        assert_eq!(strategy.name(), strategy_type);
    }

    // Test unknown strategy fails
    let unknown = create_strategy("unknown_strategy");
    assert!(unknown.is_err());
}

#[rstest]
#[case("round_robin")]
#[case("blind_forward")]
#[case("failover")]
fn test_strategy_factory_parameterized(#[case] strategy_name: &str) {
    let strategy = create_strategy(strategy_name).unwrap();
    assert_eq!(strategy.name(), strategy_name);
}

// ============================================================================
// Round Robin Strategy Tests
// ============================================================================

#[tokio::test]
async fn test_pool_round_robin_distribution() {
    // Create backends
    let backends: Vec<Box<dyn Backend>> = vec![
        Box::new(MemcachedBackend::new(
            "cache1".to_string(),
            "127.0.0.1:11211".to_string(),
        )),
        Box::new(MemcachedBackend::new(
            "cache2".to_string(),
            "127.0.0.1:11212".to_string(),
        )),
        Box::new(MemcachedBackend::new(
            "cache3".to_string(),
            "127.0.0.1:11213".to_string(),
        )),
    ];

    let strategy = create_strategy("round_robin").unwrap();
    let pool = BasicPool::new("test_pool".to_string(), backends, strategy);

    assert_eq!(pool.name(), "test_pool");
    assert_eq!(pool.backends().len(), 3);

    // Test round robin distribution
    let selections = [
        pool.select_backend("key1").await.unwrap().name().to_string(),
        pool.select_backend("key2").await.unwrap().name().to_string(),
        pool.select_backend("key3").await.unwrap().name().to_string(),
        pool.select_backend("key4").await.unwrap().name().to_string(),
        pool.select_backend("key5").await.unwrap().name().to_string(),
        pool.select_backend("key6").await.unwrap().name().to_string(),
    ];

    // Should cycle through backends
    assert_eq!(selections[0], "cache1");
    assert_eq!(selections[1], "cache2");
    assert_eq!(selections[2], "cache3");
    assert_eq!(selections[3], "cache1"); // Wraps around
    assert_eq!(selections[4], "cache2");
    assert_eq!(selections[5], "cache3");
}

#[tokio::test]
async fn test_pool_round_robin_single_backend() {
    let backends: Vec<Box<dyn Backend>> = vec![Box::new(MemcachedBackend::new(
        "only".to_string(),
        "127.0.0.1:11211".to_string(),
    ))];

    let strategy = create_strategy("round_robin").unwrap();
    let pool = BasicPool::new("single_pool".to_string(), backends, strategy);

    // With only one backend, all requests go to it
    for _ in 0..5 {
        let selected = pool.select_backend("key").await.unwrap();
        assert_eq!(selected.name(), "only");
    }
}

// ============================================================================
// Blind Forward Strategy Tests
// ============================================================================

#[tokio::test]
async fn test_pool_blind_forward_always_first() {
    let backends: Vec<Box<dyn Backend>> = vec![
        Box::new(MemcachedBackend::new(
            "primary".to_string(),
            "127.0.0.1:11211".to_string(),
        )),
        Box::new(MemcachedBackend::new(
            "secondary".to_string(),
            "127.0.0.1:11212".to_string(),
        )),
    ];

    let strategy = create_strategy("blind_forward").unwrap();
    let pool = BasicPool::new("blind_pool".to_string(), backends, strategy);

    // All requests should go to the first backend
    for i in 0..10 {
        let selected = pool.select_backend(&format!("key{}", i)).await.unwrap();
        assert_eq!(selected.name(), "primary");
    }
}

// ============================================================================
// Pool Management Tests
// ============================================================================

#[tokio::test]
async fn test_pool_multiple_strategies() {
    // Create two pools with different strategies
    let backends1: Vec<Box<dyn Backend>> = vec![
        Box::new(MemcachedBackend::new(
            "rr1".to_string(),
            "127.0.0.1:11211".to_string(),
        )),
        Box::new(MemcachedBackend::new(
            "rr2".to_string(),
            "127.0.0.1:11212".to_string(),
        )),
    ];

    let backends2: Vec<Box<dyn Backend>> = vec![
        Box::new(MemcachedBackend::new(
            "bf1".to_string(),
            "127.0.0.1:11213".to_string(),
        )),
        Box::new(MemcachedBackend::new(
            "bf2".to_string(),
            "127.0.0.1:11214".to_string(),
        )),
    ];

    let pool1 = BasicPool::new(
        "round_robin_pool".to_string(),
        backends1,
        create_strategy("round_robin").unwrap(),
    );

    let pool2 = BasicPool::new(
        "blind_forward_pool".to_string(),
        backends2,
        create_strategy("blind_forward").unwrap(),
    );

    // Verify strategies work independently
    assert_eq!(pool1.strategy().name(), "round_robin");
    assert_eq!(pool2.strategy().name(), "blind_forward");

    // Round robin should cycle
    let rr1 = pool1.select_backend("key1").await.unwrap().name().to_string();
    let rr2 = pool1.select_backend("key2").await.unwrap().name().to_string();
    assert_ne!(rr1, rr2);

    // Blind forward should always pick first
    let bf1 = pool2.select_backend("key1").await.unwrap().name().to_string();
    let bf2 = pool2.select_backend("key2").await.unwrap().name().to_string();
    assert_eq!(bf1, bf2);
    assert_eq!(bf1, "bf1");
}

#[tokio::test]
async fn test_pool_backend_properties() {
    let backend = MemcachedBackend::new("test".to_string(), "127.0.0.1:11211".to_string());

    // Test backend properties
    assert_eq!(backend.name(), "test");
    assert_eq!(backend.server(), "127.0.0.1:11211");
    assert!(!backend.uses_connection_pool()); // No pool by default with new()
}

#[tokio::test]
async fn test_empty_pool_behavior() {
    let backends: Vec<Box<dyn Backend>> = vec![];
    let strategy = create_strategy("round_robin").unwrap();
    let pool = BasicPool::new("empty_pool".to_string(), backends, strategy);

    // Pool exists but has no backends
    assert_eq!(pool.backends().len(), 0);

    // Attempting to select should fail gracefully
    let result = pool.select_backend("key").await;
    assert!(result.is_err());
}
