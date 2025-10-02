pub mod types;
pub mod parser;
pub mod file_ops;
pub mod resolver;
pub mod formula_evaluator;
pub mod actions;
pub mod docmgr;
pub mod dependency_tracker;
pub mod source_registry;

// Re-export common types for convenience in integration tests
pub use types::*;
pub use file_ops::OverseerFileHandler;