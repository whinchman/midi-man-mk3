# Task: Music Theory Tables

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 1
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/music-theory-tables
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 2
- **Dependencies**: none

## Description

Implement `engine/src/music_theory.rs`. This module provides all music theory lookups needed by the sequencer: scale intervals by mode, MIDI note numbers for a given key+mode, note name strings, and step navigation within a scale. All data lives in `const` arrays — no heap allocation is permitted.

## Acceptance Criteria

- [ ] `Key` enum defined with 12 variants: `C, Cs, D, Ds, E, F, Fs, G, Gs, A, As, B`.
- [ ] `Mode` enum defined with 7 variants: `Major, NaturalMinor, Dorian, Phrygian, Lydian, Mixolydian, Locrian`.
- [ ] `const SCALE_INTERVALS: [[u8; 7]; 7]` defined — semitone intervals for each of the 7 modes. Example: `Major = [2, 2, 1, 2, 2, 2, 1]`.
- [ ] `fn notes_in_key(key: Key, mode: Mode) -> [u8; 7]` returns the 7 MIDI note numbers for one octave starting at the key root. Root note for Key::C is MIDI 60 (C4).
- [ ] `fn note_name(midi_note: u8) -> &'static str` returns strings like `"C4"`, `"F#3"`, `"A#5"`. Sharps are used (not flats) for all accidentals.
- [ ] `fn next_note(current: u8, key: Key, mode: Mode, direction: i8) -> u8` advances or retreats within the 7-note scale, wrapping across octaves. Return value is clamped to MIDI 0–127.
- [ ] Unit tests cover: all 7 modes produce correct interval sums (should sum to 12), `notes_in_key` for C Major matches `[60,62,64,65,67,69,71]`, `note_name` for boundary values (0, 60, 127), `next_note` wraps from scale degree 7 back to degree 1 in the next octave, `next_note` clamps at MIDI 127 and 0.
- [ ] No `Vec`, `Box`, `String`, or heap allocations anywhere in the module.
- [ ] `cargo test -p engine` passes.

## Interface Contracts

These types are consumed by `engine/src/state.rs` (Step 3) and `engine/src/input.rs` (Step 6b):

```rust
// engine/src/music_theory.rs

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Key { C, Cs, D, Ds, E, F, Fs, G, Gs, A, As, B }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode { Major, NaturalMinor, Dorian, Phrygian, Lydian, Mixolydian, Locrian }

pub const SCALE_INTERVALS: [[u8; 7]; 7];

pub fn notes_in_key(key: Key, mode: Mode) -> [u8; 7];
pub fn note_name(midi_note: u8) -> &'static str;
pub fn next_note(current: u8, key: Key, mode: Mode, direction: i8) -> u8;
```

## Context

From the plan (Section 3, Engine stack): music theory is implemented as inline lookup tables, `const` arrays, no heap. C4 = MIDI note 60.

Mode interval patterns (semitones between successive scale degrees):
- Major:        [2, 2, 1, 2, 2, 2, 1]
- NaturalMinor: [2, 1, 2, 2, 1, 2, 2]
- Dorian:       [2, 1, 2, 2, 2, 1, 2]
- Phrygian:     [1, 2, 2, 2, 1, 2, 2]
- Lydian:        [2, 2, 2, 1, 2, 2, 1]
- Mixolydian:   [2, 2, 1, 2, 2, 1, 2]
- Locrian:      [1, 2, 2, 1, 2, 2, 2]

Note naming: use sharp notation for black keys (C#, D#, F#, G#, A#). Octave numbering follows MIDI standard (C4 = 60, so C0 = 12, C-1 = 0).

## Notes

