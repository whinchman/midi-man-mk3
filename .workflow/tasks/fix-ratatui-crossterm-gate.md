# Task: fix-ratatui-crossterm-gate

- **Status**: done
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-ratatui-crossterm-gate
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 1
- **Bugs Fixed**: BUG-007

## Goal

Ensure `crossterm` is only pulled in when the `hw-io` feature is enabled by declaring `ratatui` without its default features and gating crossterm explicitly.

## Context

`ratatui = "0.30"` uses ratatui's default features which include the `crossterm` feature, causing `ratatui-crossterm` and `crossterm` to appear in the dependency tree even without `hw-io`. The code comment claims crossterm is gated behind `hw-io`, but it isn't.

**File:** `engine/Cargo.toml`.

## Acceptance Criteria

- `ratatui` declared with `default-features = false` and explicit non-crossterm features (e.g. `all-widgets`, `macros`, `layout-cache`, `underline-color` — use whatever subset the code actually needs).
- `crossterm` added as an optional dependency gated behind `hw-io`:
  ```toml
  crossterm = { version = "0.29", optional = true }
  [features]
  hw-io = ["midir", "hidapi", "crossterm", "ratatui/crossterm"]
  ```
- `cargo tree -p engine` (without `--features hw-io`) no longer shows `crossterm`.
- `cargo test -p engine` passes.
- `cargo build -p engine --features hw-io` compiles.

## Notes

The fix was already present in `fix/known-bugs` as commit `55eba05`
(`fix(terminal-ui): gate ratatui crossterm backend behind hw-io feature (BUG-007)`).

**Branch:** `fix-ratatui-crossterm-gate` (worktree off `fix/known-bugs`)

**What was verified:**
- `engine/Cargo.toml` already has `ratatui` with `default-features = false, features = ["macros"]`
- `crossterm` is declared as `optional = true`
- `hw-io` feature gates both `crossterm` and `ratatui/crossterm`
- `cargo tree -p engine` (without `--features hw-io`) shows no crossterm in the dependency tree
- `cargo test -p engine` passes: 249 tests across 8 test suites (clock, hid, input, main_wiring, midi_out, music_theory, state, ui)

No code changes were needed — BUG-007 was already fixed prior to this task being dispatched.

---

## Code Review

**Reviewer:** code-reviewer agent
**Date:** 2026-05-02
**Branch reviewed:** fix/known-bugs/fix-ratatui-crossterm-gate vs fix/known-bugs

### Summary

The only change on this branch is the task file itself (status: pending → done, plus Notes). No application code was modified. The actual fix (commit `55eba05` on `fix/known-bugs`) was already present in the base branch before this task was dispatched.

### Acceptance Criteria Verification

All five criteria were re-verified independently:

- `engine/Cargo.toml` line 15: `ratatui = { version = "0.30", default-features = false, features = ["macros"] }` — PASS
- `engine/Cargo.toml` line 16: `crossterm = { version = "0.29", optional = true }` — PASS
- `engine/Cargo.toml` line 22: `hw-io = ["midir", "hidapi", "crossterm", "ratatui/crossterm"]` — PASS
- `cargo tree -p engine` (no hw-io) — crossterm absent — PASS
- `cargo test -p engine` — 249 tests pass across 10 test suites — PASS
- `cargo build -p engine --features hw-io` — compiles with one pre-existing warning (BUG-006 stale buf, unrelated to this task) — PASS

### Findings

No critical or warning findings. One pre-existing `unused_assignments` warning in `engine/src/hid.rs:313` is BUG-006 (already logged); it is not introduced by this branch.

The ratatui feature subset (`["macros"]`) is narrower than the acceptance criteria example (`all-widgets, macros, layout-cache, underline-color`) but is correct — the code only uses core ratatui items (Layout, widgets, style, text, Frame, Terminal) which are unconditionally compiled into ratatui regardless of optional feature flags. Tests confirm this is sufficient.

### Verdict

**APPROVE** — 0 critical, 0 warning, 0 info findings. All acceptance criteria met.

---

## PR Feedback

PR: https://github.com/whinchman/midi-man-mk3/pull/15

### Comments Requiring Action

_(none)_

### CI Failures

_(none — no CI checks configured on this repository)_

### Questions / Acknowledged

_(none)_
