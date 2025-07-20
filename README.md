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

## License

Apache 2.0 - See [LICENSE](LICENSE) file for details. Attribution required.

