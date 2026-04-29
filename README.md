# Kernelizer

**Get more accurate answers and use up to 40% fewer tokens — works with Claude Code and Gemini CLI.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org)
[![Claude Code Plugin](https://img.shields.io/badge/Claude%20Code-plugin-blueviolet)](https://github.com/Omkar1279/kernelizer)
[![Gemini Extension](https://img.shields.io/badge/Gemini%20CLI-extension-blue)](https://github.com/Omkar1279/kernelizer)

Kernelizer intercepts your prompt before it reaches the model and compiles it into a structured **KERNEL** template — a strict `Task / Constraints / Format / Verify` layout that eliminates ambiguity.

Research confirms the payoff: **structured prompts improve code generation accuracy by up to 20 percentage points** ([arXiv:2411.10541](https://arxiv.org/abs/2411.10541)) and **reduce total token consumption by up to 40%** by cutting the clarification rounds that vague prompts force. Fewer tokens. Right answer first time.

No API keys. No new accounts. Uses your existing `gemini` or `claude` CLI.

---

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
| `/kernelize <prompt>` | Best accuracy — scans codebase patterns, translates via LLM |
| `/kernelize-fast <prompt>` | Maximum speed — deterministic template, ~50ms, best with `@file` |

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

## Why Structured Prompts Win

Vague prompts force the model to guess scope, infer constraints, and hedge outputs — which costs tokens and produces generic answers that need correction. Every correction round costs more tokens and time. Kernelizer front-loads the structure so the model gets it right the first time.

**Before — vague prompt:**
```
fix the auth bug in @src/auth.rs
```

**After — compiled KERNEL:**
```
# Task
Identify and fix the authentication bug in `auth.rs` causing session tokens
to expire prematurely on concurrent requests.

# Constraints
- Preserve existing session token structure; no schema changes
- Limit scope to auth.rs unless a fix is impossible without touching deps
- Must not break existing test cases in tests/auth_test.rs

# Format
Return only the corrected function(s) with a one-line comment per change.

# Verify
Auth flow passes existing test suite; no regressions in session handling
under concurrent load (simulate with two simultaneous requests).
```

The model sees an unambiguous specification instead of a request. No follow-up questions. No scope creep. No hedging.

| | Vague prompt | KERNEL prompt |
|---|---|---|
| Accuracy on code tasks | baseline | **+20 percentage points** ¹ |
| Hallucination rate | baseline | **−25%** ² |
| Token usage (total session) | baseline | **up to −40%** ³ |
| Correction rounds needed | 3–5 avg | **1** |

<sub>¹ arXiv:2411.10541 — prompt format comparison across GPT-3.5 and GPT-4, code generation benchmark. ² DSPy structured prompt optimization, arXiv:2604.04869. ³ CompactPrompt structured prompting research, arXiv:2510.18043.</sub>

---

## How It Works

```
raw prompt + @file tags
        │
        ├─ resolve @file / @dir content        (~1ms, local)
        │
        ├─ scan codebase for patterns ──┐
        ├─ translate via gemini/claude  ─┘  parallel  (~2–5s)
        │
        └─ compile KERNEL markdown  ──▶  stdout  ──▶  model
```

**Codebase-aware** — automatically scans your project and injects relevant patterns as context so the model understands your existing style.

**Explicit vs advisory patterns** — if your prompt says *"like we do"* or *"our pattern"*, the KERNEL enforces strict project-style adherence. Otherwise patterns are advisory and the model is free to improve on them.

**Graceful degradation** — if neither `gemini` nor `claude` CLI is available, falls back to a deterministic offline template instantly.

---

## Bash / Universal

```bash
# Works with any model tool that reads stdin
aider --message "$(kernelizer-rs optimize 'fix the auth flow')"
kernelizer-rs optimize --fast "@src/api.rs add pagination"
```
