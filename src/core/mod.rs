pub mod backend;
pub mod strategy;
pub mod protocols;

// Re-export core traits
pub use backend::Backend;
pub use strategy::Strategy;
pub use protocols::Protocol;