//! Sequencer state — the single shared truth for all threads.
//!
//! Wrap in `Arc<RwLock<SequencerState>>` at the call site (Step 9).
//! All mutating methods take `&mut self`; callers hold the write lock.

use crate::input::InputCommand;
use crate::music_theory::{Key, Mode};

// Re-export OverlayMode from input so that existing code importing it from
// state (e.g. sequencer.rs) continues to compile without modification.
pub use crate::input::OverlayMode;

/// Step resolution for the sequencer clock.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepSize {
    /// One step = one quarter note.
    Quarter,
    /// One step = one eighth note.
    Eighth,
    /// One step = one sixteenth note.
    Sixteenth,
}

/// Pending parameter edit awaiting confirmation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PendingEdit {
    /// No pending edit.
    None,
    /// Editing the MIDI note for a step.
    Note { step: usize, midi_note: u8 },
    /// Editing the velocity for a step.
    Velocity { step: usize, velocity: u8 },
    /// Editing a named parameter under a given overlay.
    Param { overlay: OverlayMode, index: u8, value: i64 },
}

/// A single sequencer step.
#[derive(Clone, Copy, Debug)]
pub struct StepData {
    /// Whether this step fires on playback.
    pub enabled: bool,
    /// MIDI note number (0–127).
    pub midi_note: u8,
    /// MIDI velocity (0–127).
    pub velocity: u8,
}

impl Default for StepData {
    fn default() -> Self {
        Self { enabled: false, midi_note: 60, velocity: 100 }
    }
}

/// MIDI event produced by the sequencer or clock.
///
/// `NoteOn` carries `duration_nanos` so that `midi_out.rs` (Step 5) can
/// schedule the matching `NoteOff` without involving the sequencer again.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MidiEvent {
    /// Note-on with embedded duration for NoteOff scheduling.
    NoteOn { channel: u8, note: u8, velocity: u8, duration_nanos: u64 },
    /// Note-off.
    NoteOff { channel: u8, note: u8 },
    /// MIDI Start (transport).
    Start,
    /// MIDI Stop (transport).
    Stop,
    /// MIDI Continue (transport).
    Continue,
}

/// The complete sequencer state.
///
/// Designed for `Arc<RwLock<SequencerState>>` wrapping by the caller.
/// No heap allocation in any hot-path method.
#[derive(Clone, Debug)]
pub struct SequencerState {
    /// The 16 sequencer steps.
    pub steps: [StepData; 16],
    /// Current key.
    pub key: Key,
    /// Current mode/scale.
    pub mode: Mode,
    /// Tempo in BPM (20–300).
    pub tempo_bpm: u16,
    /// Swing offset in percentage points (-50 to +50).
    pub swing: i8,
    /// Step resolution.
    pub step_size: StepSize,
    /// Loop start step (0–15).
    pub loop_in: u8,
    /// Loop end step (0–15).
    pub loop_out: u8,
    /// Whether loop mode is active.
    pub loop_active: bool,
    /// Current playhead position (0–15).
    pub playhead: u8,
    /// Whether the sequencer is running.
    pub playing: bool,
    /// Whether the sequencer is paused.
    pub paused: bool,
    /// Any pending parameter edit.
    pub pending_edit: PendingEdit,
    /// Active overlay mode; set by the command processor, read by the HID thread.
    pub active_overlay: Option<OverlayMode>,
    /// Currently selected step (0–15); controlled by StepSelect/StepSelectDelta.
    pub selected_step: usize,
    /// Currently selected param index (0–6); controlled by ParamSelect/ParamSelectDelta.
    pub selected_param: u8,
}

impl Default for SequencerState {
    fn default() -> Self {
        Self {
            steps: [StepData::default(); 16],
            key: Key::C,
            mode: Mode::Major,
            tempo_bpm: 120,
            swing: 0,
            step_size: StepSize::Sixteenth,
            loop_in: 0,
            loop_out: 15,
            loop_active: false,
            playhead: 0,
            playing: false,
            paused: false,
            pending_edit: PendingEdit::None,
            active_overlay: None,
            selected_step: 0,
            selected_param: 0,
        }
    }
}

impl SequencerState {
    /// Shifts the MIDI note for `step` by `delta` scale degrees using the
    /// current key and mode. No-ops if `step` is out of range.
    pub fn apply_encoder_delta(&mut self, step: usize, delta: i8) {
        if step >= 16 {
            return;
        }
        self.steps[step].midi_note =
            crate::music_theory::next_note(self.steps[step].midi_note, self.key, self.mode, delta);
    }

    /// Toggles the enabled state for `step`. No-ops if `step` is out of range.
    pub fn toggle_step(&mut self, step: usize) {
        if step >= 16 {
            return;
        }
        self.steps[step].enabled = !self.steps[step].enabled;
    }

