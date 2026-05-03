# Key/Mode Note Shifting — Architecture Plan

**Feature:** When the user confirms a Key or Mode change via the Regular
Overlay, all 16 step notes are shifted to the nearest note within the new
key/mode. Ties (equidistant candidates) always resolve to the lower note.

---

## 1. Architecture Overview

### Data flow (existing, unchanged)

```
User presses Up/Down arrow while Regular Overlay is open
  → ParamValueDelta(d) InputCommand
  → apply_command sets PendingEdit::Param { overlay: Regular, index: 0|1, value: new_idx }

User presses Enter
  → Confirm InputCommand
  → apply_command arm PendingEdit::Param { .. }
  → calls self.apply_param_value(index, value)
  → index 0 → self.key = Key::from_index(value)
  → index 1 → self.mode = Mode::from_index(value)
  → pending_edit = None
```

The **single commit point** is `apply_param_value` inside `state.rs`. That is
where the new key/mode value is written. Inserting the snap pass immediately
after that write keeps all state mutation in one function and avoids scattered
mutation across multiple call sites.

### Key decisions

| Decision | Choice | Rationale |
|---|---|---|
| Where to snap | Inside `apply_param_value`, after writing key/mode (index 0 or 1) | Single commit point; no other caller writes key/mode. |
| Helper location | New pub fn `snap_to_key` in `music_theory.rs` | Pure function, no state dependency, easily unit-tested. |
| Loop over which steps | All 16, regardless of `enabled` | Disabled steps still have notes; re-enabling after a key change should give in-key notes. |
| Tie-break rule | Round down (take the lower candidate) | Matches spec exactly. |
| No-op guard | Check whether new key AND mode equal the old before calling snap | Avoids unnecessary mutation when confirming unchanged params. |
| MIDI range clamping | `snap_to_key` returns a value already clamped to 0–127 | No out-of-range notes can be produced. |

---

## 2. Answers to the Five Key Questions

### Q1 — Where does a Key/Mode param change get committed?

`SequencerState::apply_param_value` in `engine/src/state.rs` (line 382).

Call chain:
1. `apply_command(Confirm)` matches `PendingEdit::Param { index, value, .. }` (line 280–284).
2. Calls `self.apply_param_value(index, value)` (line 282).
3. Arms 0 and 1 write `self.key` and `self.mode` respectively (lines 384–385).

No other code path writes `self.key` or `self.mode`.

### Q2 — Step note representation

`SequencerState.steps: [StepData; 16]` — a fixed-size stack array.

```rust
pub struct StepData {
    pub enabled: bool,
    pub midi_note: u8,   // raw MIDI note number, 0–127
    pub velocity: u8,
}
```

Notes are stored as **absolute MIDI note numbers**, not scale degrees. Snapping
is therefore a MIDI-number → nearest-in-key-MIDI-number mapping.

### Q3 — Existing music-theory primitives

`engine/src/music_theory.rs` provides:

- `SCALE_INTERVALS: [[u8; 7]; 9]` — semitone steps for all 9 modes.
- `KEY_ROOT: [u8; 12]` — MIDI root note for each key (octave 4 anchor).
- `notes_in_key(key, mode) -> [u8; 7]` — returns the 7 notes of one octave of the scale.
- `next_note(current, key, mode, direction)` — scale-degree stepping. Contains
  the octave-folding arithmetic we can reuse to derive all in-key pitches
  across the full MIDI range.

`notes_in_key` only returns 7 notes in octave 4. The new `snap_to_key` helper
must extend this across all octaves (0–127).

### Q4 — Where should the snap logic live?

**In `music_theory.rs` as a new public function**, and called from
`SequencerState::apply_param_value` immediately after writing the new
key/mode.

Rationale:
- Pure function (no mutable state, no allocations — uses a fixed stack array).
- Keeps state.rs free of music-theory arithmetic.
- Matches the existing convention: `apply_encoder_delta` and `NoteDelta`
  already call into `music_theory::next_note`; this is the same pattern.

### Q5 — Edge cases

| Case | Handling |
|---|---|
| Disabled steps | Snap runs on all 16 steps. Disabled steps have notes too; they should be in-key for when they are re-enabled. |
| Note already in key | `snap_to_key` returns the note unchanged (distance 0 wins). |
| Same key + mode (no-op) | Guard in `apply_param_value`: compare new value against current before snapping. |
| Tie (equidistant) | `snap_to_key` prefers the lower candidate — use `<=` not `<` when updating best distance (`if dist < best_dist` becomes `if dist <= best_dist`, combined with iterating candidates from low to high so the first tied candidate is the lower one). |
| MIDI boundary (note 0 or 127) | `snap_to_key` clamps result to 0–127. |
| Out-of-range MIDI note in step | MIDI note is always u8 (0–255 type, but clamped to 0–127 everywhere); `snap_to_key` handles any value gracefully by clamping output. |

