use async_trait::async_trait;
use super::backend::Command;
use super::protocol::Response;

/// Core trait for side effects (replication, metrics, warming)
#[async_trait]
pub trait SideEffect: Send + Sync {
    /// Execute the side effect (safe to fail)
    async fn execute(
        &self,
        command: &dyn Command,
        response: &Response,
    ) -> Result<(), SideEffectError>;

    /// Get side effect name/type
    fn name(&self) -> &str;
}

/// Placeholder types - to be defined later
pub struct SideEffectError;