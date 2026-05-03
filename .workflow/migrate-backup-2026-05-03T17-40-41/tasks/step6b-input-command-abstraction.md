# Task: InputCommand Abstraction and Keyboard Input

- **Type**: coder
- **Status**: done
- **Repo**: midi-man-mk3
- **Parallel Group**: 4
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/input-command-abstraction
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 6b
- **Dependencies**: step3-sequencer-state-and-engine, step6-hid-report-structs

## Description

Implement `engine/src/input.rs` with the `InputCommand` and `OverlayMode` enums. Add `PendingEdit` (with the `OverlayMode` variant) and `apply_command` to `SequencerState` in `engine/src/state.rs`. Add a keyboard event loop to `engine/src/ui.rs` that translates crossterm `KeyEvent` values into `InputCommand` values sent on a `SyncSender<InputCommand>`. Stub out the F1/F2 overlay render regions in the UI.

Both the keyboard handler (this task) and the HID reader (Step 7) produce `InputCommand` values on the same channel. State mutation is handled in one place: `SequencerState::apply_command`.

This task also makes the HID connection non-fatal: if `hidapi` fails to open the device, the HID thread logs a warning and exits; the engine continues with keyboard-only input.

## Acceptance Criteria

- [ ] `InputCommand` enum defined in `engine/src/input.rs` with all variants from the plan (see Interface Contracts).
- [ ] `OverlayMode` enum defined in `engine/src/input.rs`: `Regular, Shift`.
- [ ] `PendingEdit` enum in `engine/src/state.rs` updated to use `OverlayMode` from `input.rs`: `None`, `Note { step: usize, midi_note: u8 }`, `Velocity { step: usize, velocity: u8 }`, `Param { overlay: OverlayMode, index: u8, value: i64 }`.
- [ ] `SequencerState::apply_command(cmd: InputCommand)` implemented in `engine/src/state.rs`:
  - `StepSelect(n)` sets a selected_step field and discards any pending note/velocity edit.
  - `StepSelectDelta(d)` adjusts selected step by d, wraps 0–15, discards pending note/velocity edit.
  - `NoteDelta(d)` sets `pending_edit = PendingEdit::Note { step: selected_step, midi_note: current + d }` (does not yet apply to live state).
  - `Confirm` commits pending edit to live state; if `PendingEdit::None`, no-op.
  - `ToggleStep` toggles the currently selected step.
  - `VelocityDelta(d)` sets `pending_edit = PendingEdit::Velocity { ... }`.
  - `OpenOverlay(mode)` records overlay mode in `SequencerState` (or returns an event for the UI thread — see Context).
  - `CloseOverlay` discards pending param edit.
  - `ParamSelect(n)` and `ParamSelectDelta(d)` update selected param index.
  - `ParamValueDelta(d)` sets `PendingEdit::Param { ... }` with pending value change.
- [ ] Keyboard event loop in `engine/src/ui.rs` correctly translates all key mappings from the plan (see Interface Contracts table) to `InputCommand` values.
- [ ] Keyboard loop uses crossterm `event::poll` with a 50 ms timeout so the render loop still fires at ~20 FPS between key events.
- [ ] F1/F2 overlay regions stubbed in the UI render: when overlay is active, display a placeholder row listing param names with the currently highlighted param name shown in a distinct ratatui style. Shift overlay renders `"(shift mode — coming soon)"`.
- [ ] `engine/src/hid.rs` updated: if `HidApi::new()` or `open()` fails, log a warning to stderr and return without panicking. The engine continues with keyboard-only input.
- [ ] Unit tests for `apply_command`: test each `InputCommand` variant; verify pending edit is set correctly; verify `Confirm` commits; verify `CloseOverlay` discards. Unit tests for keyboard translation: each mapped key produces the expected `InputCommand`.
- [ ] `cargo test -p engine` passes.

## Interface Contracts

