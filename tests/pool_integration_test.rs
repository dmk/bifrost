use bifrost::core::{Backend, BasicPool, Pool};
use bifrost::core::backend::MemcachedBackend;
use bifrost::core::strategy::{create_strategy};

#[tokio::test]
async fn test_pool_with_round_robin_strategy() {
    // Create backends
    let backend1 = Box::new(MemcachedBackend::new("cache1".to_string(), "127.0.0.1:11211".to_string())) as Box<dyn Backend>;
    let backend2 = Box::new(MemcachedBackend::new("cache2".to_string(), "127.0.0.1:11212".to_string())) as Box<dyn Backend>;
    let backend3 = Box::new(MemcachedBackend::new("cache3".to_string(), "127.0.0.1:11213".to_string())) as Box<dyn Backend>;

    let backends = vec![backend1, backend2, backend3];

    // Create strategy from factory (like we would from config)
    let strategy = create_strategy("round_robin").unwrap();

    // Create pool
    let pool = BasicPool::new("test_pool".to_string(), backends, strategy);

    println!("✅ Created pool '{}' with {} backends", pool.name(), pool.backends().len());

    // Test that round robin actually works
    let selected1 = pool.select_backend("test_key_1").await.unwrap();
    println!("Request 1 -> {}", selected1.name());

    let selected2 = pool.select_backend("test_key_2").await.unwrap();
    println!("Request 2 -> {}", selected2.name());

    let selected3 = pool.select_backend("test_key_3").await.unwrap();
    println!("Request 3 -> {}", selected3.name());

    let selected4 = pool.select_backend("test_key_4").await.unwrap();
    println!("Request 4 -> {}", selected4.name());

    // Verify round robin behavior
    assert_eq!(selected1.name(), "cache1");
    assert_eq!(selected2.name(), "cache2");
    assert_eq!(selected3.name(), "cache3");
    assert_eq!(selected4.name(), "cache1"); // Wraps around

    println!("✅ Round robin strategy working correctly!");
}

#[tokio::test]
async fn test_pool_with_blind_forward_strategy() {
    // Create backends
    let backend1 = Box::new(MemcachedBackend::new("cache1".to_string(), "127.0.0.1:11211".to_string())) as Box<dyn Backend>;
    let backend2 = Box::new(MemcachedBackend::new("cache2".to_string(), "127.0.0.1:11212".to_string())) as Box<dyn Backend>;

    let backends = vec![backend1, backend2];

    // Create strategy from factory
    let strategy = create_strategy("blind_forward").unwrap();

    // Create pool
    let pool = BasicPool::new("simple_pool".to_string(), backends, strategy);

    println!("✅ Created pool '{}' with blind forward strategy", pool.name());

    // Test that blind forward always picks first
    let selected1 = pool.select_backend("test_key_1").await.unwrap();
    let selected2 = pool.select_backend("test_key_2").await.unwrap();
    let selected3 = pool.select_backend("test_key_3").await.unwrap();

    // Should always be the first backend
    assert_eq!(selected1.name(), "cache1");
    assert_eq!(selected2.name(), "cache1");
    assert_eq!(selected3.name(), "cache1");

    println!("✅ Blind forward strategy working correctly!");
}

#[test]
fn test_strategy_factory_with_all_types() {
    // Test that we can create all implemented strategies
    let strategies = vec!["round_robin", "blind_forward"];

    for strategy_type in strategies {
        let strategy = create_strategy(strategy_type);
        assert!(strategy.is_ok(), "Failed to create strategy: {}", strategy_type);

        let strategy = strategy.unwrap();
        assert_eq!(strategy.name(), strategy_type);
        println!("✅ Strategy '{}' created successfully", strategy_type);
    }

    // Test unknown strategy fails
    let unknown = create_strategy("unknown_strategy");
    assert!(unknown.is_err());
    println!("✅ Unknown strategy correctly rejected");
}