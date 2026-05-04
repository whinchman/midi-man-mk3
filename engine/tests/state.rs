use engine::input::InputCommand;
use engine::music_theory::{snap_to_key, Key, Mode};
use engine::state::{
    next_rand, prob_hit, MidiEvent, OverlayMode, PendingEdit, SequencerState, StepSize,
    TempoRandType, TempoRollPoint,
};

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
        assert!(
            evt.is_some(),
            "tick should return Some at step {}",
            expected
        );
        assert_eq!(
            s.playhead, expected,
            "playhead should be {} after tick",
            expected
        );
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
    assert_eq!(
        s.playhead, 3,
        "playhead should wrap to loop_in=3 after loop_out"
    );
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
    assert!(
        s.steps[5].enabled,
        "step should be enabled after first toggle"
    );
    s.toggle_step(5);
    assert!(
        !s.steps[5].enabled,
        "step should be disabled after second toggle"
    );
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
    assert_eq!(
        s.steps[0].midi_note, before,
        "out-of-range step must not change note"
    );
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
    assert_eq!(
        s.playhead, 0,
        "playhead should wrap back to 0 after 16 ticks"
    );
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
    assert_eq!(
        s.playhead, 0,
        "full-range loop should wrap to 0 after step 15"
    );
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
        assert_eq!(
            s.playhead, 7,
            "single-step loop should stay at 7 (tick {})",
            i
        );
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
        assert_eq!(
            s.playhead, 3,
            "inverted loop should remain stuck at loop_in=3"
        );
    }
}

// --- apply_encoder_delta: additional edge cases ---

#[test]
fn apply_encoder_delta_zero_is_noop() {
    let mut s = SequencerState::default();
    let before = s.steps[0].midi_note;
    s.apply_encoder_delta(0, 0);
    assert_eq!(
        s.steps[0].midi_note, before,
        "delta=0 must not change the note"
    );
}

#[test]
fn apply_encoder_delta_large_positive_wraps_octave() {
    let mut s = SequencerState::default();
    // C4=60 in C Major. 7 scale degrees = 1 octave.
    // Delta=7 should land on C5=72.
    s.apply_encoder_delta(0, 7);
    assert_eq!(
        s.steps[0].midi_note, 72,
        "delta=7 in C Major should be C5=72"
    );
}

#[test]
fn apply_encoder_delta_large_negative_clamps_at_zero() {
    let mut s = SequencerState::default();
    // Start at C4=60. Shift note down to a very low value first.
    s.steps[0].midi_note = 2; // near bottom
                              // A very large negative delta should clamp at 0, not underflow.
    s.apply_encoder_delta(0, -100);
    assert_eq!(
        s.steps[0].midi_note, 0,
        "large negative delta should clamp at MIDI 0"
    );
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
    assert!(
        matches!(s.mode, Mode::Major),
        "default mode should be Major"
    );
    assert!(
        matches!(s.step_size, StepSize::Sixteenth),
        "default step_size should be Sixteenth"
    );
    assert_eq!(s.loop_in, 0, "default loop_in should be 0");
    assert_eq!(s.loop_out, 15, "default loop_out should be 15");
    assert!(!s.loop_active, "default loop_active should be false");
    assert_eq!(s.playhead, 0, "default playhead should be 0");
    assert!(!s.playing, "default playing should be false");
    assert!(!s.paused, "default paused should be false");
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "default pending_edit should be None"
    );
    assert!(
        s.active_overlay.is_none(),
        "default active_overlay should be None"
    );
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
        Some(MidiEvent::NoteOn {
            channel: 0,
            note: 72,
            velocity: 80,
            duration_nanos: 0
        })
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
    s.pending_edit = PendingEdit::Note {
        step: 0,
        midi_note: 64,
    };
    s.apply_command(InputCommand::StepSelect(3));
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_step_select_clears_pending_velocity_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Velocity {
        step: 0,
        velocity: 80,
    };
    s.apply_command(InputCommand::StepSelect(3));
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_step_select_does_not_clear_param_edit() {
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 2,
        value: 5,
    };
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
fn note_delta_applies_immediately_writes_to_step_note() {
    // BUG-035: NoteDelta now applies immediately; no PendingEdit::Note is created.
    let mut s = SequencerState::default();
    s.selected_step = 2;
    // default midi_note = 60 (C4), C Major. Scale-degree delta=+1 → D4=62.
    s.apply_command(InputCommand::NoteDelta(1));
    assert_eq!(
        s.steps[2].midi_note, 62,
        "NoteDelta(1) must write D4=62 directly to steps[2].midi_note"
    );
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "NoteDelta must not create a PendingEdit::Note"
    );
}

#[test]
fn apply_command_note_delta_negative_clamps_at_0() {
    // BUG-035: NoteDelta applies immediately; result is clamped to 0.
    let mut s = SequencerState::default();
    s.steps[0].midi_note = 0;
    s.apply_command(InputCommand::NoteDelta(-5));
    assert_eq!(
        s.steps[0].midi_note, 0,
        "NoteDelta(-5) from note 0 must clamp at 0"
    );
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "NoteDelta must not create a PendingEdit::Note"
    );
}

#[test]
fn apply_command_note_delta_positive_clamps_at_127() {
    // BUG-035: NoteDelta applies immediately; result is clamped to 127.
    let mut s = SequencerState::default();
    s.steps[0].midi_note = 127;
    s.apply_command(InputCommand::NoteDelta(10));
    assert_eq!(
        s.steps[0].midi_note, 127,
        "NoteDelta(10) from note 127 must clamp at 127"
    );
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "NoteDelta must not create a PendingEdit::Note"
    );
}

#[test]
fn apply_command_confirm_commits_note_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Note {
        step: 3,
        midi_note: 72,
    };
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
    s.pending_edit = PendingEdit::Velocity {
        step: 1,
        velocity: 90,
    };
    s.apply_command(InputCommand::Confirm);
    assert_eq!(s.steps[1].velocity, 90);
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn apply_command_confirm_clears_param_edit() {
    let mut s = SequencerState::default();
    s.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 3,
    };
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
    assert!(matches!(
        s.pending_edit,
        PendingEdit::Velocity {
            step: 5,
            velocity: 110
        }
    ));
}

#[test]
fn apply_command_velocity_delta_clamps_at_127() {
    let mut s = SequencerState::default();
    s.steps[0].velocity = 127;
    s.apply_command(InputCommand::VelocityDelta(50));
    assert!(matches!(
        s.pending_edit,
        PendingEdit::Velocity {
            step: 0,
            velocity: 127
        }
    ));
}

#[test]
fn apply_command_velocity_delta_clamps_at_0() {
    let mut s = SequencerState::default();
    s.steps[0].velocity = 0;
    s.apply_command(InputCommand::VelocityDelta(-5));
    assert!(matches!(
        s.pending_edit,
        PendingEdit::Velocity {
            step: 0,
            velocity: 0
        }
    ));
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
    s.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 7,
    };
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
        PendingEdit::Param {
            overlay: OverlayMode::Regular,
            index: 2,
            value: 5
        }
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
        PendingEdit::Param {
            overlay: OverlayMode::Regular,
            index: 2,
            value: 8
        }
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
    s.pending_edit = PendingEdit::Velocity {
        step: 3,
        velocity: 64,
    };
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
    s.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 4,
        value: -3,
    };
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
        Some(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 64,
            duration_nanos: 0
        }),
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
        Some(MidiEvent::NoteOn {
            channel: 0,
            note: 48,
            velocity: 1,
            duration_nanos: 0
        }),
        "NoteOn velocity must match step.velocity=1"
    );
}

