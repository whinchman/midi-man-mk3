# Task: fix-state-and-overlay

- **Status**: pending
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-state-and-overlay
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 1
- **Bugs Fixed**: BUG-004, BUG-010, BUG-011, BUG-012

## Goal

Fix four related state/overlay bugs: velocity hardcode in `tick()`, note cycling accumulation, overlay display showing raw numbers, and overlay Confirm not applying changes.

## Context

All four bugs live in `engine/src/state.rs` and `engine/src/ui_render.rs`.

### BUG-004 — `tick()` hardcodes velocity 100

`SequencerState::tick()` at line ~185 emits `MidiEvent::NoteOn { velocity: 100, .. }` instead of `step.velocity`. Velocity edits committed via `Confirm` are silently discarded. The existing test `tick_note_on_has_correct_fields` also asserts 100 and must be updated.

**Fix:** Change `velocity: 100` → `velocity: step.velocity` in the NoteOn arm of `tick()`. Update the test to set a non-default velocity (e.g. 64) and assert it is reflected.

### BUG-010 — Note cycling limited to ±1

`NoteDelta` always reads `self.steps[step].midi_note` (committed value) as the base for `next_note`. Repeated Up/Down without Enter produces the same pending value each time.

**Fix:**
```rust
InputCommand::NoteDelta(d) => {
    let step = self.selected_step;
    let base_note = match self.pending_edit {
        PendingEdit::Note { step: ps, midi_note } if ps == step => midi_note,
        _ => self.steps[step].midi_note,
    };
    let new_note = crate::music_theory::next_note(base_note, self.key, self.mode, d);
    self.pending_edit = PendingEdit::Note { step, midi_note: new_note };
}
```

Add a test: press NoteDelta(1) five times, assert pending note has advanced 5 scale degrees.

### BUG-011 — Overlay displays raw delta number

`ParamValueDelta` seeds `current_value` from 0 (or from an existing raw delta), not from the actual committed state value. The render code shows the raw integer (e.g. `[key:C->1]`).

**Fix:** Seed `current_value` from the current committed field value expressed in the same integer space. For enum params (key, mode, step_size), that's the variant index; for numeric params (swing), it's the raw value. `param_value_string` in `ui_render.rs` should format the pending value with the same formatter used for committed values (i.e. call `key_name`, `mode_name`, etc. on the resolved pending value).

The value stored in `PendingEdit::Param` should represent the fully-resolved new value in the same units as the committed field, with appropriate wrapping/clamping applied on each delta step.

### BUG-012 — Overlay Confirm does not apply param changes

The `Confirm` arm for `PendingEdit::Param` clears the edit without writing to any state field. The comment referencing "Step 7 (param overlay logic)" was never implemented.

**Fix:** Implement dispatch in the Confirm arm:
- index 0 → `self.key` (Key enum, wrap by variant count)
- index 1 → `self.mode` (Mode enum, wrap by variant count)
- index 2 → `self.swing` (clamp to i8 range, e.g. -50..=50)
- index 3 → `self.step_size` (StepSize enum, wrap)
- index 4 → `self.loop_in` / `self.loop_out` (clamp to 0..=15 per field, or as appropriate)
- index 5 → `self.paused` (toggle or bool from value != 0)
- index 6 → `self.playing` (toggle or bool from value != 0)

Add/expose `from_index` helpers on Key, Mode, StepSize if they don't exist. Add tests for commit of at least key and swing params.

## Files to Modify

- `engine/src/state.rs` — tick(), NoteDelta arm, ParamValueDelta arm, Confirm arm
- `engine/src/ui_render.rs` — param_value_string (if needed for BUG-011 display fix)
- `engine/src/music_theory.rs` (or wherever Key/Mode/StepSize are defined) — from_index helpers if missing

## Acceptance Criteria

- `tick()` emits `step.velocity` in NoteOn events.
- Repeated NoteDelta(1) advances the note cumulatively through scale degrees.
- Overlay Up/Down shows human-readable labels in the preview (e.g. `[key:C->D]` not `[key:C->1]`).
- Overlay Enter applies the pending param change to the state field.
- `cargo test -p engine` passes with new/updated tests covering all four fixes.

## Notes

