# Task: fix-clock-nanosleep-borrow

- **Status**: pending
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-clock-nanosleep-borrow
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 1
- **Bugs Fixed**: BUG-002

## Goal

Fix `add_nanos_signed` in `engine/src/clock.rs` so negative swing offsets that cross a whole-second boundary produce the correct `tv_sec` + `tv_nsec` pair.

## Context

The current implementation only adjusts `tv_nsec` and clamps to 0 when negative, silently dropping the borrow from `tv_sec`. This causes up to ~62 ms timing errors at 120 BPM with swing when the offset crosses a second boundary. `clock_nanosleep(TIMER_ABSTIME)` with a time in the past returns immediately, so the step fires too early.

**File:** `engine/src/clock.rs`, lines 114–124.
**Existing test** (`add_nanos_signed_clamps_to_zero`) only asserts `tv_nsec >= 0`; it misses the incorrect `tv_sec`.

## Acceptance Criteria

- `add_nanos_signed` performs arithmetic in full nanoseconds across both fields, then re-normalises:
  ```rust
  fn add_nanos_signed(ts: libc::timespec, nanos: i64) -> libc::timespec {
      let total_ns: i64 = ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec + nanos;
      let total_ns = total_ns.max(0);
      libc::timespec {
          tv_sec: (total_ns / 1_000_000_000) as libc::time_t,
          tv_nsec: (total_ns % 1_000_000_000) as libc::c_long,
      }
  }
  ```
- The existing test is updated to assert both `tv_sec` and `tv_nsec` for a boundary-crossing case (e.g. `tv_sec=1, tv_nsec=100_000_000, nanos=-200_000_000` → `tv_sec=0, tv_nsec=900_000_000`).
- `cargo test -p engine` passes.

## Notes

