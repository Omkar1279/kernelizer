//! Command-line interface definitions.

use clap::{Parser, Subcommand};

/// Kernelizer CLI - API-Keyless KERNEL Template Compiler
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Subcommands
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Formats the prompt and context into a KERNEL Markdown layout and prints to STDOUT
    Optimize {
        /// The unstructured user prompt
        prompt: String,
        /// Skip the repo map scan and LLM translation — deterministic template only (~50ms).
        /// Best when @file tags are present and the task is unambiguous.
        #[arg(long, default_value_t = false)]
        fast: bool,
    },
}
