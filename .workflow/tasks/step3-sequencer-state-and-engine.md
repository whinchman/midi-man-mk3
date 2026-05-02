# Task: Sequencer State and Engine

- **Type**: coder
- **Status**: done
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
    pub active_overlay: Option<OverlayMode>, // set by command processor; read by HID thread
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
    // duration_nanos: step duration in nanoseconds; midi_out.rs fires NoteOff after this delay
    NoteOn { channel: u8, note: u8, velocity: u8, duration_nanos: u64 },
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

Note-off events are owned by `midi_out.rs` (Step 5). `tick()` returns a `NoteOn` carrying `duration_nanos` (one full step period, computed by the clock thread). `midi_out.rs` spawns a short-lived thread per note that sleeps `duration_nanos` then sends the NoteOff. This design supports future per-note duration without changes to the sequencer or clock. `OverlayMode` comes from `input.rs` (Step 6b) — define a local stub enum in `state.rs` until Step 6b is merged, then replace with the import.

## Notes

### Implementation Summary (2026-05-02)

**Branch**: `sequencer-state-and-engine` (based off `feature/engine-phase1`)

**Worktree**: `.workflow/worktrees/sequencer-state-and-engine`

**What was implemented:**

- `engine/src/state.rs` — `SequencerState`, `StepData`, `StepSize`, `PendingEdit`, `OverlayMode` (local stub, `Regular`/`Shift` variants), and `MidiEvent` (with `duration_nanos: u64` on `NoteOn`). Implements `Clone`, `Default`, `tick()`, `toggle_step()`, and `apply_encoder_delta()`. No heap allocation on any hot path (`[StepData; 16]` stack array).
- `engine/src/sequencer.rs` — thin re-export shim; placeholder for Step 9 wiring.
- `engine/src/lib.rs` — module declarations for `music_theory`, `state`, `sequencer`.
- `engine/src/music_theory.rs` — ported from `feat/music-theory-tables` (dependency not yet merged into `feature/engine-phase1`).
- `Cargo.toml` / `engine/Cargo.toml` — minimal workspace scaffold (no external deps needed for this crate alone) so `cargo test -p engine` works standalone.

**Key decisions applied:**
- `OverlayMode` defined as a local stub enum (`Regular`, `Shift`) in `state.rs`; Step 6b will replace with `use crate::input::OverlayMode`.
- `MidiEvent::NoteOn` includes `duration_nanos: u64`; clock thread sets this; `midi_out.rs` (Step 5) schedules `NoteOff`.
- `tick()` returns `NoteOn` with `duration_nanos: 0` as placeholder; the clock thread overwrites before forwarding.

**Test results (`cargo test -p engine`):** 15 passed, 0 failed, clippy clean.

---

### QA Augmentation (2026-05-02)

**Agent**: qa subagent

**9 new tests added** (`engine/src/state.rs`) covering gaps identified in the post-implementation review:

| New test | Scenario covered |
|---|---|
| `tick_all_16_steps_enabled_visits_every_step` | Playhead visits all steps 0–15 exactly once and wraps to 0 |
| `tick_loop_full_range_loop_in0_loop_out15` | Full-range loop behaves like non-loop mode |
| `tick_loop_single_step_loop_in7_loop_out7` | Single-step loop stays pinned at step 7 |
| `tick_loop_inverted_loop_in3_loop_out2` | Inverted loop (loop_in > loop_out) documents current wrap behavior |
| `apply_encoder_delta_zero_is_noop` | delta=0 leaves note unchanged |
| `apply_encoder_delta_large_positive_wraps_octave` | delta=+7 in C Major wraps one octave to C5=72 |
| `apply_encoder_delta_large_negative_clamps_at_zero` | Large negative delta clamps at MIDI 0, no underflow |
| `toggle_step_double_toggle_returns_to_original` | Two toggles restore original enabled state |
| `default_state_all_fields_match_spec` | Every field of SequencerState::default() checked against spec |

**Final test results:** 24 passed, 0 failed.

**Note on inverted loop**: `loop_in=3, loop_out=2` causes the playhead to immediately wrap on every tick (next=4 > loop_out=2), pinning it at loop_in=3. This is undocumented behavior that should be guarded or documented in Step 9 when the command processor is wired up.

---

### Code Review (2026-05-02)

**Reviewer**: code-reviewer agent
**Files reviewed**: `engine/src/state.rs`, `engine/src/sequencer.rs`, `engine/src/music_theory.rs`, `engine/src/lib.rs`, `engine/Cargo.toml`, `Cargo.toml`

#### Checklist Verification

- [x] `SequencerState` has all required fields including `active_overlay: Option<OverlayMode>`
- [x] `MidiEvent::NoteOn` includes `duration_nanos: u64`
- [x] `tick()` correctly advances playhead and handles loop wrap at `loop_out+1` back to `loop_in`
- [x] `tick()` returns `None` for disabled steps
- [x] `apply_encoder_delta` calls `music_theory::next_note` correctly
- [x] No heap allocation in any hot-path method (`[StepData; 16]` stack array, all methods use `&mut self` with no Vec/Box/String)
- [x] `OverlayMode` defined as a local stub (`Regular`, `Shift`) with comment that Step 6b replaces it
- [x] `Default` impl is correct (120 BPM, C Major, Sixteenth, all steps disabled, not playing, paused=false)

#### Findings

##### [INFO] engine/src/state.rs:158-188 — tick() loop-entry behavior when playhead is outside loop boundaries

When `loop_active=true` and the playhead starts outside `[loop_in, loop_out]`, `tick()` will advance through steps between playhead and `loop_out` before entering the loop. For example, with `loop_in=5, loop_out=10, playhead=0`, the sequencer plays steps 1–10 before the loop kicks in at step 5. This is not explicitly specified in the task and may be intentional, but it could surprise callers who activate loop mode mid-sequence. No fix required for this step — document when wiring the command processor in Step 9.

##### [INFO] engine/src/state.rs:164 — `next` computed as u8 addition without overflow concern

`let next = self.playhead + 1;` — since `playhead` is bounded 0–15 and `tick()` returns early if not playing, `next` reaches at most 16, well within `u8` range. No issue.

##### [INFO] engine/Cargo.toml — no `[lib]` section but `src/lib.rs` exists

Cargo's convention-based autodiscovery finds `src/lib.rs` and builds it as the library crate root alongside the `[[bin]]` target in `main.rs`. This is valid and intentional. No issue.

##### [INFO] engine/src/state.rs — `tick_note_on_has_correct_fields` test is slightly convoluted

The test calls `s.tick()` once (advancing to step 1), then manually resets `s.playhead = 0` before calling `tick()` again to get the event under test. The logic is correct but could be simplified to start with `playhead=0` and tick once directly. Minor style note only.

#### Summary

**Total findings:** 0 critical, 0 warning, 4 info

All acceptance criteria are met. Logic is correct, heap-free, well-tested (15 tests, all edge cases covered), and the `OverlayMode` stub pattern is properly implemented. The loop-entry behavior when playhead is outside the loop region is the only potentially surprising design point, but it is not specified and does not constitute a bug at this stage.

**Verdict: APPROVE**

---

## PR Feedback

PR: https://github.com/whinchman/midi-man-mk3/pull/4

### Comments Requiring Action

_(none)_

### CI Failures

_(none — no CI checks configured on this repository)_

### Questions / Acknowledged

_(none)_