---

## 3. Implementation Plan

### Step 1 — Add `snap_to_key` to `music_theory.rs`

**File:** `engine/src/music_theory.rs`

Add a new public function after `next_note`:

```rust
/// Snap `midi_note` to the nearest note in the scale defined by `key` and `mode`.
///
/// When two candidates are equidistant (tie) the lower note is returned.
/// The result is always in 0–127.
pub fn snap_to_key(midi_note: u8, key: Key, mode: Mode) -> u8 {
    let root_offset = key_index(key) as i32;
    let intervals = SCALE_INTERVALS[mode_index(mode)];

    // Build cumulative semitone offsets within one octave: [0, i0, i0+i1, ...]
    let mut cum: [i32; 7] = [0; 7];
    for i in 1..7 {
        cum[i] = cum[i - 1] + intervals[i - 1] as i32;
    }

    let note_i32 = midi_note as i32;
    let mut best_note: i32 = 0;
    let mut best_dist: i32 = i32::MAX;

    // Iterate every octave that can produce a candidate in 0..=127.
    // MIDI root for key is root_offset (C=0, C#=1, …, B=11) but we need
    // absolute MIDI. The anchor is KEY_ROOT[key_index], which is in octave 4.
    // To cover all octaves, subtract enough octaves to start below 0.
    let anchor = KEY_ROOT[key_index(key)] as i32; // note in octave 4
    // Find the lowest octave whose root might still be <= 127
    // anchor + oct*12 >= 0  →  oct >= -anchor/12
    let oct_min = -((anchor + 11) / 12); // conservative lower bound
    let oct_max = (127 - anchor) / 12 + 1; // conservative upper bound

    for oct in oct_min..=oct_max {
        for &c in cum.iter() {
            let candidate = anchor + oct * 12 + c;
            if candidate < 0 || candidate > 127 {
                continue;
            }
            let dist = (note_i32 - candidate).abs();
            // Strict < so ties keep the first (lower) candidate encountered.
            // Candidates are iterated low-to-high within each octave,
            // and octaves are iterated in ascending order.
            if dist < best_dist {
                best_dist = dist;
                best_note = candidate;
            }
        }
    }

    best_note.clamp(0, 127) as u8
}
```

**Tie-break detail:** Iterating candidates in ascending order (`oct_min` to
`oct_max`, and `cum` is already ascending within an octave) and using `<`
(not `<=`) on `best_dist` means the **first** candidate that achieves a
given distance wins. The first encountered for any given distance is always
the lower note.

### Step 2 — Call `snap_to_key` from `apply_param_value` in `state.rs`

**File:** `engine/src/state.rs`

Modify `apply_param_value`:

```rust
fn apply_param_value(&mut self, index: u8, value: i64) {
    match index {
        0 => {
            let new_key = Key::from_index(value as usize);
            if new_key != self.key {
                self.key = new_key;
                self.snap_all_steps_to_key();
            }
        }
        1 => {
            let new_mode = Mode::from_index(value as usize);
            if new_mode != self.mode {
                self.mode = new_mode;
                self.snap_all_steps_to_key();
            }
        }
        // arms 2–7 unchanged
        2 => self.swing = value as i8,
        3 => self.step_size = StepSize::from_index(value as usize),
        4 => self.loop_in = value as u8,
        5 => self.loop_out = value as u8,
        6 => self.paused = value != 0,
        7 => {
            self.playing = value != 0;
            if self.playing {
                self.paused = false;
            }
        }
        _ => {}
    }
}
```

Add the private helper method on `SequencerState`:

```rust
/// Re-snap all 16 step notes to the nearest note in the current key and mode.
///
/// Called immediately after `self.key` or `self.mode` is updated.
/// No-heap: operates on the fixed-size `steps` array in place.
fn snap_all_steps_to_key(&mut self) {
    for step in self.steps.iter_mut() {
        step.midi_note =
            crate::music_theory::snap_to_key(step.midi_note, self.key, self.mode);
    }
}
```

No changes to `apply_command` or any other call site are needed.

