# Plan: InputCommand Abstraction and Keyboard Input (Step 6b)

## Overview

Implement the `InputCommand` / `OverlayMode` enums, wire them into `SequencerState::apply_command`, add a keyboard UI loop using crossterm, and make HID connection failures non-fatal.

## Steps

### Step 1 — engine/src/input.rs
- Define `InputCommand` enum with all variants
- Define `OverlayMode` enum (canonical definition, replacing the stub in state.rs)
- Add unit tests for enum construction

### Step 2 — engine/src/state.rs updates
- Remove the stub `OverlayMode` enum
- Import `crate::input::OverlayMode`
- Add `selected_step: usize` field to `SequencerState`
- Add `selected_param: u8` field to `SequencerState` (for ParamSelect commands)
- Update `Default` impl to include `selected_step: 0` and `selected_param: 0`
- Implement `apply_command(&mut self, cmd: InputCommand)` covering all variants
- Update existing tests to reflect new fields

### Step 3 — engine/src/hid.rs updates
- Make `HidApi::new()` and `open()` non-fatal: log warning to stderr and return without panicking

### Step 4 — engine/src/ui.rs (new file, hw-io gated)
- Keyboard event loop using crossterm `event::poll` with 50ms timeout
- Translate KeyEvent → InputCommand
- Overlay state (`Option<OverlayMode>`, `selected_param: u8`) tracked in UI thread only
- Stub overlay render regions with placeholder param list

### Step 5 — engine/src/lib.rs updates
- Add `pub mod input;`
- Add `#[cfg(feature = "hw-io")] pub mod ui;`

### Step 6 — Tests
- Unit tests for every `apply_command` variant in state.rs
- Unit tests for keyboard key-to-command translation in ui.rs (or a separate key_translation module)

## Test Cases

**apply_command tests:**
- `StepSelect(5)` sets `selected_step = 5`, clears pending edit
- `StepSelectDelta(1)` wraps correctly at 15→0
- `StepSelectDelta(-1)` wraps correctly at 0→15
- `NoteDelta(1)` sets `PendingEdit::Note` with correct step and adjusted note
- `Confirm` with `PendingEdit::Note` commits note to live state
- `Confirm` with `PendingEdit::None` is a no-op
- `ToggleStep` toggles `selected_step`
- `VelocityDelta(1)` sets `PendingEdit::Velocity`
- `OpenOverlay(Regular)` sets `active_overlay`
- `CloseOverlay` clears pending param edit
- `ParamSelect(3)` sets `selected_param = 3`
- `ParamSelectDelta(1)` advances param, wraps at 7
- `ParamValueDelta(2)` sets `PendingEdit::Param`

**Keyboard translation tests (no hw-io, pure logic):**
- Left/Right → StepSelectDelta
- Up/Down → NoteDelta
- Shift+Up/Down → VelocityDelta
- Space → ToggleStep
- Enter → Confirm
- F1 → OpenOverlay(Regular), F2 → OpenOverlay(Shift)
- Overlay active: Left/Right → ParamSelectDelta, Up/Down → ParamValueDelta, Esc → CloseOverlay
