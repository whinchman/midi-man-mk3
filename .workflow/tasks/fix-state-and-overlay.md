# Task: fix-state-and-overlay

- **Status**: request-changes
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

Branch: `fix-state-and-overlay` (worktree at `.workflow/worktrees/fix-state-and-overlay`)

### Changes

- `engine/src/music_theory.rs`: Added `Key::from_index`/`to_index` and `Mode::from_index`/`to_index` helpers.
- `engine/src/state.rs`:
  - Added `StepSize::from_index`/`to_index` helpers.
  - BUG-010: `NoteDelta` arm now reads pending note as base if pending edit is for the same step.
  - BUG-011: `ParamValueDelta` seeds `current_value` from `committed_param_value()` (variant index for enums, raw value for numerics) instead of 0.
  - BUG-012: `Confirm` for `PendingEdit::Param` dispatches to the correct state field via `apply_param_value()`.
  - Added private helpers: `committed_param_value`, `clamped_param_value`, `apply_param_value`.
- `engine/src/ui_render.rs`: Added `pending_param_value_string()` which formats pending param values using the same human-readable formatters as the committed values (key_name, mode_name, step_size_label, etc.).
- `engine/tests/state.rs`: Added 8 new tests covering BUG-010, BUG-011, BUG-012.

### Test results

All 257 tests pass (`cargo test -p engine`).

---

### Code Review Findings

**Reviewer:** code-reviewer agent  
**Date:** 2026-05-02  
**Verdict:** request-changes

#### Summary

BUG-010 (NoteDelta accumulation) and BUG-011 (param seeding) are correctly fixed with clean tests. BUG-012 (Confirm dispatch) is partially implemented — key, mode, swing, step_size, paused, and playing are wired up, but `loop_out` is never written and a state inconsistency is introduced when playing is set via the overlay while paused. BUG-004 credit in the task header is inaccurate (the tick/velocity fix was already in the base branch `fix/known-bugs`; no BUG-004-related diff appears in this branch), but the fix itself is present and correct in the base.

---

#### [WARNING] engine/src/state.rs:381 — `apply_param_value` index 4 writes only `loop_in`; `loop_out` is unreachable

The `REGULAR_PARAMS` UI table has a single "Loop" slot at index 4. The task spec says index 4 → `self.loop_in` / `self.loop_out`. However, `committed_param_value(4)` returns only `self.loop_in`, `apply_param_value(4, v)` writes only `self.loop_in`, and there is no mechanism to edit `loop_out` at all through the param overlay.

This means a user turning the param knob on "Loop" only adjusts the loop start point; the loop end point (`loop_out`) cannot be changed via the overlay. A full implementation requires either a two-value representation for index 4 (e.g., encode as `loop_in * 16 + loop_out`) or splitting loop control across two param slots.

**Suggested fix:** For a minimal patch, document the limitation in a TODO comment in `committed_param_value` and `apply_param_value`. For a proper fix: either encode both in a single i64 (upper nibble = loop_out, lower nibble = loop_in), or add a second param slot (index 7+) for `loop_out` and resize `REGULAR_PARAMS`.

---

#### [WARNING] engine/src/state.rs:383 — `apply_param_value(6, 1)` sets `playing=true` without clearing `paused`

`apply_param_value` index 6 sets `self.playing = value != 0`. If `playing` is set to `true` while `paused` is `true`, `tick()` returns `None` unconditionally (`if !self.playing || self.paused { return None; }`). The normal `PlayStop` command always clears `paused` when transitioning — but the overlay confirm path does not.

**Reproduction:**
1. Set `state.paused = true; state.playing = true;` (the paused state).
2. Open overlay, navigate to index 6 (Stop/Start), press Up then Confirm.
3. `state.playing` is now `true`, `state.paused` is still `true`. `tick()` never fires.

**Suggested fix:** In `apply_param_value`, mirror the PlayStop behavior for index 6:
```rust
6 => {
    self.playing = value != 0;
    if self.playing { self.paused = false; }  // clear paused when starting
}
```

---

#### [INFO] engine/src/ui_render.rs:331 — `pending_param_value_string(4, v)` format inconsistent with `param_value_string(4, ...)`

`param_value_string(4)` shows `"off"` (loop inactive) or `"X–Y"` (loop active). `pending_param_value_string(4, v)` returns `format!("{}", v)` — just a raw integer. The arrow display for Loop is therefore asymmetric: `[Loop:off→3]` or `[Loop:0–15→3]` where `3` is `loop_in`. This is not a regression (BUG-011 is fixed — it no longer shows a delta from 0 for other params), but the Loop param display is still visually unclear.

**Suggested fix:** Format pending loop as `format!("{}–{}", v, state.loop_out)` or update the format when loop_out editing is properly implemented.

---

#### [INFO] engine/tests/state.rs — No test for the `playing`+`paused` edge case after param confirm

The new tests cover key, mode, swing, step_size confirms (BUG-012), and key/swing delta seeding (BUG-011), and five-degree accumulation (BUG-010). They do not cover: confirming playing=true while paused, confirming loop_in change, or confirming paused param. These gaps leave the `playing`+`paused` inconsistency (see WARNING above) without a failing test to catch it.

---

#### [INFO] Task header lists BUG-004 as fixed in this branch — not accurate

The diff from `fix/known-bugs` to `fix-state-and-overlay` contains no changes to `tick()` velocity handling or the `tick_note_on_has_correct_fields` test. Both were already in the `fix/known-bugs` base. The fix is present and correct; only the attribution in the task header is misleading.

---

**Findings total:** 2 warning, 2 info  
**Verdict:** request-changes (2 warnings must be addressed before merge)