// --- apply_command: step selection ---

#[test]
fn apply_command_step_select_clamps_at_15() {
    let mut s = SequencerState::default();
    s.apply_command(InputCommand::StepSelect(99));
    assert_eq!(
        s.selected_step, 15,
        "StepSelect out of range should clamp to 15"
    );
}

#[test]
fn apply_command_step_select_delta_wraps() {
    let mut s = SequencerState::default();
    s.selected_step = 0;
    s.apply_command(InputCommand::StepSelectDelta(-1));
    assert_eq!(
        s.selected_step, 15,
        "StepSelectDelta(-1) from 0 should wrap to 15"
    );
}

#[test]
fn apply_command_toggle_step_toggles_selected() {
    let mut s = SequencerState::default();
    s.selected_step = 3;
    assert!(!s.steps[3].enabled);
    s.apply_command(InputCommand::ToggleStep);
    assert!(
        s.steps[3].enabled,
        "ToggleStep should enable the selected step"
    );
}

#[test]
fn note_delta_applies_immediately_to_step_midi_note() {
    // BUG-035: NoteDelta now applies immediately; the step note is written directly.
    let mut s = SequencerState::default();
    s.selected_step = 0;
    // default midi_note = 60 (C4), C Major. Scale-degree delta=+2 → E4=64 (C→D→E).
    s.apply_command(InputCommand::NoteDelta(2));
    assert_eq!(
        s.steps[0].midi_note, 64,
        "NoteDelta(2) from C4 in C Major must write E4=64 directly"
    );
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "NoteDelta must not create a PendingEdit::Note"
    );
}

#[test]
fn apply_command_confirm_commits_pending_note() {
    // BUG-035: NoteDelta now applies immediately; the note is visible right away
    // without a Confirm. Confirm on an empty pending_edit is a no-op.
    let mut s = SequencerState::default();
    s.selected_step = 0;
    s.apply_command(InputCommand::NoteDelta(5));
    // Note must already be written — no pending edit remains.
    let applied_note = s.steps[0].midi_note;
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "NoteDelta must not leave a PendingEdit::Note"
    );
    // Confirm is a no-op; note must remain the same.
    s.apply_command(InputCommand::Confirm);
    assert_eq!(
        s.steps[0].midi_note, applied_note,
        "Confirm after immediate NoteDelta must not change the note"
    );
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

// ── BUG-010: NoteDelta accumulates across repeated presses ────────────────────

#[test]
fn note_delta_up_five_times_advances_five_scale_degrees() {
    // BUG-035: NoteDelta applies immediately. Repeated presses accumulate because
    // each press reads from the already-updated steps[step].midi_note.
    let mut s = SequencerState::default();
    s.selected_step = 0;
    // default: C4=60, C Major. After 5 ups should be A4=69 (C→D→E→F→G→A).
    for _ in 0..5 {
        s.apply_command(InputCommand::NoteDelta(1));
    }
    assert_eq!(
        s.steps[0].midi_note, 69,
        "5× NoteDelta(1) from C4 in C Major should reach A4=69"
    );
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "no PendingEdit should remain after immediate applies"
    );
}