### Step 3 — Add unit tests in `music_theory.rs`

Add a `#[cfg(test)]` block at the bottom of `music_theory.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // snap_to_key: note already in key is returned unchanged
    #[test]
    fn snap_in_key_note_unchanged() {
        // C major scale: C4=60, D4=62, E4=64, F4=65, G4=67, A4=69, B4=71
        assert_eq!(snap_to_key(60, Key::C, Mode::Major), 60); // C4
        assert_eq!(snap_to_key(62, Key::C, Mode::Major), 62); // D4
        assert_eq!(snap_to_key(71, Key::C, Mode::Major), 71); // B4
    }

    // snap_to_key: chromatic note snaps down to nearest scale note
    #[test]
    fn snap_out_of_key_rounds_to_nearest() {
        // C# (61) is equidistant from C (60) and D (62) → round down to C (60)
        assert_eq!(snap_to_key(61, Key::C, Mode::Major), 60);
        // Bb (70) in C major: nearest are A(69) dist=1, B(71) dist=1 → round down to A(69)
        assert_eq!(snap_to_key(70, Key::C, Mode::Major), 69);
    }

    // snap_to_key: tie-break always picks lower note
    #[test]
    fn snap_tie_picks_lower_note() {
        // F# (66) in C major: F=65 (dist 1), G=67 (dist 1) → lower wins → F (65)
        assert_eq!(snap_to_key(66, Key::C, Mode::Major), 65);
    }

    // snap_to_key: works across octave boundaries
    #[test]
    fn snap_across_octaves() {
        // C5 = 72 is in C major
        assert_eq!(snap_to_key(72, Key::C, Mode::Major), 72);
        // C#5 = 73: C5(72) dist=1, D5(74) dist=1 → round down to C5(72)
        assert_eq!(snap_to_key(73, Key::C, Mode::Major), 72);
    }

    // snap_to_key: MIDI boundary notes don't panic
    #[test]
    fn snap_midi_boundaries() {
        let _ = snap_to_key(0, Key::C, Mode::Major);
        let _ = snap_to_key(127, Key::C, Mode::Major);
    }

    // snap_to_key: works for non-C keys
    #[test]
    fn snap_non_c_key() {
        // G major scale degrees: G=67, A=69, B=71, C=72, D=74, E=76, F#=78
        // Ab/G# (68) in G major: G=67 (dist 1), A=69 (dist 1) → lower wins → G(67)
        assert_eq!(snap_to_key(68, Key::G, Mode::Major), 67);
    }

    // snap_to_key: works for non-Major modes
    #[test]
    fn snap_natural_minor() {
        // A natural minor: A=69, B=71, C=72, D=74, E=76, F=77, G=79
        // Bb (70) in A natural minor: A=69 (dist 1), B=71 (dist 1) → lower → A(69)
        assert_eq!(snap_to_key(70, Key::A, Mode::NaturalMinor), 69);
    }
}
```

### Step 4 — Add integration tests in `state.rs`

Add to the `#[cfg(test)]` block in `state.rs`:

```rust
// ── Key/Mode Note Shifting ───────────────────────────────────────────────

#[test]
fn test_key_change_snaps_all_steps() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    // Set step 0 to C#4 (61) — not in C major but will be in C# major.
    state.steps[0].midi_note = 61; // C#4
    state.steps[1].midi_note = 62; // D4 — already in both C and C# major
    // Confirm Key change to C# (index 1)
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 1, // Key::Cs
    };
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.key, Key::Cs);
    // C#4 (61) is the root of C# major → stays 61
    assert_eq!(state.steps[0].midi_note, 61);
}

#[test]
fn test_mode_change_snaps_all_steps() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    // B4 (71) is in C major. In C NaturalMinor it is not (scale: C D Eb F G Ab Bb).
    // Bb (70) is closest → round down tie: Bb=70 vs B=71... wait, Bb=70 is in NaturalMinor.
    // Actually B4=71: nearest in C NaturalMinor are Bb4(70) dist=1. B is not in the scale.
    state.steps[0].midi_note = 71; // B4
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 1,
        value: 1, // Mode::NaturalMinor
    };
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.mode, Mode::NaturalMinor);
    // B4(71) nearest in-key note is Bb4(70) (dist=1), C5(72) is dist=1 → round down → Bb4=70
    assert_eq!(state.steps[0].midi_note, 70);
}

#[test]
fn test_same_key_no_snap() {
    let mut state = SequencerState::default(); // Key::C
    state.steps[0].midi_note = 61; // C#4 — out of key (just set directly)
    // Confirm Key=C again (no change)
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 0, // Key::C — same as current
    };
    state.apply_command(InputCommand::Confirm);
    // Step note should NOT have been snapped (no-op guard fired)
    assert_eq!(state.steps[0].midi_note, 61);
}

#[test]
fn test_snap_all_16_steps() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    // Set all steps to C#4 (61)
    for step in state.steps.iter_mut() {
        step.midi_note = 61;
    }
    // Change to D major
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 2, // Key::D
    };
    state.apply_command(InputCommand::Confirm);
    // All 16 steps should now be in D major
    for step in &state.steps {
        let snapped = crate::music_theory::snap_to_key(step.midi_note, Key::D, Mode::Major);
        assert_eq!(step.midi_note, snapped);
    }
}

#[test]
fn test_disabled_steps_are_snapped() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    state.steps[3].enabled = false;
    state.steps[3].midi_note = 61; // C#4 — not in C major
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 1,
        value: 0, // Mode::Major (same mode, different key)
    };
    // Change to a different key instead
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 2, // Key::D
    };
    state.apply_command(InputCommand::Confirm);
    // Disabled step should still be snapped
    let note = state.steps[3].midi_note;
    assert_eq!(
        note,
        crate::music_theory::snap_to_key(61, Key::D, Mode::Major)
    );
}
```

