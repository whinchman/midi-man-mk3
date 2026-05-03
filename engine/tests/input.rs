use engine::input::{
    overlay_key_to_command, panel_key_to_command, root_key_to_command, FocusPanel, InputCommand,
    KeyCodeSimple,
};

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
fn root_f1_sets_focus_sequencer() {
    let cmd = root_key_to_command(KeyCodeSimple::F1, false);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::Sequencer))
    ));
}

#[test]
fn root_f2_sets_focus_seq_params() {
    let cmd = root_key_to_command(KeyCodeSimple::F2, false);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::SeqParams))
    ));
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

// --- root_key_to_command: F3, F4, Plus, Minus ---

#[test]
fn root_f3_sets_focus_rand_params() {
    let cmd = root_key_to_command(KeyCodeSimple::F3, false);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::RandParams))
    ));
}

#[test]
fn root_f4_sets_focus_cli() {
    let cmd = root_key_to_command(KeyCodeSimple::F4, false);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::Cli))
    ));
}

#[test]
fn root_plus_is_bpm_delta_positive_one() {
    let cmd = root_key_to_command(KeyCodeSimple::Plus, false);
    assert!(matches!(cmd, Some(InputCommand::BpmDelta(1))));
}

#[test]
fn root_minus_is_bpm_delta_negative_one() {
    let cmd = root_key_to_command(KeyCodeSimple::Minus, false);
    assert!(matches!(cmd, Some(InputCommand::BpmDelta(-1))));
}

// --- root_key_to_command: focus keys with shift held ---

#[test]
fn root_f1_shift_sets_focus_sequencer() {
    let cmd = root_key_to_command(KeyCodeSimple::F1, true);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::Sequencer))
    ));
}

#[test]
fn root_f2_shift_sets_focus_seq_params() {
    let cmd = root_key_to_command(KeyCodeSimple::F2, true);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::SeqParams))
    ));
}

#[test]
fn root_f3_shift_sets_focus_rand_params() {
    let cmd = root_key_to_command(KeyCodeSimple::F3, true);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::RandParams))
    ));
}

#[test]
fn root_f4_shift_sets_focus_cli() {
    let cmd = root_key_to_command(KeyCodeSimple::F4, true);
    assert!(matches!(
        cmd,
        Some(InputCommand::SetFocus(FocusPanel::Cli))
    ));
}

// --- panel_key_to_command ---

#[test]
fn panel_sequencer_left_is_step_select_delta_minus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Left, FocusPanel::Sequencer);
    assert!(matches!(cmd, Some(InputCommand::StepSelectDelta(-1))));
}

#[test]
fn panel_sequencer_right_is_step_select_delta_plus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Right, FocusPanel::Sequencer);
    assert!(matches!(cmd, Some(InputCommand::StepSelectDelta(1))));
}

#[test]
fn panel_sequencer_up_is_note_delta_plus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Up, FocusPanel::Sequencer);
    assert!(matches!(cmd, Some(InputCommand::NoteDelta(1))));
}

#[test]
fn panel_sequencer_down_is_note_delta_minus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Down, FocusPanel::Sequencer);
    assert!(matches!(cmd, Some(InputCommand::NoteDelta(-1))));
}

#[test]
fn panel_sequencer_space_is_toggle_step() {
    let cmd = panel_key_to_command(KeyCodeSimple::Space, FocusPanel::Sequencer);
    assert!(matches!(cmd, Some(InputCommand::ToggleStep)));
}

#[test]
fn panel_sequencer_enter_is_confirm() {
    let cmd = panel_key_to_command(KeyCodeSimple::Enter, FocusPanel::Sequencer);
    assert!(matches!(cmd, Some(InputCommand::Confirm)));
}

#[test]
fn panel_sequencer_esc_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Esc, FocusPanel::Sequencer).is_none());
}

#[test]
fn panel_sequencer_plus_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Plus, FocusPanel::Sequencer).is_none());
}

#[test]
fn panel_sequencer_minus_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Minus, FocusPanel::Sequencer).is_none());
}

#[test]
fn panel_sequencer_backspace_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Backspace, FocusPanel::Sequencer).is_none());
}

