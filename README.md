# Bifrost

An intelligent Memcached proxy with routing, failover, and load balancing capabilities.

## Features

- **Smart Routing**: Route requests based on key patterns
- **Multiple Strategies**: Blind forward, round robin, failover, and miss failover
- **Connection Pooling**: Efficient connection management with bb8
- **YAML Configuration**: Simple, readable configuration files

## Quick Start

1. **Start backend memcached servers**:
   ```bash
   docker-compose up -d
   ```

2. **Run bifrost**:
   ```bash
   cargo run -- --config examples/simple.yaml
   ```

3. **Test the proxy**:
   ```bash
   echo "set test 0 0 5\r\nhello\r\n" | nc 127.0.0.1:11211
   ```

## Configuration

See [examples](./examples) for configuration samples including failover, load balancing, and routing setups.

## Benchmarks

Bifrost includes a reproducible benchmark suite that compares against MCRouter across six scenarios. In recent runs, Bifrost won all scenarios.

### Summary (ops/sec and latency)

| Scenario               | Bifrost ops/sec | MCRouter ops/sec | Diff   | P50 ms (B/M) | P95 ms (B/M) | P99 ms (B/M) | Winner |
|------------------------|----------------:|-----------------:|:-------|:-------------|:-------------|:-------------|:-------|
| High Read Heavy        |          57,692 |            35,595 | +62.1% | 2.21 / 4.10  | 5.89 / 8.13  | 8.32 / 10.24 | Bifrost |
| Large Values           |          33,249 |            23,281 | +42.8% | 0.76 / 1.32  | 2.51 / 4.32  | 4.83 / 5.92  | Bifrost |
| Light Read Heavy       |          36,289 |            34,188 |  +6.1% | 0.21 / 0.25  | 0.52 / 0.50  | 3.41 / 3.54  | Bifrost |
| Light Write Heavy      |          38,211 |            27,527 | +38.8% | 0.21 / 0.29  | 0.42 / 0.68  | 1.59 / 2.13  | Bifrost |
| Medium Balanced        |          50,721 |            43,253 | +17.3% | 0.67 / 0.95  | 3.41 / 3.10  | 4.64 / 4.99  | Bifrost |
| Stress Test            |          58,500 |            22,533 | +159.6%| 6.27 / 17.28 | 13.57 / 27.78| 18.18 / 32.00| Bifrost |

Notes:
- Numbers above are representative from the included suite and may vary by host. Full raw outputs and analysis are provided.
- Scenarios and parameters are defined in `tests/benchmark`.

### Reproduce

Requirements: Docker and Docker Compose.

1. Run the suite:
   ```bash
   make benchmark
   ```
2. Results will be written to `tests/benchmark/benchmark_results/`, including a CSV summary at:
   - `tests/benchmark/benchmark_results/benchmark_summary.csv`

For details, see `tests/benchmark/README.md`.

## License

Apache 2.0 - See [LICENSE](LICENSE) file for details. Attribution required.

