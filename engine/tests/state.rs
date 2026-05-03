use engine::input::InputCommand;
use engine::state::{MidiEvent, OverlayMode, PendingEdit, SequencerState, StepSize};
use engine::music_theory::{Key, Mode};

fn playing_state_all_enabled() -> SequencerState {
    let mut s = SequencerState::default();
    s.playing = true;
    for step in &mut s.steps {
        step.enabled = true;
    }
    s
}

// --- tick: basic playhead cycle ---

#[test]
fn tick_16_times_cycles_playhead() {
    let mut s = playing_state_all_enabled();
    // Start at playhead=0. After 16 ticks playhead should return to 0.
    // Tick advances THEN reads, so first tick moves 0→1.
    for expected in 1u8..=15 {
        let evt = s.tick();
        assert!(evt.is_some(), "tick should return Some at step {}", expected);
        assert_eq!(s.playhead, expected, "playhead should be {} after tick", expected);
    }
    // 16th tick: 15→0
    let evt = s.tick();
    assert!(evt.is_some());
    assert_eq!(s.playhead, 0, "playhead should wrap to 0 after 16 ticks");
}

#[test]
fn tick_does_not_wrap_before_16() {
    let mut s = playing_state_all_enabled();
    for _ in 0..15 {
        s.tick();
    }
    // After 15 ticks, playhead == 15
    assert_eq!(s.playhead, 15);
    // One more tick wraps to 0
    s.tick();
    assert_eq!(s.playhead, 0);
}

// --- tick: loop mode ---

#[test]
fn tick_loop_wraps_at_loop_out() {
    let mut s = playing_state_all_enabled();
    s.loop_in = 3;
    s.loop_out = 7;
    s.loop_active = true;
    s.playhead = 3; // start inside loop

    // Advance to step 7
    for _ in 0..4 {
        s.tick();
    }
    assert_eq!(s.playhead, 7, "playhead should reach loop_out=7");

    // Next tick must wrap back to loop_in=3
    s.tick();
    assert_eq!(s.playhead, 3, "playhead should wrap to loop_in=3 after loop_out");
}

#[test]
fn tick_loop_stays_in_range() {
    let mut s = playing_state_all_enabled();
    s.loop_in = 3;
    s.loop_out = 7;
    s.loop_active = true;
    s.playhead = 3;

    // Run for 20 ticks; playhead must always be in [3, 7]
    for _ in 0..20 {
        s.tick();
        assert!(
            s.playhead >= 3 && s.playhead <= 7,
            "playhead {} out of loop range [3,7]",
            s.playhead
        );
    }
}

// --- tick: disabled steps ---

#[test]
fn tick_disabled_step_returns_none() {
    let mut s = SequencerState::default();
    s.playing = true;
    // All steps disabled (default). First tick advances to step 1.
    let evt = s.tick();
    assert!(evt.is_none(), "disabled step should return None");
}

#[test]
fn tick_mixed_enabled_disabled() {
    let mut s = SequencerState::default();
    s.playing = true;
    // Enable only step 2 (0-indexed).
    s.steps[2].enabled = true;

    // Tick 1: playhead → 1, disabled → None
    assert!(s.tick().is_none());
    // Tick 2: playhead → 2, enabled → Some
    assert!(s.tick().is_some());
    // Tick 3: playhead → 3, disabled → None
    assert!(s.tick().is_none());
}

// --- tick: not playing / paused ---

#[test]
fn tick_returns_none_when_not_playing() {
    let mut s = SequencerState::default();
    s.playing = false;
    let evt = s.tick();
    assert!(evt.is_none());
    assert_eq!(s.playhead, 0, "playhead must not advance when not playing");
}

#[test]
fn tick_returns_none_when_paused() {
    let mut s = SequencerState::default();
    s.playing = true;
    s.paused = true;
    let evt = s.tick();
    assert!(evt.is_none());
    assert_eq!(s.playhead, 0, "playhead must not advance when paused");
}

// --- toggle_step ---

#[test]
fn toggle_step_enables_then_disables() {
    let mut s = SequencerState::default();
    assert!(!s.steps[5].enabled);
    s.toggle_step(5);
    assert!(s.steps[5].enabled, "step should be enabled after first toggle");
    s.toggle_step(5);
    assert!(!s.steps[5].enabled, "step should be disabled after second toggle");
}

