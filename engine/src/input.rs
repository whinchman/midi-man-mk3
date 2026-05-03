//! Input command abstraction — the single type flowing from all input sources into state.
//!
//! Both the keyboard handler (`ui.rs`) and the HID reader (Step 7) produce
//! `InputCommand` values on a shared `SyncSender<InputCommand>`.  State
//! mutation is handled exclusively in `SequencerState::apply_command`.

/// The overlay mode active when an F1/F2 overlay is open.
///
/// Canonical definition — `state.rs` imports this instead of defining its own stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    /// Normal (non-shift) overlay — F1.
    Regular,
    /// Shift overlay — F2; secondary functions active.
    Shift,
}

/// Every user action that can mutate sequencer state.
///
/// Produced by the keyboard loop (`ui.rs`) and the HID thread (Step 7).
/// Consumed by `SequencerState::apply_command`.
#[derive(Clone, Debug)]
pub enum InputCommand {
    /// Jump to an absolute step index (0–15).
    StepSelect(usize),
    /// Move the selected step by a signed delta; wraps modulo 16.
    StepSelectDelta(i8),
    /// Advance the MIDI note for the selected step by `delta` scale degrees
    /// (using the current key and mode, not raw semitones).
    NoteDelta(i8),
    /// Commit the pending edit to live state; no-op if no edit is pending.
    Confirm,
    /// Toggle the enabled state of the currently selected step.
    ToggleStep,
    /// Adjust the velocity for the selected step by `delta`.
    VelocityDelta(i8),
    /// Open the F1 (Regular) or F2 (Shift) overlay.
    OpenOverlay(OverlayMode),
    /// Close the active overlay and discard any pending param edit.
    CloseOverlay,
    /// Jump to an absolute param index.
    ParamSelect(u8),
    /// Move the selected param by a signed delta.
    ParamSelectDelta(i8),
    /// Adjust the value of the currently selected param by `delta`.
    ParamValueDelta(i8),
}

/// Pure function: translate a root-mode key event into an `InputCommand`.
///
/// Separated from crossterm so it can be unit-tested without the hw-io feature.
/// `shift` is true when the Shift modifier is held.
/// Returns `None` for unmapped keys.
pub fn root_key_to_command(
    key_code: KeyCodeSimple,
    shift: bool,
) -> Option<InputCommand> {
    match key_code {
        KeyCodeSimple::Left => Some(InputCommand::StepSelectDelta(-1)),
        KeyCodeSimple::Right => Some(InputCommand::StepSelectDelta(1)),
        KeyCodeSimple::Up if shift => Some(InputCommand::VelocityDelta(1)),
        KeyCodeSimple::Down if shift => Some(InputCommand::VelocityDelta(-1)),
        KeyCodeSimple::Up => Some(InputCommand::NoteDelta(1)),
        KeyCodeSimple::Down => Some(InputCommand::NoteDelta(-1)),
        KeyCodeSimple::Space => Some(InputCommand::ToggleStep),
        KeyCodeSimple::Enter => Some(InputCommand::Confirm),
        KeyCodeSimple::F1 => Some(InputCommand::OpenOverlay(OverlayMode::Regular)),
        KeyCodeSimple::F2 => Some(InputCommand::OpenOverlay(OverlayMode::Shift)),
        _ => None,
    }
}

/// Pure function: translate an overlay-mode key event into an `InputCommand`.
///
/// Returns `None` for unmapped keys.
pub fn overlay_key_to_command(key_code: KeyCodeSimple) -> Option<InputCommand> {
    match key_code {
        KeyCodeSimple::Left => Some(InputCommand::ParamSelectDelta(-1)),
        KeyCodeSimple::Right => Some(InputCommand::ParamSelectDelta(1)),
        KeyCodeSimple::Up => Some(InputCommand::ParamValueDelta(1)),
        KeyCodeSimple::Down => Some(InputCommand::ParamValueDelta(-1)),
        KeyCodeSimple::Enter => Some(InputCommand::Confirm),
        KeyCodeSimple::Esc => Some(InputCommand::CloseOverlay),
        _ => None,
    }
}

/// A minimal key code enum that mirrors the subset of crossterm keys we care
/// about.  This lives in `input.rs` (no feature gate) so the translation
/// functions can be unit-tested without the `hw-io` feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCodeSimple {
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Space bar.
    Space,
    /// Enter / Return.
    Enter,
    /// Escape.
    Esc,
    /// F1.
    F1,
    /// F2.
    F2,
    /// Any other key (ignored).
    Other,
}

