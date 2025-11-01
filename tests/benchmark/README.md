# Bifrost Benchmark Suite

Performance benchmarks comparing Bifrost vs MCRouter using YAML-driven configuration.

## Quick Start

```bash
# Run all benchmarks
make benchmark

# Run specific benchmark suite
make benchmark-stress_test
cd tests/benchmark && make run-suite SUITE=stress_test

# List available suites
cd tests/benchmark && make list
```

## Directory Structure

```
tests/benchmark/
├── Makefile                      # Benchmark-specific targets
├── run_benchmark.sh             # Main benchmark runner
├── configs/                      # YAML configuration files
│   ├── light_read_heavy.yaml
│   ├── stress_test.yaml
│   └── ...
├── benchmark_config.yaml        # Default Bifrost proxy config
├── docker-compose-benchmark.yml # Infrastructure setup
├── benchmark_analysis.py        # Results analyzer
├── mcrouter-benchmark.json      # MCRouter config
├── run_on_ec2.sh                # EC2 deployment script
└── benchmark_results/           # Output directory
```

## Available Benchmarks

| Config | Threads | Clients | Requests | Ratio | Size | Description |
|--------|---------|---------|----------|-------|------|-------------|
| `light_read_heavy` | 2 | 5 | 1K | 10:1 | 1KB | Light read-heavy load |
| `light_write_heavy` | 2 | 5 | 1K | 1:10 | 1KB | Light write-heavy load |
| `medium_balanced` | 4 | 10 | 5K | 1:1 | 1KB | Medium balanced load |
| `high_read_heavy` | 8 | 20 | 10K | 10:1 | 1KB | High read-heavy load |
| `large_values` | 4 | 10 | 5K | 3:1 | 8KB | Large value test |
| `stress_test` | 16 | 25 | 20K | 5:1 | 2KB | Stress test |

## Creating Custom Benchmarks

Create a new YAML config in `configs/`:

```yaml
# configs/my_custom_benchmark.yaml
name: "my_custom_benchmark"
description: "My Custom Benchmark"

# Memtier benchmark parameters
memtier:
  threads: 8
  clients: 15
  requests: 10000
  ratio: "2:1"
  data_size: 4096
  run_count: 3
  key_pattern: "R:R"
  print_percentiles: "50,90,95,99,99.9"

# Optional: custom Bifrost config (defaults to benchmark_config.yaml)
bifrost_config: "my_custom_config.yaml"
```

Run it:
```bash
make benchmark-my_custom_benchmark
```

## EC2 Deployment

For reproducible results on EC2:

```bash
./run_on_ec2.sh --host ec2-user@<IP> --key ~/.ssh/key.pem
```

Options:
- `--no-bootstrap` - Skip Docker installation
- `--repo <path>` - Remote repo path (default: ~/bifrost)
- `--branch <name>` - Git branch to use
- `--timeout <mins>` - Timeout (default: 90)

## Requirements

- Docker & Docker Compose
- Python 3.7+ (for analysis)
- netcat (for health checks)

## Results

Results are saved in `benchmark_results/`:
- `*.txt` - Raw memtier output
- `*.json` - Structured results
- `benchmark_summary.csv` - Comparison summary

## Architecture

- **5 memcached instances** (ports 11211-11215)
- **Bifrost proxy** on port 22122
- **MCRouter proxy** on port 11211
- **memtier_benchmark** runs in Docker container on same network

Both proxies connect to the same backends for fair comparison.