```rust
// engine/src/input.rs

#[derive(Clone, Debug)]
pub enum InputCommand {
    StepSelect(usize),
    StepSelectDelta(i8),
    NoteDelta(i8),
    Confirm,
    ToggleStep,
    VelocityDelta(i8),
    OpenOverlay(OverlayMode),
    CloseOverlay,
    ParamSelect(u8),
    ParamSelectDelta(i8),
    ParamValueDelta(i8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode { Regular, Shift }
```

`SequencerState` additions (engine/src/state.rs):
```rust
pub struct SequencerState {
    // ... existing fields from Step 3 ...
    pub selected_step: usize,        // NEW: 0–15, controlled by StepSelect/StepSelectDelta
    pub pending_edit: PendingEdit,   // NEW: replaces stub from Step 3
    // overlay state lives in UI thread only (see Context)
}

pub enum PendingEdit {
    None,
    Note { step: usize, midi_note: u8 },
    Velocity { step: usize, velocity: u8 },
    Param { overlay: OverlayMode, index: u8, value: i64 },
}

impl SequencerState {
    pub fn apply_command(&mut self, cmd: InputCommand);
}
```

Keyboard mapping (Root mode, no overlay):
| Key            | InputCommand           |
|----------------|------------------------|
| Left arrow     | `StepSelectDelta(-1)`  |
| Right arrow    | `StepSelectDelta(+1)`  |
| Up arrow       | `NoteDelta(+1)`        |
| Down arrow     | `NoteDelta(-1)`        |
| Shift+Up       | `VelocityDelta(+1)`    |
| Shift+Down     | `VelocityDelta(-1)`    |
| Space          | `ToggleStep`           |
| Enter          | `Confirm`              |
| F1             | `OpenOverlay(Regular)` |
| F2             | `OpenOverlay(Shift)`   |

Keyboard mapping (F1 or F2 overlay active):
| Key        | InputCommand        |
|------------|---------------------|
| Left arrow | `ParamSelectDelta(-1)` |
| Right arrow| `ParamSelectDelta(+1)` |
| Up arrow   | `ParamValueDelta(+1)`  |
| Down arrow | `ParamValueDelta(-1)`  |
| Enter      | `Confirm`           |
| Esc        | `CloseOverlay`      |

Regular overlay parameter list (left→right, index 0–6):
1. Key
2. Mode
3. Swing (-50 to +50)
4. Step Size (Quarter/Eighth/Sixteenth)
5. Loop (in/out/clear — three sequential Enter presses cycle through)
6. Pause (toggle)
7. Stop/Start (toggle)

`InReport` from Step 6 (`engine/src/hid.rs`) — the HID translator in Step 7 will map `InReport` fields to `InputCommand` values using the same semantics as the keyboard.

## Context

From plan Section 8, Step 6b:

**Overlay state split:** `Option<OverlayMode>` and `selected_param: u8` live in the UI thread only (presentation state). `PendingEdit` lives in `SequencerState` (shared state, visible to the HID thread for read). The `OpenOverlay` command may need to communicate the overlay mode to the UI thread via a separate mechanism — a secondary channel (`SyncSender<OverlayMode>`) or by convention (UI thread tracks overlay mode locally and does not put it in shared state). Choose whichever is simpler; document the decision in a comment.

**Confirm contract:** matches the physical surface. Parameter changes via up/down are "pending" until Enter confirms. This applies in both Root and overlay modes.

**HID non-fatal:** from plan Step 6b — "if `hidapi::HidApi::new()` or `open()` fails (device not connected), the HID thread logs a warning and exits immediately. The engine continues running with keyboard-only input."

The SyncSender<InputCommand> is the sole path into state mutation — HID and keyboard are peers on the same channel.

## Notes

Implementation complete on branch `engine-phase1/input-command-abstraction`.

**What was implemented:**

- `engine/src/input.rs` — `InputCommand` and `OverlayMode` enums (canonical definitions). Includes `KeyCodeSimple` mirror enum and two pure translation functions (`root_key_to_command`, `overlay_key_to_command`) that are feature-gate-free so they can be unit-tested without hw-io.