#[test]
fn note_delta_accumulates_then_confirm_commits_final_value() {
    // BUG-010: Confirm after accumulated presses should commit the final note.
    let mut s = SequencerState::default();
    s.selected_step = 0;
    // C4=60. 7 ups in C Major wraps to C5=72.
    for _ in 0..7 {
        s.apply_command(InputCommand::NoteDelta(1));
    }
    s.apply_command(InputCommand::Confirm);
    assert_eq!(
        s.steps[0].midi_note, 72,
        "Confirm after 7 NoteDelta(1) from C4 should write C5=72"
    );
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

// ── BUG-011: ParamValueDelta seeds from committed state value ─────────────────

#[test]
fn param_value_delta_key_seeds_from_committed_key_index() {
    // BUG-011: When state.key=D (index 2) and we press Up, pending value should
    // be 3 (D#), not 1 (would be 0+1 if seeded from 0).
    let mut s = SequencerState::default();
    s.key = Key::D; // index 2
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 0; // Key param
    s.apply_command(InputCommand::ParamValueDelta(1));
    match s.pending_edit {
        PendingEdit::Param {
            index: 0, value, ..
        } => {
            assert_eq!(value, 3, "D(2)+1 should give index 3 (D#), not 1");
        }
        other => panic!("expected PendingEdit::Param index 0, got {:?}", other),
    }
}

#[test]
fn param_value_delta_swing_seeds_from_committed_swing_value() {
    // BUG-011: When state.swing=20 and delta=-5, pending should be 15, not -5.
    let mut s = SequencerState::default();
    s.swing = 20;
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 2; // Swing param
    s.apply_command(InputCommand::ParamValueDelta(-5));
    match s.pending_edit {
        PendingEdit::Param {
            index: 2, value, ..
        } => {
            assert_eq!(value, 15, "swing(20)+(-5) should give 15, not -5");
        }
        other => panic!("expected PendingEdit::Param index 2, got {:?}", other),
    }
}

// ── BUG-012: Confirm applies pending param change to state field ───────────────

#[test]
fn confirm_param_key_applies_to_state_key() {
    // BUG-012: Confirming a key param edit must update state.key.
    let mut s = SequencerState::default();
    assert!(matches!(s.key, Key::C));
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 0;
    // Press Up 3 times from C(0): C→C#→D→D# (index 3).
    for _ in 0..3 {
        s.apply_command(InputCommand::ParamValueDelta(1));
    }
    s.apply_command(InputCommand::Confirm);
    assert!(
        matches!(s.key, Key::Ds),
        "key should be D# after confirming +3 from C"
    );
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_swing_applies_to_state_swing() {
    // BUG-012: Confirming a swing param edit must update state.swing.
    let mut s = SequencerState::default();
    assert_eq!(s.swing, 0);
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 2;
    s.apply_command(InputCommand::ParamValueDelta(15));
    s.apply_command(InputCommand::Confirm);
    assert_eq!(
        s.swing, 15,
        "swing should be 15 after confirming +15 from 0"
    );
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_mode_applies_to_state_mode() {
    // BUG-012: Confirming a mode param edit must update state.mode.
    let mut s = SequencerState::default();
    assert!(matches!(s.mode, Mode::Major)); // index 0
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 1;
    s.apply_command(InputCommand::ParamValueDelta(2)); // Major(0) + 2 = Dorian(2)
    s.apply_command(InputCommand::Confirm);
    assert!(
        matches!(s.mode, Mode::Dorian),
        "mode should be Dorian after confirming +2 from Major"
    );
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_step_size_applies_to_state() {
    // BUG-012: Confirming a step_size param edit must update state.step_size.
    let mut s = SequencerState::default();
    assert!(matches!(s.step_size, StepSize::Sixteenth)); // index 4
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 3;
    s.apply_command(InputCommand::ParamValueDelta(1)); // Sixteenth(4) + 1 = ThirtySecond(5)
    s.apply_command(InputCommand::Confirm);
    assert!(
        matches!(s.step_size, StepSize::ThirtySecond),
        "step_size should be ThirtySecond after +1"
    );
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_loop_in_applies_to_state() {
    // BUG-012: Confirming a loop_in param edit (index 4) must update state.loop_in.
    let mut s = SequencerState::default();
    assert_eq!(s.loop_in, 0, "default loop_in should be 0");
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 4; // Loop param (loop_in)
    s.apply_command(InputCommand::ParamValueDelta(5)); // 0 + 5 = 5
    s.apply_command(InputCommand::Confirm);
    assert_eq!(
        s.loop_in, 5,
        "loop_in should be 5 after confirming +5 from 0"
    );
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_loop_in_clamps_at_15() {
    // BUG-012: loop_in is clamped to 0..=15 by clamped_param_value.
    let mut s = SequencerState::default();
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 4;
    s.apply_command(InputCommand::ParamValueDelta(20)); // 0 + 20 → clamped to 15
    s.apply_command(InputCommand::Confirm);
    assert_eq!(s.loop_in, 15, "loop_in should clamp at 15 for delta=20");
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_paused_applies_to_state() {
    // BUG-012: Confirming a paused param edit (index 6) must update state.paused.
    // Param mapping: 0=Key,1=Mode,2=Swing,3=StepSize,4=loop_in,5=loop_out,
    //                6=paused,7=playing.
    let mut s = SequencerState::default();
    assert!(!s.paused, "default paused should be false");
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 6; // Pause param
    s.apply_command(InputCommand::ParamValueDelta(1)); // 0 + 1 = 1 (paused=true)
    s.apply_command(InputCommand::Confirm);
    assert!(s.paused, "paused should be true after confirming value=1");
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_paused_false_applies_to_state() {
    // BUG-012: Confirming paused=false (index 6, value 0) turns off paused.
    let mut s = SequencerState::default();
    s.paused = true; // start paused
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 6;
    // committed value = 1 (paused). Delta -1 → 0 (clamped to [0,1]).
    s.apply_command(InputCommand::ParamValueDelta(-1));
    s.apply_command(InputCommand::Confirm);
    assert!(!s.paused, "paused should be false after confirming value=0");
    assert!(matches!(s.pending_edit, PendingEdit::None));
}

#[test]
fn confirm_param_playing_while_paused_leaves_tick_non_firing() {
    // apply_param_value(7, 1) sets playing=true AND clears paused (BUG-017 fix).
    // When both playing=true and paused=false after confirm, tick() fires.
    // Param mapping: index 7 = playing.
    let mut s = SequencerState::default();
    s.paused = true;
    s.playing = false;
    s.steps[1].enabled = true;
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 7; // Stop/Start param
                          // committed value = 0 (not playing). Delta +1 → value=1 (playing=true).
    s.apply_command(InputCommand::ParamValueDelta(1));
    s.apply_command(InputCommand::Confirm);
    // After confirm: playing=true was applied and BUG-017 fix clears paused.
    assert!(s.playing, "playing should be true after confirming value=1");
    // paused was cleared by the BUG-017 fix in apply_param_value(7, 1).
    assert!(
        !s.paused,
        "paused should be cleared when playing is set via overlay (BUG-017)"
    );
}

// ── BUG-004: tick() uses step.velocity, not hardcoded 100 ────────────────────

#[test]
fn tick_velocity_127_produces_note_on_127() {
    // tick() must pass step.velocity=127 (maximum) through to NoteOn.
    let mut s = SequencerState::default();
    s.playing = true;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 60;
    s.steps[1].velocity = 127;
    s.playhead = 0;
    let evt = s.tick();
    assert_eq!(
        evt,
        Some(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 127,
            duration_nanos: 0
        }),
        "NoteOn velocity must be 127 when step.velocity=127"
    );
}

#[test]
fn tick_velocity_default_100_produces_note_on_100() {
    // tick() must pass step.velocity=100 (default) through to NoteOn.
    let mut s = SequencerState::default();
    s.playing = true;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 60;
    // velocity is default 100
    s.playhead = 0;
    let evt = s.tick();
    assert_eq!(
        evt,
        Some(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
            duration_nanos: 0
        }),
        "NoteOn velocity must be 100 when step.velocity is default"
    );
}

#[test]
fn tick_velocity_zero_produces_note_on_0() {
    // tick() must pass step.velocity=0 through to NoteOn (not substitute 100).
    let mut s = SequencerState::default();
    s.playing = true;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 60;
    s.steps[1].velocity = 0;
    s.playhead = 0;
    let evt = s.tick();
    assert_eq!(
        evt,
        Some(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 0,
            duration_nanos: 0
        }),
        "NoteOn velocity must be 0 when step.velocity=0"
    );
}

#[test]
fn tick_velocity_preserved_after_velocity_edit_committed() {
    // After committing a VelocityDelta via Confirm, tick() uses the new velocity.
    let mut s = SequencerState::default();
    s.playing = true;
    s.selected_step = 1;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 60;
    // Default velocity=100. Set to 55 via apply_command path.
    s.apply_command(InputCommand::VelocityDelta(-45)); // 100 - 45 = 55
    s.apply_command(InputCommand::Confirm);
    assert_eq!(
        s.steps[1].velocity, 55,
        "velocity should be 55 after commit"
    );
    s.playhead = 0;
    let evt = s.tick();
    assert_eq!(
        evt,
        Some(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 55,
            duration_nanos: 0
        }),
        "tick() must use committed velocity=55"
    );
}

// ── BUG-010: additional NoteDelta accumulation edge cases ─────────────────────

#[test]
fn note_delta_repeated_uses_committed_note_as_base() {
    // BUG-035: NoteDelta applies immediately. Each subsequent delta uses the
    // already-committed note as the base, so +1 then -1 returns to the start.
    let mut s = SequencerState::default();
    s.selected_step = 0;
    // step: C4=60
    s.apply_command(InputCommand::NoteDelta(1)); // C→D, committed=62
    s.apply_command(InputCommand::NoteDelta(-1)); // D→C, committed=60
    assert_eq!(
        s.steps[0].midi_note, 60,
        "NoteDelta(-1) after +1 should return to C4=60"
    );
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "no PendingEdit should remain after immediate applies"
    );
}

#[test]
fn note_delta_resets_base_when_step_changes() {
    // BUG-035: NoteDelta applies immediately. After changing selected_step,
    // a new NoteDelta uses the committed note of the newly-selected step.
    let mut s = SequencerState::default();
    s.steps[0].midi_note = 60; // C4
    s.steps[3].midi_note = 67; // G4
    s.selected_step = 0;
    s.apply_command(InputCommand::NoteDelta(3)); // applies to step 0
    s.apply_command(InputCommand::StepSelect(3)); // moves focus to step 3, clears any pending
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "StepSelect must clear pending note"
    );
    // Now delta on step 3 must use step 3's committed note (G4=67).
    s.apply_command(InputCommand::NoteDelta(1)); // G4 → next scale degree
    // G4=67 in C Major, +1 = A4=69
    assert_eq!(
        s.steps[3].midi_note, 69,
        "NoteDelta(1) from G4=67 in C Major should give A4=69"
    );
    assert!(
        matches!(s.pending_edit, PendingEdit::None),
        "no PendingEdit should remain after immediate apply"
    );
}

// ── BUG-011: ParamValueDelta additional seeding tests ────────────────────────

#[test]
fn param_value_delta_mode_seeds_from_committed_mode_index() {
    // BUG-011: When state.mode=Dorian (index 2) and delta=+1, pending should be 3 (Phrygian).
    let mut s = SequencerState::default();
    s.mode = Mode::Dorian; // index 2
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 1; // Mode param
    s.apply_command(InputCommand::ParamValueDelta(1));
    match s.pending_edit {
        PendingEdit::Param {
            index: 1, value, ..
        } => {
            assert_eq!(value, 3, "Dorian(2)+1 should give index 3 (Phrygian)");
        }
        other => panic!("expected PendingEdit::Param index 1, got {:?}", other),
    }
}

#[test]
fn param_value_delta_step_size_seeds_from_committed_value() {
    // BUG-011: When state.step_size=Eighth (index 3) and delta=+1, pending should be 4 (Sixteenth).
    let mut s = SequencerState::default();
    s.step_size = StepSize::Eighth; // index 3
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 3; // StepSize param
    s.apply_command(InputCommand::ParamValueDelta(1));
    match s.pending_edit {
        PendingEdit::Param {
            index: 3, value, ..
        } => {
            assert_eq!(value, 4, "Eighth(3)+1 should give index 4 (Sixteenth)");
        }
        other => panic!("expected PendingEdit::Param index 3, got {:?}", other),
    }
}

#[test]
fn param_value_delta_loop_in_seeds_from_committed_loop_in() {
    // BUG-011: When state.loop_in=8 and delta=+2, pending should be 10 (not 2).
    let mut s = SequencerState::default();
    s.loop_in = 8;
    s.active_overlay = Some(OverlayMode::Regular);
    s.selected_param = 4; // Loop param
    s.apply_command(InputCommand::ParamValueDelta(2));
    match s.pending_edit {
        PendingEdit::Param {
            index: 4, value, ..
        } => {
            assert_eq!(value, 10, "loop_in(8)+2 should give 10, not 2");
        }
        other => panic!("expected PendingEdit::Param index 4, got {:?}", other),
    }
}

// ── BUG-014: loop_out edit path ──────────────────────────────────────────────

#[test]
fn test_loop_out_edit_path_via_overlay() {
    let mut state = SequencerState::default();
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 5,
        value: 10,
    };
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.loop_out, 10);
    assert_eq!(state.pending_edit, PendingEdit::None);
}

