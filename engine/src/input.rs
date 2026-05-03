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
    /// Toggle playback on/off. Stopping also clears the paused flag.
    PlayStop,
    /// Apply a semitone offset to all steps' notes.
    /// 0 clears the modifier. Range: -96..=96 (actual semitones).
    NoteModifierSet(i8),
    /// Toggle per-step skip modifier on/off.
    SkipModifierToggle,
    /// Set velocity offset modifier (0 = off). Range: -127..=127.
    VelocityModifierSet(i8),
    /// Randomise all 16 step notes within the current key/mode.
    GenerateRandomSequence,
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
        KeyCodeSimple::Char('p') | KeyCodeSimple::Char('P') => Some(InputCommand::PlayStop),
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

/// Pure function: translate a key event to a Shift overlay action command.
///
/// Called only when the Shift overlay is active. Returns `None` for keys that
/// are not Shift actions (caller falls through to `overlay_key_to_command`).
pub fn shift_action_key_to_command(key_code: KeyCodeSimple) -> Option<InputCommand> {
    match key_code {
        KeyCodeSimple::Char('s') | KeyCodeSimple::Char('S') => {
            Some(InputCommand::SkipModifierToggle)
        }
        KeyCodeSimple::Char('g') | KeyCodeSimple::Char('G') => {
            Some(InputCommand::GenerateRandomSequence)
        }
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
    /// A printable character key.
    Char(char),
    /// Any other key (ignored).
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── shift_action_key_to_command ──────────────────────────────────────────

    #[test]
    fn shift_action_s_lower_maps_to_skip_modifier_toggle() {
        let cmd = shift_action_key_to_command(KeyCodeSimple::Char('s'));
        assert!(matches!(cmd, Some(InputCommand::SkipModifierToggle)));
    }

    #[test]
    fn shift_action_s_upper_maps_to_skip_modifier_toggle() {
        let cmd = shift_action_key_to_command(KeyCodeSimple::Char('S'));
        assert!(matches!(cmd, Some(InputCommand::SkipModifierToggle)));
    }

    #[test]
    fn shift_action_g_lower_maps_to_generate_random_sequence() {
        let cmd = shift_action_key_to_command(KeyCodeSimple::Char('g'));
        assert!(matches!(cmd, Some(InputCommand::GenerateRandomSequence)));
    }

    #[test]
    fn shift_action_g_upper_maps_to_generate_random_sequence() {
        let cmd = shift_action_key_to_command(KeyCodeSimple::Char('G'));
        assert!(matches!(cmd, Some(InputCommand::GenerateRandomSequence)));
    }

    #[test]
    fn shift_action_arrow_keys_return_none() {
        assert!(shift_action_key_to_command(KeyCodeSimple::Left).is_none());
        assert!(shift_action_key_to_command(KeyCodeSimple::Right).is_none());
        assert!(shift_action_key_to_command(KeyCodeSimple::Up).is_none());
        assert!(shift_action_key_to_command(KeyCodeSimple::Down).is_none());
    }

    #[test]
    fn shift_action_enter_esc_return_none() {
        assert!(shift_action_key_to_command(KeyCodeSimple::Enter).is_none());
        assert!(shift_action_key_to_command(KeyCodeSimple::Esc).is_none());
    }

    #[test]
    fn shift_action_other_chars_return_none() {
        assert!(shift_action_key_to_command(KeyCodeSimple::Char('p')).is_none());
        assert!(shift_action_key_to_command(KeyCodeSimple::Char('x')).is_none());
        assert!(shift_action_key_to_command(KeyCodeSimple::Other).is_none());
    }

    // ── overlay_key_to_command fallthrough still works ───────────────────────

    #[test]
    fn overlay_key_arrows_still_work() {
        assert!(matches!(
            overlay_key_to_command(KeyCodeSimple::Left),
            Some(InputCommand::ParamSelectDelta(-1))
        ));
        assert!(matches!(
            overlay_key_to_command(KeyCodeSimple::Esc),
            Some(InputCommand::CloseOverlay)
        ));
    }
}

