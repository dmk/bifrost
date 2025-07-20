use async_trait::async_trait;
use crate::core::backend::Backend;

// Re-export all strategy implementations
pub mod blind_forward;
pub mod round_robin;
pub mod failover;

pub use blind_forward::BlindForwardStrategy;
pub use round_robin::RoundRobinStrategy;
pub use failover::FailoverStrategy;

/// Core trait for routing strategies
#[async_trait]
pub trait Strategy: Send + Sync {
    /// Select a backend for the given request
    async fn select_backend<'a>(&self, backends: &'a [Box<dyn Backend>]) -> Option<&'a Box<dyn Backend>>;

    /// Strategy name
    fn name(&self) -> &str;
}

/// Strategy factory for creating strategies from configuration
pub fn create_strategy(strategy_type: &str) -> Result<Box<dyn Strategy>, StrategyError> {
    match strategy_type {
        "blind_forward" => Ok(Box::new(BlindForwardStrategy::new())),
        "round_robin" => Ok(Box::new(RoundRobinStrategy::new())),
        "failover" => Ok(Box::new(FailoverStrategy::new())),
        _ => Err(StrategyError::UnknownStrategy(strategy_type.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    #[error("Unknown strategy type: {0}")]
    UnknownStrategy(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_factory() {
        // Test creating known strategies
        let blind_forward = create_strategy("blind_forward").unwrap();
        assert_eq!(blind_forward.name(), "blind_forward");

        let round_robin = create_strategy("round_robin").unwrap();
        assert_eq!(round_robin.name(), "round_robin");

        let failover = create_strategy("failover").unwrap();
        assert_eq!(failover.name(), "failover");

        // Test unknown strategy
        let result = create_strategy("unknown_strategy");
        assert!(result.is_err());
    }
}