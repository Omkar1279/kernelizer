//! Kernelizer: A library for extracting LLM context and optimizing prompts.
#![deny(missing_docs)]

pub mod cli;
pub mod error;
pub mod template;

// Re-export core items
pub use error::KernelizerError;
pub use template::{extract_at_files, compile_kernel, ParsedPrompt};
