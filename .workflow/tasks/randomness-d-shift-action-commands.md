# Task: Shift Action Commands

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 3
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-d-shift-action-commands
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream D
- **Dependencies**: randomness-c-shift-param-routing

## Description

Add four new `InputCommand` variants and their `apply_command` arms to
`engine/src/state.rs` and `engine/src/input.rs`.

These are **action-style** commands (not continuous param dials): they are
triggered by button presses from the Shift overlay UI and keyboard shortcuts.

### New InputCommand variants (engine/src/input.rs)

```rust
/// Apply a semitone offset to all steps' notes.
/// 0 clears the modifier. Range: -96..=96 (actual semitones).
NoteModifierSet(i8),

/// Toggle per-step skip modifier on/off.
SkipModifierToggle,

/// Set velocity offset modifier (0 = off). Range: -127..=127.
VelocityModifierSet(i8),

/// Randomise all 16 step notes within the current key/mode.
GenerateRandomSequence,
```

### apply_command arms (engine/src/state.rs)

```rust
InputCommand::NoteModifierSet(s) => { self.note_modifier = s; }
InputCommand::SkipModifierToggle => { self.skip_modifier = !self.skip_modifier; }
InputCommand::VelocityModifierSet(v) => { self.velocity_modifier = v; }
InputCommand::GenerateRandomSequence => { self.generate_random_sequence(); }
```

### generate_random_sequence method

Add a private method on `SequencerState`:

```rust
/// Randomise all 16 steps' notes to in-key values within MIDI range 48–84.
///
/// Uses `next_rand(&mut self.rng_seed)` for each step. Enabled flags are
/// left unchanged — only `midi_note` is updated.
/// Generated note range: 48–84 (C3–C6, 3 octaves). The raw random value
/// is mapped to this range, then snapped to the current key/mode via
/// `music_theory::snap_to_key`.
fn generate_random_sequence(&mut self) {
    for step in self.steps.iter_mut() {
        let raw = next_rand(&mut self.rng_seed);
        let note_in_range = (raw % 37) as u8 + 48; // 48..=84
        step.midi_note =
            crate::music_theory::snap_to_key(note_in_range, self.key, self.mode);
    }
}
```

Key design decisions:
- MIDI range 48–84 (C3–C6, 37 semitones).
- Leave `enabled` flags alone.
- Snap each generated note to key/mode via `snap_to_key` so the sequence is
  always in-key.

## Acceptance Criteria

- [ ] `InputCommand` has `NoteModifierSet(i8)`, `SkipModifierToggle`, `VelocityModifierSet(i8)`, `GenerateRandomSequence` variants
- [ ] `apply_command` handles all four variants
- [ ] `NoteModifierSet(0)` clears the modifier (sets `note_modifier = 0`)
- [ ] `SkipModifierToggle` flips `skip_modifier` from false to true and back
- [ ] `VelocityModifierSet(v)` sets `velocity_modifier = v`
- [ ] `GenerateRandomSequence` updates all 16 `steps[i].midi_note`
- [ ] Every note produced by `GenerateRandomSequence` passes a `snap_to_key` identity check (i.e. is already in the current key/mode)
- [ ] Every note produced is in the range 48–84
- [ ] `enabled` flags are unchanged by `GenerateRandomSequence`
- [ ] `cargo test -p engine` passes with tests covering all acceptance criteria above
- [ ] `clippy` passes with no new warnings
- [ ] All new public items have a doc comment

## Interface Contracts

New `InputCommand` variants (consumed by Stream H for keyboard wiring):

```rust
// engine/src/input.rs
pub enum InputCommand {
    // … existing variants …
    NoteModifierSet(i8),
    SkipModifierToggle,
    VelocityModifierSet(i8),
    GenerateRandomSequence,
}
```

## Context

- File: `engine/src/input.rs` — `InputCommand` enum defined at line ~23.
- File: `engine/src/state.rs` — `apply_command` at line ~237; `next_rand` and
  `snap_all_steps_to_key` already exist as private helpers.
- `music_theory::snap_to_key` is already used in `snap_all_steps_to_key` —
  call it the same way.
- The `note_modifier`, `skip_modifier`, and `velocity_modifier` fields are added
  by Stream C (must be merged to base branch before this branch is cut).
- Code standard: no `unsafe`, no heap allocation, `clippy` clean.

## Notes

