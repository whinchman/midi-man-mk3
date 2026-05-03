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

---

## Code Review Findings (code-reviewer agent, 2026-05-02)

Reviewed branch `task/fix-hid-and-main` against `fix/known-bugs`.
Files reviewed: `engine/tests/hid.rs`, `engine/src/hid.rs`, `engine/src/midi_out.rs`, `engine/src/main.rs`, `engine/src/cli.rs`.

### BUG-005 — VERIFIED FIXED
`engine/tests/hid.rs` lines 138–149: `in_report_field_offsets_match_wire_spec` uses
`std::mem::offset_of!` for all 10 `InReport` fields. No `unsafe` anywhere in the test
file. Acceptance criterion met.

### BUG-006 — VERIFIED FIXED
`engine/src/hid.rs` line 323: `buf = [0u8; 64]` is the first statement inside the
`loop` body, before `device.read_timeout`. Stale-byte issue is resolved.
Acceptance criterion met.

### BUG-008 — VERIFIED FIXED
`run_hid` (hid.rs:286) now accepts `vid: u16, pid: u16` parameters.
`run_midi_out` (midi_out.rs:268) now accepts `port_name: Option<String>`.
`main.rs` lines 100–101 forward `args.hid_vid.unwrap_or(HID_VID)` and
`args.hid_pid.unwrap_or(HID_PID)` to the HID thread, and line 65 forwards
`selected_midi_port` (which was populated by `choose_midi_port(args.midi_port.as_deref())`)
to the MIDI thread. Acceptance criterion met.

### BUG-009 — VERIFIED FIXED
Clock thread is spawned only under `#[cfg(feature = "hw-io")]` (main.rs:80–88).
Shutdown sequence (main.rs:174–188): sets `hid_shutdown` flag, joins `hid_thread`,
drops `cmd_tx`, joins `cmd_thread`, then joins `clock_thread` and `midi_thread`.
Join order is correct (hid_thread dropped first releases its `cmd_tx` clone, then the
original `cmd_tx` is dropped, then cmd_thread can exit). Acceptance criterion met.

---

### [WARNING] `open_device()` in hid.rs still uses hardcoded VID/PID — dead public API
**File:** `engine/src/hid.rs`, lines 136–154
**Severity:** warning

`open_device()` is a `#[cfg(feature = "hw-io")]` public function that still opens the
device using the hardcoded `HID_VID`/`HID_PID` constants. BUG-008's fix correctly added
`vid`/`pid` parameters to `run_hid`, but `open_device` was not updated to accept them.
`open_device` is no longer called anywhere in `main.rs` (the device is opened directly
inside `run_hid`), so it is dead code. It is not flagged by the compiler because it is
`pub`. If any future caller uses it, they will not get the CLI override behaviour.

Suggested fix: Either remove `open_device` (it is unreachable from main and has no tests),
or add `vid: u16, pid: u16` parameters to match `run_hid`'s pattern.

### [WARNING] `choose_midi_port` silently falls back to port 0 with no warning when filter does not match
**File:** `engine/src/midi_out.rs`, lines 130–134
**Severity:** warning

When `filter` is `Some(f)` and no port name contains `f`, the function returns
`Some(names[0].clone())` with no log message. The companion function `select_port_idx`
(used by `open_port`) does log a fallback warning (line 73), but `choose_midi_port` does
not. A user who passes `--midi-port "my-synth"` and has no such port will silently get
the first port with no indication that their override was ignored.

Suggested fix: Add an `eprintln!` warning analogous to the one in `select_port_idx` before
returning `Some(names[0].clone())`.

### [INFO] `midi_tx.send(MidiEvent::Stop)` is sent before `hid_thread` is joined (non-blocking)
**File:** `engine/src/main.rs`, lines 161–177
**Severity:** info

`MidiEvent::Stop` is sent on `midi_tx` before `hid_shutdown` is stored and before
`hid_thread` is joined. The channel has capacity 64, so the send itself will not block.
However, if the clock thread happens to send 64 events between the Stop send and the
`drop(midi_tx)` call, the Stop could be pushed behind 64 events. In practice the clock
period is much longer than the shutdown path, so this is not a real risk. The ordering is
fine as-is but worth noting.

### [INFO] Note-off threads spawned in `dispatch` are never joined
**File:** `engine/src/midi_out.rs`, lines 226–231
**Severity:** info

`MidiEvent::NoteOn` dispatches a note-off by spawning an anonymous thread with a
`thread::sleep`. These threads hold a cloned `MidiSender` and are never stored or joined.
On normal shutdown the MIDI connection will be closed while note-off threads may still be
sleeping. This is pre-existing (not introduced by these fixes) and is acceptable at current
scope, but should be tracked for a future cleanup.

---

### Review Summary

- 0 critical findings
- 2 warning findings (open_device dead API, silent fallback in choose_midi_port)
- 2 info findings

All four targeted bugs (BUG-005, BUG-006, BUG-008, BUG-009) are correctly fixed.
The two warnings are minor issues that did not exist before this task or represent
pre-existing inconsistencies surfaced by the refactor.

**Verdict: request-changes** — the two warnings should be resolved before merge.
The `open_device` dead-code issue (warning) could be a source of confusion for future
callers; the silent fallback (warning) is a user-experience regression relative to the
matching warning already present in `select_port_idx`.

Follow-up task files: none created (warnings are targeted enough for direct inline fixes).
