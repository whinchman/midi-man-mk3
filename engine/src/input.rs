//! Input command abstraction — the single type flowing from all input sources into state.
//!
//! Both the keyboard handler (`ui.rs`) and the HID reader (Step 7) produce
//! `InputCommand` values on a shared `SyncSender<InputCommand>`.  State
//! mutation is handled exclusively in `SequencerState::apply_command`.

/// The overlay mode active when an F1/F2 overlay is open.
///
/// Canonical definition — `state.rs` imports this instead of defining its own stub.
/// Retained for HID compatibility until Task 3.2 removes its usage from `hid.rs`.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    /// Normal (non-shift) overlay — F1.
    Regular,
    /// Shift overlay — F2; secondary functions active.
    Shift,
}

/// Which panel currently holds keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPanel {
    /// F1 — step select, space, enter.
    Sequencer,
    /// F2 — left/right param select, up/down adjust.
    SeqParams,
    /// F3 — left/right param select, up/down adjust.
    RandParams,
    /// F4 — text input mode.
    Cli,
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
    /// Adjust BPM by signed delta (clamped to 20–300). Always active.
    BpmDelta(i8),
    /// Set the random seed from CLI. Updates both rand_seed and rng_seed.
    SeedSet(u32),
    /// Set MIDI channel (1-indexed input; stored 0-indexed). Sent by CLI handler.
    ChannelSet(u8),
    /// Sync the MIDI device name into state for title bar display.
    MidiDeviceName(String),
    /// Switch keyboard focus to the given panel (F1–F4).
    SetFocus(FocusPanel),
    /// Select a parameter by absolute index within the focused panel.
    PanelParamSelect(u8),
    /// Adjust the selected parameter by a signed delta within the focused panel.
    /// Applies to the F2 (SEQ PARAMS / regular) panel only.
    /// The hardware param knob also emits this variant (no panel context available in HID).
    PanelParamDelta(i8),
    /// F3 (RandParams panel): select rand-param by absolute index (0–7).
    RandParamSelect(u8),
    /// F3 (RandParams panel): adjust selected rand-param by signed delta.
    RandParamDelta(i8),
}

