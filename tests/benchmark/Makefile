# Bifrost Benchmark Suite Makefile
# Handles benchmark execution and configuration

.PHONY: all clean list help run-suite

# Default target
all: help

# Configuration
SCRIPT_DIR := $(shell pwd)
RESULTS_DIR := ./benchmark_results
DOCKER_COMPOSE_FILE := docker-compose-benchmark.yml
BENCHMARK_SCRIPT := ./run_benchmark.sh

# List all available benchmark suites
list:
	@echo "Available benchmark suites:"
	@ls -1 configs/*.yaml | sed 's/configs\///' | sed 's/\.yaml//' | sed 's/^/  - /'

# Show help
help:
	@echo "Bifrost Benchmark Suite"
	@echo ""
	@echo "Usage:"
	@echo "  make run-suite SUITE=<suite_name>     # Run specific benchmark suite"
	@echo "  make list                             # List available suites"
	@echo "  make clean                            # Clean results"
	@echo ""
	@echo "Examples:"
	@echo "  make run-suite SUITE=stress_test"
	@echo "  make run-suite SUITE=light_read_heavy"
	@echo ""
	@echo "Available suites:"
	@$(MAKE) list

# Run benchmark using YAML config file
run-suite:
	@if [ "$(SUITE)" = "all" ]; then \
		echo "🚀 Running all benchmark suites..."; \
		for config in configs/*.yaml; do \
			suite_name=$$(basename "$$config" .yaml); \
			echo ""; \
			echo "=== Running $$suite_name ==="; \
			$(MAKE) run-suite SUITE=$$suite_name; \
		done; \
		exit 0; \
	fi
	@if [ -z "$(SUITE)" ]; then \
		echo "❌ Error: SUITE not specified"; \
		echo "Usage: make run-suite SUITE=<suite_name>"; \
		echo ""; \
		$(MAKE) list; \
		exit 1; \
	fi
	@if [ ! -f "configs/$(SUITE).yaml" ]; then \
		echo "❌ Error: Suite 'configs/$(SUITE).yaml' not found"; \
		echo ""; \
		$(MAKE) list; \
		exit 1; \
	fi
	@echo "🚀 Running benchmark suite: $(SUITE)"
	$(BENCHMARK_SCRIPT) --config "configs/$(SUITE).yaml"

# Clean benchmark results
clean:
	@echo "🧹 Cleaning benchmark results..."
	rm -rf "$(RESULTS_DIR)"/*
	@echo "✅ Cleaned"

# Build bifrost image
build-bifrost:
	@echo "🏗️  Building Bifrost..."
	cd ../.. && docker buildx build --platform linux/amd64 -t bifrost:amd64 --load .

# Setup benchmark infrastructure
setup:
	@echo "🐳 Setting up benchmark infrastructure..."
	docker-compose -f $(DOCKER_COMPOSE_FILE) down > /dev/null 2>&1 || true
	docker-compose -f $(DOCKER_COMPOSE_FILE) up -d --build
	@echo "⏳ Waiting for services..."
	sleep 30
	@echo "✅ Infrastructure ready"

# Check if services are healthy
check-health:
	@echo "🔍 Checking service health..."
	@for port in 11211 11212 11213 11214 11215 22122 22123; do \
		if ! nc -z localhost "$$port" 2>/dev/null; then \
			echo "❌ Service on port $$port not ready"; \
			docker-compose -f $(DOCKER_COMPOSE_FILE) logs; \
			exit 1; \
		fi \
	done
	@echo "✅ All services ready"