#[test]
fn test_committed_param_value_loop_out() {
    let mut state = SequencerState::default();
    state.loop_out = 12;
    assert_eq!(state.committed_param_value(5), 12);
}

#[test]
fn test_param_select_delta_wraps_at_8() {
    let mut state = SequencerState::default();
    state.selected_param = 7;
    state.apply_command(InputCommand::ParamSelectDelta(1));
    assert_eq!(state.selected_param, 0);
}

// ── BUG-017: overlay confirm playing=true clears paused ─────────────────────

#[test]
fn test_confirm_playing_clears_paused() {
    let mut state = SequencerState::default();
    state.paused = true;
    state.playing = false;
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 7,
        value: 1,
    };
    state.apply_command(InputCommand::Confirm);
    assert!(state.playing, "playing should be true after confirm");
    assert!(
        !state.paused,
        "paused should be cleared when playing is set via overlay"
    );
}

#[test]
fn test_confirm_playing_false_does_not_clear_paused() {
    let mut state = SequencerState::default();
    state.paused = true;
    state.playing = true;
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 7,
        value: 0,
    };
    state.apply_command(InputCommand::Confirm);
    assert!(!state.playing);
    assert!(
        state.paused,
        "paused should be unchanged when playing is set to false"
    );
}

// ── Key/Mode Note Shifting ───────────────────────────────────────────────────

#[test]
fn test_key_change_snaps_all_steps() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    state.steps[0].midi_note = 61; // C#4 — not in C major
    state.steps[1].midi_note = 62; // D4
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 1, // Key::Cs
    };
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.key, engine::music_theory::Key::Cs);
    // C#4 (61) is the root of C# major → stays 61
    assert_eq!(state.steps[0].midi_note, 61);
}

#[test]
fn test_mode_change_snaps_all_steps() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
                                               // B4 (71) is in C major. C NaturalMinor scale: C D Eb F G Ab Bb.
                                               // Bb=70 (dist=1), C5=72 (dist=1) — tie: lower wins → Bb4=70
    state.steps[0].midi_note = 71; // B4
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 1,
        value: 1, // Mode::NaturalMinor
    };
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.mode, engine::music_theory::Mode::NaturalMinor);
    assert_eq!(state.steps[0].midi_note, 70); // snapped to Bb4
}

#[test]
fn test_same_key_no_snap() {
    let mut state = SequencerState::default(); // Key::C
    state.steps[0].midi_note = 61; // C#4 — out of key, set directly
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 0, // Key::C — same as current
    };
    state.apply_command(InputCommand::Confirm);
    // No-op guard must fire; note must NOT be snapped
    assert_eq!(state.steps[0].midi_note, 61);
}

#[test]
fn test_same_mode_no_snap() {
    let mut state = SequencerState::default(); // Mode::Major
    state.steps[0].midi_note = 61; // C#4 — out of C major, set directly
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 1,
        value: 0, // Mode::Major — same as current
    };
    state.apply_command(InputCommand::Confirm);
    // No-op guard must fire; note must NOT be snapped
    assert_eq!(state.steps[0].midi_note, 61);
}

#[test]
fn test_snap_all_16_steps() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    for step in state.steps.iter_mut() {
        step.midi_note = 61; // C#4 — not in C major
    }
    // Change to D major
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 2, // Key::D
    };
    state.apply_command(InputCommand::Confirm);
    for step in &state.steps {
        let expected = snap_to_key(
            61,
            engine::music_theory::Key::D,
            engine::music_theory::Mode::Major,
        );
        assert_eq!(step.midi_note, expected);
    }
}

#[test]
fn test_disabled_steps_are_snapped() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    state.steps[3].enabled = false;
    state.steps[3].midi_note = 61; // C#4 — not in C major
                                   // Change to D major
    state.pending_edit = PendingEdit::Param {
        overlay: OverlayMode::Regular,
        index: 0,
        value: 2, // Key::D
    };
    state.apply_command(InputCommand::Confirm);
    let expected = snap_to_key(
        61,
        engine::music_theory::Key::D,
        engine::music_theory::Mode::Major,
    );
    assert_eq!(
        state.steps[3].midi_note, expected,
        "disabled step should still be snapped"
    );
}

