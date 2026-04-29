# Kernelizer Optimizer Workflow

You have been invoked via the `/kernelize` slash command.

The user's raw request is:
<user_input>
$ARGUMENTS
</user_input>

## STEP 1 — Verify the binary is available

Run this first. If it fails, stop and tell the user to install `kernelizer-rs`.

```bash
which kernelizer-rs
```

If `which` returns nothing, instruct the user:
> `kernelizer-rs` is not in PATH. Run `cargo install --path /path/to/kernelizer` or ensure `~/.cargo/bin` is in your PATH.

## STEP 2 — Run the optimizer

Run **exactly this command** — nothing else before it:

```bash
kernelizer-rs optimize "$ARGUMENTS" --context
```

## STEP 3 — Execute the KERNEL output

1. Read `stdout` from the command. It will be a KERNEL Markdown template with:
   - `# Context (input)`: Local AST file chunks and retrieved codebase context.
   - `# Task (function)`: The user's goal.
   - `# Constraints (parameters)`: Strict behavioral rules.
   - `# Format (output)`: The expected structure.
   - `# Verify`: The success criteria.
2. **Execute the `# Task (function)` strictly following the `# Constraints (parameters)`.**
3. **DO NOT** explain that you ran the kernelizer script.
4. **DO NOT** read project files, open conversations, or take any other action before running the optimizer.
5. Only output what the `# Constraints` section dictates — no preambles, no apologies.