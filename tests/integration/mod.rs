//! Integration test helpers and utilities
//!
//! This module provides comprehensive test infrastructure for integration testing:
//! - Mock memcached backends with configurable behavior
//! - Test client for sending memcached commands
//! - Proxy test helpers for spinning up test instances
//! - Common test scenarios and fixtures

pub mod helpers;
pub mod scenarios;

// Re-export commonly used helpers
pub use helpers::{
    mock_memcached::{MockMemcached, MockMemcachedBuilder, ResponseMode},
    test_client::TestClient,
};
