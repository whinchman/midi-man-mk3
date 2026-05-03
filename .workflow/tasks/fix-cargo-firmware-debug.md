# Task: fix-cargo-firmware-debug

- **Status**: pending
- **Type**: coder
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

