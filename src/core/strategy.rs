use async_trait::async_trait;
use crate::core::backend::Backend;

/// Core trait for routing strategies
#[async_trait]
pub trait Strategy: Send + Sync {
    /// Select a backend for the given request
    async fn select_backend<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>>;

    /// Strategy name
    fn name(&self) -> &str;
}

/// Simple blind forward strategy - always uses the first backend
#[derive(Debug)]
pub struct BlindForwardStrategy {
    pub name: String,
}

impl BlindForwardStrategy {
    pub fn new() -> Self {
        Self {
            name: "blind_forward".to_string(),
        }
    }
}

impl Default for BlindForwardStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for BlindForwardStrategy {
    async fn select_backend<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>> {
        backends.first()
    }

    fn name(&self) -> &str {
        &self.name
    }
}