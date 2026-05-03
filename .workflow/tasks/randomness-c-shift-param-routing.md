# Task: Shift Overlay Param Routing Infrastructure

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 2
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-c-shift-param-routing
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream C
- **Dependencies**: randomness-a-rng-infra

## Description

Extend `engine/src/state.rs` to support the Shift overlay's 8 named parameters
via the same `ParamValueDelta` / `Confirm` flow that the Regular overlay already
uses. Also add all new `SequencerState` fields required by the Shift feature set.

This stream can start once Stream A's `rng_seed` field and helpers exist. It
does **not** need the unconditional seed-advance in `tick()` to be present yet.

### New enums (add to `state.rs` alongside `StepSize`)

```rust
/// When the tempo randomness roll fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TempoRollPoint {
    Off,
    Step,   // every step
    Beat,   // every beat (4 steps at 1/16 resolution)
    Seq,    // every sequence loop
}

impl TempoRollPoint {
    pub const COUNT: usize = 4;
    pub fn from_index(i: usize) -> Self { … }
    pub fn to_index(self) -> usize { … }
}

/// Shape of the tempo randomness curve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TempoRandType {
    Random,
    Up,
    Down,
    Breathe,
    PingPong,
}

impl TempoRandType {
    pub const COUNT: usize = 5;
    pub fn from_index(i: usize) -> Self { … }
    pub fn to_index(self) -> usize { … }
}
```

Both enums follow the `Key` / `Mode` / `StepSize` pattern already in the file.

### New fields on `SequencerState`

```rust
// --- Randomness ---
pub tempo_rand: u8,               // 0–100; Default: 0
pub tempo_roll_point: TempoRollPoint,  // Default: TempoRollPoint::Off
pub tempo_variance_max: u8,       // 1–99; Default: 10
pub tempo_rand_type: TempoRandType,    // Default: TempoRandType::Random
pub scale_quant: bool,            // Default: false

// --- Shift modifiers ---
/// Semitone offset applied to every NoteOn. 0 = off. Single i8 storing
/// actual semitones. ParamValueDelta steps ±1 while abs ≤ 12, then ±12
/// (one octave) beyond that. Max ±96 (8 octaves). Display: semitones ≤ 12,
/// octaves > 12.
pub note_modifier: i8,            // Default: 0
/// When true, every step is muted at play time.
pub skip_modifier: bool,          // Default: false
/// Velocity offset (-127..=127). 0 = off. Clamped to 0–127 at emit.
pub velocity_modifier: i8,        // Default: 0
```

### Overlay-aware dispatch

The existing `ParamValueDelta` arm uses `committed_param_value` /
`clamped_param_value` unconditionally (Regular overlay only). Extend it to
route to shift-specific methods when `active_overlay == Some(OverlayMode::Shift)`:

```rust
// In apply_command ParamValueDelta arm:
let current_value = match self.pending_edit {
    PendingEdit::Param { index: pi, value, .. } if pi == index => value,
    _ => match overlay {
        OverlayMode::Regular => self.committed_param_value(index),
        OverlayMode::Shift   => self.shift_committed_param_value(index),
    },
};
let new_value = match overlay {
    OverlayMode::Regular => self.clamped_param_value(index, current_value + d as i64),
    OverlayMode::Shift   => self.shift_clamped_param_value(index, current_value + d as i64),
};
```

Similarly, the `Confirm` arm must route `apply_param_value` vs.
`shift_apply_param_value` based on the overlay stored in `PendingEdit::Param`.

### Shift param index map

| Index | Field | Type | Range |
|-------|-------|------|-------|
| 0 | `note_rand` | u8 | 0–100 |
| 1 | `tempo_rand` | u8 | 0–100 |
| 2 | `tempo_roll_point` | enum | TempoRollPoint (4 variants) |
| 3 | `tempo_variance_max` | u8 | 1–99 |
| 4 | `tempo_rand_type` | enum | TempoRandType (5 variants) |
| 5 | `step_rand` | u8 | 0–100 |
| 6 | `scale_quant` | bool | 0 or 1 |
| 7 | (reserved) | — | — (returns 0, no-op on apply) |

Note: `note_rand` and `step_rand` are also added by Stream B. If Stream B is
merged first, those fields will already exist — do not add them again. If this
stream lands first, Stream B should skip those field additions when it merges.
Coordinate via the base branch.

### New private methods

```rust
fn shift_committed_param_value(&self, index: u8) -> i64;
fn shift_clamped_param_value(&self, index: u8, value: i64) -> i64;
fn shift_apply_param_value(&mut self, index: u8, value: i64);
```

`note_modifier` uses a stepped clamp: `±1` increments while `abs(value) ≤ 12`,
then `±12` increments beyond. `shift_clamped_param_value` for index 0 does NOT
handle this stepping — the stepping is a UI concern handled in the
`ParamValueDelta` delta application. The clamp in `shift_clamped_param_value`
simply enforces the max range: `value.clamp(-96, 96)`.

## Acceptance Criteria

- [ ] `TempoRollPoint` and `TempoRandType` enums exist with `COUNT`, `from_index`, `to_index`
- [ ] All new `SequencerState` fields exist with correct `Default` values
- [ ] `shift_committed_param_value`, `shift_clamped_param_value`, `shift_apply_param_value` implemented for all 8 shift indices
- [ ] `ParamValueDelta` arm routes correctly based on `active_overlay`
- [ ] `Confirm` arm routes correctly based on `PendingEdit::Param { overlay, .. }`
- [ ] Each shift param round-trips: set via `ParamValueDelta` + `Confirm`, read back from state matches expected value
- [ ] Index 7 (reserved) is a safe no-op (returns 0, apply is empty)
- [ ] `SequencerState` remains `Clone`
- [ ] `cargo test -p engine` passes with round-trip tests for each of the 7 active shift indices
- [ ] `clippy` passes with no new warnings
- [ ] All new public items have a doc comment

## Interface Contracts

Fields produced by this task, consumed by streams D, E, F, G:

```rust
// engine/src/state.rs — SequencerState
pub tempo_rand: u8,
pub tempo_roll_point: TempoRollPoint,
pub tempo_variance_max: u8,
pub tempo_rand_type: TempoRandType,
pub scale_quant: bool,
pub note_modifier: i8,
pub skip_modifier: bool,
pub velocity_modifier: i8,
// (note_rand and step_rand may already exist from Stream B)
```

Enums produced, consumed by Stream F (clock.rs):

```rust
pub enum TempoRollPoint { Off, Step, Beat, Seq }
pub enum TempoRandType  { Random, Up, Down, Breathe, PingPong }
```

Methods produced, consumed by Stream G (ui_render.rs):

```rust
// (private, but Stream G needs the fields to render values)
fn shift_committed_param_value(&self, index: u8) -> i64;
```

## Context

- File: `engine/src/state.rs`
- `SequencerState` struct defined at line ~112; `Default` at ~149.
- `committed_param_value` at line ~350; `clamped_param_value` at ~364;
  `apply_param_value` at ~382.
- `ParamValueDelta` arm at line ~318; `Confirm` arm at ~265.
- `active_overlay` field already exists on `SequencerState`.
- `PendingEdit::Param` already carries an `overlay: OverlayMode` field — use it
  in the `Confirm` arm to decide which apply method to call.
- Code standard: no `unsafe`, no heap allocation, `clippy` clean.

## Notes

