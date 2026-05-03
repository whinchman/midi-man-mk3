# Plan: HID Report Structs

## Overview

Define the USB HID IN/OUT report byte layout as `repr(C)` Rust structs for
both the engine and firmware crates. No shared crate; each side gets its own
copy. Unit tests live in `engine/src/hid.rs`.

## Steps

### Step 1 — Stub Cargo workspace
- `Cargo.toml` (workspace with `engine` and `firmware` members)
- `engine/Cargo.toml`, `firmware/Cargo.toml`
- `engine/src/lib.rs`, `firmware/src/lib.rs`

### Step 2 — `engine/src/hid.rs`
- `HID_VID` / `HID_PID` constants
- `InReport` (`repr(C)`, 64 bytes)
- `InReport::from_bytes(&[u8; 64]) -> InReport`
- `OutReport` (`repr(C)`, 64 bytes)
- `OutReport::to_bytes(&self) -> [u8; 64]`
- Unit tests: round-trip nonzero, all-zero decode, `to_bytes` round-trip,
  length assertions, VID/PID constants

### Step 3 — `firmware/src/report.rs`
- Byte-identical struct definitions for `InReport` and `OutReport`
- Comment pointing to spec: `// See midi-man-mk3-mvp.md Section 4`
