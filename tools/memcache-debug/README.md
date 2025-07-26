# Memcache Debug Server

A Ruby-based memcached server implementation for testing and debugging the Bifrost proxy. This tool provides extensive logging, delay simulation, and failure testing capabilities to help validate proxy behavior.

## Features

- **Full Memcached Protocol Support**: ASCII, Binary, and Meta protocols
- **Request Logging**: Detailed, colorized logging of all requests and responses
- **Failure Simulation**: Random connection drops to test failover logic
- **Artificial Delays**: Simulate slow backend responses
- **Statistics**: Built-in metrics collection and reporting
- **Multi-Server Support**: Run multiple configured servers from YAML

## Quick Start

```bash
# Start basic server on default port (11212)
./debug_server.rb

# Start with custom options
./debug_server.rb -p 11213 -d 100 -f 0.1 -v
```

## Command Line Options

| Option | Description | Default |
|--------|-------------|---------|
| `-p, --port PORT` | Port to listen on | 11212 |
| `-h, --host HOST` | Host to bind to | 127.0.0.1 |
| `-d, --delay MS` | Artificial delay in milliseconds | 0 |
| `-f, --failure-rate RATE` | Failure simulation rate (0.0-1.0) | 0.0 |
| `-q, --quiet` | Quiet mode (errors only) | false |
| `-v, --verbose` | Enable verbose output | false |
| `--no-color` | Disable colored output | false |
| `-c, --config FILE` | Load multiple servers from YAML | - |

## Architecture

The server is built with a modular design:

```
debug_server.rb          # Main server implementation
servers.yml              # Multi-server configuration example
lib/
├── logging.rb           # ColorLogger and logging utilities
├── server_utils.rb      # Storage, stats, and simulation modules
├── protocol.rb          # Protocol module loader
└── protocol/
    ├── ascii.rb         # ASCII protocol handler
    ├── binary.rb        # Binary protocol handler
    └── meta.rb          # Meta protocol handler
```

## Testing with Bifrost

### 1. Start Debug Servers

```bash
# Fast server
./debug_server.rb -p 11213 -v

# Slow server (200ms delay)
./debug_server.rb -p 11214 -d 200 -v

# Unreliable server (10% failure rate)
./debug_server.rb -p 11215 -f 0.1 -v
```

### 2. Create Bifrost Config

```yaml
# examples/debug_test.yaml
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  fast_backend:
    server: "127.0.0.1:11213"
  slow_backend:
    server: "127.0.0.1:11214"
  unreliable_backend:
    server: "127.0.0.1:11215"

pools:
  debug_pool:
    backends: ["fast_backend", "slow_backend", "unreliable_backend"]
    strategy: "failover"

routes:
  - pattern: "*"
    pool: "debug_pool"
```

### 3. Run Tests

```bash
# Start Bifrost
cargo run -- --config examples/debug_test.yaml

# Send test commands
echo -e "set test 0 0 5\r\nhello\r\n" | nc 127.0.0.1 11211
echo -e "get test\r\n" | nc 127.0.0.1 11211
```

## Multi-Server Configuration

Use `servers.yml` to define multiple servers:

```yaml
servers:
  - name: "fast-server"
    port: 11213
    verbose: true
  - name: "slow-server"
    port: 11214
    delay_ms: 200
    verbose: true
  - name: "unreliable-server"
    port: 11215
    failure_rate: 0.1
    verbose: true
```

Then run:
```bash
./debug_server.rb --config servers.yml
```

## Output Examples

### Normal Operation
```
2024-01-15 10:30:15.123 [INFO] Debug Memcached Server starting on 127.0.0.1:11212
2024-01-15 10:30:15.124 [INFO] [127.0.0.1:54321] New connection
2024-01-15 10:30:15.125 [DEBUG] [127.0.0.1:54321] RX: set test 0 0 5
2024-01-15 10:30:15.126 [INFO] SET: test = "hello" (5 bytes)
```

### Failure Simulation
```
2024-01-15 10:30:20.456 [DEBUG] [127.0.0.1:54322] RX: get test
2024-01-15 10:30:20.457 [WARN] [127.0.0.1:54322] SIMULATING FAILURE for command: get test
2024-01-15 10:30:20.458 [DEBUG] [127.0.0.1:54322] Connection closed
```

## Protocol Support

**Supported Commands:**
- `GET/GETS <key>` - Retrieve values
- `SET/ADD/REPLACE <key> <flags> <exptime> <bytes>` - Store values
- `DELETE <key>` - Delete keys
- `STATS` - Server statistics
- `VERSION` - Server version
- `FLUSH_ALL` - Clear all data
- `QUIT` - Close connection

## Use Cases

1. **Failover Testing**: High failure rates to test proxy failover logic
2. **Performance Testing**: Artificial delays to simulate network latency
3. **Request Analysis**: Log all requests to understand routing patterns
4. **Protocol Validation**: Verify correct memcached protocol implementation
5. **Load Testing**: Multiple server instances with different characteristics

## Requirements

- Ruby (standard library only, no external dependencies)
- Unix-like system (tested on macOS and Linux)

## Installation

No installation required - just make the script executable:

```bash
chmod +x debug_server.rb
```