    /// Advances the playhead by one step and returns a `MidiEvent::NoteOn` if
    /// the new step is enabled, or `None` otherwise.
    ///
    /// Returns `None` immediately when not playing or when paused.
    ///
    /// `duration_nanos` is set to 0 here; the clock thread must overwrite it
    /// with the actual step period before forwarding the event to `midi_out`.
    pub fn tick(&mut self) -> Option<MidiEvent> {
        if !self.playing || self.paused {
            return None;
        }

        // Advance playhead
        let next = self.playhead + 1;
        if self.loop_active {
            if next > self.loop_out {
                self.playhead = self.loop_in;
            } else {
                self.playhead = next;
            }
        } else if next >= 16 {
            self.playhead = 0;
        } else {
            self.playhead = next;
        }

        let step = &self.steps[self.playhead as usize];
        if step.enabled {
            Some(MidiEvent::NoteOn {
                channel: 0,
                note: step.midi_note,
                velocity: step.velocity,
                duration_nanos: 0,
            })
        } else {
            None
        }
    }

    /// Apply an `InputCommand` to the sequencer state.
    ///
    /// This is the single entry point for all state mutation driven by user
    /// input.  Both the keyboard thread and the HID thread send `InputCommand`
    /// values on a shared channel; the consumer calls this method while holding
    /// the write lock.
    pub fn apply_command(&mut self, cmd: InputCommand) {
        match cmd {
            InputCommand::StepSelect(n) => {
                self.selected_step = n.min(15);
                // Discard any pending note or velocity edit on step change.
                if matches!(self.pending_edit, PendingEdit::Note { .. } | PendingEdit::Velocity { .. }) {
                    self.pending_edit = PendingEdit::None;
                }
            }
            InputCommand::StepSelectDelta(d) => {
                // Wrapping arithmetic modulo 16.
                let current = self.selected_step as i32;
                let next = ((current + d as i32).rem_euclid(16)) as usize;
                self.selected_step = next;
                if matches!(self.pending_edit, PendingEdit::Note { .. } | PendingEdit::Velocity { .. }) {
                    self.pending_edit = PendingEdit::None;
                }
            }
            InputCommand::NoteDelta(d) => {
                let step = self.selected_step;
                let current_note = self.steps[step].midi_note;
                // Saturating add so we stay in 0–127.
                let new_note = (current_note as i16 + d as i16).clamp(0, 127) as u8;
                self.pending_edit = PendingEdit::Note { step, midi_note: new_note };
            }
            InputCommand::Confirm => {
                match self.pending_edit {
                    PendingEdit::None => { /* no-op */ }
                    PendingEdit::Note { step, midi_note } => {
                        if step < 16 {
                            self.steps[step].midi_note = midi_note;
                        }
                        self.pending_edit = PendingEdit::None;
                    }
                    PendingEdit::Velocity { step, velocity } => {
                        if step < 16 {
                            self.steps[step].velocity = velocity;
                        }
                        self.pending_edit = PendingEdit::None;
                    }
                    PendingEdit::Param { .. } => {
                        // Param commits are handled by Step 7 (param overlay logic).
                        // Clear the pending edit after confirmation.
                        self.pending_edit = PendingEdit::None;
                    }
                }
            }
            InputCommand::ToggleStep => {
                let step = self.selected_step;
                self.toggle_step(step);
            }
            InputCommand::VelocityDelta(d) => {
                let step = self.selected_step;
                let current_vel = self.steps[step].velocity;
                let new_vel = (current_vel as i16 + d as i16).clamp(0, 127) as u8;
                self.pending_edit = PendingEdit::Velocity { step, velocity: new_vel };
            }
            InputCommand::OpenOverlay(mode) => {
                // Record the active overlay so HID can read it.
                // The UI thread also tracks this locally for rendering decisions.
                self.active_overlay = Some(mode);
            }
            InputCommand::CloseOverlay => {
                self.active_overlay = None;
                // Discard any pending param edit.
                if matches!(self.pending_edit, PendingEdit::Param { .. }) {
                    self.pending_edit = PendingEdit::None;
                }
            }
            InputCommand::ParamSelect(n) => {
                self.selected_param = n.min(6);
            }
            InputCommand::ParamSelectDelta(d) => {
                // 7 params (indices 0–6), wrap modulo 7.
                let current = self.selected_param as i32;
                let next = ((current + d as i32).rem_euclid(7)) as u8;
                self.selected_param = next;
            }
            InputCommand::ParamValueDelta(d) => {
                let overlay = match self.active_overlay {
                    Some(m) => m,
                    None => return, // No overlay open — ignore.
                };
                let index = self.selected_param;
                let current_value = match self.pending_edit {
                    PendingEdit::Param { index: pi, value, .. } if pi == index => value,
                    _ => 0,
                };
                self.pending_edit = PendingEdit::Param {
                    overlay,
                    index,
                    value: current_value + d as i64,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // default midi_note = 60
        s.apply_command(InputCommand::NoteDelta(1));
        assert!(matches!(s.pending_edit, PendingEdit::Note { step: 2, midi_note: 61 }));
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
        let mut s = SequencerState::default();
        s.selected_param = 6;
        s.apply_command(InputCommand::ParamSelectDelta(1));
        assert_eq!(s.selected_param, 0, "param wraps past 6 to 0");
    }

    #[test]
    fn apply_command_param_select_delta_wraps_at_0() {
        let mut s = SequencerState::default();
        s.selected_param = 0;
        s.apply_command(InputCommand::ParamSelectDelta(-1));
        assert_eq!(s.selected_param, 6, "param wraps below 0 to 6");
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
}