// ── RNG Infrastructure ───────────────────────────────────────────────────────

#[test]
fn test_prob_hit_zero_always_false() {
    let mut seed = 0x853C_49E6_748F_EA9Bu64;
    for _ in 0..1000 {
        assert!(
            !prob_hit(&mut seed, 0),
            "prob_hit(0) must always return false"
        );
    }
}

#[test]
fn test_prob_hit_hundred_always_true() {
    let mut seed = 0x853C_49E6_748F_EA9Bu64;
    for _ in 0..1000 {
        assert!(
            prob_hit(&mut seed, 100),
            "prob_hit(100) must always return true"
        );
    }
}

#[test]
fn test_prob_hit_fifty_percent_statistical() {
    let mut seed = 0x853C_49E6_748F_EA9Bu64;
    let mut hits: u32 = 0;
    let n = 10_000u32;
    for _ in 0..n {
        if prob_hit(&mut seed, 50) {
            hits += 1;
        }
    }
    let ratio = hits as f64 / n as f64;
    assert!(
        ratio >= 0.45 && ratio <= 0.55,
        "prob_hit(50) hit rate {ratio:.4} outside [0.45, 0.55]"
    );
}

#[test]
fn test_rng_seed_advances_every_tick_even_when_not_playing() {
    let mut state = SequencerState::default();
    assert!(!state.playing, "default state should not be playing");
    let seed_before = state.rng_seed;
    state.tick();
    assert_ne!(
        state.rng_seed, seed_before,
        "rng_seed must advance on tick() even when not playing"
    );
}

#[test]
fn test_rng_seed_default_value() {
    let state = SequencerState::default();
    assert_eq!(state.rng_seed, 0x853C_49E6_748F_EA9B);
}

#[test]
fn test_sequencer_state_is_clone() {
    let state = SequencerState::default();
    let cloned = state.clone();
    assert_eq!(cloned.rng_seed, state.rng_seed);
}

#[test]
fn test_rng_seed_advances_every_tick_when_paused() {
    let mut state = SequencerState::default();
    state.playing = true;
    state.paused = true;
    let seed_before = state.rng_seed;
    state.tick();
    assert_ne!(
        state.rng_seed, seed_before,
        "rng_seed must advance on tick() even when paused"
    );
}

#[test]
fn test_rng_seed_advances_every_tick_when_playing() {
    let mut state = SequencerState::default();
    state.playing = true;
    state.paused = false;
    let seed_before = state.rng_seed;
    state.tick();
    assert_ne!(
        state.rng_seed, seed_before,
        "rng_seed must advance on tick() when playing"
    );
}

#[test]
fn test_next_rand_produces_distinct_values() {
    let mut seed = 0x853C_49E6_748F_EA9Bu64;
    let v1 = next_rand(&mut seed);
    let v2 = next_rand(&mut seed);
    let v3 = next_rand(&mut seed);
    assert_ne!(
        v1, v2,
        "consecutive next_rand calls must produce distinct values"
    );
    assert_ne!(
        v2, v3,
        "consecutive next_rand calls must produce distinct values"
    );
}

// ── Step Randomness (Stream B) ───────────────────────────────────────────────

#[test]
fn test_step_rand_default_zero() {
    let state = SequencerState::default();
    assert_eq!(state.step_rand, 0, "step_rand must default to 0");
}

#[test]
fn test_note_rand_default_zero() {
    let state = SequencerState::default();
    assert_eq!(state.note_rand, 0, "note_rand must default to 0");
}

#[test]
fn test_step_rand_zero_always_fires() {
    let mut state = SequencerState::default();
    state.playing = true;
    state.paused = false;
    state.step_rand = 0;
    for step in state.steps.iter_mut() {
        step.enabled = true;
    }
    state.playhead = 15;
    let mut fires = 0u32;
    for _ in 0..1000 {
        if state.tick().is_some() {
            fires += 1;
        }
    }
    assert_eq!(
        fires, 1000,
        "step_rand=0 must fire on every enabled step (got {fires})"
    );
}

#[test]
fn test_step_rand_hundred_never_fires() {
    let mut state = SequencerState::default();
    state.playing = true;
    state.paused = false;
    state.step_rand = 100;
    for step in state.steps.iter_mut() {
        step.enabled = true;
    }
    for _ in 0..1000 {
        assert!(state.tick().is_none(), "step_rand=100 must never fire");
    }
}

#[test]
fn test_step_rand_fifty_statistical() {
    let mut state = SequencerState::default();
    state.playing = true;
    state.paused = false;
    state.step_rand = 50;
    for step in state.steps.iter_mut() {
        step.enabled = true;
    }
    let n = 1000u32;
    let mut fires = 0u32;
    for _ in 0..n {
        if state.tick().is_some() {
            fires += 1;
        }
    }
    let ratio = fires as f64 / n as f64;
    assert!(
        ratio >= 0.40 && ratio <= 0.60,
        "step_rand=50 hit rate {ratio:.4} outside [0.40, 0.60]"
    );
}

// ── Stream E: Modifiers in tick() ───────────────────────────────────────────

/// Helper: build a playing state with a single enabled step at position 0.
fn playing_state_with_step(midi_note: u8, velocity: u8) -> SequencerState {
    let mut state = SequencerState::default();
    state.playing = true;
    state.paused = false;
    for s in state.steps.iter_mut() {
        s.enabled = false;
    }
    state.steps[0].enabled = true;
    state.steps[0].midi_note = midi_note;
    state.steps[0].velocity = velocity;
    state.playhead = 15;
    state
}

/// Advance state until it emits Some, up to `limit` ticks.
fn tick_until_some(state: &mut SequencerState, limit: usize) -> Option<MidiEvent> {
    for _ in 0..limit {
        let ev = state.tick();
        if ev.is_some() {
            return ev;
        }
    }
    None
}

#[test]
fn test_skip_modifier_false_steps_fire() {
    let mut state = playing_state_with_step(60, 80);
    state.skip_modifier = false;
    let ev = tick_until_some(&mut state, 32);
    assert!(
        ev.is_some(),
        "skip_modifier=false must not suppress enabled steps"
    );
}

#[test]
fn test_skip_modifier_true_returns_none() {
    let mut state = playing_state_with_step(60, 80);
    state.skip_modifier = true;
    for _ in 0..100 {
        assert!(
            state.tick().is_none(),
            "skip_modifier=true must suppress all steps"
        );
    }
}

