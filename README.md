# Bifröst

> *The intelligent memcached proxy that bridges cache realms*

Bifröst is a high-performance memcached proxy written in Rust that provides intelligent routing, parallel processing, and advanced cache management features. Named after the rainbow bridge from Norse mythology that connects different realms, Bifröst seamlessly connects and manages multiple memcached backends with smart routing capabilities.

## Features

### 🌈 **Intelligent Routing**
- **Blind Forward**: Raw passthrough mode for simple proxy scenarios
- **Multi-tier Architecture**: Route requests between warm and cold cache tiers
- **Custom Route Handlers**: Non-memcached API endpoints for cache warming and management
- **Load Balancing**: Distribute requests across multiple backend servers

### ⚡ **Parallel Processing**
- **Dual-tier Requests**: Simultaneously query warm and cold caches
- **Background Handling**: Drop slower responses in the background

### 🔄 **Replication & Consistency**
- **Write-on-miss**: Automatically replicate data to maintain consistency
- **Configurable Replication**: Fine-tune replication behavior per route or backend
- **Cache Warming**: Custom endpoints to proactively populate caches

### ⚙️ **Configuration-driven**
- **YAML/JSON Configuration**: Cloud-native configuration format
- **Per-route Policies**: Different strategies for different key patterns
- **Health Checking**: Background monitoring improves request performance by avoiding dead backends

## Quick Start

```bash
# Install Bifröst
cargo install bifrost

# Create a basic configuration
cat > bifrost.yaml << EOF
listeners:
  main:
    bind: "127.0.0.1:11211"

backends:
  warm_cache:
    type: "memcached_pool"
    servers: ["127.0.0.1:11212"]
  cold_cache:
    type: "memcached_pool"
    servers: ["127.0.0.1:11213"]

routes:
  default:
    matcher: "*"
    strategy: "warm_first"
    primary: "warm_cache"
    fallback: "cold_cache"
EOF

# Start the proxy
bifrost --config bifrost.yaml
```

## Configuration

Bifröst uses YAML or JSON configuration files with four main sections: listeners, backends, routes, and side effects.

### Configuration Structure

```yaml
# Listeners define input endpoints
listeners:
  main:
    bind: "0.0.0.0:11211"
  admin:
    bind: "0.0.0.0:11212"

# Backends define cache servers and connection pools
backends:
  warm_cache:
    type: "memcached_pool"
    servers:
      - "cache1.example.com:11211"
      - "cache2.example.com:11211"
    tier: "warm"
  cold_cache:
    type: "memcached_pool"
    servers: ["cache3.example.com:11211"]
    tier: "cold"
  backup_cache:
    type: "memcached_single"
    server: "backup.example.com:11211"

# Routes define request matching and routing strategies
routes:
  user_data:
    matcher: "user:*"
    strategy: "warm_first"
    primary: "warm_cache"
    fallback: "cold_cache"
    side_effects: ["backup_replication"]
  session_data:
    matcher: "session:*"
    strategy: "parallel"
    backends: ["warm_cache", "cold_cache"]
    side_effects: ["metrics_logging"]
  default:
    matcher: "*"
    strategy: "blind_forward"
    backend: "warm_cache"

# Side effects execute after main response (safe to fail)
side_effects:
  backup_replication:
    type: "memcached_write"
    target: "backup_cache"
  metrics_logging:
    type: "http_post"
    url: "http://metrics.internal/cache-hits"
```

### Routing Strategies

- **`blind_forward`**: Simple passthrough to a single backend
- **`parallel`**: Query multiple backends simultaneously, return fastest response
- **`warm_first`**: Try primary backend first, fallback to secondary on miss
- **`round_robin`**: Distribute requests across multiple backends
- **`consistent_hash`**: Use consistent hashing for key distribution
- **`load_balanced`**: Distribute based on backend health and load

## Architecture

```
┌─────────────┐    ┌─────────────────┐    ┌─────────────┐
│ Client      │───▶│ Bifröst Proxy   │───▶│ Warm Cache  │
│ Application │    │                 │    │ (Primary)   │
└─────────────┘    │ ┌─────────────┐ │    └─────────────┘
                   │ │ Listeners   │ │           │
                   │ │ Routes      │ │───────────┼─────────┐
                   │ │ Strategies  │ │           │         │
                   │ └─────────────┘ │           ▼         ▼
                   │                 │    ┌─────────────┐ ┌─────────────┐
                   │ Side Effects    │───▶│ Cold Cache  │ │ Backup      │
                   │ (Async)         │    │ (Fallback)  │ │ Replication │
                   └─────────────────┘    └─────────────┘ └─────────────┘
```

## Use Cases

- **Multi-tier Caching**: Separate warm (SSD) and cold (network) cache layers
- **Cache Migration**: Gradually migrate between different cache infrastructures
- **High Availability**: Automatic failover between primary and backup cache clusters
- **Cache Warming**: Proactive cache population via custom API endpoints
- **Load Testing**: Parallel requests to test cache performance under load
- **Observability**: Centralized metrics and monitoring for distributed cache systems

## Performance

Built with Rust and Tokio, Bifröst is designed for:
- **Low Latency**: Sub-millisecond routing decisions
- **High Throughput**: Handle thousands of concurrent connections
- **Memory Efficiency**: Minimal memory footprint and zero-copy where possible
- **CPU Efficiency**: Leverage all available CPU cores with async processing

## Installation

### From Source
```bash
git clone https://github.com/yourusername/bifrost
cd bifrost
cargo build --release
./target/release/bifrost --help
```

### Using Cargo
```bash
cargo install bifrost
```

### Docker
```bash
docker run -p 11211:11211 -v $(pwd)/bifrost.yaml:/etc/bifrost.yaml bifrost:latest
```

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details on our code of conduct and the process for submitting pull requests.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Named after Bifröst, the rainbow bridge from Norse mythology
- Inspired by the need for intelligent cache management in distributed systems
- Built with ❤️ in Rust
