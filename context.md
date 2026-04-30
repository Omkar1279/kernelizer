# Kernelizer — Project Context

**Purpose:** Hyper-low-latency Rust CLI that intercepts unstructured prompts and reformats them into a strict KERNEL markdown template before forwarding to heavy reasoning models. No API keys required — uses the host machine's `claude` CLI as the LLM subprocess.

---

## Pipeline (src/main.rs)

```
raw prompt
  → extract_at_files()        [template.rs] — strip @file/@dir tags, read content
  → detects_follow_pattern_intent()  [template.rs] — keyword scan for style-match signals
  → build_repo_map()  (spawn_blocking) [repomap.rs] — tree-sitter AST walk → rich sig map
  → translate_with_cli()      [template.rs] — claude --bare --model haiku → KernelParams JSON
  → compile_kernel()          [template.rs] — assemble final KERNEL markdown → STDOUT
```

`--fast` flag skips repo map + LLM translation entirely (deterministic template, ~10ms).

---

## Key Types

```rust
// template.rs
struct KernelParams { task, constraints: Vec<String>, format, verify }
struct ParsedPrompt { cleaned_prompt: String, explicit_files_content: String }

// repomap.rs (internal)
struct SigEntry { line: usize, sig: String, calls: Vec<String> }
```

---

## Source Files

| File | Role |
|------|------|
| `src/main.rs` | Async orchestrator, tokio runtime, spawn_blocking for repo map |
| `src/cli.rs` | Clap `Cli` + `Commands::Optimize { prompt, fast }` |
| `src/template.rs` | `extract_at_files`, `translate_with_cli`, `compile_kernel`, `detects_follow_pattern_intent` |
| `src/repomap.rs` | Tree-sitter AST walk → full signatures + call-graph edges |
| `src/error.rs` | `KernelizerError { Io, ParseError }` via thiserror |
| `src/lib.rs` | Crate root; `pub mod` declarations |

---

## Repo Map Architecture (src/repomap.rs)

Tree-sitter parses all `.rs/.py/.js/.ts/.tsx/.go` files. For each:
- **Signature**: extracted from node start → body-block start (collapsed whitespace, stripped `{`). Captures full multi-line sigs including return types and generic bounds.
- **Call edges**: `call_expression` (field `function`) + `method_call_expression` (field `method`) within each function body → deduplicated list of callee names.

Output format per function:
```
  L{line}: {full_signature}  →[callee1, callee2, ...]
```

Files ranked by prompt-keyword overlap. Hard cap: **4 000 chars** (build) / **3 000 chars** (sent to Haiku in translation).

Supported language paradigms:
- **Rust**: `fn`, `struct`, `enum`, `trait`, `impl`, `impl Trait for Type`, `type` aliases
- **Python**: `def`, `class` (with inheritance)
- **JS/TS**: `function`, `class`, `interface`, `type` alias, method definitions
- **Go**: `func`, type declarations

---

## KERNEL Output Format

```markdown
# Context (input)
## Explicitly Tagged Files: ...
## Repo Map (function & type signatures): ...
## Piped Standard Input: ...

# Task (function)
<single unambiguous goal>

# Constraints (parameters)
- <up to 4 codebase-specific constraints>
- Tool Usage: ...
- Pattern Adherence: ...

# Format (output)
<exact output structure>

# Verify
<how to test with real code paths>
```

---

## Design Decisions

- **No SDK dependency**: LLM calls via `claude --bare -p` subprocess → binary stays provider-agnostic.
- **Graceful degradation**: if `claude` CLI absent, `translate_with_cli` returns `None` → deterministic template used.
- **`--fast` mode**: skip repo map + Haiku → pure deterministic output for latency-critical cases.
- **Haiku for translation**: fast and cheap; repo map + explicit files give it enough codebase context.
- **`lancedb_context` in KernelParams**: placeholder for future RAG integration — not yet implemented.
- **`spawn_blocking`**: repo map is CPU-bound AST walk; kept off async executor to avoid blocking Haiku call.

---

## Plugin Integration

- `commands/kernelize.toml` → `/kernelize` slash command → `kernelizer-rs optimize "{{args}}"`
- `skills/kernelize/SKILL.md` → skill workflow: verify binary, run with `--context`, execute KERNEL strictly
- `gemini-extension.json` → Gemini CLI extension (deprecated, Gemini removed from codebase)

---

## Token Budget

| Stage | Cap |
|-------|-----|
| Explicit file content sent to Haiku | 2 000 chars |
| Repo map sent to Haiku | 3 000 chars |
| Repo map built (full, for compile_kernel) | 4 000 chars |

---

## Build

```bash
cargo build           # dev
cargo build --release # optimized
cargo install --path . # install to PATH
kernelizer-rs optimize "your prompt" [--fast]
```
