# Task: HID Host Reader/Writer (Engine)

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 5
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/hid-host-reader-writer
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 7
- **Dependencies**: step6-hid-report-structs, step6b-input-command-abstraction

## Description

Complete `engine/src/hid.rs` with the full HID host reader/writer loop. The function `run_hid` opens the Pico HID device by VID/PID, polls for `InReport` data, translates each report into `InputCommand` values sent on the shared channel, and writes back an `OutReport` (LED state) after every state change. If the device is not connected, the thread exits gracefully without panicking.

Step 6b already made the device-not-found path non-fatal. This step adds the full read/translate/write loop and the translation from `InReport` fields to `InputCommand` values.

## Acceptance Criteria

- [ ] `pub fn run_hid(cmd_tx: SyncSender<InputCommand>, state: Arc<RwLock<SequencerState>>, ui_notify: SyncSender<()>)` implemented in `engine/src/hid.rs`.
- [ ] Opens HID device using `HID_VID` and `HID_PID` constants (defined in Step 6). If open fails, logs warning and returns immediately.
- [ ] Main loop calls `device.read_timeout(&mut buf, 5)` (5 ms timeout) — non-blocking equivalent.
- [ ] On successful read: parse `InReport::from_bytes(&buf)`; check sequence number; if two consecutive reports share the same sequence number, log a warning.
- [ ] Translate `InReport` fields to `InputCommand` values and send on `cmd_tx`:
  - `encoder_deltas[i] != 0` → `InputCommand::NoteDelta(encoder_deltas[i])` for step i (or `InputCommand::ParamValueDelta` if an overlay is active — derive overlay state by reading `state` or receiving a signal; see Step 6b Context).
  - `step_buttons` bit i set → `InputCommand::StepSelect(i)` followed by `InputCommand::ToggleStep`.
  - `param_buttons` bit 0 (index 0) → `InputCommand::OpenOverlay(Regular)` → `InputCommand::ParamSelect(0)` (Key).
  - `param_buttons` bit 1 → `InputCommand::ParamSelect(1)` (Mode).
  - `param_buttons` bits 2–6 → `InputCommand::ParamSelect(index)` for respective params.
  - `param_buttons` bit 8 (index 8 = param button 9, loop) → loop in/out/clear cycling logic.
  - `param_buttons` bit 10 (index 10 = param button 11, pause) → pause toggle via `ParamValueDelta` or a direct sequencer call.
  - `param_buttons` bit 11 (index 11 = param button 12, stop/start) → stop/start toggle.
  - `tempo_delta != 0` → adjust `state.tempo_bpm` directly (no InputCommand needed; this is a direct write).
  - `param_knob_delta != 0` → `InputCommand::ParamValueDelta(param_knob_delta)`.
- [ ] After applying commands, acquire read lock on `state`; compute `OutReport` with `led_state` bits set from `steps[i].enabled` for i in 0..16; write via `device.write()`.
- [ ] Send on `ui_notify` channel to wake the UI thread after processing a report.
- [ ] Thread exits cleanly when `cmd_tx` channel is disconnected.
- [ ] Integration test: construct a synthetic `InReport` buffer (encoder delta on step 0, step button 3 press, param button 1 press); call the translation logic; assert the expected `InputCommand` sequence is produced. Verify `OutReport` LED bytes match expected enable state.
- [ ] `cargo test -p engine` passes.

## Interface Contracts

```rust
// engine/src/hid.rs

use std::sync::{Arc, RwLock, mpsc::SyncSender};
use crate::state::SequencerState;
use crate::input::InputCommand;

pub const HID_VID: u16 = 0x2E8A;  // defined in Step 6
pub const HID_PID: u16 = 0x000A;  // defined in Step 6

pub fn run_hid(
    cmd_tx: SyncSender<InputCommand>,
    state: Arc<RwLock<SequencerState>>,
    ui_notify: SyncSender<()>,
);
```

`InReport` (from Step 6):
```rust
pub struct InReport {
    pub report_id: u8,
    pub seq: u8,
    pub flags: u8,
    pub step_buttons: [u8; 2],
    pub step_enable_state: [u8; 2],
    pub param_buttons: [u8; 2],
    pub encoder_deltas: [i8; 16],
    pub tempo_delta: i8,
    pub param_knob_delta: i8,
    pub reserved: [u8; 37],
}
impl InReport {
    pub fn from_bytes(buf: &[u8; 64]) -> InReport;
}
```

`OutReport` (from Step 6):
```rust
pub struct OutReport {
    pub report_id: u8,
    pub seq_echo: u8,
    pub led_state: [u8; 2],
    pub reserved: [u8; 60],
}
impl OutReport {
    pub fn to_bytes(&self) -> [u8; 64];
}
```

`InputCommand` (from Step 6b, `engine/src/input.rs`) — full enum, see step6b task file.

`SequencerState` (from Step 3) — read lock used to compute LED state for OutReport.

## Context

From plan Section 8, Step 7.

Param button to parameter mapping (param_buttons bitmask, 0-indexed from LSB):
- Bit 0 → param button 1 → Key (overlay param index 0)
- Bit 1 → param button 2 → Mode (overlay param index 1)
- Bit 2 → param button 3 → Swing (overlay param index 2)
- Bit 3 → param button 4 → Step Size (overlay param index 3)
- Bits 4–7 → param buttons 5–8 → reserved/future
- Bit 8 → param button 9 → Loop in/out/clear
- Bit 9 → param button 10 → reserved
- Bit 10 → param button 11 → Pause
- Bit 11 → param button 12 → Stop/Start

The LED state in `OutReport` mirrors `steps[i].enabled` directly: bit i of `led_state` = `steps[i].enabled`.

HID connection is optional: the engine is fully functional via keyboard alone (Step 6b). This thread is a peer on the same `SyncSender<InputCommand>` channel.

`hidapi` crate 2.x (wraps libhidapi) is already declared as an engine dependency (added in Step 1).

## Notes

