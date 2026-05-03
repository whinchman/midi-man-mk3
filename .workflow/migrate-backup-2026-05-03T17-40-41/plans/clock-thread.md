# Plan: Clock Thread (step4)

## Overview

Implement `engine/src/clock.rs` — the real-time tick loop that drives the sequencer.
Uses `libc::clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)` for drift-free timing.
Sends `MidiEvent` values over a `SyncSender` for the MIDI output thread.

## Steps

### Step 1: Copy prerequisite files

- Copy `state.rs` and `sequencer.rs` from the sequencer worktree into the clock-thread worktree.
- Update `lib.rs` to declare all modules including `clock`.

### Step 2: Implement `clock.rs`

Files to create:
- `engine/src/clock.rs`

Key logic:
- `run_clock(state, midi_tx)` runs in its own thread.
- At start, try `libc::sched_setscheduler(0, SCHED_FIFO, priority=50)`; log warning on failure.
- Get initial absolute time via `clock_gettime(CLOCK_MONOTONIC)`.
- Loop:
  1. Read `state` (read lock): get `tempo_bpm`, `step_size`, `swing`, `playing`.
  2. Compute `tick_nanos = 60_000_000_000 / (bpm as u64 * steps_per_beat)`.
  3. Compute `swing_offset_nanos = swing as i64 * tick_nanos as i64 / 100`.
  4. Determine wake time: even step → `next_abs`, odd step → `next_abs + swing_offset`.
  5. Sleep via `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &wake_time)`.
  6. If not playing, advance `next_abs` by `tick_nanos` and continue.
  7. Acquire write lock, call `state.tick()`, release immediately.
  8. If `Some(NoteOn { .. })`, set `duration_nanos = tick_nanos`, send on `midi_tx`.
  9. If `midi_tx.send()` returns `Err`, break (channel disconnected).
  10. Increment step counter (for swing even/odd determination), advance `next_abs`.

### Step 3: Unit tests (no actual sleep)

In `clock.rs` `#[cfg(test)]` module:
- Test tick_nanos formula for 120 BPM / Sixteenth = 31_250_000 ns.
- Test tick_nanos formula for 120 BPM / Quarter = 500_000_000 ns.
- Test swing_offset math: swing=50, tick_nanos=1_000_000 → offset=500_000.
- Test swing_offset negative: swing=-25, tick_nanos=1_000_000 → offset=-250_000.
- Test playhead wraps at 32 ticks using SequencerState directly (no sleep).

### Step 4: Verify

- `cargo test -p engine` passes.
- `cargo build -p engine --release` succeeds.