#[test]
fn panel_seq_params_up_is_panel_param_delta_plus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Up, FocusPanel::SeqParams);
    assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(1))));
}

#[test]
fn panel_seq_params_down_is_panel_param_delta_minus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Down, FocusPanel::SeqParams);
    assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(-1))));
}

#[test]
fn panel_seq_params_left_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Left, FocusPanel::SeqParams).is_none());
}

#[test]
fn panel_seq_params_right_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Right, FocusPanel::SeqParams).is_none());
}

#[test]
fn panel_seq_params_esc_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Esc, FocusPanel::SeqParams).is_none());
}

#[test]
fn panel_seq_params_space_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Space, FocusPanel::SeqParams).is_none());
}

#[test]
fn panel_rand_params_up_is_panel_param_delta_plus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Up, FocusPanel::RandParams);
    assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(1))));
}

#[test]
fn panel_rand_params_down_is_panel_param_delta_minus_1() {
    let cmd = panel_key_to_command(KeyCodeSimple::Down, FocusPanel::RandParams);
    assert!(matches!(cmd, Some(InputCommand::PanelParamDelta(-1))));
}

#[test]
fn panel_rand_params_left_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Left, FocusPanel::RandParams).is_none());
}

#[test]
fn panel_rand_params_right_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Right, FocusPanel::RandParams).is_none());
}

#[test]
fn panel_rand_params_esc_returns_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Esc, FocusPanel::RandParams).is_none());
}

#[test]
fn panel_cli_all_keys_return_none() {
    assert!(panel_key_to_command(KeyCodeSimple::Left, FocusPanel::Cli).is_none());
    assert!(panel_key_to_command(KeyCodeSimple::Right, FocusPanel::Cli).is_none());
    assert!(panel_key_to_command(KeyCodeSimple::Up, FocusPanel::Cli).is_none());
    assert!(panel_key_to_command(KeyCodeSimple::Down, FocusPanel::Cli).is_none());
    assert!(panel_key_to_command(KeyCodeSimple::Enter, FocusPanel::Cli).is_none());
    assert!(panel_key_to_command(KeyCodeSimple::Space, FocusPanel::Cli).is_none());
    assert!(panel_key_to_command(KeyCodeSimple::Esc, FocusPanel::Cli).is_none());
    assert!(panel_key_to_command(KeyCodeSimple::Backspace, FocusPanel::Cli).is_none());
}

// --- KeyCodeSimple::Backspace ---

#[test]
fn backspace_key_returns_none_from_all_panels() {
    let key = KeyCodeSimple::Backspace;
    assert!(panel_key_to_command(key, FocusPanel::Sequencer).is_none());
    assert!(panel_key_to_command(key, FocusPanel::SeqParams).is_none());
    assert!(panel_key_to_command(key, FocusPanel::RandParams).is_none());
    assert!(panel_key_to_command(key, FocusPanel::Cli).is_none());
}

// --- FocusPanel derives ---

#[test]
fn focus_panel_clone_produces_equal_value() {
    let a = FocusPanel::SeqParams;
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn focus_panel_copy_allows_use_after_pass_to_function() {
    let panel = FocusPanel::RandParams;
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
fn focus_panel_partial_eq_same_variant() {
    assert_eq!(FocusPanel::Sequencer, FocusPanel::Sequencer);
    assert_eq!(FocusPanel::SeqParams, FocusPanel::SeqParams);
    assert_eq!(FocusPanel::RandParams, FocusPanel::RandParams);
    assert_eq!(FocusPanel::Cli, FocusPanel::Cli);
}

#[test]
fn focus_panel_partial_eq_different_variants() {
    assert_ne!(FocusPanel::Sequencer, FocusPanel::SeqParams);
    assert_ne!(FocusPanel::SeqParams, FocusPanel::RandParams);
    assert_ne!(FocusPanel::RandParams, FocusPanel::Cli);
    assert_ne!(FocusPanel::Cli, FocusPanel::Sequencer);
}

// --- PanelParamSelect and PanelParamDelta round-trip ---

#[test]
fn panel_param_select_roundtrip_preserves_index() {
    let cmd = InputCommand::PanelParamSelect(5);
    assert!(matches!(cmd.clone(), InputCommand::PanelParamSelect(5)));
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
