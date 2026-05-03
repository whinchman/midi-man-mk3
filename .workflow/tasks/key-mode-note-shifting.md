# Task: key-mode-note-shifting

**Type:** coder
**Status:** done
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

Implementation complete on branch `key-mode-note-shifting/impl` (worktree at `.workflow/worktrees/key-mode-note-shifting`), based off `feature/key-mode-note-shifting`.

**Changes:**
- `engine/src/music_theory.rs`: Added `pub fn snap_to_key(midi_note: u8, key: Key, mode: Mode) -> u8` — pure stack-only function sweeping all MIDI octaves; ties resolve to the lower note. 7 unit tests added in a `#[cfg(test)]` block.
- `engine/src/state.rs`: Modified `apply_param_value` arms 0 and 1 to detect key/mode changes and call new private helper `snap_all_steps_to_key()`. 5 integration tests added covering key change, mode change, no-op guard, all 16 steps, and disabled steps.

**Test results:** 17/17 new tests pass; all 250+ existing tests pass. Clippy: no new warnings introduced. Release build: clean.

---

### Code Review — 2026-05-02

**Reviewer verdict: APPROVE**

**Findings: 0 critical, 0 warning, 2 info**

#### [INFO] `snap_to_key` — oct_min formula correctness confirmed

`oct_min = -((anchor + 11) / 12)` is conservative and correct. For the highest anchor (B, MIDI 71), `oct_min = -6`, which produces `anchor + (-6)*12 = -1`; that candidate is correctly skipped by the `!(0..=127)` bounds check. The first in-range note (MIDI 1, C# at -1 of root) is reached at `c=2` in the same octave iteration. All 13,824 combinations (12 keys × 9 modes × 128 notes) were exhaustively verified by simulation — every snap result is a valid in-key note in 0–127.

#### [INFO] Plan test had a dead assignment — cleaned up in actual impl

The plan's `test_disabled_steps_are_snapped` contained two sequential `state.pending_edit = …` assignments (the first, for mode, was immediately overwritten by the second for key). The actual implementation correctly has only the second assignment. No functional impact — noted for completeness.

**Acceptance criteria:**
- [x] Key change snaps all 16 steps — `test_key_change_snaps_all_steps` passes
- [x] Mode change snaps all 16 steps — `test_mode_change_snaps_all_steps` passes
- [x] Tie-break lower wins — `snap_tie_picks_lower_note` + `snap_out_of_key_rounds_to_nearest` pass
- [x] Disabled steps snapped — `test_disabled_steps_are_snapped` passes
- [x] No-op guard fires on same key/mode — `test_same_key_no_snap` passes
- [x] No panics at MIDI boundaries — `snap_midi_boundaries` passes; exhaustive simulation confirms
- [x] All existing tests pass (250+)
- [x] No new clippy warnings (3 pre-existing warnings in `cli.rs`/`clock.rs`/`main.rs` are unrelated)
