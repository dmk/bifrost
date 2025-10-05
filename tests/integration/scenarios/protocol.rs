//! Protocol integration tests
//!
//! End-to-end tests for protocol handling with mock backends
//! Note: Pure protocol parsing tests are in the main test files

use crate::integration::{MockMemcached, MockMemcachedBuilder, ResponseMode, TestClient};
use bifrost::core::protocols::{AsciiProtocol, Protocol};
use std::time::Duration;

// ============================================================================
// End-to-End Protocol Tests with Mock Backend
// ============================================================================

#[tokio::test]
async fn test_protocol_basic_get_set() {
    let mock = MockMemcached::new().await.unwrap();
    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // SET a value
    assert!(client.set("testkey", "hello_world", 0).await.unwrap());

    // GET the value back
    let value = client.get("testkey").await.unwrap();
    assert_eq!(value, Some("hello_world".to_string()));

    // Verify the mock recorded the requests
    let stats = mock.stats();
    assert_eq!(stats.sets, 1);
    assert_eq!(stats.gets, 1);

    client.quit().await.unwrap();
    mock.shutdown();
}

#[tokio::test]
async fn test_protocol_delete_operations() {
    let mock = MockMemcached::new().await.unwrap();
    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // SET a value
    assert!(client.set("delkey", "to_delete", 0).await.unwrap());

    // DELETE the value
    assert!(client.delete("delkey").await.unwrap());

    // Verify it's gone
    let value = client.get("delkey").await.unwrap();
    assert_eq!(value, None);

    // DELETE non-existent key should return false
    assert!(!client.delete("nonexistent").await.unwrap());

    client.quit().await.unwrap();
    mock.shutdown();
}

#[tokio::test]
async fn test_protocol_version_and_stats() {
    let mock = MockMemcached::new().await.unwrap();
    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // VERSION command
    let version = client.version().await.unwrap();
    assert!(version.contains("mock"));

    // STATS command
    let stats = client.stats(None).await.unwrap();
    assert!(stats.contains("STAT"));
    assert!(stats.contains("END"));

    client.quit().await.unwrap();
    mock.shutdown();
}

#[tokio::test]
async fn test_protocol_with_slow_backend() {
    let mock = MockMemcachedBuilder::new()
        .latency(Duration::from_millis(50))
        .build()
        .await
        .unwrap();

    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    let start = std::time::Instant::now();
    let _ = client.version().await.unwrap();
    let elapsed = start.elapsed();

    // Should take at least 50ms due to latency
    assert!(elapsed >= Duration::from_millis(50));

    client.quit().await.unwrap();
    mock.shutdown();
}

#[tokio::test]
async fn test_protocol_always_miss_mode() {
    let mock = MockMemcachedBuilder::new()
        .response_mode(ResponseMode::AlwaysMiss)
        .with_data("key1".to_string(), b"value1".to_vec())
        .build()
        .await
        .unwrap();

    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // Even though data exists in storage, AlwaysMiss mode returns nothing
    let value = client.get("key1").await.unwrap();
    assert_eq!(value, None);

    client.quit().await.unwrap();
    mock.shutdown();
}

#[tokio::test]
async fn test_protocol_instance_exists() {
    // Simple test to verify the protocol can be instantiated
    let protocol = AsciiProtocol::new();
    assert_eq!(protocol.name(), "ascii");
}

#[tokio::test]
async fn test_protocol_get_nonexistent_key() {
    let mock = MockMemcached::new().await.unwrap();
    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // GET a key that doesn't exist
    let value = client.get("nonexistent").await.unwrap();
    assert_eq!(value, None);

    client.quit().await.unwrap();
    mock.shutdown();
}

#[tokio::test]
async fn test_protocol_multiple_operations() {
    let mock = MockMemcached::new().await.unwrap();
    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // Perform multiple operations in sequence
    assert!(client.set("key1", "value1", 0).await.unwrap());
    assert!(client.set("key2", "value2", 0).await.unwrap());
    assert!(client.set("key3", "value3", 0).await.unwrap());

    assert_eq!(client.get("key1").await.unwrap(), Some("value1".to_string()));
    assert_eq!(client.get("key2").await.unwrap(), Some("value2".to_string()));
    assert_eq!(client.get("key3").await.unwrap(), Some("value3".to_string()));

    assert!(client.delete("key2").await.unwrap());
    assert_eq!(client.get("key2").await.unwrap(), None);

    let stats = mock.stats();
    assert_eq!(stats.sets, 3);
    assert_eq!(stats.gets, 4);
    assert_eq!(stats.deletes, 1);

    client.quit().await.unwrap();
    mock.shutdown();
}

#[tokio::test]
async fn test_protocol_with_preloaded_data() {
    let mock = MockMemcachedBuilder::new()
        .with_data("preloaded1".to_string(), b"data1".to_vec())
        .with_data("preloaded2".to_string(), b"data2".to_vec())
        .build()
        .await
        .unwrap();

    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // Verify preloaded data exists
    assert_eq!(
        client.get("preloaded1").await.unwrap(),
        Some("data1".to_string())
    );
    assert_eq!(
        client.get("preloaded2").await.unwrap(),
        Some("data2".to_string())
    );

    client.quit().await.unwrap();
    mock.shutdown();
}

// Note: Removed pure unit tests for protocol parsing - those belong in
// tests/protocol_integration_test.rs as they don't require the mock backend
// and are already covered there.

// Note: Pipelined commands test removed due to complexity in mock implementation.
// Pipelining is better tested with real memcached in E2E tests.

// Old tests that were moved:
// - test_ascii_protocol_command_reconstruction() -> already in protocol_integration_test.rs
// - test_protocol_error_handling() -> already in protocol_integration_test.rs  
// - test_response_parsing_edge_cases() -> already in protocol_integration_test.rs
// - test_protocol_noreply_handling() -> already in protocol_integration_test.rs
// - test_multikey_get_routing() -> already in protocol_integration_test.rs

#[allow(dead_code)]
fn removed_duplicate_unit_tests_note() {
    // Test that we can parse and reconstruct commands correctly
    let _commands = vec![
        "GET mykey\r\n",
        "SET mykey 0 300 5\r\n",
        "SET mykey 123 600 10 noreply\r\n",
        "DELETE mykey\r\n",
        "DELETE mykey noreply\r\n",
        "INCR counter 5\r\n",
        "DECR counter 3 noreply\r\n",
        "FLUSH_ALL\r\n",
        "FLUSH_ALL 300\r\n",
        "STATS\r\n",
        "STATS items\r\n",
        "VERSION\r\n",
        "QUIT\r\n",
    ];

    // This function just holds a note about removed tests
    // The actual tests are in tests/protocol_integration_test.rs
}
