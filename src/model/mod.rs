pub mod config;
pub mod dest;
pub mod error;
pub mod plan;
pub mod policy;
pub mod source;

// Re-export commonly used types
pub use config::*;
pub use error::*;
pub use source::*;

/**
 * Chunks streamed out of a `btrfs send` subprocess
 */
pub enum BtrfsSendChunk {
    StdoutBytes(Vec<u8>, u64),
    ProcessExit(u32, String), // exit code and captured stderr
}
