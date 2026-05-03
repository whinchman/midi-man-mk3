# Task: fix-hid-and-main

- **Status**: done
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-hid-and-main
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 1
- **Bugs Fixed**: BUG-005, BUG-006, BUG-008, BUG-009

## Goal

Fix four bugs in `engine/src/hid.rs` and `engine/src/main.rs`: unsafe transmute in test, stale buffer bytes, CLI args not forwarded, and clock thread not joined.

## Context

All four bugs touch `hid.rs` and/or `main.rs`, so they are batched together to avoid merge conflicts.

### BUG-005 — `unsafe transmute` in test violates Safe-Rust standard

`in_report_field_offsets_match_wire_spec` in `engine/src/hid.rs` (line ~317) uses `unsafe { std::mem::transmute::<InReport, [u8; 64]> }`. The code standard forbids unsafe without a justifying comment, and the transmute is technically unsound if a non-align-1 field is ever added.

**Fix:** Replace with `std::mem::offset_of!` assertions (stable since Rust 1.77):
```rust
use std::mem::offset_of;
assert_eq!(offset_of!(InReport, report_id), 0);
assert_eq!(offset_of!(InReport, seq), 1);
// ... etc for each field
```

### BUG-006 — `run_hid` reuses buffer; partial reads leave stale bytes

`buf` is declared once before the loop. `hidapi::read_timeout` only writes `n` bytes; the remaining bytes keep their previous values. Any `n > 0` but `n < 64` produces a corrupt `InReport`.

**Fix:** Zero `buf` at the start of each loop iteration:
```rust
loop {
    buf = [0u8; 64];
    let n = match device.read_timeout(&mut buf, 5) { ... };
    ...
}
```
Or add a short-read guard (`if n < 64 { continue; }`). Either approach is acceptable; pick whichever is cleaner.

### BUG-008 — CLI args `--midi-port`, `--hid-vid`, `--hid-pid` parsed but never forwarded

`parse_args()` returns `CliArgs` with `midi_port`, `hid_vid`, `hid_pid`, but `main()` logs them and ignores them. `run_midi_out` calls the private `open_first_port()` regardless; `run_hid` uses hardcoded `HID_VID`/`HID_PID` constants.

**Fix:**
- Add `port_filter: Option<&str>` to `run_midi_out` (or a helper `open_port_by_name`). Pass `args.midi_port.as_deref()`.
- Add `vid: u16, pid: u16` params to `run_hid`. Default to `HID_VID`/`HID_PID` when `args.hid_vid`/`hid_pid` are `None`. Pass `args.hid_vid.unwrap_or(HID_VID)` and `args.hid_pid.unwrap_or(HID_PID)`.

### BUG-009 — Clock thread never exits in non-hw-io builds; not joined on shutdown

In non-hw-io builds `midi_rx` is dropped immediately, so the clock thread loops forever in its sleep. In hw-io builds `_clock_thread` is dropped without `join()` — the Stop event may not flush before the process exits.

**Fix:** Join all threads explicitly after dropping senders:
```rust
let _ = midi_tx.send(MidiEvent::Stop);
drop(midi_tx);
drop(cmd_tx);
let _ = _cmd_thread.join();
let _ = _clock_thread.join();
let _ = _midi_thread.join();
```
For non-hw-io builds where the clock loop never naturally exits: add a shutdown `AtomicBool` or `SyncSender<()>` so the clock thread checks for a stop signal each iteration. Alternatively, skip spawning the clock thread entirely in non-hw-io builds (simplest fix).

## Files to Modify

- `engine/src/hid.rs` — BUG-005 (test), BUG-006 (run_hid loop), BUG-008 (run_hid signature)
- `engine/src/midi_out.rs` — BUG-008 (run_midi_out signature / port selection)
- `engine/src/main.rs` — BUG-008 (pass args to threads), BUG-009 (join threads)

## Acceptance Criteria

- No `unsafe` in `hid.rs` tests; `offset_of!` assertions cover all `InReport` fields.
- `run_hid` buffer is zeroed each iteration (or short reads are skipped).
- `cargo run -p engine --features hw-io -- --midi-port "foo" --hid-vid 0x1234 --hid-pid 0x5678` uses the overrides (verify by log output or code inspection).
- All spawned threads are joined in `main()` before the process exits.
- `cargo test -p engine` passes.

## Notes

All four bugs were already resolved in prior commits on `fix/known-bugs` before
this task was dispatched. Verification confirmed each fix is in place:

- **BUG-005**: `engine/tests/hid.rs` line 138 — `in_report_field_offsets_match_wire_spec`
  uses `std::mem::offset_of!` for all 10 `InReport` fields; no `unsafe` present.
- **BUG-006**: `engine/src/hid.rs` line 323 — `buf = [0u8; 64]` zeroed at the top of
  each loop iteration before `device.read_timeout`.
- **BUG-008**: `run_hid` accepts `vid: u16, pid: u16`; `run_midi_out` accepts
  `port_name: Option<String>`; `main.rs` forwards `args.hid_vid`, `args.hid_pid`,
  and `selected_midi_port` to the respective thread spawners.
- **BUG-009**: Clock thread is only spawned under `#[cfg(feature = "hw-io")]`;
  `main.rs` explicitly joins `cmd_thread`, `clock_thread`, and `midi_thread` in
  dependency order after dropping all senders.

Branch: `task/fix-hid-and-main` (worktree off `fix/known-bugs`)
Test result: `cargo test -p engine` — 249 tests, 0 failures.
s