/**
 * model: Data structures and domain models
 */
pub mod model;

/**
 * commands: CLI commands and entry points
 */
pub mod cli;

/**
 * services: Core business logic and internal routines
 */
pub mod service;

/**
 * utils: Utilities and shared dependencies
 */
pub mod utils;

// Re-export commonly used types for convenience
pub use model::{config::*, error::BR, source::*};
