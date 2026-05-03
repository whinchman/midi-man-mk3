# Task: fix-hid-and-main-followup

- **Status**: done
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-hid-and-main-followup
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 2
- **Bugs Fixed**: BUG-013, BUG-015, BUG-016

## Goal

Address three review findings on the fix-hid-and-main branch before merge.

## Context

Code review of fix-hid-and-main approved the core fixes but raised three issues:

### BUG-015 — Dead `open_device()` function in hid.rs

`open_device()` at `engine/src/hid.rs:136` is a public function that hardcodes `HID_VID`/`HID_PID`. After the BUG-008 fix, `run_hid` now takes explicit `vid`/`pid` parameters and no longer calls `open_device()`. The function is dead code and its hardcoded constants are now misleading.

**Fix:** Remove `open_device()` entirely (it is unused). If any test references it, update the test to call `run_hid` directly or remove the test.

### BUG-016 — `choose_midi_port` silently falls back to port 0 on filter miss

`engine/src/midi_out.rs:130` — when `port_name` filter is provided but no port name contains the filter string, the function falls back to port 0 with no log message, unlike `select_port_idx` which logs a warning. A user passing `--midi-port "nonexistent"` gets the first port with no indication the filter was ignored.

**Fix:** Add an `eprintln!` warning (or `log::warn!` if a logger is set up) when a non-None filter matches no ports, before falling back:
```rust
eprintln!("[midi] port filter {:?} matched no ports; falling back to port 0", name);
```

### BUG-013 — `.cargo/config.toml` references non-existent `CARGO_CONFIG_TOML` env var

The comment added by fix-cargo-config-tmp-paths tells developers they can activate the local override via `CARGO_CONFIG_TOML`. This env var does not exist in Cargo. The correct mechanism is `cargo build --config .cargo/config.local.toml` or exporting the variables directly in the shell.

**Fix:** Update the comment in `.cargo/config.toml` to document the correct invocation: `cargo build --config .cargo/config.local.toml` (or `cargo test --config ...`).

## Acceptance Criteria

- `open_device()` removed from `engine/src/hid.rs`; no dead code warnings from clippy on that file.
- `choose_midi_port` emits a warning log/eprintln when the filter matches no ports.
- `.cargo/config.toml` comment references `--config .cargo/config.local.toml` not a non-existent env var.
- `cargo test -p engine` passes.

## Notes

Implemented on branch `task/fix-hid-and-main-followup` (worktree at
`.workflow/worktrees/fix-hid-and-main-followup`), based off `fix/known-bugs`.

Changes in commit f87b7a7:
- **BUG-015**: Removed dead `open_device()` function from `engine/src/hid.rs`
  (26 lines of code deleted). `run_hid` already takes explicit `vid`/`pid`
  params and never called it.
- **BUG-016**: Added `eprintln!` warning in `choose_midi_port`
  (`engine/src/midi_out.rs`) when the filter string matches no port names,
  before the fallback to port 0.
- **BUG-013**: Updated the developer comment in `.cargo/config.toml` to
  reference `cargo build --config .cargo/config.local.toml` instead of the
  non-existent `CARGO_CONFIG_TOML` env var.

`cargo test -p engine`: 249 tests, all passed.

---

### Code Review — fix-hid-and-main-followup (2026-05-02)

Reviewed branch: `task/fix-hid-and-main-followup`
Diff base: `fix/known-bugs`
Reviewer: code-reviewer agent

#### BUG-015 (open_device removed)

PASS. `open_device()` is fully gone from `engine/src/hid.rs` — confirmed via diff and grep. No test references it. `HID_VID` and `HID_PID` constants remain (correctly, since `run_hid` callers use them as defaults). Acceptance criterion met.

#### BUG-016 (choose_midi_port filter warning)

PASS with one info note. The `eprintln!` is added at `engine/src/midi_out.rs:134` before the `unwrap_or` fallback, precisely as specified. The `names` slice is guaranteed non-empty at that point (the `ports.is_empty()` guard at lines 121-124 returns `None` first), so `names[0]` in the fallback is safe.

Info: The new warning uses the prefix `[midi]` while the companion `select_port_idx` uses `[midi_out]`. Both identify the MIDI subsystem but inconsistently. Not a bug, just a minor style divergence.

Info: `choose_midi_port` (the hw-io hardware path) is not covered by unit tests because it drives real ALSA ports; no test is expected. `select_port_idx` (the pure helper) has existing unit-test coverage. No new test is required.

#### BUG-013 (.cargo/config.toml comment)

PASS. Comment now reads `cargo build --config .cargo/config.local.toml` with no reference to the non-existent `CARGO_CONFIG_TOML` env var. Acceptance criterion met.

#### No new issues found.

All three acceptance criteria are satisfied. `cargo test -p engine` (249 tests) reported as passing by the implementing agent.

**Verdict: approve — 0 critical, 0 warning, 2 info**

Info findings (no action required):
1. `engine/src/midi_out.rs:134` — warning prefix `[midi]` differs from `select_port_idx`'s `[midi_out]`. Cosmetic; no impact on correctness.
2. `engine/src/midi_out.rs:134` — `choose_midi_port` is not unit-tested (hardware path). Expected and acceptable; `select_port_idx` carries the testable logic.

Additional finding outside the diff scope (pre-existing, not introduced by this branch):
- `.workflow/BUGS.md` contains two entries with the ID `BUG-015` (lines 427 and 519). The second one (about `apply_param_value` in `state.rs`) should be renumbered (e.g. BUG-017) to avoid confusion. Not a code defect — no action needed on this branch.
