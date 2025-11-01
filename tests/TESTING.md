# Bifrost Testing Guide

This document provides an overview of the testing infrastructure and best practices for Bifrost.

## Test Structure

```
tests/
├── TESTING.md                      # This document
├── README.md                       # E2E and benchmark tests
├── integration/                    # Integration test framework
│   ├── README.md                   # Framework documentation
│   ├── mod.rs
│   ├── helpers/
│   │   ├── mock_memcached.rs      # Mock memcached backend
│   │   └── test_client.rs         # Test memcached client
│   └── scenarios/
│       ├── protocol.rs             # Protocol integration tests
│       ├── pool.rs                 # Pool/strategy tests
│       └── failover.rs             # Failover behavior tests
├── integration_tests.rs            # Integration test runner
├── config_loading_test.rs          # Config parsing tests
├── connection_pool_test.rs         # Connection pool tests
├── route_table_test.rs             # Routing tests
├── e2e/                            # End-to-end shell scripts
│   └── proxy_test.sh
└── benchmark/                      # Performance benchmarks
    ├── run_benchmark.sh
    └── ...
```

## Test Categories

### 1. Unit Tests (in source files)

Located in `src/**/*.rs` files with `#[cfg(test)]` modules.

**What they test:**
- Pure functions
- Individual components in isolation
- Protocol parsing logic
- Data structures

**Example:**
```bash
# Run all unit tests
cargo test --lib

# Run specific module
cargo test --lib protocols
```

**Speed:** ⚡ Very fast (< 100ms typically)

### 2. Integration Tests

Located in `tests/integration/` using the custom test framework.

**What they test:**
- Component interaction with mock backends
- Pool selection strategies
- Failover configuration
- Protocol handling end-to-end
- Mock backend behavior

**Example:**
```bash
# Run all integration tests
cargo test --test integration_tests

# Run specific scenario
cargo test --test integration_tests protocol
cargo test --test integration_tests pool
cargo test --test integration_tests failover

# Run with output
cargo test --test integration_tests -- --nocapture
```

**Speed:** ⚡ Fast (27 tests in 60ms)

**Key Features:**
- ✅ No external dependencies (Docker, memcached, etc.)
- ✅ Deterministic (full control over backend behavior)
- ✅ Can simulate failures, latency, cache misses
- ✅ Request recording for detailed assertions
- ✅ CI-friendly

### 3. Standard Integration Tests

Located in `tests/*.rs` files (excluding `integration/`).

**What they test:**
- Config loading and validation
- Connection pool behavior
- Route table construction
- Component wiring

**Example:**
```bash
# Run specific test file
cargo test --test config_loading_test
cargo test --test route_table_test
```

**Speed:** ⚡ Fast (< 1s each)

### 4. End-to-End Tests

Located in `tests/e2e/` as shell scripts.

**What they test:**
- Full proxy functionality with real memcached
- Docker compose integration
- Multi-backend scenarios
- Real network behavior

**Example:**
```bash
# Start backends
docker-compose up -d

# Run proxy
cargo run &

# Run tests
./tests/e2e/proxy_test.sh
```

**Speed:** 🐌 Slower (requires Docker startup)

**When to use:**
- Verify real memcached compatibility
- Test production-like deployment
- Validate networking behavior

### 5. Benchmarks

Located in `tests/benchmark/`.

**What they test:**
- Throughput (ops/sec)
- Latency (P50, P95, P99)
- Comparison vs MCRouter
- Stress testing

**Example:**
```bash
cd tests/benchmark
./run_benchmark.sh stress_test
# or run all benchmarks
make benchmark
```

**Speed:** 🐌 Slow (minutes)

## Test Matrix Summary

| Type | Location | Speed | Dependencies | Use Case |
|------|----------|-------|--------------|----------|
| Unit | `src/**/*.rs` | ⚡⚡⚡ | None | Pure logic |
| Integration | `tests/integration/` | ⚡⚡ | None | Component interaction |
| Standard Integration | `tests/*.rs` | ⚡⚡ | None | Module wiring |
| E2E | `tests/e2e/` | 🐌 | Docker, memcached | Real-world validation |
| Benchmark | `tests/benchmark/` | 🐌🐌 | Docker, memtier | Performance |

