//! Error types for the Kernelizer library.

use thiserror::Error;

/// Comprehensive error type for Kernelizer operations.
#[derive(Debug, Error)]
pub enum KernelizerError {
    /// I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic parsing error.
    #[error("Failed to parse code")]
    ParseError,
}
