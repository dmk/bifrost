# Integration Test Framework

This directory contains the integration test framework for Bifrost, providing reusable helpers and comprehensive test scenarios.

## Structure

```
integration/
├── README.md                      # This file
├── mod.rs                         # Module exports
├── helpers/                       # Test infrastructure
│   ├── mod.rs
│   ├── mock_memcached.rs         # Mock memcached backend
│   └── test_client.rs            # Test client for memcached protocol
└── scenarios/                     # Test scenarios
    ├── mod.rs
    ├── protocol.rs                # Protocol integration tests
    ├── pool.rs                    # Pool and strategy tests
    └── failover.rs                # Failover behavior tests
```

## Test Helpers

### MockMemcached

A configurable mock memcached server for testing without external dependencies.

**Features:**
- Normal operation mode (parses and responds to commands)
- AlwaysMiss mode (simulates cache misses)
- AlwaysSuccess mode (always returns success)
- Failure mode (simulates backend failures)
- Slow mode (adds artificial latency)
- Custom response mode
- Request recording for assertions
- Statistics tracking
- Preloaded data support

**Example:**
```rust
use integration::{MockMemcached, MockMemcachedBuilder, ResponseMode};

// Simple mock
let mock = MockMemcached::new().await?;

// Configured mock
let mock = MockMemcachedBuilder::new()
    .response_mode(ResponseMode::AlwaysMiss)
    .latency(Duration::from_millis(50))
    .with_data("key1".to_string(), b"value1".to_vec())
    .build()
    .await?;

// Use the mock
let addr = mock.addr();
// ... connect and test ...

// Check stats
let stats = mock.stats();
assert_eq!(stats.gets, 5);

mock.shutdown();
```

### TestClient

A simple memcached protocol client for testing.

**Features:**
- Typed command methods (get, set, delete, etc.)
- Raw command sending
- Response parsing
- Pipeline support
- Simple API for common operations

**Example:**
```rust
use integration::TestClient;

let mut client = TestClient::connect("127.0.0.1:11211").await?;

// Type-safe commands
assert!(client.set("key", "value", 0).await?);
let value = client.get("key").await?;
assert_eq!(value, Some("value".to_string()));

// Raw commands
let response = client.send_command("VERSION").await?;

// Cleanup
client.quit().await?;
```

## Test Scenarios

### Protocol Tests (`scenarios/protocol.rs`)

End-to-end tests for protocol handling with mock backends.

**Coverage:**
- Basic GET/SET/DELETE operations
- VERSION and STATS commands
- Slow backend behavior
- Cache miss scenarios
- Multiple operations in sequence
- Preloaded data

**Note:** Pure protocol parsing tests (command reconstruction, error handling, etc.) remain in the source code as unit tests (`src/core/protocols/ascii.rs`).

### Pool Tests (`scenarios/pool.rs`)

Tests for pool selection strategies and backend management.

**Coverage:**
- Round-robin distribution
- Blind forward strategy
- Failover strategy
- Strategy factory
- Empty pool behavior
- Multiple pools with different strategies
- Backend properties

### Failover Tests (`scenarios/failover.rs`)

Tests for failover configuration and behavior.

**Coverage:**
- Failover config loading
- Miss-failover config loading
- Multi-tier failover setups
- Strategy factory for failover
- Route table integration

## Running Tests

```bash
# Run all integration tests
cargo test --test integration_tests

# Run specific scenario
cargo test --test integration_tests protocol

# Run with output
cargo test --test integration_tests -- --nocapture

# Run single-threaded (useful for debugging)
cargo test --test integration_tests -- --test-threads=1
```

## Adding New Tests

### 1. Simple Integration Test

Add to an existing scenario file:

```rust
#[tokio::test]
async fn test_my_feature() {
    let mock = MockMemcached::new().await.unwrap();
    let mut client = TestClient::connect(mock.addr()).await.unwrap();
    
    // Your test logic here
    
    client.quit().await.unwrap();
    mock.shutdown();
}
```

### 2. New Test Scenario

Create a new file in `scenarios/`:

```rust
// scenarios/my_feature.rs

use crate::integration::{MockMemcached, TestClient};

#[tokio::test]
async fn test_my_scenario() {
    // Test implementation
}
```

Add to `scenarios/mod.rs`:
```rust
pub mod my_feature;
```

### 3. Complex Mock Behavior

Use `MockMemcachedBuilder` for custom configurations:

```rust
let mock = MockMemcachedBuilder::new()
    .response_mode(ResponseMode::Slow(Duration::from_millis(100)))
    .with_data("key1".to_string(), b"value1".to_vec())
    .with_data("key2".to_string(), b"value2".to_vec())
    .build()
    .await
    .unwrap();
```

## Best Practices

1. **Use `MockMemcached` for unit-level integration tests** - Fast, no dependencies
2. **Use real memcached for E2E tests** - In `tests/e2e/` directory
3. **Always cleanup** - Call `client.quit()` and `mock.shutdown()`
4. **Test one thing per test** - Keep tests focused and readable
5. **Use descriptive names** - `test_protocol_handles_slow_backend` vs `test1`
6. **Check statistics** - Use `mock.stats()` to verify behavior
7. **Avoid timeouts** - Keep tests fast (< 100ms each)

## Design Philosophy

- **Fast**: No Docker startup, tests run in milliseconds
- **Deterministic**: Full control over backend behavior
- **Isolated**: Each test gets its own mock backend
- **Comprehensive**: Cover edge cases impossible with real backends
- **Maintainable**: Clear structure, reusable helpers

## Limitations

- **Protocol simplification**: Mock doesn't implement full memcached protocol
- **No clustering**: Single backend simulation only
- **Limited pipelining**: Complex pipelined requests may not work
- **Storage is in-memory**: No persistence testing

For these scenarios, use the E2E tests in `tests/e2e/` with real memcached.
