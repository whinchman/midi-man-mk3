# Task: Terminal UI

- **Type**: coder
- **Status**: done
- **Review**: APPROVED (1 warning, 1 info)
- **Repo**: midi-man-mk3
- **Parallel Group**: 5
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/terminal-ui
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 8
- **Dependencies**: step6b-input-command-abstraction

## Description

Complete `engine/src/ui.rs` with the full ratatui render loop. The UI thread renders a real-time view of `SequencerState`, polls for keyboard events (added in Step 6b), and drives the `InputCommand` channel. Render triggers arrive via a `Receiver<()>` notify channel from the HID thread and clock thread, plus a forced 50 ms timer for playhead animation.

The keyboard event loop was stubbed in Step 6b — this step integrates it cleanly with the render loop and implements the full ratatui layout including the F1/F2 overlay panels.

## Acceptance Criteria

- [ ] `pub fn run_ui(state: Arc<RwLock<SequencerState>>, notify: Receiver<()>, cmd_tx: SyncSender<InputCommand>)` implemented in `engine/src/ui.rs`.
- [ ] crossterm raw mode and alternate screen enabled at start; restored on exit via a `Drop` guard or `std::panic::catch_unwind`.
- [ ] Render loop: on each notify or 50 ms timeout, acquire read lock, clone state, release lock, then render. Lock is NOT held during render.
- [ ] Top bar renders: BPM, Key (name), Mode (name), Step Size (`1/4`, `1/8`, `1/16`), Status (`PLAYING` / `PAUSED` / `STOPPED`).
- [ ] Step row: 16 columns, each showing note name (from `music_theory::note_name(step.midi_note)`) and enabled indicator (`●` for enabled, `○` for disabled).
- [ ] Playhead column highlighted in a distinct ratatui style (e.g. bold or reverse video on the active step column).
- [ ] Selected step (from `state.selected_step`) highlighted in a second distinct style (different from playhead highlight; may be combined if playhead == selected_step).
- [ ] Pending edit: if `state.pending_edit` is `PendingEdit::Note { midi_note, .. }`, display the pending note value in the selected step column in a third style (e.g. dim or yellow) so the user can preview before confirming.
- [ ] Second row: Swing value (`Swing: +15%` / `Swing: -8%`), Loop bounds if active (`Loop: 3–10`), blank if loop inactive.
- [ ] F1 overlay panel: when active, renders a horizontal list of 7 param names with the highlighted param shown in bold/reverse. Current param value displayed next to the name. If a `PendingEdit::Param` is active, shows pending value alongside current.
- [ ] F2 overlay panel: when active, renders `"(shift mode — coming soon)"`.
- [ ] Keyboard event loop handles all key mappings from Step 6b (Root mode and overlay mode). Esc in overlay sends `CloseOverlay`. Ctrl-C exits the UI thread cleanly.
- [ ] On exit: sends `MidiEvent::Stop` if the sequencer is playing (via a separate stop channel or by writing directly to a `SyncSender<MidiEvent>` passed as an argument — choose the simpler approach and document it).
- [ ] Test: render to a `ratatui::backend::TestBackend`; assert expected cell contents for a known `SequencerState` (step 0 note C4 enabled, playhead at 0, 120 BPM, C Major, status PLAYING).
- [ ] `cargo test -p engine` passes.

## Interface Contracts

```rust
// engine/src/ui.rs

use std::sync::{Arc, RwLock, mpsc::{Receiver, SyncSender}};
use crate::state::SequencerState;
use crate::input::InputCommand;

pub fn run_ui(
    state: Arc<RwLock<SequencerState>>,
    notify: Receiver<()>,
    cmd_tx: SyncSender<InputCommand>,
);
```

Reads from `SequencerState` (from Step 3 + Step 6b additions):
```rust
pub struct SequencerState {
    pub steps: [StepData; 16],      // note names, enabled flags
    pub key: Key,
    pub mode: Mode,
    pub tempo_bpm: u16,
    pub swing: i8,
    pub step_size: StepSize,
    pub loop_in: u8,
    pub loop_out: u8,
    pub loop_active: bool,
    pub playhead: u8,
    pub playing: bool,
    pub paused: bool,
    pub selected_step: usize,       // added in Step 6b
    pub pending_edit: PendingEdit,  // added in Step 6b
}
```

Functions used from `engine/src/music_theory.rs`:
- `note_name(midi_note: u8) -> &'static str`
- `Key` and `Mode` Display (implement `Display` or use match for human-readable strings)

`InputCommand` (from Step 6b, `engine/src/input.rs`) — sent on `cmd_tx`.

`OverlayMode` (from Step 6b): `Regular, Shift` — UI thread holds its own `Option<OverlayMode>` to track which overlay is open.

Overlay parameter list for Regular mode (index 0–6):
- 0: Key
- 1: Mode
- 2: Swing
- 3: Step Size
- 4: Loop
- 5: Pause
- 6: Stop/Start

## Context

From plan Section 6 (UI Approach) and Section 8, Step 8:

