#![allow(missing_docs)]

#[cfg(feature = "cli")]
use clap::Parser;
#[cfg(feature = "cli")]
use kernelizer_rs::cli::{Cli, Commands};
#[cfg(feature = "cli")]
use std::io::{self, IsTerminal, Read};

#[cfg(feature = "cli")]
#[tokio::main]
async fn main() -> Result<(), kernelizer_rs::KernelizerError> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Optimize { prompt, fast } => {
            // 1. Read STDIN if piped
            let mut stdin_context = String::new();
            if !io::stdin().is_terminal() {
                io::stdin().read_to_string(&mut stdin_context)?;
            }

            // 2. Parse explicit @files
            let parsed_prompt = kernelizer_rs::template::extract_at_files(prompt);

            // 3. Detect pattern intent on the raw prompt before @tags are stripped
            let follow_pattern =
                kernelizer_rs::template::detects_follow_pattern_intent(prompt);

            // 4 & 5. Build repo map and translate in parallel — both are independent.
            // build_repo_map is CPU-bound (tree-sitter AST walk), so spawn_blocking keeps
            // it off the async executor while translate_with_cli awaits the network.
            // On --fast, skip both: no repo map, no LLM call.
            let (repo_map, translated_params) = if *fast {
                (String::new(), None)
            } else {
                // Build the repo map first (~5–50ms CPU) — it's fast enough that the
                // overhead is negligible compared to the translate call (~2–15s network).
                // Doing it sequentially ensures Haiku always receives the full context.
                eprintln!(
                    "\x1b[36m⚙️  Building repo map...\x1b[0m"
                );
                let map_prompt = parsed_prompt.cleaned_prompt().to_owned();
                let repo_map = tokio::task::spawn_blocking(move || {
                    kernelizer_rs::repomap::build_repo_map(&map_prompt)
                })
                .await
                .unwrap_or_default();

                eprintln!(
                    "\x1b[36m⚙️  Refining prompt with repo map context...\x1b[0m"
                );
                let params = kernelizer_rs::template::translate_with_cli(
                    parsed_prompt.cleaned_prompt(),
                    parsed_prompt.explicit_content(),
                    &repo_map,
                )
                .await;

                if params.is_none() {
                    eprintln!(
                        "\x1b[33m⚠️  Translation failed. Using deterministic template.\x1b[0m"
                    );
                }
                (repo_map, params)
            };

            // 6. Compile and print
            let optimized_prompt = kernelizer_rs::template::compile_kernel(
                &parsed_prompt,
                &repo_map,
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
    eprintln!(
        "Kernelizer must be compiled with the 'cli' feature to run the CLI application."
    );
    std::process::exit(1);
}
