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
    /// Adjust the MIDI note for the selected step by `delta` semitones.
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- root_key_to_command ---

    #[test]
    fn root_left_arrow_is_step_select_delta_minus_1() {
        let cmd = root_key_to_command(KeyCodeSimple::Left, false);
        assert!(matches!(cmd, Some(InputCommand::StepSelectDelta(-1))));
    }

    #[test]
    fn root_right_arrow_is_step_select_delta_plus_1() {
        let cmd = root_key_to_command(KeyCodeSimple::Right, false);
        assert!(matches!(cmd, Some(InputCommand::StepSelectDelta(1))));
    }

    #[test]
    fn root_up_arrow_no_shift_is_note_delta_plus_1() {
        let cmd = root_key_to_command(KeyCodeSimple::Up, false);
        assert!(matches!(cmd, Some(InputCommand::NoteDelta(1))));
    }

    #[test]
    fn root_down_arrow_no_shift_is_note_delta_minus_1() {
        let cmd = root_key_to_command(KeyCodeSimple::Down, false);
        assert!(matches!(cmd, Some(InputCommand::NoteDelta(-1))));
    }

    #[test]
    fn root_up_arrow_with_shift_is_velocity_delta_plus_1() {
        let cmd = root_key_to_command(KeyCodeSimple::Up, true);
        assert!(matches!(cmd, Some(InputCommand::VelocityDelta(1))));
    }

    #[test]
    fn root_down_arrow_with_shift_is_velocity_delta_minus_1() {
        let cmd = root_key_to_command(KeyCodeSimple::Down, true);
        assert!(matches!(cmd, Some(InputCommand::VelocityDelta(-1))));
    }

    #[test]
    fn root_space_is_toggle_step() {
        let cmd = root_key_to_command(KeyCodeSimple::Space, false);
        assert!(matches!(cmd, Some(InputCommand::ToggleStep)));
    }

    #[test]
    fn root_enter_is_confirm() {
        let cmd = root_key_to_command(KeyCodeSimple::Enter, false);
        assert!(matches!(cmd, Some(InputCommand::Confirm)));
    }

    #[test]
    fn root_f1_opens_regular_overlay() {
        let cmd = root_key_to_command(KeyCodeSimple::F1, false);
        assert!(matches!(cmd, Some(InputCommand::OpenOverlay(OverlayMode::Regular))));
    }

    #[test]
    fn root_f2_opens_shift_overlay() {
        let cmd = root_key_to_command(KeyCodeSimple::F2, false);
        assert!(matches!(cmd, Some(InputCommand::OpenOverlay(OverlayMode::Shift))));
    }

    #[test]
    fn root_other_key_returns_none() {
        let cmd = root_key_to_command(KeyCodeSimple::Other, false);
        assert!(cmd.is_none());
    }

    #[test]
    fn root_esc_returns_none_in_root_mode() {
        // Esc is only mapped in overlay mode.
        let cmd = root_key_to_command(KeyCodeSimple::Esc, false);
        assert!(cmd.is_none());
    }

    // --- overlay_key_to_command ---

    #[test]
    fn overlay_left_is_param_select_delta_minus_1() {
        let cmd = overlay_key_to_command(KeyCodeSimple::Left);
        assert!(matches!(cmd, Some(InputCommand::ParamSelectDelta(-1))));
    }

    #[test]
    fn overlay_right_is_param_select_delta_plus_1() {
        let cmd = overlay_key_to_command(KeyCodeSimple::Right);
        assert!(matches!(cmd, Some(InputCommand::ParamSelectDelta(1))));
    }

    #[test]
    fn overlay_up_is_param_value_delta_plus_1() {
        let cmd = overlay_key_to_command(KeyCodeSimple::Up);
        assert!(matches!(cmd, Some(InputCommand::ParamValueDelta(1))));
    }

    #[test]
    fn overlay_down_is_param_value_delta_minus_1() {
        let cmd = overlay_key_to_command(KeyCodeSimple::Down);
        assert!(matches!(cmd, Some(InputCommand::ParamValueDelta(-1))));
    }

    #[test]
    fn overlay_enter_is_confirm() {
        let cmd = overlay_key_to_command(KeyCodeSimple::Enter);
        assert!(matches!(cmd, Some(InputCommand::Confirm)));
    }

    #[test]
    fn overlay_esc_is_close_overlay() {
        let cmd = overlay_key_to_command(KeyCodeSimple::Esc);
        assert!(matches!(cmd, Some(InputCommand::CloseOverlay)));
    }

    #[test]
    fn overlay_other_returns_none() {
        let cmd = overlay_key_to_command(KeyCodeSimple::Other);
        assert!(cmd.is_none());
    }
}
