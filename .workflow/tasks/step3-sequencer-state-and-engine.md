# Task: Sequencer State and Engine

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 2
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/sequencer-state-and-engine
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 3
- **Dependencies**: step2-music-theory-tables

## Description

Implement `engine/src/state.rs` and `engine/src/sequencer.rs`. Define the `SequencerState` struct (shared between the clock, HID, and UI threads), all related enums, and the core sequencer logic: playhead advance, loop handling, step toggle, encoder note delta, and tick-level MIDI event generation.

No heap allocation in any hot-path method. State is designed to be wrapped in `Arc<RwLock<SequencerState>>` by the caller (Step 9).

## Acceptance Criteria

- [ ] `SequencerState` struct defined in `engine/src/state.rs` with fields matching the plan exactly (see Interface Contracts below).
- [ ] `StepData` struct defined: `enabled: bool`, `midi_note: u8`.
- [ ] `StepSize` enum defined: `Quarter, Eighth, Sixteenth`.
- [ ] `PendingEdit` enum defined in `engine/src/state.rs`: `None`, `Note { step: usize, midi_note: u8 }`, `Velocity { step: usize, velocity: u8 }`, `Param { overlay: OverlayMode, index: u8, value: i64 }`. (`OverlayMode` imported from `input.rs` — define a stub or placeholder in `state.rs` if `input.rs` does not exist yet; Step 6b will wire it up.)
- [ ] `SequencerState` implements `Clone` and `Default`: default is all steps disabled, Key::C, Mode::Major, 120 BPM, swing 0, step size Sixteenth, loop inactive, playhead 0, not playing, not paused.
- [ ] `MidiEvent` enum defined (can live in `state.rs` or a new `midi_event.rs`): `NoteOn { channel: u8, note: u8, velocity: u8 }`, `NoteOff { channel: u8, note: u8 }`, `Start`, `Stop`, `Continue`.
- [ ] `SequencerState::apply_encoder_delta(step: usize, delta: i8)` implemented — calls `music_theory::next_note` to shift `steps[step].midi_note` by `delta`.
- [ ] `SequencerState::toggle_step(step: usize)` implemented — flips `steps[step].enabled`.
- [ ] `SequencerState::tick(&mut self) -> Option<MidiEvent>` implemented:
  - If not playing or paused, returns `None`.
  - Advances `playhead` by 1; if `loop_active`, wraps at `loop_out + 1` back to `loop_in`; otherwise wraps at 16.
  - If the new step is enabled, returns `Some(MidiEvent::NoteOn { channel: 0, note: steps[playhead].midi_note, velocity: 100 })`.
  - If the new step is disabled, returns `None`.
- [ ] Unit tests:
  - Ticking 16 times from a fresh state (all steps enabled, playing=true) cycles playhead 0→15 and back to 0.
  - With loop_in=3, loop_out=7, loop_active=true: playhead wraps at step 7 back to step 3.
  - Disabled steps return `None` from `tick`.
  - `toggle_step` toggles and `apply_encoder_delta` changes the note correctly.
- [ ] No `Vec`, `Box`, `String`, or heap allocations in hot-path methods.
- [ ] `cargo test -p engine` passes.

## Interface Contracts

```rust
// engine/src/state.rs

use crate::music_theory::{Key, Mode};

pub struct SequencerState {
    pub steps: [StepData; 16],
    pub key: Key,
    pub mode: Mode,
    pub tempo_bpm: u16,       // 20–300
    pub swing: i8,            // -50 to +50
    pub step_size: StepSize,  // Quarter, Eighth, Sixteenth
    pub loop_in: u8,          // 0–15
    pub loop_out: u8,         // 0–15
    pub loop_active: bool,
    pub playhead: u8,         // 0–15
    pub playing: bool,
    pub paused: bool,
    pub pending_edit: PendingEdit,
}

pub struct StepData {
    pub enabled: bool,
    pub midi_note: u8,
}

#[derive(Clone, Copy)]
pub enum StepSize { Quarter, Eighth, Sixteenth }

// PendingEdit — OverlayMode stub acceptable here; Step 6b finalizes
pub enum PendingEdit {
    None,
    Note { step: usize, midi_note: u8 },
    Velocity { step: usize, velocity: u8 },
    Param { index: u8, value: i64 },
}

// MidiEvent — may live in state.rs or a sibling module
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    Start,
    Stop,
    Continue,
}

impl SequencerState {
    pub fn apply_encoder_delta(&mut self, step: usize, delta: i8);
    pub fn toggle_step(&mut self, step: usize);
    pub fn tick(&mut self) -> Option<MidiEvent>;
}
```

Types imported from Step 2 (`engine/src/music_theory.rs`):
- `Key` — 12-variant enum
- `Mode` — 7-variant enum
- `next_note(current: u8, key: Key, mode: Mode, direction: i8) -> u8`

## Context

From plan Section 8, Step 3. The `SequencerState` is the single shared truth for all threads. It will be wrapped in `Arc<RwLock<SequencerState>>` by `main.rs` (Step 9). All methods that mutate state take `&mut self` — callers hold the write lock while calling them.

`sequencer.rs` may be a thin re-export or contain additional engine logic not in `state.rs`. At minimum the plan lists both files — keep `state.rs` for the struct/impl and `sequencer.rs` for any higher-level wiring that doesn't belong in the struct itself.

Note-off events are not generated by `tick()` at MVP — they are sent by the MIDI output thread at a fixed offset after note-on (one step duration). This simplification is acceptable for MVP. If note-off logic is added, it belongs in `clock.rs` (Step 4) or `midi_out.rs` (Step 5).

## Notes

