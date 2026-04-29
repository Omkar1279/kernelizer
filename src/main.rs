#![allow(missing_docs)]

#[cfg(feature = "cli")]
use clap::Parser;
#[cfg(feature = "cli")]
use kernelizer_rs::cli::{Cli, Commands};
#[cfg(feature = "cli")]
use std::io::{self, Read, IsTerminal};

#[cfg(feature = "cli")]
#[tokio::main]
async fn main() -> Result<(), kernelizer_rs::KernelizerError> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Optimize { prompt, fast, context: _ } => {
            // 1. Read STDIN if piped
            let mut stdin_context = String::new();
            if !io::stdin().is_terminal() {
                io::stdin().read_to_string(&mut stdin_context)?;
            }

            // 2. Parse explicit @files
            let parsed_prompt = kernelizer_rs::template::extract_at_files(prompt);

            // 3. Detect pattern intent on the raw prompt before @tags are stripped
            let follow_pattern = kernelizer_rs::template::detects_follow_pattern_intent(prompt);

            // 4 & 5. Scan patterns and translate in parallel — both are independent.
            // scan_codebase_patterns is CPU-bound (walkdir + file I/O), so spawn_blocking
            // keeps it off the async executor while translate_with_cli awaits the network.
            // On --fast, skip both: no pattern scan, no LLM call.
            let (patterns_context, translated_params) = if *fast {
                (String::new(), None)
            } else {
                let scan_prompt = parsed_prompt.cleaned_prompt().to_owned();
                let scan_future = tokio::task::spawn_blocking(move || {
                    kernelizer_rs::template::scan_codebase_patterns(&scan_prompt)
                });

                eprintln!("\x1b[36m⚙️  Refining prompt with host CLI (gemini/claude)...\x1b[0m");
                let translate_future = kernelizer_rs::template::translate_with_cli(
                    parsed_prompt.cleaned_prompt(),
                    parsed_prompt.explicit_content(),
                );

                let (scan_result, params) = tokio::join!(
                    async { scan_future.await.unwrap_or_default() },
                    translate_future
                );

                if params.is_none() {
                    eprintln!("\x1b[33m⚠️  Failed to refine prompt. Falling back to deterministic template.\x1b[0m");
                }
                (scan_result, params)
            };

            // 6. Compile and print
            let optimized_prompt = kernelizer_rs::template::compile_kernel(
                &parsed_prompt,
                &patterns_context,
                follow_pattern,
                &stdin_context,
                translated_params.as_ref(),
            );
            println!("{optimized_prompt}");
        }
    }

    Ok(())
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("Kernelizer must be compiled with the 'cli' feature to run the CLI application.");
    std::process::exit(1);
}