#[test]
fn toggle_step_out_of_range_is_noop() {
    let mut s = SequencerState::default();
    s.toggle_step(16); // no panic, no effect
    s.toggle_step(100);
}

// --- apply_encoder_delta ---

#[test]
fn apply_encoder_delta_increases_note() {
    let mut s = SequencerState::default();
    // Default midi_note = 60 (C4), C Major.
    // Delta +1 → D4 = 62.
    s.apply_encoder_delta(0, 1);
    assert_eq!(s.steps[0].midi_note, 62, "C4+1 in C Major should be D4=62");
}

#[test]
fn apply_encoder_delta_decreases_note() {
    let mut s = SequencerState::default();
    // C4=60, delta -1 → B3=59.
    s.apply_encoder_delta(0, -1);
    assert_eq!(s.steps[0].midi_note, 59, "C4-1 in C Major should be B3=59");
}

#[test]
fn apply_encoder_delta_out_of_range_is_noop() {
    let mut s = SequencerState::default();
    let before = s.steps[0].midi_note;
    s.apply_encoder_delta(16, 1);
    assert_eq!(s.steps[0].midi_note, before, "out-of-range step must not change note");
}

// --- Default values ---

#[test]
fn default_state_is_sane() {
    let s = SequencerState::default();
    assert_eq!(s.tempo_bpm, 120);
    assert_eq!(s.swing, 0);
    assert!(!s.playing);
    assert!(!s.paused);
    assert!(!s.loop_active);
    assert_eq!(s.playhead, 0);
    assert!(matches!(s.step_size, StepSize::Sixteenth));
    assert!(matches!(s.key, Key::C));
    assert!(matches!(s.mode, Mode::Major));
    assert!(matches!(s.pending_edit, PendingEdit::None));
    assert!(s.active_overlay.is_none());
    assert_eq!(s.selected_step, 0);
    assert_eq!(s.selected_param, 0);
    for step in &s.steps {
        assert!(!step.enabled);
    }
}

// --- tick: all 16 steps visited ---

#[test]
fn tick_all_16_steps_enabled_visits_every_step() {
    let mut s = playing_state_all_enabled();
    // Playhead starts at 0. Tick 16 times and collect every playhead position.
    let mut visited = [false; 16];
    for _ in 0..16 {
        s.tick();
        visited[s.playhead as usize] = true;
    }
    for i in 0..16 {
        assert!(visited[i], "step {} was never visited", i);
    }
    // After exactly 16 ticks, playhead must be back at 0.
    assert_eq!(s.playhead, 0, "playhead should wrap back to 0 after 16 ticks");
}

// --- tick: loop boundary edge cases ---

#[test]
fn tick_loop_full_range_loop_in0_loop_out15() {
    let mut s = playing_state_all_enabled();
    s.loop_in = 0;
    s.loop_out = 15;
    s.loop_active = true;
    s.playhead = 0;

    // With full range loop the behavior should be identical to no-loop.
    // After 15 ticks, playhead == 15; one more tick wraps back to loop_in=0.
    for _ in 0..15 {
        s.tick();
    }
    assert_eq!(s.playhead, 15);
    s.tick();
    assert_eq!(s.playhead, 0, "full-range loop should wrap to 0 after step 15");
}

#[test]
fn tick_loop_single_step_loop_in7_loop_out7() {
    let mut s = playing_state_all_enabled();
    s.loop_in = 7;
    s.loop_out = 7;
    s.loop_active = true;
    s.playhead = 7; // already at the only step in the loop

    // Every tick must stay at step 7.
    for i in 0..10 {
        s.tick();
        assert_eq!(s.playhead, 7, "single-step loop should stay at 7 (tick {})", i);
    }
}

#[test]
fn tick_loop_inverted_loop_in3_loop_out2() {
    // Inverted loop: loop_in > loop_out. Current implementation advances
    // through steps 3..N until next > loop_out(2), which immediately wraps
    // back to loop_in=3. This means only step 3 is ever reached from step 3
    // (tick advances by 1, lands on 4 > 2, wraps to 3 immediately).
    // The test documents the current behavior so regressions are caught.
    let mut s = playing_state_all_enabled();
    s.loop_in = 3;
    s.loop_out = 2;
    s.loop_active = true;
    s.playhead = 3;

    // From playhead=3: next=4 > loop_out=2, so wraps to loop_in=3.
    s.tick();
    assert_eq!(
        s.playhead, 3,
        "inverted loop: playhead should wrap back to loop_in=3 immediately"
    );

    // Behavior is stable across multiple ticks.
    for _ in 0..5 {
        s.tick();
        assert_eq!(s.playhead, 3, "inverted loop should remain stuck at loop_in=3");
    }
}

