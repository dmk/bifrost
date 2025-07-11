use async_trait::async_trait;

/// Core trait for input endpoints (memcached port, admin port, HTTP)
#[async_trait]
pub trait Listener: Send + Sync {
    /// Start listening for connections
    async fn start(&self) -> Result<(), ListenerError>;

    /// Stop the listener
    async fn stop(&self) -> Result<(), ListenerError>;

    /// Get listener bind address
    fn bind_address(&self) -> &str;
}

/// Placeholder types - to be defined later
pub struct ListenerError;