/// Pure function: translate a root-mode key event into an `InputCommand`.
///
/// Separated from crossterm so it can be unit-tested without the hw-io feature.
/// `shift` is true when the Shift modifier is held.
/// Returns `None` for unmapped keys.
pub fn root_key_to_command(key_code: KeyCodeSimple, shift: bool) -> Option<InputCommand> {
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
        KeyCodeSimple::F1 => Some(InputCommand::SetFocus(FocusPanel::Sequencer)),
        KeyCodeSimple::F2 => Some(InputCommand::SetFocus(FocusPanel::SeqParams)),
        KeyCodeSimple::F3 => Some(InputCommand::SetFocus(FocusPanel::RandParams)),
        KeyCodeSimple::F4 => Some(InputCommand::SetFocus(FocusPanel::Cli)),
        KeyCodeSimple::Plus => Some(InputCommand::BpmDelta(1)),
        KeyCodeSimple::Minus => Some(InputCommand::BpmDelta(-1)),
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

/// Translate a key event into an `InputCommand` based on which panel has focus.
///
/// Returns `None` for unmapped keys. `FocusPanel::Cli` always returns `None`
/// because the caller manages text input directly in `UiState`.
pub fn panel_key_to_command(key: KeyCodeSimple, focus: FocusPanel) -> Option<InputCommand> {
    match focus {
        FocusPanel::Sequencer => match key {
            KeyCodeSimple::Left => Some(InputCommand::StepSelectDelta(-1)),
            KeyCodeSimple::Right => Some(InputCommand::StepSelectDelta(1)),
            KeyCodeSimple::Up => Some(InputCommand::NoteDelta(1)),
            KeyCodeSimple::Down => Some(InputCommand::NoteDelta(-1)),
            KeyCodeSimple::Space => Some(InputCommand::ToggleStep),
            KeyCodeSimple::Enter => Some(InputCommand::Confirm),
            _ => None,
        },
        FocusPanel::SeqParams | FocusPanel::RandParams => match key {
            KeyCodeSimple::Up => Some(InputCommand::PanelParamDelta(1)),
            KeyCodeSimple::Down => Some(InputCommand::PanelParamDelta(-1)),
            // Left/Right param navigation is handled by the caller which adjusts
            // the local param index and then sends PanelParamSelect(new_idx).
            _ => None,
        },
        FocusPanel::Cli => None,
    }
}

/// Returns the char that a key inserts into the CLI line, if any.
///
/// Used by `translate_key` (hw-io) and unit tests (no feature gate).
/// Maps `Char(c)` → `Some(c)`, `Space` → `Some(' ')`, `Plus` → `Some('+')`,
/// `Minus` → `Some('-')`, and all other variants → `None`.
pub fn cli_key_to_char(key: KeyCodeSimple) -> Option<char> {
    match key {
        KeyCodeSimple::Char(c) => Some(c),
        KeyCodeSimple::Space => Some(' '),
        KeyCodeSimple::Plus => Some('+'),
        KeyCodeSimple::Minus => Some('-'),
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
    /// F3.
    F3,
    /// F4.
    F4,
    /// `+` or `=` key (BPM up).
    Plus,
    /// `-` key (BPM down).
    Minus,
    /// Backspace key.
    Backspace,
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

    // ── root_key_to_command — focus switching ────────────────────────────────

    #[test]
    fn f1_sets_focus_sequencer() {
        let cmd = root_key_to_command(KeyCodeSimple::F1, false);
        assert!(matches!(
            cmd,
            Some(InputCommand::SetFocus(FocusPanel::Sequencer))
        ));
    }

    #[test]
    fn f2_sets_focus_seq_params() {
        let cmd = root_key_to_command(KeyCodeSimple::F2, false);
        assert!(matches!(
            cmd,
            Some(InputCommand::SetFocus(FocusPanel::SeqParams))
        ));
    }

    #[test]
    fn f3_sets_focus_rand_params() {
        let cmd = root_key_to_command(KeyCodeSimple::F3, false);
        assert!(matches!(
            cmd,
            Some(InputCommand::SetFocus(FocusPanel::RandParams))
        ));
    }

    #[test]
    fn f4_sets_focus_cli() {
        let cmd = root_key_to_command(KeyCodeSimple::F4, false);
        assert!(matches!(cmd, Some(InputCommand::SetFocus(FocusPanel::Cli))));
    }

    #[test]
    fn plus_key_sends_bpm_delta_positive() {
        let cmd = root_key_to_command(KeyCodeSimple::Plus, false);
        assert!(matches!(cmd, Some(InputCommand::BpmDelta(1))));
    }

    #[test]
    fn minus_key_sends_bpm_delta_negative() {
        let cmd = root_key_to_command(KeyCodeSimple::Minus, false);
        assert!(matches!(cmd, Some(InputCommand::BpmDelta(-1))));
    }

    // ── panel_key_to_command ─────────────────────────────────────────────────

    #[test]
    fn sequencer_focus_left_maps_to_step_select_delta_minus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Left, FocusPanel::Sequencer);
        assert!(matches!(cmd, Some(InputCommand::StepSelectDelta(-1))));
    }

    #[test]
    fn sequencer_focus_right_maps_to_step_select_delta_plus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Right, FocusPanel::Sequencer);
        assert!(matches!(cmd, Some(InputCommand::StepSelectDelta(1))));
    }

    #[test]
    fn sequencer_focus_up_maps_to_note_delta_plus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Up, FocusPanel::Sequencer);
        assert!(matches!(cmd, Some(InputCommand::NoteDelta(1))));
    }

    #[test]
    fn sequencer_focus_down_maps_to_note_delta_minus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Down, FocusPanel::Sequencer);
        assert!(matches!(cmd, Some(InputCommand::NoteDelta(-1))));
    }

    #[test]
    fn sequencer_focus_space_maps_to_toggle_step() {
        let cmd = panel_key_to_command(KeyCodeSimple::Space, FocusPanel::Sequencer);
        assert!(matches!(cmd, Some(InputCommand::ToggleStep)));
    }

    #[test]
    fn sequencer_focus_enter_maps_to_confirm() {
        let cmd = panel_key_to_command(KeyCodeSimple::Enter, FocusPanel::Sequencer);
        assert!(matches!(cmd, Some(InputCommand::Confirm)));
    }

    #[test]
    fn seq_params_focus_up_maps_to_panel_param_delta_plus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Up, FocusPanel::SeqParams);
        assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(1))));
    }

    #[test]
    fn seq_params_focus_down_maps_to_panel_param_delta_minus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Down, FocusPanel::SeqParams);
        assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(-1))));
    }

    #[test]
    fn rand_params_focus_up_maps_to_panel_param_delta_plus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Up, FocusPanel::RandParams);
        assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(1))));
    }

    #[test]
    fn rand_params_focus_down_maps_to_panel_param_delta_minus_one() {
        let cmd = panel_key_to_command(KeyCodeSimple::Down, FocusPanel::RandParams);
        assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(-1))));
    }

    #[test]
    fn cli_focus_returns_none_for_all_keys() {
        assert!(panel_key_to_command(KeyCodeSimple::Left, FocusPanel::Cli).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Right, FocusPanel::Cli).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Up, FocusPanel::Cli).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Down, FocusPanel::Cli).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Enter, FocusPanel::Cli).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Space, FocusPanel::Cli).is_none());
    }

    #[test]
    fn seq_params_left_right_return_none_for_caller_to_handle() {
        assert!(panel_key_to_command(KeyCodeSimple::Left, FocusPanel::SeqParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Right, FocusPanel::SeqParams).is_none());
    }

    // ── overlay_key_to_command — retained for HID compatibility ──────────────

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

    // ── panel_key_to_command — unmapped keys return None ─────────────────────

    #[test]
    fn sequencer_focus_unmapped_keys_return_none() {
        assert!(panel_key_to_command(KeyCodeSimple::Esc, FocusPanel::Sequencer).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Plus, FocusPanel::Sequencer).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Minus, FocusPanel::Sequencer).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Backspace, FocusPanel::Sequencer).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Other, FocusPanel::Sequencer).is_none());
    }

    #[test]
    fn seq_params_focus_unmapped_keys_return_none() {
        assert!(panel_key_to_command(KeyCodeSimple::Esc, FocusPanel::SeqParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Space, FocusPanel::SeqParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Enter, FocusPanel::SeqParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Plus, FocusPanel::SeqParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Minus, FocusPanel::SeqParams).is_none());
    }

    #[test]
    fn rand_params_focus_left_right_return_none_for_caller_to_handle() {
        assert!(panel_key_to_command(KeyCodeSimple::Left, FocusPanel::RandParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Right, FocusPanel::RandParams).is_none());
    }

    #[test]
    fn rand_params_focus_unmapped_keys_return_none() {
        assert!(panel_key_to_command(KeyCodeSimple::Esc, FocusPanel::RandParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Space, FocusPanel::RandParams).is_none());
        assert!(panel_key_to_command(KeyCodeSimple::Backspace, FocusPanel::RandParams).is_none());
    }

    // ── KeyCodeSimple::Backspace — present and handled ────────────────────────

    #[test]
    fn backspace_variant_exists_and_returns_none_from_all_panels() {
        // Backspace is in the enum (compile-time guarantee via this use site).
        // panel_key_to_command must return None for every panel.
        let key = KeyCodeSimple::Backspace;
        assert!(panel_key_to_command(key, FocusPanel::Sequencer).is_none());
        assert!(panel_key_to_command(key, FocusPanel::SeqParams).is_none());
        assert!(panel_key_to_command(key, FocusPanel::RandParams).is_none());
        assert!(panel_key_to_command(key, FocusPanel::Cli).is_none());
    }

    // ── root_key_to_command — focus keys with shift held ─────────────────────

    #[test]
    fn f1_with_shift_still_sets_focus_sequencer() {
        let cmd = root_key_to_command(KeyCodeSimple::F1, true);
        assert!(matches!(
            cmd,
            Some(InputCommand::SetFocus(FocusPanel::Sequencer))
        ));
    }

    #[test]
    fn f2_with_shift_still_sets_focus_seq_params() {
        let cmd = root_key_to_command(KeyCodeSimple::F2, true);
        assert!(matches!(
            cmd,
            Some(InputCommand::SetFocus(FocusPanel::SeqParams))
        ));
    }

    #[test]
    fn f3_with_shift_still_sets_focus_rand_params() {
        let cmd = root_key_to_command(KeyCodeSimple::F3, true);
        assert!(matches!(
            cmd,
            Some(InputCommand::SetFocus(FocusPanel::RandParams))
        ));
    }

    #[test]
    fn f4_with_shift_still_sets_focus_cli() {
        let cmd = root_key_to_command(KeyCodeSimple::F4, true);
        assert!(matches!(cmd, Some(InputCommand::SetFocus(FocusPanel::Cli))));
    }

    // ── FocusPanel derives: Clone, Copy, Debug, PartialEq, Eq ────────────────

    #[test]
    fn focus_panel_clone_produces_equal_value() {
        let original = FocusPanel::SeqParams;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn focus_panel_copy_allows_independent_use_after_move_context() {
        let panel = FocusPanel::RandParams;
        // Copy: pass to function and still use the binding afterward.
        let _ = panel_key_to_command(KeyCodeSimple::Up, panel);
        // If FocusPanel were not Copy this would be a compile error.
        let _ = panel_key_to_command(KeyCodeSimple::Down, panel);
    }

    #[test]
    fn focus_panel_debug_format_contains_variant_name() {
        assert!(format!("{:?}", FocusPanel::Sequencer).contains("Sequencer"));
        assert!(format!("{:?}", FocusPanel::SeqParams).contains("SeqParams"));
        assert!(format!("{:?}", FocusPanel::RandParams).contains("RandParams"));
        assert!(format!("{:?}", FocusPanel::Cli).contains("Cli"));
    }

    #[test]
    fn focus_panel_partial_eq_same_variant_is_equal() {
        assert_eq!(FocusPanel::Sequencer, FocusPanel::Sequencer);
        assert_eq!(FocusPanel::SeqParams, FocusPanel::SeqParams);
        assert_eq!(FocusPanel::RandParams, FocusPanel::RandParams);
        assert_eq!(FocusPanel::Cli, FocusPanel::Cli);
    }

    #[test]
    fn focus_panel_partial_eq_different_variants_are_not_equal() {
        assert_ne!(FocusPanel::Sequencer, FocusPanel::SeqParams);
        assert_ne!(FocusPanel::SeqParams, FocusPanel::RandParams);
        assert_ne!(FocusPanel::RandParams, FocusPanel::Cli);
        assert_ne!(FocusPanel::Cli, FocusPanel::Sequencer);
    }

    // ── PanelParamSelect and PanelParamDelta round-trip ──────────────────────

    #[test]
    fn panel_param_select_roundtrip_preserves_index() {
        let cmd = InputCommand::PanelParamSelect(5);
        let cloned = cmd.clone();
        assert!(matches!(cloned, InputCommand::PanelParamSelect(5)));
    }

    #[test]
    fn panel_param_select_zero_index_roundtrip() {
        let cmd = InputCommand::PanelParamSelect(0);
        assert!(matches!(cmd.clone(), InputCommand::PanelParamSelect(0)));
    }

    #[test]
    fn panel_param_select_max_u8_roundtrip() {
        let cmd = InputCommand::PanelParamSelect(u8::MAX);
        assert!(matches!(cmd.clone(), InputCommand::PanelParamSelect(255)));
    }

    #[test]
    fn panel_param_delta_negative_roundtrip() {
        let cmd = InputCommand::PanelParamDelta(-3);
        assert!(matches!(cmd.clone(), InputCommand::PanelParamDelta(-3)));
    }

    #[test]
    fn panel_param_delta_positive_roundtrip() {
        let cmd = InputCommand::PanelParamDelta(1);
        assert!(matches!(cmd.clone(), InputCommand::PanelParamDelta(1)));
    }

    #[test]
    fn panel_param_delta_max_i8_roundtrip() {
        let cmd = InputCommand::PanelParamDelta(i8::MAX);
        assert!(matches!(cmd.clone(), InputCommand::PanelParamDelta(127)));
    }

    #[test]
    fn panel_param_delta_min_i8_roundtrip() {
        let cmd = InputCommand::PanelParamDelta(i8::MIN);
        assert!(matches!(cmd.clone(), InputCommand::PanelParamDelta(-128)));
    }

    // ── cli_key_to_char ───────────────────────────────────────────────────────

    #[test]
    fn cli_key_to_char_returns_char_for_lowercase_p() {
        assert_eq!(cli_key_to_char(KeyCodeSimple::Char('p')), Some('p'));
    }

    #[test]
    fn cli_key_to_char_returns_space_for_space() {
        assert_eq!(cli_key_to_char(KeyCodeSimple::Space), Some(' '));
    }

    #[test]
    fn cli_key_to_char_returns_plus() {
        assert_eq!(cli_key_to_char(KeyCodeSimple::Plus), Some('+'));
    }

    #[test]
    fn cli_key_to_char_returns_minus() {
        assert_eq!(cli_key_to_char(KeyCodeSimple::Minus), Some('-'));
    }

    #[test]
    fn cli_key_to_char_returns_none_for_enter() {
        assert_eq!(cli_key_to_char(KeyCodeSimple::Enter), None);
    }

    #[test]
    fn cli_key_to_char_returns_none_for_up() {
        assert_eq!(cli_key_to_char(KeyCodeSimple::Up), None);
    }
}
