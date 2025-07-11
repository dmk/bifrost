use async_trait::async_trait;
use super::backend::Command;
use super::protocol::Response;

/// Core trait for request routing
#[async_trait]
pub trait Router: Send + Sync {
    /// Route a command to appropriate backend(s)
    async fn route(&self, command: &dyn Command) -> Result<Response, RouterError>;
}

/// Core trait for pattern matching (glob, regex, etc.)
pub trait Matcher: Send + Sync {
    /// Check if a key matches this pattern
    fn matches(&self, key: &str) -> bool;

    /// Get the pattern string
    fn pattern(&self) -> &str;
}

/// Placeholder types - to be defined later
pub struct RouterError;