#[test]
fn test_note_modifier_zero_is_noop() {
    let mut state = playing_state_with_step(60, 80);
    state.note_modifier = 0;
    state.note_rand = 100;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        assert_eq!(note, 60, "note_modifier=0 must not change the note");
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_note_modifier_positive_adds_semitones() {
    let mut state = playing_state_with_step(60, 80);
    state.note_modifier = 7;
    state.note_rand = 100;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        assert_eq!(note, 67, "note_modifier=7 must add 7 semitones");
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_note_modifier_positive_clamped_to_127() {
    let mut state = playing_state_with_step(125, 80);
    state.note_modifier = 7;
    state.note_rand = 100;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        assert_eq!(
            note, 127,
            "note clamped to 127 when modifier pushes it above max"
        );
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_note_modifier_negative_subtracts_semitones() {
    let mut state = playing_state_with_step(60, 80);
    state.note_modifier = -12;
    state.note_rand = 100;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        assert_eq!(note, 48, "note_modifier=-12 must subtract 12 semitones");
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_note_modifier_negative_clamped_to_zero() {
    let mut state = playing_state_with_step(5, 80);
    state.note_modifier = -12;
    state.note_rand = 100;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        assert_eq!(note, 0, "note clamped to 0 when modifier pushes it below 0");
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_note_modifier_with_note_rand_100_always_applied() {
    let mut state = playing_state_with_step(60, 80);
    state.note_modifier = 7;
    state.note_rand = 100;
    for _ in 0..50 {
        state.playhead = 15;
        let ev = state.tick().expect("step 0 must fire when step_rand=0");
        if let MidiEvent::NoteOn { note, .. } = ev {
            assert_eq!(note, 67, "note_rand=100 must always apply note_modifier");
        } else {
            panic!("expected NoteOn");
        }
    }
}

#[test]
fn test_note_modifier_with_note_rand_0_never_applied() {
    let mut state = playing_state_with_step(60, 80);
    state.note_modifier = 7;
    state.note_rand = 0;
    for _ in 0..50 {
        state.playhead = 15;
        let ev = state.tick().expect("step 0 must fire when step_rand=0");
        if let MidiEvent::NoteOn { note, .. } = ev {
            assert_eq!(note, 60, "note_rand=0 must never apply note_modifier");
        } else {
            panic!("expected NoteOn");
        }
    }
}

#[test]
fn test_scale_quant_false_no_snapping() {
    let mut state = playing_state_with_step(61, 80); // C#4, not in C major
    state.scale_quant = false;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        assert_eq!(note, 61, "scale_quant=false must not snap the note");
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_scale_quant_true_snaps_to_key() {
    let mut state = playing_state_with_step(61, 80); // C#4, not in C major
    state.scale_quant = true;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        let expected = snap_to_key(61, Key::C, Mode::Major);
        assert_eq!(note, expected, "scale_quant=true must snap note to key");
        assert_ne!(note, 61, "snapped note must differ from out-of-key input");
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_scale_quant_applied_after_note_modifier() {
    let mut state = playing_state_with_step(60, 80);
    state.note_modifier = 1;
    state.note_rand = 100;
    state.scale_quant = true;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { note, .. } = ev {
        let expected = snap_to_key(61, Key::C, Mode::Major);
        assert_eq!(
            note, expected,
            "scale_quant must be applied after note_modifier"
        );
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_velocity_modifier_zero_unchanged() {
    let mut state = playing_state_with_step(60, 80);
    state.velocity_modifier = 0;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { velocity, .. } = ev {
        assert_eq!(velocity, 80, "velocity_modifier=0 must not change velocity");
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_velocity_modifier_positive_adds() {
    let mut state = playing_state_with_step(60, 80);
    state.velocity_modifier = 20;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { velocity, .. } = ev {
        assert_eq!(
            velocity, 100,
            "velocity_modifier=20 must add 20 to velocity"
        );
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_velocity_modifier_positive_clamped_to_127() {
    let mut state = playing_state_with_step(60, 120);
    state.velocity_modifier = 20;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { velocity, .. } = ev {
        assert_eq!(
            velocity, 127,
            "velocity clamped to 127 when modifier pushes it above max"
        );
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_velocity_modifier_negative_subtracts() {
    let mut state = playing_state_with_step(60, 80);
    state.velocity_modifier = -20;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { velocity, .. } = ev {
        assert_eq!(
            velocity, 60,
            "velocity_modifier=-20 must subtract 20 from velocity"
        );
    } else {
        panic!("expected NoteOn");
    }
}

#[test]
fn test_velocity_modifier_negative_clamped_to_zero() {
    let mut state = playing_state_with_step(60, 10);
    state.velocity_modifier = -20;
    let ev = tick_until_some(&mut state, 32).expect("should fire");
    if let MidiEvent::NoteOn { velocity, .. } = ev {
        assert_eq!(
            velocity, 0,
            "velocity clamped to 0 when modifier pushes it below 0"
        );
    } else {
        panic!("expected NoteOn");
    }
}

// ── TempoRollPoint enum ──────────────────────────────────────────────────────

#[test]
fn test_tempo_roll_point_round_trip() {
    for i in 0..TempoRollPoint::COUNT {
        let v = TempoRollPoint::from_index(i);
        assert_eq!(
            v.to_index(),
            i,
            "TempoRollPoint round-trip failed for index {i}"
        );
    }
}

#[test]
fn test_tempo_roll_point_from_index_wraps() {
    assert_eq!(TempoRollPoint::from_index(0), TempoRollPoint::Off);
    assert_eq!(TempoRollPoint::from_index(1), TempoRollPoint::Step);
    assert_eq!(TempoRollPoint::from_index(2), TempoRollPoint::Beat);
    assert_eq!(TempoRollPoint::from_index(3), TempoRollPoint::Seq);
    assert_eq!(TempoRollPoint::from_index(4), TempoRollPoint::Off);
}

#[test]
fn test_tempo_roll_point_count() {
    assert_eq!(TempoRollPoint::COUNT, 4);
}

// ── TempoRandType enum ───────────────────────────────────────────────────────

#[test]
fn test_tempo_rand_type_round_trip() {
    for i in 0..TempoRandType::COUNT {
        let v = TempoRandType::from_index(i);
        assert_eq!(
            v.to_index(),
            i,
            "TempoRandType round-trip failed for index {i}"
        );
    }
}

#[test]
fn test_tempo_rand_type_from_index_wraps() {
    assert_eq!(TempoRandType::from_index(0), TempoRandType::Random);
    assert_eq!(TempoRandType::from_index(1), TempoRandType::Up);
    assert_eq!(TempoRandType::from_index(2), TempoRandType::Down);
    assert_eq!(TempoRandType::from_index(3), TempoRandType::Breathe);
    assert_eq!(TempoRandType::from_index(4), TempoRandType::PingPong);
    assert_eq!(TempoRandType::from_index(5), TempoRandType::Random);
}

#[test]
fn test_tempo_rand_type_count() {
    assert_eq!(TempoRandType::COUNT, 5);
}

// ── SequencerState new field defaults ────────────────────────────────────────

#[test]
fn test_new_fields_defaults() {
    let state = SequencerState::default();
    assert_eq!(state.tempo_rand, 0);
    assert_eq!(state.tempo_roll_point, TempoRollPoint::Off);
    assert_eq!(state.tempo_variance_max, 10);
    assert_eq!(state.tempo_rand_type, TempoRandType::Random);
    assert!(!state.scale_quant);
    assert_eq!(state.note_modifier, 0);
    assert!(!state.skip_modifier);
    assert_eq!(state.velocity_modifier, 0);
}

#[test]
fn test_sequencer_state_with_new_fields_is_clone() {
    let mut state = SequencerState::default();
    state.tempo_rand = 42;
    state.tempo_roll_point = TempoRollPoint::Beat;
    state.tempo_rand_type = TempoRandType::PingPong;
    state.scale_quant = true;
    state.note_modifier = -12;
    state.skip_modifier = true;
    state.velocity_modifier = 10;
    let cloned = state.clone();
    assert_eq!(cloned.tempo_rand, 42);
    assert_eq!(cloned.tempo_roll_point, TempoRollPoint::Beat);
    assert_eq!(cloned.tempo_rand_type, TempoRandType::PingPong);
    assert!(cloned.scale_quant);
    assert_eq!(cloned.note_modifier, -12);
    assert!(cloned.skip_modifier);
    assert_eq!(cloned.velocity_modifier, 10);
}

// ── shift_committed_param_value ──────────────────────────────────────────────

#[test]
fn test_shift_committed_param_value_defaults() {
    let state = SequencerState::default();
    assert_eq!(state.shift_committed_param_value(0), 0);
    assert_eq!(state.shift_committed_param_value(1), 0);
    assert_eq!(state.shift_committed_param_value(2), 0);
    assert_eq!(state.shift_committed_param_value(3), 10);
    assert_eq!(state.shift_committed_param_value(4), 0);
    assert_eq!(state.shift_committed_param_value(5), 0);
    assert_eq!(state.shift_committed_param_value(6), 0);
    assert_eq!(state.shift_committed_param_value(7), 0);
}

#[test]
fn test_shift_committed_param_value_reflects_state() {
    let mut state = SequencerState::default();
    state.tempo_rand = 75;
    state.tempo_roll_point = TempoRollPoint::Beat;
    state.tempo_variance_max = 50;
    state.tempo_rand_type = TempoRandType::Breathe;
    state.scale_quant = true;
    assert_eq!(state.shift_committed_param_value(1), 75);
    assert_eq!(
        state.shift_committed_param_value(2),
        TempoRollPoint::Beat.to_index() as i64
    );
    assert_eq!(state.shift_committed_param_value(3), 50);
    assert_eq!(
        state.shift_committed_param_value(4),
        TempoRandType::Breathe.to_index() as i64
    );
    assert_eq!(state.shift_committed_param_value(6), 1);
}

// ── shift_clamped_param_value ────────────────────────────────────────────────

#[test]
fn test_shift_clamped_param_value_tempo_rand() {
    let state = SequencerState::default();
    assert_eq!(state.shift_clamped_param_value(1, -5), 0);
    assert_eq!(state.shift_clamped_param_value(1, 0), 0);
    assert_eq!(state.shift_clamped_param_value(1, 50), 50);
    assert_eq!(state.shift_clamped_param_value(1, 100), 100);
    assert_eq!(state.shift_clamped_param_value(1, 150), 100);
}

#[test]
fn test_shift_clamped_param_value_tempo_roll_point_wraps() {
    let state = SequencerState::default();
    assert_eq!(state.shift_clamped_param_value(2, 0), 0);
    assert_eq!(state.shift_clamped_param_value(2, 3), 3);
    assert_eq!(state.shift_clamped_param_value(2, 4), 0);
    assert_eq!(state.shift_clamped_param_value(2, -1), 3);
}

#[test]
fn test_shift_clamped_param_value_tempo_variance_max() {
    let state = SequencerState::default();
    assert_eq!(state.shift_clamped_param_value(3, 0), 1);
    assert_eq!(state.shift_clamped_param_value(3, 1), 1);
    assert_eq!(state.shift_clamped_param_value(3, 50), 50);
    assert_eq!(state.shift_clamped_param_value(3, 99), 99);
    assert_eq!(state.shift_clamped_param_value(3, 100), 99);
}

#[test]
fn test_shift_clamped_param_value_tempo_rand_type_wraps() {
    let state = SequencerState::default();
    assert_eq!(state.shift_clamped_param_value(4, 0), 0);
    assert_eq!(state.shift_clamped_param_value(4, 4), 4);
    assert_eq!(state.shift_clamped_param_value(4, 5), 0);
    assert_eq!(state.shift_clamped_param_value(4, -1), 4);
}

#[test]
fn test_shift_clamped_param_value_scale_quant() {
    let state = SequencerState::default();
    assert_eq!(state.shift_clamped_param_value(6, 0), 0);
    assert_eq!(state.shift_clamped_param_value(6, 1), 1);
    assert_eq!(state.shift_clamped_param_value(6, -1), 0);
    assert_eq!(state.shift_clamped_param_value(6, 2), 1);
}

#[test]
fn test_shift_clamped_param_value_reserved_is_passthrough() {
    let state = SequencerState::default();
    assert_eq!(state.shift_clamped_param_value(7, 42), 42);
    assert_eq!(state.shift_clamped_param_value(7, -99), -99);
}

// ── shift_apply_param_value ──────────────────────────────────────────────────

#[test]
fn test_shift_apply_param_value_tempo_rand() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(1, 80);
    assert_eq!(state.tempo_rand, 80);
}

#[test]
fn test_shift_apply_param_value_tempo_roll_point() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(2, TempoRollPoint::Seq.to_index() as i64);
    assert_eq!(state.tempo_roll_point, TempoRollPoint::Seq);
}

#[test]
fn test_shift_apply_param_value_tempo_variance_max() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(3, 33);
    assert_eq!(state.tempo_variance_max, 33);
}

#[test]
fn test_shift_apply_param_value_tempo_rand_type() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(4, TempoRandType::Down.to_index() as i64);
    assert_eq!(state.tempo_rand_type, TempoRandType::Down);
}

#[test]
fn test_shift_apply_param_value_scale_quant() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(6, 1);
    assert!(state.scale_quant);
    state.shift_apply_param_value(6, 0);
    assert!(!state.scale_quant);
}

#[test]
fn test_shift_apply_param_value_note_rand_is_noop() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(0, 50);
}

#[test]
fn test_shift_apply_param_value_step_rand_is_noop() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(5, 50);
}

#[test]
fn test_shift_apply_param_value_reserved_is_noop() {
    let mut state = SequencerState::default();
    state.shift_apply_param_value(7, 99);
}

// ── Shift overlay round-trip via apply_command ───────────────────────────────

fn open_shift_overlay(state: &mut SequencerState) {
    state.apply_command(InputCommand::OpenOverlay(OverlayMode::Shift));
}

#[test]
fn test_shift_overlay_round_trip_tempo_rand() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 1;
    state.apply_command(InputCommand::ParamValueDelta(50));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.tempo_rand, 50);
}

#[test]
fn test_shift_overlay_round_trip_tempo_roll_point() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 2;
    state.apply_command(InputCommand::ParamValueDelta(2));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.tempo_roll_point, TempoRollPoint::Beat);
}

#[test]
fn test_shift_overlay_round_trip_tempo_variance_max() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 3;
    state.apply_command(InputCommand::ParamValueDelta(30));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.tempo_variance_max, 40);
}

#[test]
fn test_shift_overlay_round_trip_tempo_rand_type() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 4;
    state.apply_command(InputCommand::ParamValueDelta(4));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.tempo_rand_type, TempoRandType::PingPong);
}

