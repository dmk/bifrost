pub mod backend;
pub mod strategy;
pub mod protocol;

// Re-export core traits
pub use backend::Backend;
pub use strategy::Strategy;
pub use protocol::Protocol;