// --- apply_encoder_delta: additional edge cases ---

#[test]
fn apply_encoder_delta_zero_is_noop() {
    let mut s = SequencerState::default();
    let before = s.steps[0].midi_note;
    s.apply_encoder_delta(0, 0);
    assert_eq!(s.steps[0].midi_note, before, "delta=0 must not change the note");
}

#[test]
fn apply_encoder_delta_large_positive_wraps_octave() {
    let mut s = SequencerState::default();
    // C4=60 in C Major. 7 scale degrees = 1 octave.
    // Delta=7 should land on C5=72.
    s.apply_encoder_delta(0, 7);
    assert_eq!(s.steps[0].midi_note, 72, "delta=7 in C Major should be C5=72");
}

#[test]
fn apply_encoder_delta_large_negative_clamps_at_zero() {
    let mut s = SequencerState::default();
    // Start at C4=60. Shift note down to a very low value first.
    s.steps[0].midi_note = 2; // near bottom
    // A very large negative delta should clamp at 0, not underflow.
    s.apply_encoder_delta(0, -100);
    assert_eq!(s.steps[0].midi_note, 0, "large negative delta should clamp at MIDI 0");
}

// --- toggle_step: double-toggle identity ---

#[test]
fn toggle_step_double_toggle_returns_to_original() {
    let mut s = SequencerState::default();
    let original = s.steps[4].enabled;
    s.toggle_step(4);
    s.toggle_step(4);
    assert_eq!(
        s.steps[4].enabled, original,
        "double-toggle must return to original state"
    );
}

// --- Default state: exhaustive field check ---

#[test]
fn default_state_all_fields_match_spec() {
    let s = SequencerState::default();
    assert_eq!(s.tempo_bpm, 120, "default tempo_bpm should be 120");
    assert_eq!(s.swing, 0, "default swing should be 0");
    assert!(matches!(s.key, Key::C), "default key should be C");
    assert!(matches!(s.mode, Mode::Major), "default mode should be Major");
    assert!(matches!(s.step_size, StepSize::Sixteenth), "default step_size should be Sixteenth");
    assert_eq!(s.loop_in, 0, "default loop_in should be 0");
    assert_eq!(s.loop_out, 15, "default loop_out should be 15");
    assert!(!s.loop_active, "default loop_active should be false");
    assert_eq!(s.playhead, 0, "default playhead should be 0");
    assert!(!s.playing, "default playing should be false");
    assert!(!s.paused, "default paused should be false");
    assert!(matches!(s.pending_edit, PendingEdit::None), "default pending_edit should be None");
    assert!(s.active_overlay.is_none(), "default active_overlay should be None");
    assert_eq!(s.selected_step, 0, "default selected_step should be 0");
    assert_eq!(s.selected_param, 0, "default selected_param should be 0");
    for (i, step) in s.steps.iter().enumerate() {
        assert!(!step.enabled, "default step {} should be disabled", i);
    }
}

// --- MidiEvent content ---

#[test]
fn tick_note_on_has_correct_fields() {
    let mut s = SequencerState::default();
    s.playing = true;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 72;
    s.steps[1].velocity = 80;

    s.tick(); // move to step 1
    // step 1 is enabled with note 72 and velocity 80
    // (playhead was at 0, so first tick moves to 1)
    let evt = {
        // Reset and re-tick cleanly
        s.playhead = 0;
        s.tick()
    };
    assert_eq!(
        evt,
        Some(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 80, duration_nanos: 0 })
    );
}

// --- apply_command ---

#[test]
fn apply_command_step_select_sets_selected_step() {
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::StepSelect(7));
    assert_eq!(s.selected_step, 7);
}

#[test]
fn apply_command_step_select_clamps_to_15() {
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::StepSelect(20));
    assert_eq!(s.selected_step, 15);
}

