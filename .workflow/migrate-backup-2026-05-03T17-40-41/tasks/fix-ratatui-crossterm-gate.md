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