- `engine/src/state.rs` — Removed stub `OverlayMode` and re-exported the canonical one from `input.rs`. Added `selected_step: usize` and `selected_param: u8` fields with wrapping navigation. Added `velocity: u8` field to `StepData`. Implemented `apply_command` covering all 11 `InputCommand` variants with correct note/velocity clamping (0–127), step wrapping (mod 16), param wrapping (mod 7), Confirm commit semantics, and CloseOverlay discard.

- `engine/src/hid.rs` — Added non-fatal `open_device()` helper (hw-io gated) that logs a warning to stderr and returns `None` instead of panicking when HidApi init or device open fails.

- `engine/src/ui.rs` — Keyboard event loop (hw-io gated). Uses crossterm `event::poll` with 50 ms timeout for ~20 FPS render. Translates root-mode and overlay-mode key events to `InputCommand`. Overlay state (`Option<OverlayMode>`, `selected_param`) tracked in UI thread only. Ratatui render stub with Regular overlay param list (highlighted selected param) and Shift overlay placeholder.

- `engine/src/lib.rs` — Added `pub mod input;` and `#[cfg(feature = "hw-io")] pub mod ui;`.

**Test results:** 157 tests passing (108 pre-existing + 19 keyboard translation tests in input.rs + 30 apply_command tests in state.rs). Clippy: 2 pre-existing warnings in clock.rs, no new warnings.

---

## Code Review — 2026-05-02

**Reviewer:** code-reviewer agent
**Verdict:** REQUEST-CHANGES
**Findings:** 0 critical, 2 warning, 2 info

### [WARNING] engine/src/state.rs:185 — `tick()` hardcodes velocity 100; ignores `StepData.velocity`

`StepData.velocity` was added in this step and the `VelocityDelta`/`Confirm` pipeline correctly commits it to `StepData`. However, `tick()` still emits `velocity: 100` in every `MidiEvent::NoteOn` instead of reading `step.velocity`. Every note plays at velocity 100 regardless of user edits. The existing test `tick_note_on_has_correct_fields` masks this because it also asserts `velocity: 100`. Logged as BUG-004 in `.workflow/BUGS.md`.

Fix: change line 185 to `velocity: step.velocity,` and update the test to assert a non-default velocity.

### [WARNING] engine/src/hid.rs:317 — `unsafe { std::mem::transmute(report) }` in test violates project code standard

The code standard requires "Safe Rust only — no unsafe without a comment explaining why." The comment is present but the justification is incomplete. The `transmute` can be replaced entirely with `std::mem::offset_of!` (stable since Rust 1.77) with no unsafe at all. Logged as BUG-005 in `.workflow/BUGS.md`.

Fix: replace the transmute block with `offset_of!(InReport, field)` assertions.

### [INFO] engine/src/ui.rs:98 — Dead code in `update_local_overlay`: `Confirm` arm has no effect

`InputCommand::CloseOverlay | InputCommand::Confirm` is a combined match arm. The body checks `if matches!(cmd, InputCommand::CloseOverlay)` — meaning the `Confirm` branch of the arm always takes the else path and does nothing. `Confirm` in this arm is dead code. The functional behavior is correct (Confirm should not close the overlay), but the combined arm is misleading and likely a leftover from an earlier refactor.

Fix: split into `InputCommand::CloseOverlay => { ui.overlay = None; }` and remove `Confirm` from this arm entirely.

### [INFO] engine/src/state.rs:268 — `ParamSelect(n)` does not clamp `n` to 0–6

`StepSelect(n)` clamps to 15 (line 202: `n.min(15)`), but `ParamSelect(n)` stores `n` directly with no bounds check. Passing `ParamSelect(7)` sets `selected_param = 7`, which is out of the 0–6 range. Downstream, `ParamValueDelta` uses `selected_param` as the `index` field in `PendingEdit::Param`, so an out-of-range index would propagate into shared state. Since only Step 7 (HID) produces `ParamSelect` and has 7 physical buttons, this cannot be triggered from the keyboard, but defensive clamping is consistent with the existing `StepSelect` treatment.

Fix: add `.min(6)` clamp: `self.selected_param = n.min(6);`