#[test]
fn apply_command_step_select_clears_pending_note_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Note { step: 0, midi_note: 64 };
    s.apply_command(InputCommand::StepSelect(3));
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_step_select_clears_pending_velocity_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Velocity { step: 0, velocity: 80 };
    s.apply_command(InputCommand::StepSelect(3));
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_step_select_does_not_clear_param_edit() {
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.pending_edit = PendingEdit::Param { overlay: OverlayMode::Regular, index: 2, value: 5 };
    s.apply_command(InputCommand::StepSelect(3));
    assert!(matches!(s.pending_edit, PendingEdit::Param { .. }));
}

#[test]
fn apply_command_step_select_delta_wraps_at_15() {
    let mut s = SequencerState::default();
    s.selected_step = 15;
    s.apply_command(InputCommand::StepSelectDelta(1));
    assert_eq!(s.selected_step, 0, "wrapping past 15 should give 0");
}

#[test]
fn apply_command_step_select_delta_wraps_at_0() {
    let mut s = SequencerState::default();
    s.selected_step = 0;
    s.apply_command(InputCommand::StepSelectDelta(-1));
    assert_eq!(s.selected_step, 15, "wrapping below 0 should give 15");
}

#[test]
fn apply_command_step_select_delta_advances_normally() {
    let mut s = SequencerState::default();
    s.selected_step = 5;
    s.apply_command(InputCommand::StepSelectDelta(3));
    assert_eq!(s.selected_step, 8);
}

#[test]
fn apply_command_note_delta_sets_pending_note_edit() {
    let mut s = SequencerState::default();
    s.selected_step = 2;
    // default midi_note = 60 (C4), C Major. Scale-degree delta=+1 → D4=62.
    s.apply_command(InputCommand::NoteDelta(1));
    assert!(matches!(s.pending_edit, PendingEdit::Note { step: 2, midi_note: 62 }));
}

#[test]
fn apply_command_note_delta_negative_clamps_at_0() {
    let mut s = SequencerState::default();
    s.steps[0].midi_note = 0;
    s.apply_command(InputCommand::NoteDelta(-5));
    assert!(matches!(s.pending_edit, PendingEdit::Note { step: 0, midi_note: 0 }));
}

#[test]
fn apply_command_note_delta_positive_clamps_at_127() {
    let mut s = SequencerState::default();
    s.steps[0].midi_note = 127;
    s.apply_command(InputCommand::NoteDelta(10));
    assert!(matches!(s.pending_edit, PendingEdit::Note { step: 0, midi_note: 127 }));
}

#[test]
fn apply_command_confirm_commits_note_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Note { step: 3, midi_note: 72 };
    s.apply_command(InputCommand::Confirm);
    assert_eq!(s.steps[3].midi_note, 72);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_confirm_with_no_pending_is_noop() {
    let mut s = SequencerState::default();
    let note_before = s.steps[0].midi_note;
    s.apply_command(InputCommand::Confirm);
    assert_eq!(s.steps[0].midi_note, note_before);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_confirm_commits_velocity_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Velocity { step: 1, velocity: 90 };
    s.apply_command(InputCommand::Confirm);
    assert_eq!(s.steps[1].velocity, 90);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_confirm_clears_param_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Param { overlay: OverlayMode::Regular, index: 0, value: 3 };
    s.apply_command(InputCommand::Confirm);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_toggle_step_toggles_selected_step() {
    let mut s = SequencerState::default();
    s.selected_step = 4;
    assert!(!s.steps[4].enabled);
    s.apply_command(InputCommand::ToggleStep);
    assert!(s.steps[4].enabled);
    s.apply_command(InputCommand::ToggleStep);
    assert!(!s.steps[4].enabled);
}

#[test]
fn apply_command_velocity_delta_sets_pending_velocity_edit() {
    let mut s = SequencerState::default();
    s.selected_step = 5;
    // default velocity = 100
    s.apply_command(InputCommand::VelocityDelta(10));
    assert!(matches!(s.pending_edit, PendingEdit::Velocity { step: 5, velocity: 110 }));
}

#[test]
fn apply_command_velocity_delta_clamps_at_127() {
    let mut s = SequencerState::default();
    s.steps[0].velocity = 127;
    s.apply_command(InputCommand::VelocityDelta(50));
    assert!(matches!(s.pending_edit, PendingEdit::Velocity { step: 0, velocity: 127 }));
}

#[test]
fn apply_command_velocity_delta_clamps_at_0() {
    let mut s = SequencerState::default();
    s.steps[0].velocity = 0;
    s.apply_command(InputCommand::VelocityDelta(-5));
    assert!(matches!(s.pending_edit, PendingEdit::Velocity { step: 0, velocity: 0 }));
}

#[test]
fn apply_command_open_overlay_sets_active_overlay() {
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::OpenOverlay(OverlayMode::Regular));
    assert_eq!(s.active_overlay, Some(OverlayMode::Regular));
    s.apply_command(InputCommand::OpenOverlay(OverlayMode::Shift));
    assert_eq!(s.active_overlay, Some(OverlayMode::Shift));
}