#[test]
fn test_shift_overlay_round_trip_scale_quant() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 6;
    state.apply_command(InputCommand::ParamValueDelta(1));
    state.apply_command(InputCommand::Confirm);
    assert!(state.scale_quant);
}

#[test]
fn test_shift_overlay_reserved_index_is_noop() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 7;
    state.apply_command(InputCommand::ParamValueDelta(1));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.pending_edit, PendingEdit::None);
}

#[test]
fn test_shift_overlay_multiple_deltas_accumulate() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 1;
    state.apply_command(InputCommand::ParamValueDelta(20));
    state.apply_command(InputCommand::ParamValueDelta(20));
    state.apply_command(InputCommand::ParamValueDelta(10));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.tempo_rand, 50);
}

#[test]
fn test_regular_overlay_still_works_after_shift_changes() {
    let mut state = SequencerState::default();
    state.apply_command(InputCommand::OpenOverlay(OverlayMode::Regular));
    state.selected_param = 2; // swing
    state.apply_command(InputCommand::ParamValueDelta(10));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.swing, 10);
}

#[test]
fn test_shift_overlay_round_trip_note_rand_stub_is_noop() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 0;
    state.apply_command(InputCommand::ParamValueDelta(50));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.pending_edit, PendingEdit::None);
}

#[test]
fn test_shift_overlay_round_trip_step_rand_stub_is_noop() {
    let mut state = SequencerState::default();
    open_shift_overlay(&mut state);
    state.selected_param = 5;
    state.apply_command(InputCommand::ParamValueDelta(50));
    state.apply_command(InputCommand::Confirm);
    assert_eq!(state.pending_edit, PendingEdit::None);
}