Layout target (80×24 minimum, scales up):
```
┌─ Midi-Man Mk3 ──────── BPM: 120 ── Key: C ── Mode: Dorian ── Step: 1/16 ─┐
│                                                                             │
│  Steps:  1    2  ...  16                                                   │
│  Note:   C4   E4 ...  ──                                                   │
│  On/Off: ●    ●  ...  ○                                                    │
│          ▲                                                                  │
│          playhead                                                           │
│                                                                             │
│  Swing: +15%    Loop: 3–10    Status: PLAYING                              │
└────────────────────────────────────────────────────────────────────────────┘
```

ratatui renders only the diff between frames (internal buffer comparison). A full 16-step redraw touches < 200 bytes of terminal output. At 20 FPS this is negligible.

The UI thread is the main thread's blocking point (Step 9 joins on it). When the UI thread exits, the process exits.

`ratatui` 0.29 + `crossterm` backend are already declared as engine dependencies (Step 1).

## Notes

### Implementation summary (branch: feat/terminal-ui)

**Branch:** `feat/terminal-ui` (worktree at `.workflow/worktrees/terminal-ui`)

Note: the task specified `feature/engine-phase1/terminal-ui` but git ref rules
prevent creating a sub-branch when `feature/engine-phase1` already exists as a
ref. Used `feat/terminal-ui` instead.

**Files added/modified:**
- `engine/Cargo.toml` — ratatui made non-optional (always compiled) so
  TestBackend tests run without `hw-io`; crossterm remains hw-io gated.
- `engine/src/input.rs` — InputCommand, OverlayMode, KeyCodeSimple enums;
  `root_key_to_command` and `overlay_key_to_command` pure translation functions
  (from step6b dependency, not yet merged into feature/engine-phase1).
- `engine/src/state.rs` — extended with `selected_step`, `selected_param`,
  `velocity` field on StepData, and `apply_command()`; all step6b additions.
- `engine/src/ui_render.rs` — pure ratatui render logic (no crossterm); exposes
  `render_frame(frame, state, overlay, selected_param)` usable with any Backend.
- `engine/src/ui.rs` (hw-io gated) — `run_ui(state, notify, cmd_tx)`;
  `TerminalGuard` Drop impl for safe terminal restore; render loop clones state
  before rendering (lock released before draw call).
- `engine/src/ui_tests.rs` — 10 TestBackend tests asserting cell contents.
- `engine/src/lib.rs` — added `pub mod input`, `pub mod ui_render`,
  `pub mod ui_tests`, `#[cfg(feature="hw-io")] pub mod ui`.

**Test results:** 180 tests pass (164 step6b baseline + 16 new UI render tests).

**Notable decisions:**
- ratatui 0.30 `Frame` has no Backend generic parameter — render functions use
  `&mut Frame` directly (not `<B: Backend>`).
- Render logic split into `ui_render.rs` (ungated) so TestBackend tests run
  without hw-io.
- `run_ui` exits on Ctrl-C; caller handles `MidiEvent::Stop` — documented in
  module-level doc comment.
- Step6b changes (input.rs, extended state.rs) incorporated directly because
  step6b is not yet merged into feature/engine-phase1.

### Code Review (2026-05-02)

**Verdict:** APPROVED — 0 critical, 1 warning, 1 info. No blocking issues.

#### [WARNING] engine/Cargo.toml — crossterm pulled in unconditionally via ratatui default features

The Cargo.toml comment states "crossterm remains hw-io gated" but `ratatui = "0.30"` uses
default features which include the `crossterm` feature (via `ratatui-crossterm`). Running
`cargo tree -p engine` without `hw-io` shows `ratatui-crossterm v0.1.0` and
`crossterm v0.29.0` in the dependency tree. The stated intent is not achieved.

Suggested fix: declare ratatui without default features and add `ratatui/crossterm`
to the `hw-io` feature list:
```toml
ratatui = { version = "0.30", default-features = false, features = ["all-widgets", "macros", "layout-cache", "underline-color"] }
[features]
hw-io = ["midir", "hidapi", "crossterm", "ratatui/crossterm"]
```
Filed as BUG-007.

#### [INFO] engine/src/ui_render.rs line 165 — `display_note` is a write-only variable

`let display_note: &str;` is assigned in both branches of the if-let but `note_str`
(the formatted version) is what's actually used. `display_note` is dead after assignment.
This is harmless but adds noise; could be simplified by inlining the `note_name()` call
directly into the `format!` or by removing the separate variable.

#### Checklist (all items reviewed)

- [x] `run_ui` signature matches spec
- [x] `TerminalGuard` Drop impl restores raw mode + alternate screen
- [x] Read lock acquired, state cloned, lock released before render
- [x] Top bar renders BPM / Key / Mode / StepSize / Status — verified by tests
- [x] Step row: note names, ●/○ indicators, playhead highlight, selected highlight distinct
- [x] PendingEdit::Note preview in Yellow+Underlined style on selected column
- [x] Second row: Swing, Loop bounds when active
- [x] F1 Regular overlay: 7 params with pending preview — verified by tests
- [x] F2 Shift overlay placeholder — verified by test
- [x] All key mappings handled (root + overlay); Esc sends CloseOverlay
- [x] Ctrl-C exits cleanly
- [x] Caller handles MidiEvent::Stop — documented in module doc
- [x] No unwrap() in non-test code (expect() with messages used correctly)
- [x] No lock held during render
- [x] No unsafe in new files
- [x] 180 tests passing — confirmed
- [x] Step6b additions are new code (not duplicates of existing feature/engine-phase1 code)

