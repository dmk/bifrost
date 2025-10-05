//! Integration tests for Bifrost
//!
//! This test suite uses the integration test framework located in tests/integration/
//! Run with: cargo test --test integration_tests

// Include the integration test infrastructure
mod integration;

// Re-export all scenarios so their tests are discovered
// This imports all the #[tokio::test] functions from the scenario modules
pub use integration::scenarios::*;
