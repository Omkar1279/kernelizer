# Kernelizer

**Get better answers from your AI model, using fewer tokens.**

Kernelizer intercepts your prompt before it reaches the model and compiles it into a strict **KERNEL** template — a structured `Task / Constraints / Format / Verify` layout that eliminates ambiguity. Structured prompts produce more accurate, on-target responses and cost significantly fewer tokens than conversational back-and-forth.

Uses your existing `gemini` or `claude` CLI as the formatting layer. No API keys. No extra accounts.

## Install

```bash
cargo install --git https://github.com/Omkar1279/kernelizer kernelizer-rs
```

Requires `~/.cargo/bin` in `PATH`.

---

## Claude Code

```bash
claude plugin add https://github.com/Omkar1279/kernelizer
```

| Command | When to use |
|---------|-------------|
| `/kernelize <prompt>` | Best accuracy — scans your codebase for patterns, translates via LLM |
| `/kernelize-fast <prompt>` | Fastest — deterministic template, ~50ms, ideal with `@file` tags |

```
/kernelize fix the auth bug in @src/auth.rs
/kernelize-fast add rate limiting to @accounts/views.py
```

---

## Gemini CLI

```bash
gemini extensions install https://github.com/Omkar1279/kernelizer
```

```
/kernelize suggest an indexing strategy for @src/db.rs
```

---

## Why It Works

Vague prompts force models to guess your intent, producing generic answers that need multiple follow-up rounds. Each correction costs tokens. Kernelizer front-loads the structure so the model gets it right the first time.

```
"fix the auth bug in @src/auth.rs"
            │
            ▼
# Task
Identify and fix the authentication bug in `auth.rs` ...

# Constraints
- Preserve existing session token structure
- No changes outside auth.rs unless strictly required
- Must not break existing test cases

# Format
Return only the corrected code with a one-line explanation per change.

# Verify
Auth flow passes existing tests; no regressions in session handling.
```

**Fewer tokens** — one structured prompt replaces 3–5 clarification rounds.  
**More accurate** — constraints and verify criteria eliminate model hallucinations and scope creep.  
**Codebase-aware** — automatically scans your project for relevant patterns and injects them as context.

---

## Bash / Universal

```bash
aider --message "$(kernelizer-rs optimize 'fix the auth flow')"
kernelizer-rs optimize --fast "@src/api.rs add pagination"
```
