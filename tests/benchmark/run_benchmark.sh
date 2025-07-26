#!/bin/bash

# Direct Benchmark Execution - Bifrost vs MCRouter
# Runs benchmarks directly from host using docker run

set -e

echo "🚀 Bifrost vs MCRouter Benchmark"
echo "======================================="
echo
set -e

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "❌ Docker is not running. Please start Docker and try again."
    exit 1
fi

# Check if docker-compose is available
if ! command -v docker-compose &> /dev/null; then
    echo "❌ docker-compose is not installed. Please install it and try again."
    exit 1
fi

echo "✅ Docker is running"
echo

# Build bifrost if needed
echo "📦 Building Bifrost..."
if ! docker build -t bifrost:benchmark . > /dev/null 2>&1; then
    echo "❌ Failed to build Bifrost. Please check your Docker setup."
    exit 1
fi
echo "✅ Bifrost built successfully"
echo

# Start all services
echo "🐳 Starting benchmark infrastructure..."
docker-compose -f docker-compose-benchmark.yml down > /dev/null 2>&1 || true
docker-compose -f docker-compose-benchmark.yml up -d

echo "⏳ Waiting for all services to be ready..."
sleep 30

# Check service health
echo "🔍 Checking service health..."

services=("memcached1:11211" "memcached2:11212" "memcached3:11213" "memcached4:11214" "memcached5:11215")
for service in "${services[@]}"; do
    if ! nc -z localhost "${service#*:}" 2>/dev/null; then
        echo "❌ Service $service is not ready"
        docker-compose -f docker-compose-benchmark.yml logs
        exit 1
    fi
done

# Check proxies
if ! nc -z localhost 22122 2>/dev/null; then
    echo "❌ Bifrost (port 22122) is not ready"
    docker-compose -f docker-compose-benchmark.yml logs bifrost
    exit 1
fi

if ! nc -z localhost 22123 2>/dev/null; then
    echo "❌ MCRouter (port 22123) is not ready"
    docker-compose -f docker-compose-benchmark.yml logs mcrouter
    exit 1
fi

echo "✅ All services are ready"
echo

# Configuration
NETWORK="bifrost_default"
RESULTS_DIR="./benchmark_results"

# Create results directory
mkdir -p "$RESULTS_DIR"

echo "📊 Running direct benchmarks..."
echo

# Function to run a single benchmark test
run_test() {
    local proxy_name="$1"
    local host="$2"
    local port="$3"
    local test_name="$4"
    local threads="$5"
    local clients="$6"
    local requests="$7"
    local ratio="$8"
    local value_size="$9"

    echo "🔧 Testing $proxy_name: $test_name"

    local output_file="$RESULTS_DIR/${proxy_name}_${test_name}.txt"

    docker run --rm \
        --network "$NETWORK" \
        -v "$(pwd)/$RESULTS_DIR:/results" \
        redislabs/memtier_benchmark:latest \
        memtier_benchmark \
        --server="$host" \
        --port="$port" \
        --protocol=memcache_text \
        --threads="$threads" \
        --clients="$clients" \
        --requests="$requests" \
        --ratio="$ratio" \
        --data-size="$value_size" \
        --key-pattern=R:R \
        --print-percentiles=50,90,95,99,99.9 \
        --run-count=3 \
        --out-file="/results/${proxy_name}_${test_name}.txt" \
        --json-out-file="/results/${proxy_name}_${test_name}.json"

    echo "✅ $proxy_name $test_name completed"
    echo
}

# Connectivity tests first
echo "🔍 Testing connectivity..."

echo "Testing Bifrost..."
if docker run --rm --network "$NETWORK" redislabs/memtier_benchmark:latest \
    memtier_benchmark --server=bifrost --port=22122 --protocol=memcache_text \
    --test-time=2 --clients=1 --threads=1 --ratio=1:1 > /dev/null 2>&1; then
    echo "✅ Bifrost is accessible"
else
    echo "❌ Bifrost connection failed"
    exit 1
fi

echo "Testing MCRouter..."
if docker run --rm --network "$NETWORK" redislabs/memtier_benchmark:latest \
    memtier_benchmark --server=mcrouter --port=11211 --protocol=memcache_text \
    --test-time=2 --clients=1 --threads=1 --ratio=1:1 > /dev/null 2>&1; then
    echo "✅ MCRouter is accessible"
else
    echo "❌ MCRouter connection failed"
    exit 1
fi

echo
echo "🚀 Starting benchmark tests..."
echo

# Test 1: Light Load (Read Heavy)
echo "=== Test 1: Light Load (Read Heavy) ==="
run_test "bifrost" "bifrost" "22122" "light_read_heavy" 2 5 1000 "10:1" 1024
run_test "mcrouter" "mcrouter" "11211" "light_read_heavy" 2 5 1000 "10:1" 1024

# Test 2: Light Load (Write Heavy)
echo "=== Test 2: Light Load (Write Heavy) ==="
run_test "bifrost" "bifrost" "22122" "light_write_heavy" 2 5 1000 "1:10" 1024
run_test "mcrouter" "mcrouter" "11211" "light_write_heavy" 2 5 1000 "1:10" 1024

# Test 3: Medium Load (Balanced)
echo "=== Test 3: Medium Load (Balanced) ==="
run_test "bifrost" "bifrost" "22122" "medium_balanced" 4 10 5000 "1:1" 1024
run_test "mcrouter" "mcrouter" "11211" "medium_balanced" 4 10 5000 "1:1" 1024

# Test 4: High Load (Read Heavy)
echo "=== Test 4: High Load (Read Heavy) ==="
run_test "bifrost" "bifrost" "22122" "high_read_heavy" 8 20 10000 "10:1" 1024
run_test "mcrouter" "mcrouter" "11211" "high_read_heavy" 8 20 10000 "10:1" 1024

# Test 5: Large Values
echo "=== Test 5: Large Values ==="
run_test "bifrost" "bifrost" "22122" "large_values" 4 10 5000 "3:1" 8192
run_test "mcrouter" "mcrouter" "11211" "large_values" 4 10 5000 "3:1" 8192

# Test 6: Stress Test
echo "=== Test 6: Stress Test ==="
run_test "bifrost" "bifrost" "22122" "stress_test" 16 25 20000 "5:1" 2048
run_test "mcrouter" "mcrouter" "11211" "stress_test" 16 25 20000 "5:1" 2048

echo "🎉 All benchmarks completed!"
echo
echo "📁 Results saved in: $RESULTS_DIR"
echo "📊 Generated files:"
ls -la "$RESULTS_DIR"
echo

# Run analysis if available
if command -v python3 &> /dev/null && [ -f "benchmark_analysis.py" ]; then
    echo "🔍 Running analysis..."
    python3 benchmark_analysis.py --results-dir "$RESULTS_DIR"
else
    echo "💡 To analyze results: python3 benchmark_analysis.py --results-dir $RESULTS_DIR"
fi

echo "✅ Benchmark suite completed successfully!"