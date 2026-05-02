# Task: Clock Thread

- **Type**: coder
- **Status**: in-progress
- **Repo**: midi-man-mk3
- **Parallel Group**: 3
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/clock-thread
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 4
- **Dependencies**: step3-sequencer-state-and-engine

## Description

Implement `engine/src/clock.rs`. This is the real-time tick loop that drives the sequencer forward. It runs in a dedicated `std::thread`, uses `libc::clock_nanosleep` with `CLOCK_MONOTONIC` and absolute wake times to prevent drift, re-reads tempo and step size on every tick, and applies swing offsets on odd steps. It sends `MidiEvent` values on a `SyncSender` channel for the MIDI output thread to consume.

## Acceptance Criteria

- [ ] `pub fn run_clock(state: Arc<RwLock<SequencerState>>, midi_tx: SyncSender<MidiEvent>)` implemented in `engine/src/clock.rs`.
- [ ] Clock uses `libc::clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, ...)` for tick timing — absolute wake times, not relative sleeps, to prevent drift accumulation.
- [ ] Tick period computed from `state.tempo_bpm` and `state.step_size` on each iteration so tempo and step-size changes take effect on the next tick without restarting the thread.
- [ ] Tick period formula: `tick_nanos = 60_000_000_000 / (bpm * steps_per_beat)` where steps_per_beat is 1 for Quarter, 2 for Eighth, 4 for Sixteenth.
- [ ] Swing applied: even steps (0-indexed: 0, 2, 4…) fire at `next_abs`; odd steps (1, 3, 5…) fire at `next_abs + swing_offset_nanos`. Swing offset = `swing_factor * tick_nanos / 100` where `swing_factor` is `state.swing` (range -50 to +50).
- [ ] On each tick: compute `tick_nanos` for the current tempo/step size, acquire write lock, call `state.tick()`, release lock immediately. If `Some(MidiEvent::NoteOn)` returned, set `duration_nanos = tick_nanos` on the event before sending on `midi_tx`. Clock does NOT send NoteOff — that is owned by `midi_out.rs`.
- [ ] SCHED_FIFO priority 50 requested at thread start via `libc::sched_setscheduler`; if denied (non-root), logs a warning to stderr and continues.
- [ ] Thread exits cleanly when `midi_tx` channel is disconnected (sender dropped).
- [ ] Unit tests (no actual sleep): mock `SequencerState` advancing 32 ticks and verify playhead is at position 0 after 32 ticks (wraps at 16). Verify swing offset math for representative BPM/swing values.
- [ ] `cargo test -p engine` passes.

## Interface Contracts

```rust
// engine/src/clock.rs

use std::sync::{Arc, RwLock, mpsc::SyncSender};
use crate::state::{SequencerState, MidiEvent};

pub fn run_clock(state: Arc<RwLock<SequencerState>>, midi_tx: SyncSender<MidiEvent>);
```

Depends on `SequencerState` fields (from Step 3):
- `tempo_bpm: u16` — read each tick
- `step_size: StepSize` — read each tick
- `swing: i8` — read each tick
- `playing: bool` — read each tick (if not playing, sleep but don't advance)
- `fn tick(&mut self) -> Option<MidiEvent>` — called under write lock

`MidiEvent` enum (from Step 3):
```rust
pub enum MidiEvent {
    // duration_nanos passed through from clock; midi_out.rs fires NoteOff after this delay
    NoteOn { channel: u8, note: u8, velocity: u8, duration_nanos: u64 },
    NoteOff { channel: u8, note: u8 },
    Start, Stop, Continue,
}
```

## Context

From plan Section 8, Step 4 and Section 5 (MIDI Output / Timing):

At 120 BPM with 1/16 note steps: tick period = 60,000 ms / (120 × 16) ≈ 31.25 ms = 31,250,000 ns.

Swing formula from plan:
```
Even steps: play at tick_time
Odd steps:  play at tick_time + (swing_factor × tick_period / 100)
swing_factor range: -50 to +50
```

SCHED_FIFO: set priority 50 via `libc::sched_setscheduler`. Failure is non-fatal; log warning and continue. This is a best-effort real-time request for personal use on a Pi Zero 2W.

The clock thread holds the write lock only for the duration of calling `state.tick()` — release immediately after. Do not hold the lock during the sleep.

`libc` is already a declared dependency (added in Step 1).

## Notes