#[test]
fn apply_command_close_overlay_clears_active_overlay() {
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.apply_command(InputCommand::CloseOverlay);
    assert!(s.active_overlay.is_none());
}

#[test]
fn apply_command_close_overlay_discards_param_edit() {
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.pending_edit = PendingEdit::Param { overlay: OverlayMode::Regular, index: 0, value: 7 };
    s.apply_command(InputCommand::CloseOverlay);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_param_select_sets_selected_param() {
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::ParamSelect(3));
    assert_eq!(s.selected_param, 3);
}

#[test]
fn apply_command_param_select_delta_wraps_at_7() {
    // Now 8 params (0–7): index 7 + 1 wraps to 0.
    let mut s = SequencerState::default();
    s.selected_param = 7;
    s.apply_command(InputCommand::ParamSelectDelta(1));
    assert_eq!(s.selected_param, 0, "param wraps past 7 to 0");
}

#[test]
fn apply_command_param_select_delta_wraps_at_0() {
    // Now 8 params (0–7): index 0 - 1 wraps to 7.
    let mut s = SequencerState::default();
    s.selected_param = 0;
    s.apply_command(InputCommand::ParamSelectDelta(-1));
    assert_eq!(s.selected_param, 7, "param wraps below 0 to 7");
}

#[test]
fn apply_command_param_value_delta_sets_pending_param_edit() {
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 2;
    s.apply_command(InputCommand::ParamValueDelta(5));
    assert!(matches!(
        s.pending_edit,
        PendingEdit::Param { overlay: OverlayMode::Regular, index: 2, value: 5 }
    ));
}

#[test]
fn apply_command_param_value_delta_accumulates() {
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 2;
    s.apply_command(InputCommand::ParamValueDelta(5));
    s.apply_command(InputCommand::ParamValueDelta(3));
    assert!(matches!(
        s.pending_edit,
        PendingEdit::Param { overlay: OverlayMode::Regular, index: 2, value: 8 }
    ));
}

#[test]
fn apply_command_param_value_delta_no_overlay_is_noop() {
    let mut s = SequencerState::default();
    // No overlay open
    s.apply_command(InputCommand::ParamValueDelta(5));
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn default_state_has_selected_step_0() {
    let s = SequencerState::default();
    assert_eq!(s.selected_step, 0);
}

#[test]
fn default_state_has_selected_param_0() {
    let s = SequencerState::default();
    assert_eq!(s.selected_param, 0);
}

// --- apply_command: boundary conditions not yet covered ---

#[test]
fn apply_command_param_select_clamps_at_6() {
    // ParamSelect(n) should clamp n to the valid range 0–7 (8 params total).
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::ParamSelect(10));
    assert_eq!(s.selected_param, 7, "ParamSelect(10) should clamp to 7");
}

#[test]
fn apply_command_close_overlay_with_no_pending_is_noop() {
    // CloseOverlay with PendingEdit::None should leave pending_edit as None.
    let mut s = SequencerState::default();
    assert!(matches!(s.pending_edit, PendingEdit::None));
    s.apply_command(InputCommand::CloseOverlay);
    assert!(matches!(s.pending_edit, PendingEdit::None));
    assert!(s.active_overlay.is_none());
}

