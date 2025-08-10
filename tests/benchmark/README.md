# Bifrost Benchmark Suite

This directory contains all benchmark-related files for testing Bifrost performance against MCRouter.

## Directory Structure

```
tests/benchmark/
├── README.md                    # This file
├── benchmark_config.yaml        # Bifrost configuration for benchmarks
├── docker-compose-benchmark.yml # Docker setup for benchmark infrastructure
├── run_benchmark.sh             # Main benchmark execution script (Docker Compose)
├── run_on_ec2.sh                # Orchestrate a full EC2 run over SSH
├── remote_bootstrap.sh          # Remote bootstrap to install Docker & deps
├── benchmark_analysis.py        # Python script to analyze benchmark results
├── mcrouter-benchmark.json      # MCRouter configuration for benchmarks
├── mcrouter-config.json         # Additional MCRouter config
└── benchmark_results/           # Benchmark output and analysis results
```

## Quick Start

1. **Run benchmarks:**
   ```bash
   cd tests/benchmark
   ./run_benchmark.sh
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

### Run on EC2 (reproducible)

Provision an EC2 instance (recommended: c6i.2xlarge, Ubuntu 22.04 or Amazon Linux 2). Ensure inbound SSH allowed from your IP.

From your workstation at repo root:

```bash
cd tests/benchmark
chmod +x run_on_ec2.sh remote_bootstrap.sh run_benchmark.sh
./run_on_ec2.sh --host ec2-user@<EC2_PUBLIC_IP> --key ~/.ssh/<key>.pem
```

This will:
- Install Docker and dependencies remotely (can be skipped with `--no-bootstrap`).
- Rsync this repo to `~/bifrost` on the instance.
- Build Bifrost for linux/amd64 and run the benchmark suite via Docker Compose.
- Download `tests/benchmark/benchmark_results/` back to your machine.

Flags:
- `--repo` to choose a different remote path.
- `--branch` to set checkout branch remotely (defaults to your current branch name).
- `--timeout` to cap remote run (default 90 min).

## Benchmark Scenarios

The benchmark suite tests the following scenarios:

- **Light Load (Read Heavy)**: 1 thread, 1 connection, 10k requests
- **Light Load (Write Heavy)**: 1 thread, 1 connection, 10k requests
- **Medium Load (Balanced)**: 4 threads, 10 connections, 20k requests
- **High Load (Read Heavy)**: 8 threads, 20 connections, 160k requests
- **Large Values**: 1 thread, 1 connection, 20k requests with large values
- **Stress Test**: 16 threads, 25 connections, 320k requests

## Requirements

- Docker and Docker Compose (the EC2 bootstrap installs these)
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

## Fairness notes ("@benchmark/ to be fair")

- Both proxies run on the same host and are hit from the same memtier container network to avoid network asymmetry.
- Bifrost and MCRouter connect to the same 5 memcached instances with identical server flags.
- Bifrost image is built with `linux/amd64` to match the base images and EC2 arch; MCRouter uses the same arch.
- memtier runs with identical parameters for both proxies per scenario and prints p50/p95/p99 to compare tails.