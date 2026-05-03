use engine::input::{
    overlay_key_to_command, root_key_to_command, FocusPanel, InputCommand, KeyCodeSimple,
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
