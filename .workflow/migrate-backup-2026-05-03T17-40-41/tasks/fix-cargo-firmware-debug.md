# Task: fix-cargo-firmware-debug

- **Status**: done
- **Type**: code-reviewer
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-cargo-firmware-debug
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 1
- **Bugs Fixed**: BUG-001

## Goal

Prevent firmware release builds from embedding full debug symbols by adding a package-level profile override in the workspace `Cargo.toml`.

## Context

`[profile.release]` at the workspace root sets `debug = 2` (full DWARF). Because Cargo applies workspace profiles to all members, `cargo build -p firmware --release` embeds full debug info into the firmware ELF — a flash-overflow risk on RP2040's 2 MB flash as the firmware grows.

**File:** `Cargo.toml` (workspace root), lines 13–16.

## Acceptance Criteria

- Workspace `Cargo.toml` has a `[profile.release.package.firmware]` section with `debug = false` (or equivalent stripping override).
- `cargo build -p engine --release` still works (engine retains `debug = 2` for profiling).
- `cargo test -p engine` passes.

## Notes

Added `[profile.release.package.firmware]` with `debug = false` to `/Cargo.toml`
(workspace root). Firmware release builds no longer embed full DWARF symbols;
engine release builds retain `debug = 2` for profiling.

Branch: `task/fix-cargo-firmware-debug` (worktree off `fix/known-bugs`)
Commit: a5f5357 — fix(firmware): strip debug symbols from firmware release builds

`cargo test -p engine`: 249 tests, 0 failures.

---

### Code Review — fix-cargo-firmware-debug

**Reviewer:** code-reviewer agent
**Date:** 2026-05-02
**Branch reviewed:** `task/fix-cargo-firmware-debug` vs `fix/known-bugs`
**Commit:** a5f5357

#### Findings

No bugs, security issues, logic errors, or code standard violations found.

**Correctness:** `[profile.release.package.<name>]` is valid Cargo per-package profile override syntax (stable since Rust 1.51). `debug = false` is a documented accepted value (equivalent to `debug = 0`) — strips all debug symbols.

**Scope:** The override only sets `debug = false`; all other release profile settings (`opt-level = "s"`, `lto = true`, `codegen-units = 1`) are inherited unchanged from `[profile.release]`. Engine crate has no override and retains `debug = 2`.

**Acceptance criteria:** All three criteria are met:
1. `[profile.release.package.firmware]` with `debug = false` is present in `Cargo.toml`.
2. Engine retains `debug = 2` — no engine override exists.
3. `cargo test -p engine` passed (249 tests, 0 failures, per coder notes).

**Comment quality:** Clear rationale comment explaining flash-overflow risk and the engine profiling tradeoff.

#### Summary

- 0 critical, 0 warning, 0 info findings
- **Verdict: approve**

---

## PR Feedback

PR: https://github.com/whinchman/midi-man-mk3/pull/14

### Comments Requiring Action

(none)

### CI Failures

(none — no CI checks configured on this repository)

### Questions / Acknowledged

(none)
