# Bifrost Benchmark Suite

This directory contains all benchmark-related files for testing Bifrost performance against MCRouter.

## Directory Structure

```
tests/benchmark/
├── README.md                    # This file
├── BENCHMARK_README.md          # Detailed benchmark documentation
├── benchmark_config.yaml        # Bifrost configuration for benchmarks
├── docker-compose-benchmark.yml # Docker setup for benchmark infrastructure
├── run_benchmark_direct.sh      # Main benchmark execution script
├── benchmark_analysis.py        # Python script to analyze benchmark results
├── benchmark_client.py          # Benchmark client implementation
├── mcrouter-benchmark.json      # MCRouter configuration for benchmarks
├── mcrouter-config.json/        # Additional MCRouter config files
└── benchmark_results/           # Benchmark output and analysis results
```

## Quick Start

1. **Run benchmarks:**
   ```bash
   cd tests/benchmark
   ./run_benchmark_direct.sh
   ```

2. **View results:**
   ```bash
   cd tests/benchmark/benchmark_results
   ls -la
   ```

3. **Analyze results:**
   ```bash
   cd tests/benchmark
   python3 benchmark_analysis.py
   ```

## Benchmark Scenarios

The benchmark suite tests the following scenarios:

- **Light Load (Read Heavy)**: 1 thread, 1 connection, 10k requests
- **Light Load (Write Heavy)**: 1 thread, 1 connection, 10k requests
- **Medium Load (Balanced)**: 4 threads, 10 connections, 20k requests
- **High Load (Read Heavy)**: 8 threads, 20 connections, 160k requests
- **Large Values**: 1 thread, 1 connection, 20k requests with large values
- **Stress Test**: 16 threads, 25 connections, 320k requests

## Requirements

- Docker and Docker Compose
- Python 3.7+
- memcached (for local testing)
- mcrouter (for comparison)

## Configuration

- `benchmark_config.yaml`: Bifrost configuration with connection pooling
- `mcrouter-benchmark.json`: MCRouter configuration for fair comparison
- `docker-compose-benchmark.yml`: 5 memcached instances + services

## Results

Benchmark results are stored in `benchmark_results/` with:
- Raw benchmark output (`.txt` files)
- JSON data for analysis (`.json` files)
- Summary CSV with comparisons (`benchmark_summary.csv`)

See `BENCHMARK_README.md` for detailed documentation.