#[test]
fn apply_command_step_select_delta_with_no_pending_leaves_pending_none() {
    // StepSelectDelta when pending_edit is None should not change pending_edit.
    let mut s = SequencerState::default();
    s.selected_step = 5;
    s.apply_command(InputCommand::StepSelectDelta(1));
    assert_eq!(s.selected_step, 6);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_confirm_with_pending_velocity_commits_to_correct_step() {
    // Confirm with PendingEdit::Velocity commits the velocity to the exact step
    // referenced in the edit, not the currently selected step.
    let mut s = SequencerState::default();
    s.selected_step = 0;
    // Simulate: user moved to step 3, set velocity, then moved back to step 0.
    s.pending_edit = PendingEdit::Velocity { step: 3, velocity: 64 };
    s.apply_command(InputCommand::Confirm);
    assert_eq!(s.steps[3].velocity, 64, "velocity committed to step 3");
    // Other steps must be untouched.
    assert_eq!(s.steps[0].velocity, 100, "step 0 velocity must be default");
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_confirm_param_clears_pending_edit() {
    // Confirm with PendingEdit::Param clears the pending edit.
    // Actual param application is deferred to Step 7; here we verify
    // the pending edit is cleared so it does not accumulate.
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.pending_edit = PendingEdit::Param { overlay: OverlayMode::Regular, index: 4, value: -3 };
    s.apply_command(InputCommand::Confirm);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

// --- tick: velocity round-trips ---

#[test]
fn tick_note_on_uses_step_velocity_64() {
    // A step with velocity=64 must produce NoteOn with velocity=64 (not the
    // former hardcoded 100).
    let mut s = SequencerState::default();
    s.playing = true;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 60;
    s.steps[1].velocity = 64;
    s.playhead = 0; // next tick advances to 1
    let evt = s.tick();
    assert_eq!(
        evt,
        Some(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 64, duration_nanos: 0 }),
        "NoteOn velocity must match step.velocity=64"
    );
}

#[test]
fn tick_note_on_uses_step_velocity_1() {
    // A step with velocity=1 (near minimum) must produce NoteOn with velocity=1.
    let mut s = SequencerState::default();
    s.playing = true;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 48;
    s.steps[1].velocity = 1;
    s.playhead = 0;
    let evt = s.tick();
    assert_eq!(
        evt,
        Some(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 1, duration_nanos: 0 }),
        "NoteOn velocity must match step.velocity=1"
    );
}

// --- apply_command: step selection ---

#[test]
fn apply_command_step_select_clamps_at_15() {
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::StepSelect(99));
    assert_eq!(s.selected_step, 15, "StepSelect out of range should clamp to 15");
}

#[test]
fn apply_command_step_select_delta_wraps() {
    let mut s = SequencerState::default();
    s.selected_step = 0;
    s.apply_command(InputCommand::StepSelectDelta(-1));
    assert_eq!(s.selected_step, 15, "StepSelectDelta(-1) from 0 should wrap to 15");
}

#[test]
fn apply_command_toggle_step_toggles_selected() {
    let mut s = SequencerState::default();
    s.selected_step = 3;
    assert!(!s.steps[3].enabled);
    s.apply_command(InputCommand::ToggleStep);
    assert!(s.steps[3].enabled, "ToggleStep should enable the selected step");
}

#[test]
fn apply_command_note_delta_creates_pending_edit() {
    let mut s = SequencerState::default();
    s.selected_step = 0;
    // default midi_note = 60 (C4), C Major. Scale-degree delta=+2 → E4=64 (C→D→E).
    s.apply_command(InputCommand::NoteDelta(2));
    assert!(matches!(s.pending_edit, PendingEdit::Note { step: 0, midi_note: 64 }));
}

#[test]
fn apply_command_confirm_commits_pending_note() {
    let mut s = SequencerState::default();
    s.selected_step = 0;
    s.apply_command(InputCommand::NoteDelta(5));
    let pending = match s.pending_edit {
        PendingEdit::Note { midi_note, .. } => midi_note,
        _ => panic!("expected PendingEdit::Note"),
    };
    s.apply_command(InputCommand::Confirm);
    assert_eq!(s.steps[0].midi_note, pending, "Confirm should commit pending note");
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_open_close_overlay() {
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::OpenOverlay(OverlayMode::Regular));
    assert_eq!(s.active_overlay, Some(OverlayMode::Regular));
    s.apply_command(InputCommand::CloseOverlay);
    assert!(s.active_overlay.is_none());
}
