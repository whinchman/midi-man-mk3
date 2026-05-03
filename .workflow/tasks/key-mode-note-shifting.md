# Task: key-mode-note-shifting

**Type:** coder
**Status:** pending
**Feature Branch:** feature/key-mode-note-shifting
**Branch:** feature/key-mode-note-shifting/key-mode-note-shifting
**Base Branch:** feature/key-mode-note-shifting
**Parallel Group:** 1

## Goal

When the user confirms a Key or Mode change via the Regular Overlay, snap all
16 step `midi_note` values to the nearest note in the new key/mode.
Ties (equidistant candidates) always resolve to the lower note.

## Context

See full plan at `.workflow/plans/key-mode-note-shifting.md`.

**Commit point:** `apply_param_value` in `engine/src/state.rs` — this is the
single place where `self.key` and `self.mode` are written. No other call sites.

**Note representation:** `StepData.midi_note: u8` — raw MIDI note number 0–127.

**Existing primitives in `engine/src/music_theory.rs`:**
- `SCALE_INTERVALS: [[u8; 7]; 9]`
- `KEY_ROOT: [u8; 12]`
- `key_index(key) -> usize`
- `mode_index(mode) -> usize`
- `notes_in_key` (only covers octave 4 — do NOT use for snap, must sweep all octaves)

## Implementation

### Step 1 — `engine/src/music_theory.rs`

Add after `next_note`:

```rust
/// Snap `midi_note` to the nearest note in the scale defined by `key` and `mode`.
/// Ties resolve to the lower note. Result is always in 0–127.
pub fn snap_to_key(midi_note: u8, key: Key, mode: Mode) -> u8 {
    let intervals = SCALE_INTERVALS[mode_index(mode)];
    let mut cum: [i32; 7] = [0; 7];
    for i in 1..7 {
        cum[i] = cum[i - 1] + intervals[i - 1] as i32;
    }
    let note_i32 = midi_note as i32;
    let mut best_note: i32 = 0;
    let mut best_dist: i32 = i32::MAX;
    let anchor = KEY_ROOT[key_index(key)] as i32;
    let oct_min = -((anchor + 11) / 12);
    let oct_max = (127 - anchor) / 12 + 1;
    for oct in oct_min..=oct_max {
        for &c in cum.iter() {
            let candidate = anchor + oct * 12 + c;
            if candidate < 0 || candidate > 127 { continue; }
            let dist = (note_i32 - candidate).abs();
            if dist < best_dist {
                best_dist = dist;
                best_note = candidate;
            }
        }
    }
    best_note.clamp(0, 127) as u8
}
```

Add unit tests (see plan §3 Step 3 for full test list).

### Step 2 — `engine/src/state.rs`

Add private helper on `SequencerState`:

```rust
fn snap_all_steps_to_key(&mut self) {
    for step in self.steps.iter_mut() {
        step.midi_note = crate::music_theory::snap_to_key(step.midi_note, self.key, self.mode);
    }
}
```

Modify `apply_param_value` arms 0 and 1 to add a change-detection guard and
call `snap_all_steps_to_key()` after writing the new value:

```rust
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
```

Add integration tests (see plan §3 Step 4 for full test list).

## Acceptance Criteria

- [ ] Confirming a Key change snaps all 16 step midi_note values to nearest in-key note
- [ ] Confirming a Mode change snaps all 16 step midi_note values to nearest in-key note
- [ ] Equidistant candidates: lower note always wins
- [ ] Disabled steps are snapped the same as enabled steps
- [ ] Confirming the same key/mode already set: no step notes change (no-op guard)
- [ ] `snap_to_key` never panics for midi_note 0–127 and any valid Key/Mode
- [ ] All existing `cargo test -p engine` tests pass
- [ ] `clippy` passes with no new warnings

## Notes

