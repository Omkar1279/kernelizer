//! Template generation and prompt parsing logic.

use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tokio::process::Command;

/// The structured output from the LLM prompt translation.
#[derive(Deserialize, Debug)]
pub struct KernelParams {
    /// The focused task.
    pub task: String,
    /// The inferred technical constraints.
    pub constraints: Vec<String>,
    /// The expected output format.
    pub format: String,
    /// The success criteria to verify.
    pub verify: String,
}

/// Parsed user prompt with extracted explicit file contexts.
#[derive(Debug, Clone)]
pub struct ParsedPrompt {
    cleaned_prompt: String,
    explicit_files_content: String,
}

impl ParsedPrompt {
    /// Returns the cleaned prompt stripped of `@file` tags.
    #[must_use]
    pub fn cleaned_prompt(&self) -> &str {
        &self.cleaned_prompt
    }

    /// Returns the concatenated content of all explicitly tagged files.
    #[must_use]
    pub fn explicit_content(&self) -> &str {
        &self.explicit_files_content
    }
}

fn read_dir_files(dir: &Path, content: &mut String) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                read_dir_files(&path, content);
            } else if path.is_file() {
                if let Ok(file_content) = fs::read_to_string(&path) {
                    content.push_str(&format!(
                        "**File: {}**\n```\n{}\n```\n\n",
                        path.display(),
                        file_content
                    ));
                }
            }
        }
    }
}

/// Extracts explicit file tags (e.g., `@src/main.rs`) from a raw prompt,
/// reads their contents locally, and returns a cleaned prompt and the file contents.
#[must_use]
pub fn extract_at_files(raw_prompt: &str) -> ParsedPrompt {
    let mut explicit_content = String::new();
    let mut cleaned_prompt = String::with_capacity(raw_prompt.len());

    for word in raw_prompt.split_whitespace() {
        if let Some(file_path) = word.strip_prefix('@') {
            if !file_path.is_empty() {
                let path = Path::new(file_path);
                let mut valid_tag = false;

                if path.is_file() {
                    if let Ok(content) = fs::read_to_string(path) {
                        explicit_content.push_str(&format!(
                            "**File: {file_path}**\n```\n{content}\n```\n\n"
                        ));
                    } else {
                        eprintln!(
                            "\x1b[33m[kernelizer] Warning: Could not read explicitly tagged file: {file_path}\x1b[0m"
                        );
                    }
                    valid_tag = true;
                } else if path.is_dir() {
                    let mut dir_content = String::new();
                    read_dir_files(path, &mut dir_content);
                    if !dir_content.is_empty() {
                        explicit_content.push_str(&dir_content);
                    }
                    valid_tag = true;
                }

                if valid_tag {
                    // Keep a readable reference in the Task description
                    if !cleaned_prompt.is_empty() {
                        cleaned_prompt.push(' ');
                    }
                    let label = Path::new(file_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(file_path);
                    cleaned_prompt.push_str(&format!("`{label}`"));
                    continue;
                } else {
                    eprintln!(
                        "\x1b[33m[kernelizer] Warning: Could not find explicitly tagged path: {file_path}\x1b[0m"
                    );
                    // It was an invalid tag (maybe an @mention), so keep it in the prompt.
                }
            }
        }

        if !cleaned_prompt.is_empty() {
            cleaned_prompt.push(' ');
        }
        cleaned_prompt.push_str(word);
    }

    ParsedPrompt {
        cleaned_prompt,
        explicit_files_content: explicit_content,
    }
}

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "to", "for", "of", "in", "on", "at", "add", "with", "our",
    "we", "and", "or", "is", "it", "be", "as", "by", "that", "this", "from",
    "into", "file", "files", "code", "use", "using", "make", "like", "can",
    "should", "would", "will", "all", "also", "not", "but", "do", "does",
];

const SKIP_DIRS: &[&str] = &[
    "target", "node_modules", ".git", "venv", ".venv", "__pycache__",
    "vendor", "dist", "build", ".tox", "migrations", "static", "media",
];

const SOURCE_EXTENSIONS: &[&str] = &[
    "py", "rs", "ts", "tsx", "js", "jsx", "go", "java", "rb", "php", "cs",
];