---

## 4. File-by-File Change Summary

| File | Change |
|---|---|
| `engine/src/music_theory.rs` | Add `pub fn snap_to_key(midi_note: u8, key: Key, mode: Mode) -> u8` + unit tests. |
| `engine/src/state.rs` | In `apply_param_value`: add change-detection guard for index 0 and 1, call `snap_all_steps_to_key()`. Add private `fn snap_all_steps_to_key(&mut self)`. Add integration tests. |

No changes to `sequencer.rs`, `input.rs`, `ui_render.rs`, `ui.rs`, `clock.rs`,
`midi_out.rs`, or `hid.rs`.

---

## 5. Trade-offs and Alternatives

### Alternative A — Snap inside `apply_command` at the `Confirm` arm

Pros: Visible at the top-level command dispatcher.
Cons: `apply_command` would need to inspect the overlay type and param index
to decide whether to snap; this duplicates knowledge that `apply_param_value`
already encapsulates. Any future non-Confirm path that writes key/mode
(e.g. a HID shortcut) would miss the snap.

**Rejected.** The inside-`apply_param_value` approach is more robust to future
code paths.

### Alternative B — Generate a full 128-note lookup table in `snap_to_key`

Pros: O(1) lookup after table build.
Cons: 128 bytes of stack allocation on every call; overkill for 16 steps.
The proposed linear scan over ~10 octaves × 7 notes = ~70 iterations is
fast enough and uses no heap.

**Rejected.** Stack scan is sufficient and matches the project's no-heap rule.

### Alternative C — Store notes as scale degrees, convert to MIDI at play time

Would make snapping trivially free (change key/mode, MIDI output shifts).
Cons: Requires changing the `StepData` struct and all MIDI output code; large
blast radius. Out of scope for this feature.

**Rejected.** Too large a refactor for this targeted change.

---

## 6. Acceptance Criteria

- [ ] Confirming a Key change via the Regular Overlay snaps all 16 step `midi_note` values to the nearest note in the new key (current mode unchanged).
- [ ] Confirming a Mode change via the Regular Overlay snaps all 16 step `midi_note` values to the nearest note in the new mode (current key unchanged).
- [ ] When two scale notes are equidistant (tie), the lower note is always chosen.
- [ ] Steps with `enabled = false` are snapped exactly the same as enabled steps.
- [ ] Confirming the same key or mode that is already set produces no change to any step note (no-op guard).
- [ ] All existing `cargo test -p engine` tests continue to pass.
- [ ] `snap_to_key` never panics for any `midi_note` value 0–127 and any valid Key/Mode combination.
- [ ] `snap_to_key` result is always in 0–127.
- [ ] `clippy` passes with no new warnings.

---

## 7. Dependencies and Prerequisites

None. All required primitives (`SCALE_INTERVALS`, `KEY_ROOT`, `key_index`,
`mode_index`) already exist in `music_theory.rs`. No new crates, no schema
changes, no environment changes.

---

## 8. Recommended Agent Type

**coder** — this is a self-contained Rust implementation with clear function
signatures, no external dependencies, and full test coverage specified above.
