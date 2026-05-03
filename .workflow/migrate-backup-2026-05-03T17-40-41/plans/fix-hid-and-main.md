# Plan: fix-hid-and-main

## Overview

Fix four bugs in `engine/src/hid.rs`, `engine/src/midi_out.rs`, and `engine/src/main.rs`.
All four were already resolved in prior commits on `fix/known-bugs`; this task
verifies the fixes, confirms tests pass, and marks the work done.

## Bug Summary

| Bug | File | Fix |
|-----|------|-----|
| BUG-005 | `engine/tests/hid.rs` | Replace `unsafe transmute` with `offset_of!` assertions |
| BUG-006 | `engine/src/hid.rs` | Zero `buf = [0u8; 64]` at top of each loop iteration |
| BUG-008 | `engine/src/hid.rs`, `engine/src/midi_out.rs`, `engine/src/main.rs` | Add `vid`/`pid` params to `run_hid`; forward CLI args |
| BUG-009 | `engine/src/main.rs` | Join all threads; skip clock in non-hw-io builds |

## Steps

1. Verify `engine/tests/hid.rs::in_report_field_offsets_match_wire_spec` uses `offset_of!` — confirmed (line 138).
2. Verify `engine/src/hid.rs::run_hid` zeroes `buf` each iteration — confirmed (line 323).
3. Verify `run_hid` accepts `vid: u16, pid: u16` params — confirmed (line 290-292).
4. Verify `run_midi_out` accepts `port_name: Option<String>` — confirmed (line 268).
5. Verify `main.rs` joins `cmd_thread`, `clock_thread`, `midi_thread` — confirmed (lines 182-188).
6. Verify clock thread is only spawned under `#[cfg(feature = "hw-io")]` — confirmed (line 80).
7. Run `cargo test -p engine` — 249 tests, 0 failures.

## Test Command

```
cargo test -p engine
```
