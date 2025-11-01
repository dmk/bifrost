#!/bin/bash

# Bifrost vs MCRouter Benchmark Runner
# Runs benchmarks using Docker Compose infrastructure

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NETWORK="benchmark_default"
RESULTS_DIR="./benchmark_results"

# Parse command line arguments
SUITE_FILE=""
SPECIFIC_TEST=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --config)
      SUITE_FILE="$2"
      shift 2
      ;;
    -*)
      echo "Unknown option: $1"
      exit 1
      ;;
    *)
      SPECIFIC_TEST="$1"
      shift
      ;;
  esac
done

# If no specific suite file given, but a test name is provided,
# assume it's a suite name and construct the config file path
if [ -z "$SUITE_FILE" ] && [ -n "$SPECIFIC_TEST" ]; then
  SUITE_FILE="configs/${SPECIFIC_TEST}.yaml"
fi

# Require a suite file
if [ -z "$SUITE_FILE" ]; then
  echo "❌ Error: No suite specified"
  echo "Usage: $0 --config <config_file> | <suite_name>"
  echo "Examples:"
  echo "  $0 --config configs/stress_test.yaml"
  echo "  $0 stress_test"
  exit 1
fi

# YAML parsing functions
parse_yaml() {
  local yaml_file="$1"

  # Use yq if available for proper YAML parsing
  if command -v yq &> /dev/null; then
    # Use yq to flatten the YAML into key=value pairs
    yq -o=props "$yaml_file" 2>/dev/null | sed 's/=/ /' | while read -r key value; do
      echo "$key=$value"
    done
  else
    # Basic YAML parser for simple key-value structures
    awk '
    BEGIN {
      in_block = 0
      block_indent = 0
      indent_stack[0] = 0
      indent_level = 0
      prefix_stack[0] = ""
    }

    /^[[:space:]]*[^[:space:]#]/ {
      # Count leading spaces
      match($0, /^[[:space:]]*/)
      current_indent = RLENGTH

      # Remove comments
      sub(/[[:space:]]*#.*$/, "")

      # Skip empty lines
      if ($0 == "") next

      # Determine indent level change
      if (current_indent > indent_stack[indent_level]) {
        indent_level++
        indent_stack[indent_level] = current_indent
      } else if (current_indent < indent_stack[indent_level]) {
        while (indent_level > 0 && current_indent < indent_stack[indent_level]) {
          indent_level--
        }
      }

      # Build current prefix
      current_prefix = ""
      for (i = 1; i <= indent_level; i++) {
        if (prefix_stack[i] != "") {
          if (current_prefix != "") current_prefix = current_prefix "."
          current_prefix = current_prefix prefix_stack[i]
        }
      }

      # Handle block scalars (|)
      if ($0 ~ /\|$/) {
        in_block = 1
        block_indent = current_indent
        sub(/\|$/, "")
        key = $0
        sub(/:$/, "", key)
        # Trim spaces from key
        sub(/^[[:space:]]*/, "", key)
        sub(/[[:space:]]*$/, "", key)
        next
      }

      if (in_block) {
        if (current_indent > block_indent || $0 == "") {
          # Continuation of block
          if ($0 != "") {
            block_content = block_content "\n" $0
          }
        } else {
          # End of block
          full_key = current_prefix
          if (full_key != "") full_key = full_key "."
          full_key = full_key key
          print full_key "=" block_content
          in_block = 0
          block_content = ""
          # Process current line after block
        }
      }

      if (!in_block) {
        if ($0 ~ /:/ && $0 !~ /\[.*\]/) {
          # Split on colon followed by one or more spaces (not zero)
          split($0, parts, /:[[:space:]]+/)
          key = parts[1]
          # Reconstruct value from remaining parts (in case value contains colons)
          value = ""
          for (i = 2; i <= length(parts); i++) {
            if (value != "") value = value ":"
            value = value parts[i]
          }

          # Trim spaces from key
          sub(/^[[:space:]]*/, "", key)
          sub(/[[:space:]]*$/, "", key)

          # Trim spaces from value and remove quotes
          sub(/^[[:space:]]*/, "", value)
          sub(/[[:space:]]*$/, "", value)
          gsub(/^["\047]|["\047]$/, "", value)

          if (value == "") {
            # This is a section header (like "memtier:")
            # Remove trailing colon from key
            section_key = key
            sub(/:$/, "", section_key)
            prefix_stack[indent_level + 1] = section_key
          } else {
            # This is a key-value pair
            full_key = current_prefix
            if (full_key != "") full_key = full_key "."
            full_key = full_key key
            print full_key "=" value
          }
        }
      }
    }
    END {
      if (in_block) {
        full_key = current_prefix
        if (full_key != "" && key != "") full_key = full_key "."
        full_key = full_key key
        print full_key "=" block_content
      }
    }
    ' "$yaml_file"
  fi
}

load_config() {
  local config_file="$1"

  if [ ! -f "$config_file" ]; then
    echo "❌ Config file not found: $config_file"
    exit 1
  fi

  echo "📄 Loading config: $config_file"

  # Parse YAML and export variables
  while IFS='=' read -r key value; do
    # Convert YAML keys to bash variables
    # memtier.threads -> MEMTIER_THREADS
    bash_key=$(echo "$key" | tr '[:lower:]' '[:upper:]' | tr '.' '_')
    export "$bash_key"="$value"
  done < <(parse_yaml "$config_file")
}

echo "🚀 Bifrost vs MCRouter Benchmark"
echo "   Suite: $SUITE_FILE"
echo "======================================="
echo

# Load suite configuration
load_config "$SUITE_FILE"

# Set test parameters from config
TEST_NAME="${NAME:-$(basename "$SUITE_FILE" .yaml)}"
DESCRIPTION="${DESCRIPTION:-$TEST_NAME}"
THREADS="${MEMTIER_THREADS:-4}"
CLIENTS="${MEMTIER_CLIENTS:-10}"
REQUESTS="${MEMTIER_REQUESTS:-1000}"
RATIO="${MEMTIER_RATIO:-1:1}"
DATA_SIZE="${MEMTIER_DATA_SIZE:-1024}"
RUN_COUNT="${MEMTIER_RUN_COUNT:-3}"
KEY_PATTERN="${MEMTIER_KEY_PATTERN:-R:R}"
PRINT_PERCENTILES="${MEMTIER_PRINT_PERCENTILES:-50,90,95,99,99.9}"
BIFROST_CONFIG="${BIFROST_CONFIG:-benchmark_config.yaml}"

# Check Docker
if ! docker info > /dev/null 2>&1; then
    echo "❌ Docker is not running"
    exit 1
fi

# Build Bifrost image
echo "🏗️  Building Bifrost..."
if ! docker buildx inspect > /dev/null 2>&1; then
    docker buildx create --use --name bifrost-builder >/dev/null 2>&1 || true
fi
docker buildx build --platform linux/amd64 -t bifrost:amd64 --load ../.. || exit 1
echo "✅ Built bifrost:amd64"
echo

# Start services
echo "🐳 Starting services..."
docker-compose -f docker-compose-benchmark.yml down > /dev/null 2>&1 || true
docker-compose -f docker-compose-benchmark.yml up -d --build
echo "⏳ Waiting for services..."
sleep 30

# Check service health
for port in 11211 11212 11213 11214 11215 22122 22123; do
    if ! nc -z localhost "$port" 2>/dev/null; then
        echo "❌ Service on port $port not ready"
        docker-compose -f docker-compose-benchmark.yml logs
        exit 1
    fi
done
echo "✅ All services ready"
echo

# Create results directory
mkdir -p "$RESULTS_DIR"
rm -rf "$RESULTS_DIR/*"

# Execute a benchmark
execute_benchmark() {
    local proxy_name="$1"
    local host="$2"
    local port="$3"
    local test_name="$4"
    local description="$5"
    local threads="$6"
    local clients="$7"
    local requests="$8"
    local ratio="$9"
    local data_size="${10}"

    echo "🔧 Testing $proxy_name: $test_name ($description)"

    docker run --rm \
        --platform linux/amd64 \
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
        --data-size="$data_size" \
        --key-pattern="$KEY_PATTERN" \
        --print-percentiles="$PRINT_PERCENTILES" \
        --run-count="$RUN_COUNT" \
        --out-file="/results/${proxy_name}_${test_name}.txt" \
        --json-out-file="/results/${proxy_name}_${test_name}.json"

    echo "✅ $proxy_name $test_name completed"
    echo
}

# Run the benchmark
echo "🚀 Running benchmark..."
echo

echo "=== $DESCRIPTION ==="
execute_benchmark "bifrost" "bifrost" "22122" "$TEST_NAME" "$DESCRIPTION" "$THREADS" "$CLIENTS" "$REQUESTS" "$RATIO" "$DATA_SIZE"
execute_benchmark "mcrouter" "mcrouter" "11211" "$TEST_NAME" "$DESCRIPTION" "$THREADS" "$CLIENTS" "$REQUESTS" "$RATIO" "$DATA_SIZE"

# Show results
echo "🎉 Benchmarks completed!"
echo
echo "📁 Results: $RESULTS_DIR"
ls -lh "$RESULTS_DIR"
echo

# Run analysis
if command -v python3 &> /dev/null && [ -f "benchmark_analysis.py" ]; then
    echo "🔍 Running analysis..."
    python3 benchmark_analysis.py --results-dir "$RESULTS_DIR"
else
    echo "💡 To analyze: python3 benchmark_analysis.py --results-dir $RESULTS_DIR"
fi

echo "✅ Done!"
