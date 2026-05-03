# Task: fix-cargo-config-tmp-paths

- **Status**: done
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-cargo-config-tmp-paths
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 1
- **Bugs Fixed**: BUG-003

## Goal

Remove host-specific `/tmp` path workarounds from `.cargo/config.toml` and document the local-override pattern instead.

## Context

`.cargo/config.toml` unconditionally sets `PKG_CONFIG_PATH = "/tmp/alsa-pkg"` and `rustflags = ["-L", "/tmp/alsa-lib"]`. These are workarounds for a system missing `alsa-lib-devel` that were committed to source. They break CI and other developer machines.

**File:** `.cargo/config.toml`, lines 11 and 17.

## Acceptance Criteria

- `[env]` `PKG_CONFIG_PATH` entry and `[target.x86_64-unknown-linux-gnu]` `rustflags` entry removed from `.cargo/config.toml`.
- `.gitignore` gains an entry for `.cargo/config.local.toml`.
- A comment in `.cargo/config.toml` (or in a build note at the top of `engine/src/midi_out.rs`) documents the local-override pattern for developers who need the workaround:
  ```toml
  # .cargo/config.local.toml  (gitignored)
  [env]
  PKG_CONFIG_PATH = "/tmp/alsa-pkg"
  [target.x86_64-unknown-linux-gnu]
  rustflags = ["-L", "/tmp/alsa-lib"]
  ```
- `cargo test -p engine` passes on the current system (with whatever alsa setup is present).

## Notes

**Completed on branch `fix/known-bugs` (commit 75b7cdd).**

The `/tmp/alsa-pkg` and `/tmp/alsa-lib` entries had already been removed from
`.cargo/config.toml` in a prior commit (9d3207e). This task added the two
remaining acceptance criteria:

- `.gitignore` — added entry `.cargo/config.local.toml` so local override
  files are never accidentally committed.
- `.cargo/config.toml` — added a multi-line comment block documenting the
  local-override pattern (create `.cargo/config.local.toml` with the `[env]`
  and `[target.x86_64-unknown-linux-gnu]` stanzas) for developers whose
  systems lack `alsa-lib-devel`.

`cargo test -p engine` passed: 249 tests across 8 test files, 0 failures.

---

## Code Review (2026-05-02)

**Reviewer:** code-reviewer agent
**Branch reviewed:** `fix/known-bugs` (commit 75b7cdd)
**Verdict:** approve (with one warning)

### Acceptance Criteria Check

| Criterion | Status |
|---|---|
| `PKG_CONFIG_PATH` entry removed from `.cargo/config.toml` | PASS (done in prior commit 9d3207e) |
| `rustflags` `/tmp` entry removed from `.cargo/config.toml` | PASS (done in prior commit 9d3207e) |
| `.gitignore` gains `.cargo/config.local.toml` entry | PASS |
| Comment in `.cargo/config.toml` documents local-override pattern | PASS |
| `cargo test -p engine` passes | PASS (249 tests, 0 failures) |

### Findings

#### [WARNING] `.cargo/config.toml` comment references non-existent `CARGO_CONFIG_TOML` env var

**File:** `.cargo/config.toml`, lines 7–8 on the `fix/known-bugs` branch (commit 75b7cdd)

The comment tells developers they can point Cargo at `.cargo/config.local.toml` "via `CARGO_CONFIG_TOML`". No such environment variable exists in Cargo (as of Cargo 1.93). The real options are:
- `--config` on the command line (`cargo build --config .cargo/config.local.toml`)
- Setting `CARGO_HOME` to a directory whose `config.toml` has the overrides (too heavy-handed)
- Appending `KEY=VALUE` pairs with `--config KEY=VALUE` flags

A developer following the comment will search for `CARGO_CONFIG_TOML`, find nothing, and assume the override mechanism does not work.

**Suggested fix:** Replace "via `CARGO_CONFIG_TOML` or by temporarily editing this file" with the actual mechanism:

```
# To activate without editing this file, pass --config on the command line:
#   cargo build --config .cargo/config.local.toml
# Or export the values directly in your shell before building:
#   export PKG_CONFIG_PATH=/tmp/alsa-pkg
#   export RUSTFLAGS="-L /tmp/alsa-lib"
```

**Severity:** warning — misleading developer documentation; does not affect correctness or CI.

### Summary

- 0 critical findings
- 1 warning finding (misleading env var name in a code comment)
- 0 info findings

The primary goal of BUG-003 (removing hardcoded `/tmp` paths from committed config) is fully achieved. All acceptance criteria pass. The warning is a documentation inaccuracy that could confuse developers but does not affect builds or test execution. Approving with the expectation that the comment is updated in a follow-up.
