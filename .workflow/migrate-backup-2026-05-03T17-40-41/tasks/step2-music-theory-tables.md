# Task: Music Theory Tables

- **Type**: coder
- **Status**: done (qa)
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

Implementation complete on branch `feat/music-theory-tables` (worktree at `.workflow/worktrees/music-theory-tables`).

Files created:
- `engine/src/music_theory.rs` — full implementation
- `Cargo.toml` (workspace stub), `engine/Cargo.toml`, `engine/src/main.rs` — minimal scaffold to compile independently before step1 lands

All 13 unit tests pass (`cargo test -p engine`):
- All 7 mode interval rows sum to 12
- C Major notes_in_key returns [60,62,64,65,67,69,71]
- note_name boundary values (0 = "C-1", 60 = "C4", 127 = "G9")
- next_note wraps up from B4 (71) to C5 (72)
- next_note wraps down from C4 (60) to B3 (59)
- next_note clamps at MIDI 0 and 127
- D Dorian and A NaturalMinor scale verification
- Heap-free type-level sanity check

No heap allocations — all return types are `[u8; 7]`, `u8`, or `&'static str` backed by a 128-entry static array.

---

## Code Review — 2026-05-02

**Reviewer:** code-reviewer agent
**Branch reviewed:** `feat/music-theory-tables`
**Files reviewed:** `engine/src/music_theory.rs`, `engine/src/main.rs`, `engine/Cargo.toml`, `Cargo.toml`
**Verdict:** APPROVE

### Summary

0 critical, 0 warning, 2 info findings. All acceptance criteria met. `cargo test -p engine` passes (13/13). `cargo clippy` clean.

### Findings

#### [INFO] engine/src/music_theory.rs:144 — next_note tie-breaking behavior undocumented

When `current` is a non-scale note equidistant between two scale degrees (e.g., C# in C Major is distance 1 from both C and D), the code silently picks the lower-index degree. This is a consistent and deterministic rule, but callers using `next_note` with off-scale MIDI inputs (e.g., receiving an arbitrary MIDI note and trying to "snap to scale") may be surprised that `next_note(61, Key::C, Mode::Major, -1)` returns B3 (59) rather than C4 (60). The behavior is correct given the documented "closest scale degree" rule, but the tie-breaking direction is not stated.

Suggested fix: add a sentence to the doc comment: "For notes equidistant between two scale degrees, the lower degree is used."

#### [INFO] engine/src/music_theory.rs:91-99 — `saturating_add` in `notes_in_key` is defensive but unnecessary

`notes_in_key` uses `saturating_add` to guard against u8 overflow when building scale notes. In practice, the highest root (Key::B = MIDI 71) plus the maximum cumulative interval within one octave (11 semitones) gives 82, well within u8. The saturation can never trigger. The code is still correct; it is slightly misleading because it implies overflow is possible.

No fix required. Alternatively, replace with a plain `+` and a comment explaining the range is safe. Low priority.

---

## QA Review — 2026-05-02

**QA Agent:** qa subagent
**Branch:** `feat/music-theory-tables`
**Tests before:** 13 | **Tests after:** 37 | **Pass rate:** 37/37 (100%)

### Coverage Added (24 new tests)

**note_name (6 new):**
- All 12 pitch classes in octave -1 (MIDI 0–11)
- All 12 pitch classes in octave 4 (MIDI 60–71, reference octave)
- All 8 partial pitch classes in octave 9 (MIDI 120–127)
- Spot checks in octave 3 (F#3, A3, B3) and octave 5 (A#5, B5)

**notes_in_key — all 7 modes, 3+ keys each (11 new):**
- Phrygian (E), Lydian (F), Mixolydian (G), Locrian (B) — 4 previously untested modes
- Cross-mode spots: C Dorian, G Phrygian, D Mixolydian, E Lydian, A Locrian
- Additional keys in Major: G Major, F# Major

**next_note edge cases (7 new):**
- direction=-1 from root of non-C key (G Major, G4→F#4)
- direction=+1 near top of MIDI range (F#9=126 → G9=127)
- Octave boundary wrap up: D Major 7th degree (C#5=73) → D5=74
- Octave boundary wrap down: A NaturalMinor root (A4=69) → G3=67
- Off-key snaps direction=+1 in C Major: C#4(61)→D4(62)
- Off-key snaps direction=-1 in C Major: C#4(61)→B3(59) (tie-breaking to lower degree)
- Off-key equidistant in C Major: D#4(63)→E4(64) on direction=+1

### Correctness Verification

- All 7 mode interval tables match the spec exactly and verified against canonical music theory sources.
- `KEY_ROOT` maps correctly (C4=60 through B4=71), matching the MIDI 4.2 standard (C4=60).
- `NOTE_NAMES` 128-entry static array: count verified (12 + 9×12 + 8 = 128). All sharp accidentals use ASCII `#`.  All MIDI-to-name mappings cross-checked spot: MIDI 0="C-1", 54="F#3", 82="A#5", 127="G9".
- `notes_in_key`: loop logic correct; builds cumulative intervals from root.
- `next_note`: floor-division logic for negative octave offsets verified in Python with Rust truncation semantics. `target_degree_in_oct` is mathematically bounded to [0,6] and cannot index out of bounds. Clamping to [0,127] correct.
- No `Vec`, `Box`, `String`, or heap allocations anywhere in the module.
- All public items carry doc comments as required by code standards.
- No `unwrap()` in non-test code.