/// Scans the project (CWD) for code snippets relevant to the prompt task.
/// Returns formatted snippets for the KERNEL Context section.
/// This replaces the 39 Gemini tool calls that did the same thing at ~9s each.
#[must_use]
pub fn scan_codebase_patterns(prompt: &str) -> String {
    let keywords: Vec<String> = prompt
        .split_whitespace()
        .flat_map(|w| {
            // Strip punctuation/backticks, lowercase
            let clean = w
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_lowercase();
            // Also emit joined variant for bigrams later
            Some(clean).into_iter()
        })
        .filter(|w| w.len() > 3 && !STOP_WORDS.contains(&w.as_str()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Generate joined bigram variants: ["rate", "limiting"] → "rate_limiting", "ratelimit", "rate-limiting"
    let words: Vec<&str> = prompt
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 3 && !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();

    let mut search_terms: Vec<String> = keywords.clone();
    for pair in words.windows(2) {
        let joined = format!("{}{}", pair[0].to_lowercase(), pair[1].to_lowercase());
        let underscored = format!("{}_{}", pair[0].to_lowercase(), pair[1].to_lowercase());
        search_terms.push(joined);
        search_terms.push(underscored);
    }
    search_terms.dedup();

    if search_terms.is_empty() {
        return String::new();
    }

    let mut snippets: Vec<String> = Vec::new();
    let mut seen_files: HashSet<String> = HashSet::new();

    'walk: for entry in walkdir::WalkDir::new(".")
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Prune skip dirs early — avoids descending into large trees
            let name = e.file_name().to_string_lossy();
            !SKIP_DIRS.iter().any(|d| *d == name.as_ref())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !SOURCE_EXTENSIONS.contains(&ext) {
            continue;
        }

        let Ok(content) = fs::read_to_string(path) else { continue };
        let lower = content.to_lowercase();

        if !search_terms.iter().any(|kw| lower.contains(kw.as_str())) {
            continue;
        }

        let path_str = path.display().to_string();
        if !seen_files.insert(path_str.clone()) {
            continue;
        }

        // Extract up to 20 lines around the first matching line
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let ll = line.to_lowercase();
            if search_terms.iter().any(|kw| ll.contains(kw.as_str())) {
                let start = i.saturating_sub(2);
                let end = (i + 18).min(lines.len());
                let snippet = lines[start..end].join("\n");
                snippets.push(format!(
                    "**{}** (line {}):\n```\n{}\n```",
                    path_str,
                    i + 1,
                    snippet
                ));
                break;
            }
        }

        if snippets.len() >= 5 {
            break 'walk;
        }
    }

    snippets.join("\n\n")
}

/// Returns true when the prompt explicitly signals the user wants to follow the
/// project's existing coding style (rather than the best general solution).
#[must_use]
pub fn detects_follow_pattern_intent(prompt: &str) -> bool {
    const FOLLOW_SIGNALS: &[&str] = &[
        "like we do",
        "like we have",
        "following our",
        "consistent with",
        "matching our",
        "same as existing",
        "our pattern",
        "our style",
        "as we have",
        "follow our",
        "use our",
        "like existing",
        "like the existing",
        "as implemented",
        "our existing",
        "project style",
        "project pattern",
    ];
    let lower = prompt.to_lowercase();
    FOLLOW_SIGNALS.iter().any(|s| lower.contains(s))
}

/// Translates a vague prompt into strict KERNEL parameters using local CLI tools.
pub async fn translate_with_cli(prompt: &str, file_context: &str) -> Option<KernelParams> {
    let system_prompt = "You are an expert Prompt Engineer. Convert the user's vague request into strict KERNEL framework parameters. Rules: Task: single unambiguous goal. Constraints: infer technical constraints (max 3) — be specific to the code provided. Format: exact output structure. Verify: how to test this. Respond ONLY in JSON matching this schema: {\"task\":\"...\",\"constraints\":[\"...\"],\"format\":\"...\",\"verify\":\"...\"}. Do not use markdown blocks, output raw JSON.";

    // Cap context sent to the fast translation model — enough to infer code-specific
    // constraints without bloating the payload for large files.
    const TRANSLATE_CONTEXT_CAP: usize = 1500;
    let truncated_context = if file_context.len() > TRANSLATE_CONTEXT_CAP {
        // Walk back to the nearest valid char boundary — never split a multi-byte char
        let mut end = TRANSLATE_CONTEXT_CAP;
        while !file_context.is_char_boundary(end) {
            end -= 1;
        }
        &file_context[..end]
    } else {
        file_context
    };

    let full_prompt = if truncated_context.is_empty() {
        format!("{}\n\nUser Request: {}", system_prompt, prompt)
    } else {
        format!("{}\n\nFile Context (use to make constraints specific):\n{}\n\nUser Request: {}", system_prompt, truncated_context, prompt)
    };

    // Try gemini flash first
    let mut cmd_gemini = Command::new("gemini");
    cmd_gemini.arg("-y").arg("-o").arg("text").arg("-m").arg("gemini-2.5-flash").arg("-p").arg(&full_prompt);

    if let Ok(output) = cmd_gemini.output().await {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let cleaned = stdout.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
            if let Ok(parsed) = serde_json::from_str::<KernelParams>(cleaned) {
                return Some(parsed);
            }
        }
    }

    // Fallback to claude haiku
    let mut cmd_claude = Command::new("claude");
    cmd_claude.arg("-p").arg(&full_prompt).arg("--model").arg("claude-haiku-4-5-20251001");

    if let Ok(output) = cmd_claude.output().await {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let cleaned = stdout.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
            if let Ok(parsed) = serde_json::from_str::<KernelParams>(cleaned) {
                return Some(parsed);
            }
        }
    }

    None
}

