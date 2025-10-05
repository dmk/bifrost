//! Test scenarios for integration testing
//!
//! Each module contains tests for specific functionality:
//! - protocol: Protocol parsing and handling tests
//! - pool: Pool selection and strategy tests
//! - failover: Failover behavior tests

// Re-export all test modules so they can be discovered by cargo test
pub mod failover;
pub mod pool;
pub mod protocol;
