# Task: fix-clock-nanosleep-borrow

- **Status**: done
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

QA pass (2026-05-02): Added 12 new tests to `engine/tests/clock.rs` covering the actual epoch-clamp path (sub-second start going below zero), zero-offset identity, multi-second positive/negative spans, and 120 BPM swing integration scenarios (+50 delay, -50 advance, -50 clamp-to-epoch). Clock test count increased from 32 to 44. Full suite: 261 tests, 0 failures. Commit: 07c21b2 on branch `task/fix-clock-nanosleep-borrow`.

The fix was already present in `fix/known-bugs` at the time the task was dispatched. `add_nanos_signed` in `engine/src/clock.rs` (lines 125–132) correctly computes the total in full nanoseconds before re-normalising, handling borrow across the second boundary. The test `add_nanos_signed_clamps_to_zero` in `engine/tests/clock.rs` already asserts both `tv_sec=0` and `tv_nsec=900_000_000` for the boundary-crossing case (`tv_sec=1, tv_nsec=100_000_000, nanos=-200_000_000`). An additional test `add_nanos_signed_negative_crosses_second_boundary` covers the 120 BPM swing scenario. All 249 tests pass.

Branch: `task/fix-clock-nanosleep-borrow` (worktree at `.workflow/worktrees/fix-clock-nanosleep-borrow`), based off `fix/known-bugs`.

---

## Code Review — 2026-05-02

**Reviewer:** code-reviewer agent
**Branch reviewed:** `task/fix-clock-nanosleep-borrow`
**Base branch:** `fix/known-bugs`
**Files examined:** `engine/src/clock.rs`, `engine/tests/clock.rs`

### Diff summary

`git diff fix/known-bugs...task/fix-clock-nanosleep-borrow` produces no output — the task branch is identical to its base branch. The fix was already committed directly to `fix/known-bugs` before the task branch was cut; the task branch carries no independent commits. The review therefore evaluates the implementation as it stands on both branches.

---

### Findings

#### [INFO] engine/src/clock.rs:125–132 — Implementation matches acceptance criteria exactly

`add_nanos_signed` folds both fields into a single `i64` total, clamps to 0, then re-normalises via integer division and modulo. This is the exact algorithm specified in the acceptance criteria. All four boundary cases verified arithmetically:

- `1.1s - 0.2s = 0.9s` → `tv_sec=0, tv_nsec=900_000_000` — correct
- `5.010s - 62.5ms = 4.9475s` → `tv_sec=4, tv_nsec=947_500_000` — correct
- `0s - 1ms` → clamped to `tv_sec=0, tv_nsec=0` — correct (becomes past time, clock_nanosleep returns immediately)
- `0.999s + 100ms = 1.099s` → `tv_sec=1, tv_nsec=99_000_000` — correct

No overflow risk: `ts.tv_sec as i64 * 1_000_000_000` can overflow for `tv_sec > 9_223_372_036` (year 292), which is not a practical concern for `CLOCK_MONOTONIC`.

#### [INFO] engine/tests/clock.rs — All acceptance criteria for tests are met

- `add_nanos_signed_clamps_to_zero` now asserts both `tv_sec=0` AND `tv_nsec=900_000_000` for the boundary-crossing case. The original weak assertion (`tv_nsec >= 0`) has been replaced with the correct strong assertions.
- `add_nanos_signed_negative_crosses_second_boundary` covers the 120 BPM swing scenario (`5.010s - 62.5ms`).
- `add_nanos_signed_positive_crosses_second_boundary` verifies the positive carry path.
- All tests are pure unit tests; no external I/O, no HTTP calls, no mocks needed.
- `unwrap()` appears only in test code, which is permitted by project standards.

#### [INFO] engine/tests/clock.rs — Test name `add_nanos_signed_clamps_to_zero` is misleading

The test exercises the boundary-crossing subtraction path (`1.1s - 0.2s = 0.9s`), which does NOT clamp to zero — it produces 0.9 seconds. The name implies clamping but the body tests borrow propagation. This causes no test failures and does not affect correctness, but a future reader looking for the actual clamp-to-zero test (negative total going below epoch) will not find one under that name. No dedicated test covers `tv_sec=0, tv_nsec=50_000_000, nanos=-100_000_000` → `tv_sec=0, tv_nsec=0` (the actual clamp case). This is an info-level naming/coverage observation only.

#### [INFO] engine/src/clock.rs — Code standards compliance

- No `unwrap()` in non-test code; all fallible operations use `expect()` with messages.
- All `unsafe` blocks have explaining comments (`// SAFETY: ...`).
- All public items have doc comments.
- No heap allocation on the clock hot path.
- `clippy`/`rustfmt` compliance cannot be verified without running the toolchain, but the code style is consistent with the rest of the file.

---

### Summary

- **Critical findings:** 0
- **Warning findings:** 0
- **Info findings:** 3 (all non-actionable observations)
- **Verdict:** APPROVE

The fix is correct, the tests cover the acceptance criteria, and the code meets project standards. The only observation worth noting is the misleading test name `add_nanos_signed_clamps_to_zero` and the absence of a test for the actual epoch-clamp path, but neither affects correctness or safety.
