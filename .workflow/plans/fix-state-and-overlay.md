# Plan: fix-state-and-overlay

## Overview
Fix four bugs in `engine/src/state.rs` and `engine/src/ui_render.rs`.

## Steps

### Step 1 — BUG-004: tick() velocity hardcode
- File: `engine/src/state.rs` ~line 185
- Change `velocity: 100` to `velocity: step.velocity`
- Write test: set step velocity to 64, assert tick() emits NoteOn with velocity 64

### Step 2 — BUG-010: NoteDelta accumulation
- File: `engine/src/state.rs` NoteDelta arm
- Seed base note from pending_edit if pending for same step, else from committed step
- Write test: apply NoteDelta(1) five times, assert pending note advanced 5 scale degrees

### Step 3 — BUG-011 & BUG-012: ParamValueDelta and Confirm
- Add `from_index` helpers on Key, Mode, StepSize in `engine/src/music_theory.rs` and `engine/src/state.rs`
- Fix ParamValueDelta: seed current_value from actual committed state (variant index for enums, raw for numeric)
- Store resolved value, not raw delta
- Fix ui_render.rs: format pending param value using human-readable formatter (key_name/mode_name etc)
- Fix Confirm arm for PendingEdit::Param: dispatch to correct state field
- Write tests: commit key param, commit swing param

## Dependencies
- Steps 1 and 2 are independent
- Step 3 depends on from_index helpers being present before implementing Confirm dispatch
