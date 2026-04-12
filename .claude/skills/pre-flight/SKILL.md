---
name: pre-flight
description: Run project tests and linter to verify a feature is fully functioning and well written. Use before commits, after implementing a feature, or when asked for a preflight check.
user-invocable: true
disable-model-invocation: false
allowed-tools: Bash(cargo *)
---

# Pre-Flight Check

Run a full pre-flight verification of the current codebase. Execute each step sequentially — stop and report on first failure.

## Steps

1. **Format check**: Run `cargo fmt --all -- --check`. Report any formatting violations.
2. **Lint**: Run `cargo clippy --workspace -- -D warnings`. Report any clippy warnings as errors.
3. **Tests**: Run `cargo test --workspace`. Report any test failures with the failing test name and output.
4. **Build check**: Run `cargo build --workspace`. Confirm clean compilation with no warnings.

## Reporting

After all steps complete (or on first failure), produce a summary:

```
Pre-flight results:
  Format:  PASS / FAIL
  Lint:    PASS / FAIL
  Tests:   PASS / FAIL (X passed, Y failed)
  Build:   PASS / FAIL
```

If any step fails, list the specific errors and suggest fixes.