## Running Tests

### Quick Test (Development)

```bash
# Run all fast tests (unit + integration)
cargo test

# Output: 129 tests pass in ~12 seconds
```

### Full Test Suite

```bash
# 1. Fast tests
cargo test

# 2. Clippy (linting)
make clippy

# 3. Format check
make fmt

# 4. E2E tests (requires Docker)
docker-compose up -d
cargo run &
./tests/e2e/proxy_test.sh
```

### CI/CD Pipeline

```bash
# Typical CI workflow
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt -- --check
```

## Current Test Coverage

**Total: 129 tests**
- 88 unit tests (source files)
- 27 integration tests (mock framework)
- 14 standard integration tests (config, routing, pools)

**Execution Time:**
- Unit tests: ~1.4s
- Integration tests: ~60ms
- Standard integration: ~11s
- **Total:** ~12.5 seconds

## Writing New Tests

### Quick Integration Test

```rust
// tests/integration/scenarios/protocol.rs

use crate::integration::{MockMemcached, TestClient};

#[tokio::test]
async fn test_my_feature() {
    let mock = MockMemcached::new().await.unwrap();
    let mut client = TestClient::connect(mock.addr()).await.unwrap();

    // Test logic
    assert!(client.set("key", "value", 0).await.unwrap());
    assert_eq!(client.get("key").await.unwrap(), Some("value".to_string()));

    client.quit().await.unwrap();
    mock.shutdown();
}
```

### Advanced Mock Configuration

```rust
use crate::integration::{MockMemcachedBuilder, ResponseMode};
use std::time::Duration;

let mock = MockMemcachedBuilder::new()
    .response_mode(ResponseMode::Slow(Duration::from_millis(100)))
    .with_data("key1".to_string(), b"value1".to_vec())
    .with_data("key2".to_string(), b"value2".to_vec())
    .build()
    .await
    .unwrap();
```

## Best Practices

### ✅ DO

1. **Keep tests fast** - Use mocks for integration tests
2. **Test one thing** - Focused, single-purpose tests
3. **Use descriptive names** - `test_protocol_handles_cache_miss` not `test1`
4. **Clean up** - Always call `mock.shutdown()` and `client.quit()`
5. **Use assertions** - Verify behavior with `assert_eq!`, `assert!`
6. **Check statistics** - Use `mock.stats()` to verify request counts

### ❌ DON'T

1. **Don't use Docker in fast tests** - Reserve for E2E only
2. **Don't test multiple things** - Split into separate tests
3. **Don't add timeouts** - Tests should be deterministic
4. **Don't share state** - Each test should be independent
5. **Don't duplicate tests** - Protocol parsing is in source, not integration
6. **Don't commit commented tests** - Remove or fix them

## Debugging Tests

### Run Single Test

```bash
cargo test test_protocol_basic_get_set -- --nocapture
```

### Run with Debug Logging

```bash
RUST_LOG=debug cargo test test_name -- --nocapture
```

### Run Single-Threaded

```bash
cargo test -- --test-threads=1
```

### Check Specific Test File

```bash
cargo test --test integration_tests
```

## Test Performance

The integration test framework is designed for speed:

- **Mock backend startup:** < 1ms
- **Client connection:** < 1ms
- **Individual test:** 1-3ms average
- **27 tests total:** 60ms

This enables:
- ✅ Fast development feedback loop
- ✅ Quick CI/CD pipelines
- ✅ Extensive test coverage without slowdown

## Future Improvements

Potential areas for expansion:

1. **More failure scenarios**
   - Network partitions
   - Partial backend failures
   - Connection pool exhaustion

2. **Advanced protocol tests**
   - Binary protocol support
   - Pipelining with complex commands
   - Large value handling

3. **Performance regression tests**
   - Automated latency tracking
   - Throughput benchmarks in CI
   - Memory usage monitoring

4. **Chaos testing**
   - Random failures
   - Latency spikes
   - Resource constraints

## Questions?

See the detailed documentation:
- Integration framework: `tests/integration/README.md`
- E2E tests: `tests/README.md`
- Benchmarks: `tests/benchmark/README.md`
