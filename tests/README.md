# Bifrost Tests

This directory contains tests for the Bifrost proxy.

## Test Categories

### Unit Tests
Individual component tests in Rust files (e.g., `connection_pool_test.rs`, `route_table_test.rs`)

### Integration Tests
End-to-end tests in the `e2e/` directory

### Performance Tests
Benchmark suite in the `benchmark/` directory

## End-to-End Tests

The `e2e/` directory contains end-to-end tests that test the complete proxy functionality.

### Prerequisites

1. **Docker & Docker Compose** - For running memcached backends
2. **Netcat (`nc`)** - For testing network connections
3. **Bifrost proxy** - Must be running

### Running E2E Tests

1. **Start the memcached backends:**
   ```bash
   docker-compose up -d
   ```

2. **Start the Bifrost proxy:**
   ```bash
   cargo run
   ```

3. **Run the comprehensive test suite:**
   ```bash
   ./tests/e2e/proxy_test.sh
   ```

### Individual Test Scripts

- `proxy_test.sh` - Main comprehensive test suite
- `test_memcached.sh` - Basic memcached functionality tests
- `interactive_test.sh` - Interactive testing with persistent connections

### Test Coverage

The comprehensive test suite covers:

- ✅ **Basic Operations**: SET, GET, DELETE commands
- ✅ **Data Persistence**: Verifies data is stored in backend
- ✅ **Error Handling**: Nonexistent keys, invalid commands
- ✅ **Multiple Operations**: Sequential commands in single connection
- ✅ **Proxy Forwarding**: Bidirectional client ↔ backend communication

## Performance Benchmarks

The `benchmark/` directory contains a comprehensive benchmark suite that compares Bifrost performance against MCRouter.

### Quick Start

```bash
cd tests/benchmark
./run_benchmark.sh stress_test
# or
make benchmark
```

### What's Tested

- **6 different workload scenarios** (light, medium, high load)
- **Throughput comparison** (ops/sec)
- **Latency analysis** (P50, P95, P99 percentiles)
- **Connection pooling performance**
- **Stress testing**

### Results

Benchmark results are stored in `tests/benchmark/benchmark_results/` with detailed analysis and comparison reports.

See `tests/benchmark/README.md` for complete documentation.

## Adding New Tests

To add new tests to the main test suite:

1. Create a new test function in `e2e/proxy_test.sh`
2. Add the test to the main execution flow
3. Follow the existing pattern using `run_test` helper function

### CI/CD Integration

The test scripts are designed to be CI/CD friendly:
- Exit code 0 on success, 1 on failure
- Colored output for human readability
- Detailed error reporting
- Automatic cleanup and setup