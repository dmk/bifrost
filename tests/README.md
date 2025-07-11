# Bifrost Tests

This directory contains tests for the Bifrost proxy.

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

### Adding New Tests

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