// ── Stream D: Shift action commands ─────────────────────────────────────────

#[test]
fn test_note_modifier_set_applies_value() {
    let mut state = SequencerState::default();
    state.apply_command(InputCommand::NoteModifierSet(5));
    assert_eq!(state.note_modifier, 5);
}

#[test]
fn test_note_modifier_set_clears_with_zero() {
    let mut state = SequencerState::default();
    state.note_modifier = 12;
    state.apply_command(InputCommand::NoteModifierSet(0));
    assert_eq!(state.note_modifier, 0);
}

#[test]
fn test_note_modifier_set_negative_value() {
    let mut state = SequencerState::default();
    state.apply_command(InputCommand::NoteModifierSet(-24));
    assert_eq!(state.note_modifier, -24);
}

#[test]
fn test_skip_modifier_toggle_false_to_true() {
    let mut state = SequencerState::default();
    assert!(!state.skip_modifier);
    state.apply_command(InputCommand::SkipModifierToggle);
    assert!(state.skip_modifier);
}

#[test]
fn test_skip_modifier_toggle_true_to_false() {
    let mut state = SequencerState::default();
    state.skip_modifier = true;
    state.apply_command(InputCommand::SkipModifierToggle);
    assert!(!state.skip_modifier);
}

#[test]
fn test_skip_modifier_toggle_round_trip() {
    let mut state = SequencerState::default();
    state.apply_command(InputCommand::SkipModifierToggle);
    state.apply_command(InputCommand::SkipModifierToggle);
    assert!(!state.skip_modifier, "double toggle should return to false");
}

#[test]
fn test_velocity_modifier_set_applies_value() {
    let mut state = SequencerState::default();
    state.apply_command(InputCommand::VelocityModifierSet(64));
    assert_eq!(state.velocity_modifier, 64);
}

#[test]
fn test_velocity_modifier_set_clears_with_zero() {
    let mut state = SequencerState::default();
    state.velocity_modifier = 50;
    state.apply_command(InputCommand::VelocityModifierSet(0));
    assert_eq!(state.velocity_modifier, 0);
}

#[test]
fn test_velocity_modifier_set_negative_value() {
    let mut state = SequencerState::default();
    state.apply_command(InputCommand::VelocityModifierSet(-127));
    assert_eq!(state.velocity_modifier, -127);
}

#[test]
fn test_generate_random_sequence_updates_all_notes() {
    let mut state = SequencerState::default();
    for step in state.steps.iter_mut() {
        step.midi_note = 60;
    }
    let enabled_before: [bool; 16] = core::array::from_fn(|i| state.steps[i].enabled);
    state.apply_command(InputCommand::GenerateRandomSequence);
    for (i, step) in state.steps.iter().enumerate() {
        assert!(
            step.midi_note >= 48 && step.midi_note <= 84,
            "step {i} midi_note {} out of range 48–84",
            step.midi_note
        );
        let snapped = snap_to_key(step.midi_note, state.key, state.mode);
        assert_eq!(
            step.midi_note, snapped,
            "step {i} note {} is not in key/mode",
            step.midi_note
        );
        assert_eq!(
            step.enabled, enabled_before[i],
            "step {i} enabled flag changed by GenerateRandomSequence"
        );
    }
}

#[test]
fn test_generate_random_sequence_enabled_flags_preserved_mixed() {
    let mut state = SequencerState::default();
    for (i, step) in state.steps.iter_mut().enumerate() {
        step.enabled = i % 2 == 0;
    }
    let enabled_before: [bool; 16] = core::array::from_fn(|i| state.steps[i].enabled);
    state.apply_command(InputCommand::GenerateRandomSequence);
    for (i, step) in state.steps.iter().enumerate() {
        assert_eq!(
            step.enabled, enabled_before[i],
            "step {i} enabled flag changed"
        );
    }
}

#[test]
fn test_generate_random_sequence_notes_in_range_48_84() {
    let mut state = SequencerState::default();
    for _ in 0..20 {
        state.apply_command(InputCommand::GenerateRandomSequence);
        for (i, step) in state.steps.iter().enumerate() {
            assert!(
                step.midi_note >= 48 && step.midi_note <= 84,
                "step {i} note {} out of range 48–84 after repeated GenerateRandomSequence",
                step.midi_note
            );
        }
    }
}

#[test]
fn test_generate_random_sequence_notes_in_key() {
    let mut state = SequencerState::default(); // Key::C, Mode::Major
    for _ in 0..20 {
        state.apply_command(InputCommand::GenerateRandomSequence);
        for (i, step) in state.steps.iter().enumerate() {
            let snapped = snap_to_key(step.midi_note, state.key, state.mode);
            assert_eq!(
                step.midi_note, snapped,
                "step {i} note {} not in key C Major after GenerateRandomSequence",
                step.midi_note
            );
        }
    }
}

#[test]
fn test_generate_random_sequence_produces_variety() {
    let mut state = SequencerState::default();
    state.apply_command(InputCommand::GenerateRandomSequence);
    let first: [u8; 16] = core::array::from_fn(|i| state.steps[i].midi_note);
    let mut all_same = true;
    for _ in 0..9 {
        state.apply_command(InputCommand::GenerateRandomSequence);
        let current: [u8; 16] = core::array::from_fn(|i| state.steps[i].midi_note);
        if current != first {
            all_same = false;
            break;
        }
    }
    assert!(
        !all_same,
        "GenerateRandomSequence produced identical sequences 10 times in a row"
    );
}
