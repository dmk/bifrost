#!/bin/bash

# Bifrost Proxy End-to-End Tests
# This script tests the complete proxy functionality

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
PROXY_HOST="localhost"
PROXY_PORT="11211"
BACKEND_HOST="localhost"
BACKEND_PORT="11212"

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Helper functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

run_test() {
    local test_name="$1"
    local test_command="$2"
    local expected_pattern="$3"

    echo -e "\n${YELLOW}Running test: $test_name${NC}"
    TESTS_RUN=$((TESTS_RUN + 1))

    # Run the test command
    local result
    result=$(eval "$test_command" 2>&1)

    # Check if expected pattern is found (with multiline support)
    if echo "$result" | grep -Pzo "$expected_pattern" > /dev/null 2>&1 || echo "$result" | tr '\n' ' ' | grep -q "$expected_pattern"; then
        log_info "✓ PASSED: $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        log_error "✗ FAILED: $test_name"
        log_error "Expected pattern: $expected_pattern"
        log_error "Actual result: $result"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

# Check if proxy is running
check_proxy_running() {
    log_info "Checking if Bifrost proxy is running..."
    if ! nc -z $PROXY_HOST $PROXY_PORT 2>/dev/null; then
        log_error "Proxy is not running on $PROXY_HOST:$PROXY_PORT"
        log_error "Please start the proxy with: cargo run"
        exit 1
    fi
    log_info "Proxy is running on $PROXY_HOST:$PROXY_PORT"
}

# Check if backend is running
check_backend_running() {
    log_info "Checking if backend memcached is running..."
    if ! nc -z $BACKEND_HOST $BACKEND_PORT 2>/dev/null; then
        log_error "Backend is not running on $BACKEND_HOST:$BACKEND_PORT"
        log_error "Please start the backend with: docker-compose up"
        exit 1
    fi
    log_info "Backend is running on $BACKEND_HOST:$BACKEND_PORT"
}

# Clean up backend before tests
cleanup_backend() {
    log_info "Cleaning up backend memcached..."
    echo -e "flush_all\r\nquit\r\n" | nc $BACKEND_HOST $BACKEND_PORT > /dev/null 2>&1 || true
}

# Test functions
test_proxy_set_command() {
    local cmd="{ printf 'set test_key 0 0 5\r\nhello\r\n'; sleep 1; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT"
    run_test "Proxy SET command" "$cmd" "STORED"
}

test_proxy_get_command() {
    local cmd="{ printf 'get test_key\r\n'; sleep 1; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT"
    run_test "Proxy GET command" "$cmd" "VALUE test_key 0 5"
}

test_proxy_get_value() {
    local cmd="{ printf 'get test_key\r\n'; sleep 1; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT"
    run_test "Proxy GET value" "$cmd" "hello"
}

test_proxy_nonexistent_key() {
    local cmd="{ printf 'get nonexistent_key\r\n'; sleep 1; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT"
    run_test "Proxy GET nonexistent key" "$cmd" "END"
}

test_proxy_delete_command() {
    # First set a key
    { printf 'set delete_test 0 0 4\r\ntest\r\n'; sleep 1; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT > /dev/null 2>&1

    # Then delete it
    local cmd="{ printf 'delete delete_test\r\n'; sleep 1; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT"
    run_test "Proxy DELETE command" "$cmd" "DELETED"
}

test_data_persistence() {
    # Set data through proxy (persist = 7 characters)
    local set_result
    set_result=$({ printf 'set persist_test 0 0 7\r\npersist\r\n'; sleep 1; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT 2>&1)

    # Verify the SET worked
    if ! echo "$set_result" | grep -q "STORED"; then
        log_error "Failed to set data through proxy: $set_result"
        return 1
    fi

    # Verify it's in backend directly
    local cmd="{ printf 'get persist_test\r\n'; sleep 1; printf 'quit\r\n'; } | nc $BACKEND_HOST $BACKEND_PORT"
    run_test "Data persistence to backend" "$cmd" "persist"
}

test_multiple_operations() {
    # val1 and val2 are both 4 characters each
    local cmd="{ printf 'set multi1 0 0 4\r\nval1\r\nset multi2 0 0 4\r\nval2\r\nget multi1\r\nget multi2\r\n'; sleep 2; printf 'quit\r\n'; } | nc $PROXY_HOST $PROXY_PORT"
    run_test "Multiple operations" "$cmd" "STORED.*STORED.*VALUE multi1.*val1.*VALUE multi2.*val2"
}

# Main test execution
main() {
    log_info "Starting Bifrost Proxy End-to-End Tests"
    log_info "======================================="

    # Pre-test checks
    check_proxy_running
    check_backend_running
    cleanup_backend

    # Run tests
    log_info "Running tests..."

    test_proxy_set_command
    test_proxy_get_command
    test_proxy_get_value
    test_proxy_nonexistent_key
    test_proxy_delete_command
    test_data_persistence
    test_multiple_operations

    # Test summary
    echo -e "\n${YELLOW}Test Summary:${NC}"
    echo "============="
    echo "Total tests: $TESTS_RUN"
    echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
    echo -e "${RED}Failed: $TESTS_FAILED${NC}"

    if [ $TESTS_FAILED -eq 0 ]; then
        log_info "🎉 All tests passed!"
        exit 0
    else
        log_error "❌ Some tests failed!"
        exit 1
    fi
}

# Run main function
main "$@"