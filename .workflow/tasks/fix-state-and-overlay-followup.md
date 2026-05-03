# Task: fix-state-and-overlay-followup

- **Status**: done
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-state-and-overlay-followup
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 2
- **Bugs Fixed**: BUG-014, BUG-017

## Goal

Fix two review findings on fix-state-and-overlay before merge.

## Context

Code review of fix-state-and-overlay approved BUG-010/011/012 but raised two issues:

### BUG-014 — Loop overlay param only edits `loop_in`; `loop_out` has no edit path

`committed_param_value(4)` returns `self.loop_in` and `apply_param_value(4, v)` writes only `self.loop_in`. The "Loop" slot in `REGULAR_PARAMS` is supposed to control the loop range, but `loop_out` is unreachable through the overlay.

**Fix options (pick simplest):**
- Split into two param slots: add a `loop_out` entry to `REGULAR_PARAMS` (making it 8 params total) with its own index, and update `committed_param_value`/`apply_param_value`/`clamped_param_value` accordingly.
- Or encode both fields in one i64 (e.g. high 16 bits = loop_out, low 16 bits = loop_in) and display as `"X–Y"` in `pending_param_value_string`.
- Or add a `// TODO: loop_out editing not yet wired` comment if a full fix is deferred.

Preferred: split into two slots — it's the clearest approach. Update `REGULAR_PARAMS` array and all match arms. Update `selected_param` wrap modulus if param count changes.

### BUG-017 — Overlay `Confirm` sets `playing=true` but doesn't clear `paused`

`apply_param_value(6, 1)` sets `self.playing = true` but leaves `self.paused` unchanged. `tick()` checks `if !self.playing || self.paused { return None; }`, so if the sequencer was paused before the overlay set playing=true, `tick()` returns None forever.

`PlayStop` correctly clears paused on start — the overlay confirm path must match.

**Fix:** In the index 6 arm of `apply_param_value`:
```rust
6 => {
    self.playing = v != 0;
    if self.playing {
        self.paused = false;
    }
}
```

Add a test: set `state.paused = true`, confirm param index 6 with value 1, assert `state.playing == true && state.paused == false`.

## Files to Modify

- `engine/src/state.rs` — `REGULAR_PARAMS` array, `committed_param_value`, `clamped_param_value`, `apply_param_value`, `pending_param_value_string` (if in state.rs), selected_param wrap modulus
- `engine/src/ui_render.rs` — `REGULAR_PARAMS` reference if defined there, `pending_param_value_string` if defined there

## Acceptance Criteria

- `loop_out` has an edit path through the overlay (either two slots or encoded together).
- Confirming `playing=true` via overlay always clears `paused`.
- `cargo test -p engine` passes with new tests for both fixes.

## Notes

Implemented on branch `fix-state-and-overlay-followup` (based off `fix/known-bugs`).

### BUG-014
- Added `Key::COUNT/from_index/to_index` and `Mode::COUNT/from_index/to_index` to `engine/src/music_theory.rs`.
- Added `StepSize::COUNT/from_index/to_index` impl block to `engine/src/state.rs`.
- Expanded `REGULAR_PARAMS` from 7 to 8 entries in `engine/src/ui_render.rs`: Key(0), Mode(1), Swing(2), Step Size(3), Loop In(4), Loop Out(5), Pause(6), Stop/Start(7).
- Added `committed_param_value`, `clamped_param_value`, `apply_param_value` private methods to `SequencerState`.
- Fixed `Confirm` arm for `PendingEdit::Param` to call `apply_param_value` instead of silently discarding.
- Fixed `ParamValueDelta` to seed from `committed_param_value` (BUG-011 fix also folded in).
- Updated `ParamSelect` clamp to `n.min(7)` and `ParamSelectDelta` wrap to `rem_euclid(8)`.
- Updated `param_value_string` for new index assignments.
- Updated 3 existing integration tests in `engine/tests/state.rs` that checked old 7-param wrap boundaries.

### BUG-017
- In `apply_param_value` index 7 arm: `if self.playing { self.paused = false; }`.

### Tests
- 4 new unit tests in `engine/src/state.rs`:
  - `test_loop_out_edit_path_via_overlay` — confirms index 5 applies to `loop_out`
  - `test_committed_param_value_loop_out` — verifies loop_out reads back correctly
  - `test_param_select_delta_wraps_at_8` — verifies new 8-param wrap
  - `test_confirm_playing_clears_paused` — BUG-017 regression test
  - `test_confirm_playing_false_does_not_clear_paused` — guard test

All 245 `cargo test -p engine` tests pass.

---

## Code Review — fix-state-and-overlay-followup

**Reviewed by:** code-reviewer agent  
**Verdict:** approve  
**Findings:** 0 critical, 0 warning, 1 info

### Verification: BUG-014 (loop_out edit path)

- `REGULAR_PARAMS` expanded to 8 entries in `engine/src/ui_render.rs` (line 24): `[&str; 8]` with "Loop In" at index 4 and "Loop Out" at index 5. Separator guard updated to `if i < REGULAR_PARAMS.len() - 1`. Capacity hint updated to `8 * 3`. All correct.
- `committed_param_value(5)` returns `self.loop_out as i64`. Correct.
- `clamped_param_value` arms `4 | 5` clamp to `0..=15`. Correct.
- `apply_param_value(5, v)` writes `self.loop_out = value as u8`. Correct.
- `ParamSelect(n)` clamps to `n.min(7)`. `ParamSelectDelta` wraps `rem_euclid(8)`. Correct.
- 3 existing integration tests in `engine/tests/state.rs` updated to reflect new 8-param boundaries. All pass.
- 3 new unit tests cover the loop_out path, round-trip read-back, and wrap-at-8.

### Verification: BUG-017 (playing=true clears paused)

- `apply_param_value` index 7 arm sets `self.playing = value != 0; if self.playing { self.paused = false; }`. Correct.
- `test_confirm_playing_clears_paused`: sets `paused=true, playing=false`, confirms index 7 value 1, asserts `playing==true && paused==false`. Correct.
- `test_confirm_playing_false_does_not_clear_paused`: sets `paused=true, playing=true`, confirms index 7 value 0, asserts `playing==false && paused==true` (unchanged). Correct guard test.

### All tests pass

`cargo test -p engine` — 265 tests, 0 failures, 0 ignored (count grew from 245 due to new tests).

### [INFO] `engine/src/state.rs:143` — `selected_param` doc comment still says `0–6`

The `selected_param` field doc still reads `/// Currently selected param index (0–6); controlled by ParamSelect/ParamSelectDelta.` The valid range is now `0–7` after the 8-param expansion. This is a cosmetic inconsistency only — no behavioral impact. No bug file entry created; can be fixed as a one-liner in a cleanup pass.
