# Bifröst Core Traits Design

> Core trait definitions for the intelligent memcached proxy

## 🏗️ Project Structure

```
src/
├── lib.rs              # Main entry point
└── core/               # Core trait definitions
    ├── mod.rs          # Module organization
    ├── backend.rs      # Backend abstraction
    ├── strategy.rs     # Routing strategies
    ├── protocol.rs     # Protocol handling
    ├── side_effect.rs  # Async side effects
    ├── listener.rs     # Input endpoints
    └── router.rs       # Request routing
```

## 📐 Core Traits

### 1. Backend
Abstracts different types of cache backends.
```rust
pub trait Backend: Send + Sync {
    async fn execute(&self, command: &dyn Command) -> Result<Response, BackendError>;
    async fn health_check(&self) -> HealthStatus;
    fn name(&self) -> &str;
}
```

**Future implementations**: MemcachedPool, MemcachedSingle, HttpEndpoint

### 2. Strategy
Defines different routing strategies for handling requests.
```rust
pub trait Strategy: Send + Sync {
    async fn execute(&self, command: &dyn Command, backends: &[&dyn Backend]) -> Result<StrategyResult, StrategyError>;
    fn name(&self) -> &str;
}
```

**Future implementations**: BlindForward, WarmFirst, Parallel, RoundRobin, ConsistentHash

### 3. Protocol
Handles memcached protocol parsing and serialization.
```rust
pub trait Protocol: Send + Sync {
    async fn parse(&self, data: &[u8]) -> Result<Box<dyn Command>, ProtocolError>;
    async fn serialize(&self, response: &Response) -> Result<Vec<u8>, ProtocolError>;
}
```

**Future implementations**: MemcachedAscii, MemcachedBinary

### 4. SideEffect
Handles async post-processing operations.
```rust
pub trait SideEffect: Send + Sync {
    async fn execute(&self, command: &dyn Command, response: &Response) -> Result<(), SideEffectError>;
    fn name(&self) -> &str;
}
```

**Future implementations**: MemcachedReplication, HttpLogging, MetricsCollection

### 5. Listener
Handles different input endpoints.
```rust
pub trait Listener: Send + Sync {
    async fn start(&self) -> Result<(), ListenerError>;
    async fn stop(&self) -> Result<(), ListenerError>;
    fn bind_address(&self) -> &str;
}
```

**Future implementations**: MemcachedListener, HttpListener, AdminListener

### 6. Router & Matcher
Handles request routing and pattern matching.
```rust
pub trait Router: Send + Sync {
    async fn route(&self, command: &dyn Command) -> Result<Response, RouterError>;
}

pub trait Matcher: Send + Sync {
    fn matches(&self, key: &str) -> bool;
    fn pattern(&self) -> &str;
}
```

**Future implementations**: ConfigurableRouter, GlobMatcher, RegexMatcher

## 🎯 Design Principles

1. **Trait-focused**: Everything is abstracted behind traits for maximum extensibility
2. **Async-first**: All I/O operations are async using async-trait
3. **Simple core**: Minimal dependencies, complex types defined later
4. **Modular**: Each trait lives in its own module
5. **Placeholder types**: Error and response types are stubs for now

## 🚀 Next Steps

1. **Add concrete types**: Define proper Command, Response, Error types
2. **Implement basic strategies**: Start with BlindForward
3. **Add configuration**: YAML/JSON parsing for route setup
4. **Build server**: Tokio-based request handling
5. **Protocol implementation**: Memcached ASCII parsing

This foundation provides a clean separation of concerns and makes the system highly extensible while keeping the core simple.