# Task: fix-ratatui-crossterm-gate

- **Status**: pending
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

