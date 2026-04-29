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
        /// Skip the LLM translation step and use the deterministic template directly.
        /// Saves 5-10s; best when @file tags are present and the task is unambiguous.
        #[arg(short, long)]
        fast: bool,

        /// Include relevant context from the codebase and environment.
        #[arg(short, long)]
        context: bool,

        /// The unstructured user prompt
        #[arg(value_name = "PROMPT")]
        prompt: String,
    },
}