/// Compiles the final KERNEL Markdown template combining the user's intent,
/// explicit files, STDIN, codebase patterns, and vector search context.
#[must_use]
pub fn compile_kernel(
    parsed_prompt: &ParsedPrompt,
    patterns_context: &str,
    follow_pattern: bool,
    stdin_context: &str,
    translated_params: Option<&KernelParams>,
) -> String {
    let mut output = String::new();

    let has_explicit_files = !parsed_prompt.explicit_files_content.is_empty();
    let has_patterns = !patterns_context.is_empty();

    let has_any_context = has_explicit_files || has_patterns || !stdin_context.is_empty();

    if has_any_context {
        output.push_str("# Context (input)\n");
    }

    if has_explicit_files {
        output.push_str("## Explicitly Tagged Files:\n");
        output.push_str(&parsed_prompt.explicit_files_content);
    }

    if has_patterns {
        output.push_str("## Existing Codebase Patterns:\n");
        output.push_str(patterns_context);
        output.push_str("\n\n");
    }

    if !stdin_context.is_empty() {
        output.push_str("## Piped Standard Input:\n```\n");
        output.push_str(stdin_context);
        output.push_str("\n```\n\n");
    }

    // Build the tool-usage and scope constraints based on what context is present
    let tool_usage_constraint = match (has_explicit_files, has_patterns) {
        (true, _) => "- Tool Usage (CRITICAL): The files in ## Explicitly Tagged Files are pre-loaded in full — do NOT re-read or re-fetch them. Trust them as complete and authoritative. Only call tools for files not already provided.\n- Scope: Limit changes to the files provided in # Context unless the task explicitly requires otherwise.\n",
        (false, true) => "- Tool Usage: The ## Existing Codebase Patterns section provides relevant context already found in the project. Do not re-search for patterns already shown above.\n",
        (false, false) => "- Tool Usage (IMPORTANT): You are encouraged to use your available tools to gather more context. However, your FINAL answer must strictly conform to these constraints, avoiding apologies or chatty preambles.\n",
    };

    // Pattern-following constraint: hard rule if user explicitly asked to follow project style,
    // soft reference otherwise (existing code may not be best practice)
    let pattern_constraint = if has_patterns {
        if follow_pattern {
            Some("- Pattern Adherence (STRICT): The user explicitly requested the project's coding style. You MUST implement using the exact patterns shown in ## Existing Codebase Patterns — no deviations.\n")
        } else {
            Some("- Pattern Reference (ADVISORY): ## Existing Codebase Patterns shows how this project currently approaches similar problems. Use it as context but improve on it if a better, idiomatic solution exists.\n")
        }
    } else {
        None
    };

    if let Some(params) = translated_params {
        output.push_str("# Task (function)\n");
        output.push_str(&params.task);
        output.push_str("\n\n");

        output.push_str("# Constraints (parameters)\n");
        for constraint in &params.constraints {
            output.push_str(&format!("- {}\n", constraint));
        }
        output.push_str(tool_usage_constraint);
        if let Some(pc) = pattern_constraint {
            output.push_str(pc);
        }
        output.push('\n');

        output.push_str("# Format (output)\n");
        output.push_str(&params.format);
        output.push_str("\n\n");

        output.push_str("# Verify\n");
        output.push_str(&params.verify);
        output.push('\n');
    } else {
        output.push_str("# Task (function)\n");
        output.push_str(&parsed_prompt.cleaned_prompt);
        output.push_str("\n\n");

        output.push_str("# Constraints (parameters)\n");
        output.push_str("- Keep it simple: One clear goal. Do not add unnecessary conversational fluff.\n");
        output.push_str("- Narrow scope: Address only the prompt's explicit request. Do not write unrelated functions.\n");
        output.push_str("- Explicit constraints: Follow established design patterns and idiomatic rules for the language provided.\n");
        output.push_str(tool_usage_constraint);
        if let Some(pc) = pattern_constraint {
            output.push_str(pc);
        }
        output.push('\n');

        output.push_str("# Format (output)\n");
        output.push_str("Strictly return the requested code, architecture, or fix. Provide direct solutions without preamble.\n\n");

        output.push_str("# Verify\n");
        output.push_str("Ensure output matches the requested format exactly and success criteria are easy to verify. Ensure reproducible results using specific versions if applicable.\n");
    }

    output
}
