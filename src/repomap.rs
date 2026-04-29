//! Tree-sitter based repo map: extracts function/class/struct signatures from the codebase
//! and returns a structured, token-budgeted overview for the KERNEL translation prompt.

use std::cmp::Reverse;
use std::fs;
use tree_sitter::{Language, Node, Parser};

const SKIP_DIRS: &[&str] = &[
    "target", "node_modules", ".git", "venv", ".venv", "__pycache__",
    "vendor", "dist", "build", ".tox", "migrations", "static", "media",
];

/// Node kinds that represent function/method definitions — stop recursing into their bodies.
const FUNCTION_KINDS: &[&str] = &[
    "function_item",       // Rust: fn foo()
    "function_definition", // Python: def foo()
    "function_declaration",// JS/TS/Go: function foo()
    "method_definition",   // JS/TS: foo() { } inside class
    "method_declaration",  // Go: func (r Recv) foo()
];

/// Node kinds that represent type/namespace containers — collect their header and recurse for methods.
const CONTAINER_KINDS: &[&str] = &[
    "struct_item",              // Rust: struct Foo
    "enum_item",                // Rust: enum Foo
    "trait_item",               // Rust: trait Foo
    "impl_item",                // Rust: impl Foo { ... }
    "type_alias_item",          // Rust: type Foo = ...
    "class_definition",         // Python: class Foo
    "class_declaration",        // JS/TS: class Foo
    "interface_declaration",    // TS: interface Foo
    "type_alias_declaration",   // TS: type Foo = ...
    "type_declaration",         // Go: type Foo struct
];

fn collect_sigs(node: Node, source: &[u8], out: &mut Vec<(usize, String)>) {
    let kind = node.kind();
    let is_fn = FUNCTION_KINDS.contains(&kind);
    let is_container = CONTAINER_KINDS.contains(&kind);

    if is_fn || is_container {
        let start = node.start_byte();
        let row = node.start_position().row;
        let line_end = source[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|n| start + n)
            .unwrap_or(source.len());

        if let Ok(line) = std::str::from_utf8(&source[start..line_end]) {
            // Strip trailing { or : so signatures look clean
            let sig = line.trim().trim_end_matches(|c| c == '{' || c == ':').trim();
            if !sig.is_empty() && sig.len() <= 120 {
                out.push((row + 1, sig.to_string()));
            }
        }

        // Don't recurse into function bodies — avoids collecting nested closures/lambdas
        if is_fn {
            return;
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_sigs(child, source, out);
        }
    }
}

fn language_for_ext(ext: &str) -> Option<Language> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        _ => None,
    }
}

fn parse_sigs(source: &[u8], language: Language) -> Vec<(usize, String)> {
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let mut sigs = Vec::new();
    collect_sigs(tree.root_node(), source, &mut sigs);
    sigs
}

/// Walks the project, parses every supported source file with tree-sitter, and produces
/// a concise repo map: one file header per file followed by its function/type signatures.
/// Files most relevant to `prompt` are sorted first; total output is capped at ~3 000 chars.
#[must_use]
pub fn build_repo_map(prompt: &str) -> String {
    let keywords: Vec<String> = prompt
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| w.len() > 3)
        .collect();

    let mut file_sigs: Vec<(String, Vec<(usize, String)>)> = Vec::new();

    for entry in walkdir::WalkDir::new(".")
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !SKIP_DIRS.iter().any(|d| *d == name.as_ref())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let Some(language) = language_for_ext(ext) else { continue };
        let Ok(source) = fs::read(path) else { continue };

        let sigs = parse_sigs(&source, language);
        if !sigs.is_empty() {
            file_sigs.push((path.display().to_string(), sigs));
        }
    }

    if file_sigs.is_empty() {
        return String::new();
    }

    // Files that match more prompt keywords float to the top
    file_sigs.sort_by_key(|(path, sigs)| {
        let haystack = format!(
            "{} {}",
            path,
            sigs.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(" ")
        )
        .to_lowercase();
        let matches = keywords
            .iter()
            .filter(|kw| haystack.contains(kw.as_str()))
            .count();
        Reverse(matches)
    });

    const BUDGET: usize = 3_000;
    let mut out = String::new();

    for (file, sigs) in &file_sigs {
        let block = format!(
            "{}:\n{}\n",
            file,
            sigs.iter()
                .map(|(line, sig)| format!("  L{line}: {sig}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        if out.len() + block.len() > BUDGET {
            break;
        }
        out.push_str(&block);
    }

    out
}
