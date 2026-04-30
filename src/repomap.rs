//! Tree-sitter repo map: full multi-line signatures + call-graph edges.
//!
//! Each function entry records its complete signature (not just the first line)
//! and the deduplicated list of functions it calls, giving the LLM enough context
//! to reason about call chains, types, and language paradigms.

use std::cmp::Reverse;
use std::fs;
use tree_sitter::{Language, Node, Parser};

const SKIP_DIRS: &[&str] = &[
    "target", "node_modules", ".git", "venv", ".venv", "__pycache__",
    "vendor", "dist", "build", ".tox", "migrations", "static", "media",
];

const FUNCTION_KINDS: &[&str] = &[
    "function_item",        // Rust: fn foo()
    "function_definition",  // Python: def foo()
    "function_declaration", // JS/TS/Go: function foo()
    "method_definition",    // JS/TS: foo() {} inside class
    "method_declaration",   // Go: func (r Recv) foo()
];

const CONTAINER_KINDS: &[&str] = &[
    "struct_item",               // Rust: struct Foo
    "enum_item",                 // Rust: enum Foo
    "trait_item",                // Rust: trait Foo
    "impl_item",                 // Rust: impl Foo / impl Trait for Foo
    "type_alias_item",           // Rust: type Foo = ...
    "class_definition",          // Python: class Foo(Base)
    "class_declaration",         // JS/TS: class Foo
    "interface_declaration",     // TS: interface Foo<T>
    "type_alias_declaration",    // TS: type Foo = ...
    "type_declaration",          // Go: type Foo struct
];

// Body-block node kinds — signature is everything *before* the matching child.
const BODY_KINDS: &[&str] = &[
    "block",                  // Rust fn body, Go
    "statement_block",        // JS/TS fn body
    "suite",                  // Python fn/class body
    "declaration_list",       // Rust trait/impl body
    "field_declaration_list", // Rust struct body
    "enum_variant_list",      // Rust enum body
];

struct SigEntry {
    line: usize,
    sig: String,
    calls: Vec<String>,
}

/// Collect every function name called within `node`'s subtree.
fn collect_calls_in(node: Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                let raw = std::str::from_utf8(&source[func.start_byte()..func.end_byte()])
                    .unwrap_or("")
                    .trim();
                // Take the last identifier in a::b::c or a.b.c chains
                let name = raw
                    .rsplit(|c: char| c == ':' || c == '.')
                    .find(|s| !s.is_empty())
                    .unwrap_or(raw)
                    .trim();
                if !name.is_empty()
                    && name.len() <= 40
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    out.push(name.to_string());
                }
            }
        }
        // Rust method calls: receiver.method(args)
        "method_call_expression" => {
            if let Some(method) = node.child_by_field_name("method") {
                let name = std::str::from_utf8(&source[method.start_byte()..method.end_byte()])
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !name.is_empty() && name.len() <= 40 {
                    out.push(name);
                }
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_calls_in(child, source, out);
        }
    }
}

/// Extract the full signature of a node — everything before its body block —
/// with internal whitespace collapsed and trailing `{` / `:` stripped.
fn extract_sig(node: Node, source: &[u8]) -> Option<String> {
    let body_start = (0..node.child_count())
        .filter_map(|i| node.child(i as u32))
        .find(|c| BODY_KINDS.contains(&c.kind()))
        .map(|b| b.start_byte());

    let end = body_start.unwrap_or(node.end_byte());
    let bytes = source.get(node.start_byte()..end)?;
    let raw = std::str::from_utf8(bytes).ok()?;

    let normalized: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized
        .trim_end_matches(|c: char| c == '{' || c == ':')
        .trim()
        .to_string();

    if trimmed.is_empty() || trimmed.len() > 200 {
        return None;
    }
    Some(trimmed)
}

fn collect_sigs(node: Node, source: &[u8], out: &mut Vec<SigEntry>) {
    let kind = node.kind();
    let is_fn = FUNCTION_KINDS.contains(&kind);
    let is_container = CONTAINER_KINDS.contains(&kind);

    if is_fn || is_container {
        let line = node.start_position().row + 1;

        if let Some(sig) = extract_sig(node, source) {
            let calls = if is_fn {
                let mut raw_calls = Vec::new();
                if let Some(body) = (0..node.child_count())
                    .filter_map(|i| node.child(i as u32))
                    .find(|c| BODY_KINDS.contains(&c.kind()))
                {
                    collect_calls_in(body, source, &mut raw_calls);
                }
                raw_calls.sort_unstable();
                raw_calls.dedup();
                // Drop trivial/noisy names (operators, single chars) and cap at 7
                raw_calls.retain(|n| n.len() > 1);
                raw_calls.truncate(7);
                raw_calls
            } else {
                Vec::new()
            };

            out.push(SigEntry { line, sig, calls });
        }

        if is_fn {
            return; // Don't recurse into function bodies for further signatures
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

fn parse_file(source: &[u8], language: Language) -> Vec<SigEntry> {
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    collect_sigs(tree.root_node(), source, &mut entries);
    entries
}

/// Walks the project, parses every supported source file with tree-sitter, and produces
/// a rich repo map: full function signatures (including return types and generic bounds)
/// annotated with their call targets. Files most relevant to `prompt` appear first;
/// total output is capped at ~4 000 chars.
#[must_use]
pub fn build_repo_map(prompt: &str) -> String {
    let keywords: Vec<String> = prompt
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| w.len() > 3)
        .collect();

    let mut file_entries: Vec<(String, Vec<SigEntry>)> = Vec::new();

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

        let entries = parse_file(&source, language);
        if !entries.is_empty() {
            file_entries.push((path.display().to_string(), entries));
        }
    }

    if file_entries.is_empty() {
        return String::new();
    }

    // Files whose path + signatures overlap most with prompt keywords float to the top
    file_entries.sort_by_key(|(path, sigs)| {
        let haystack = format!(
            "{} {}",
            path,
            sigs.iter().map(|e| e.sig.as_str()).collect::<Vec<_>>().join(" ")
        )
        .to_lowercase();
        let matches = keywords
            .iter()
            .filter(|kw| haystack.contains(kw.as_str()))
            .count();
        Reverse(matches)
    });

    const BUDGET: usize = 4_000;
    let mut out = String::new();

    for (file, entries) in &file_entries {
        let lines: Vec<String> = entries
            .iter()
            .map(|e| {
                if e.calls.is_empty() {
                    format!("  L{}: {}", e.line, e.sig)
                } else {
                    format!("  L{}: {}  →[{}]", e.line, e.sig, e.calls.join(", "))
                }
            })
            .collect();

        let block = format!("{}:\n{}\n", file, lines.join("\n"));
        if out.len() + block.len() > BUDGET {
            break;
        }
        out.push_str(&block);
    }

    out
}
