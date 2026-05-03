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
    /// One step = one whole note.
    Whole,
    /// One step = one half note.
    Half,
    /// One step = one quarter note.
    Quarter,
    /// One step = one eighth note.
    Eighth,
    /// One step = one sixteenth note.
    Sixteenth,
    /// One step = one thirty-second note.
    ThirtySecond,
}

impl StepSize {
    /// Number of StepSize variants.
    pub const COUNT: usize = 6;

    /// Convert a zero-based index (mod 6) to the corresponding StepSize variant.
    pub fn from_index(i: usize) -> Self {
        match i % Self::COUNT {
            0 => StepSize::Whole,
            1 => StepSize::Half,
            2 => StepSize::Quarter,
            3 => StepSize::Eighth,
            4 => StepSize::Sixteenth,
            _ => StepSize::ThirtySecond,
        }
    }

    /// Return the zero-based index of this StepSize variant.
    pub fn to_index(self) -> usize {
        match self {
            StepSize::Whole => 0,
            StepSize::Half => 1,
            StepSize::Quarter => 2,
            StepSize::Eighth => 3,
            StepSize::Sixteenth => 4,
            StepSize::ThirtySecond => 5,
        }
    }
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
    /// MIDI channel to output on (0–15, where 0 = channel 1 in MIDI spec).
    pub midi_channel: u8,
    /// RNG state for all randomness streams; advanced on every tick.
    pub rng_seed: u64,
    /// Step Randomness (0–100): per-step probability that an enabled step fires.
    /// 0 = always fires (existing behaviour). 100 = never fires.
    pub step_rand: u8,
    /// Note Randomness (0–100): per-step probability that the note modifier is
    /// applied. Only relevant when `note_modifier != 0`.
    /// 0 = modifier never applied. 100 = modifier always applied.
    pub note_rand: u8,
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
            midi_channel: 0,
            rng_seed: 0x853C_49E6_748F_EA9B,
            step_rand: 0,
            note_rand: 0,
        }
    }
}

/// Advance seed and return a pseudo-random u64 (Xorshift64).
fn next_rand(seed: &mut u64) -> u64 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *seed = x;
    x
}

/// Returns true with probability `chance/100`. `chance` is clamped to 0–100.
#[allow(dead_code)]
fn prob_hit(seed: &mut u64, chance: u8) -> bool {
    if chance == 0 {
        return false;
    }
    if chance >= 100 {
        return true;
    }
    (next_rand(seed) % 100) < chance as u64
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
        next_rand(&mut self.rng_seed);
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

        // Step Randomness: probabilistic mute of the whole step.
        // step_rand is the mute probability (0 = never mute, 100 = always mute).
        if prob_hit(&mut self.rng_seed, self.step_rand) {
            return None;
        }

        let step = &self.steps[self.playhead as usize];
        if step.enabled {
            // TODO(stream-E): apply note_rand gate here — prob_hit(&mut self.rng_seed, self.note_rand)
            // determines whether the note modifier is applied.
            Some(MidiEvent::NoteOn {
                channel: self.midi_channel,
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
                // BUG-010 fix: use pending note as base so repeated presses accumulate.
                let base_note = match self.pending_edit {
                    PendingEdit::Note { step: ps, midi_note } if ps == step => midi_note,
                    _ => self.steps[step].midi_note,
                };
                let new_note = crate::music_theory::next_note(base_note, self.key, self.mode, d);
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
                    PendingEdit::Param { index, value, .. } => {
                        // BUG-012 fix: apply the pending param value to the state field.
                        self.apply_param_value(index, value);
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
                self.selected_param = n.min(7);
            }
            InputCommand::ParamSelectDelta(d) => {
                // 8 params (indices 0–7), wrap modulo 8.
                let current = self.selected_param as i32;
                let next = ((current + d as i32).rem_euclid(8)) as u8;
                self.selected_param = next;
            }
            InputCommand::ParamValueDelta(d) => {
                let overlay = match self.active_overlay {
                    Some(m) => m,
                    None => return, // No overlay open — ignore.
                };
                let index = self.selected_param;
                // BUG-011 fix: seed from the current committed state value so the
                // pending value is always in the same unit space as the state field.
                let current_value = match self.pending_edit {
                    PendingEdit::Param { index: pi, value, .. } if pi == index => value,
                    _ => self.committed_param_value(index),
                };
                let new_value = self.clamped_param_value(index, current_value + d as i64);
                self.pending_edit = PendingEdit::Param { overlay, index, value: new_value };
            }
            InputCommand::PlayStop => {
                if self.playing {
                    self.playing = false;
                    self.paused = false;
                } else {
                    self.playing = true;
                }
            }
        }
    }

    /// Return the current committed state value for param `index` as an i64
    /// in the same unit space used by `PendingEdit::Param`.
    ///
    /// Enum params use the variant index; numeric params use the raw value.
    /// Regular overlay indices: 0=Key, 1=Mode, 2=Swing, 3=StepSize,
    /// 4=loop_in, 5=loop_out, 6=paused, 7=playing.
    fn committed_param_value(&self, index: u8) -> i64 {
        match index {
            0 => self.key.to_index() as i64,
            1 => self.mode.to_index() as i64,
            2 => self.swing as i64,
            3 => self.step_size.to_index() as i64,
            4 => self.loop_in as i64,
            5 => self.loop_out as i64,
            6 => self.paused as i64,
            7 => self.playing as i64,
            _ => 0,
        }
    }

    /// Clamp or wrap the raw `value` into the valid range for param `index`.
    fn clamped_param_value(&self, index: u8, value: i64) -> i64 {
        match index {
            0 => value.rem_euclid(Key::COUNT as i64),
            1 => value.rem_euclid(Mode::COUNT as i64),
            2 => value.clamp(-50, 50),
            3 => value.rem_euclid(StepSize::COUNT as i64),
            4 | 5 => value.clamp(0, 15),
            6 | 7 => value.clamp(0, 1),
            _ => value,
        }
    }

    /// Write the resolved `value` for param `index` back to the matching state field.
    ///
    /// Regular overlay indices: 0=Key, 1=Mode, 2=Swing, 3=StepSize,
    /// 4=loop_in, 5=loop_out, 6=paused, 7=playing.
    /// BUG-017: setting playing=true (index 7, value 1) also clears paused.
    fn apply_param_value(&mut self, index: u8, value: i64) {
        match index {
            0 => {
                let new_key = Key::from_index(value as usize);
                if new_key != self.key {
                    self.key = new_key;
                    self.snap_all_steps_to_key();
                }
            }
            1 => {
                let new_mode = Mode::from_index(value as usize);
                if new_mode != self.mode {
                    self.mode = new_mode;
                    self.snap_all_steps_to_key();
                }
            }
            2 => self.swing = value as i8,
            3 => self.step_size = StepSize::from_index(value as usize),
            4 => self.loop_in = value as u8,
            5 => self.loop_out = value as u8,
            6 => self.paused = value != 0,
            7 => {
                self.playing = value != 0;
                if self.playing {
                    self.paused = false;
                }
            }
            _ => {}
        }
    }

    /// Re-snap all 16 step notes to the nearest note in the current key and mode.
    ///
    /// Called immediately after `self.key` or `self.mode` is updated.
    /// No heap allocation: operates on the fixed-size `steps` array in place.
    fn snap_all_steps_to_key(&mut self) {
        for step in self.steps.iter_mut() {
            step.midi_note =
                crate::music_theory::snap_to_key(step.midi_note, self.key, self.mode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::OverlayMode;

    // ── BUG-014: loop_out edit path ──────────────────────────────────────────

    #[test]
    fn test_loop_out_edit_path_via_overlay() {
        let mut state = SequencerState::default();
        // Simulate a pending param edit for loop_out (index 5) with value 10.
        state.pending_edit = PendingEdit::Param {
            overlay: OverlayMode::Regular,
            index: 5,
            value: 10,
        };
        // Confirm should apply loop_out = 10.
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

    // ── BUG-017: overlay confirm playing=true clears paused ─────────────────

    #[test]
    fn test_confirm_playing_clears_paused() {
        let mut state = SequencerState::default();
        state.paused = true;
        state.playing = false;
        // Simulate a pending param edit: index 7 (playing), value 1.
        state.pending_edit = PendingEdit::Param {
            overlay: OverlayMode::Regular,
            index: 7,
            value: 1,
        };
        state.apply_command(InputCommand::Confirm);
        assert!(state.playing, "playing should be true after confirm");
        assert!(!state.paused, "paused should be cleared when playing is set via overlay");
    }

    #[test]
    fn test_confirm_playing_false_does_not_clear_paused() {
        let mut state = SequencerState::default();
        state.paused = true;
        state.playing = true;
        // Set playing=false via overlay — paused should be left unchanged.
        state.pending_edit = PendingEdit::Param {
            overlay: OverlayMode::Regular,
            index: 7,
            value: 0,
        };
        state.apply_command(InputCommand::Confirm);
        assert!(!state.playing);
        assert!(state.paused, "paused should be unchanged when playing is set to false");
    }

    // ── Key/Mode Note Shifting ───────────────────────────────────────────────

    #[test]
    fn test_key_change_snaps_all_steps() {
        let mut state = SequencerState::default(); // Key::C, Mode::Major
        state.steps[0].midi_note = 61; // C#4 — not in C major
        state.steps[1].midi_note = 62; // D4
        // Confirm Key change to C# (index 1)
        state.pending_edit = PendingEdit::Param {
            overlay: OverlayMode::Regular,
            index: 0,
            value: 1, // Key::Cs
        };
        state.apply_command(InputCommand::Confirm);
        assert_eq!(state.key, crate::music_theory::Key::Cs);
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
        assert_eq!(state.mode, crate::music_theory::Mode::NaturalMinor);
        assert_eq!(state.steps[0].midi_note, 70); // snapped to Bb4
    }

    #[test]
    fn test_same_key_no_snap() {
        let mut state = SequencerState::default(); // Key::C
        state.steps[0].midi_note = 61; // C#4 — out of key, set directly
        // Confirm Key=C again (no change)
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
        // Confirm Mode=Major again (no change)
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
            let expected = crate::music_theory::snap_to_key(
                61,
                crate::music_theory::Key::D,
                crate::music_theory::Mode::Major,
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
        let expected = crate::music_theory::snap_to_key(
            61,
            crate::music_theory::Key::D,
            crate::music_theory::Mode::Major,
        );
        assert_eq!(state.steps[3].midi_note, expected, "disabled step should still be snapped");
    }

    // ── RNG Infrastructure ───────────────────────────────────────────────────

    #[test]
    fn test_prob_hit_zero_always_false() {
        let mut seed = 0x853C_49E6_748F_EA9Bu64;
        for _ in 0..1000 {
            assert!(!prob_hit(&mut seed, 0), "prob_hit(0) must always return false");
        }
    }

    #[test]
    fn test_prob_hit_hundred_always_true() {
        let mut seed = 0x853C_49E6_748F_EA9Bu64;
        for _ in 0..1000 {
            assert!(prob_hit(&mut seed, 100), "prob_hit(100) must always return true");
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
        assert_ne!(state.rng_seed, seed_before, "rng_seed must advance on tick() even when not playing");
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
        // Confirm next_rand is not an identity function and advances state each call.
        let mut seed = 0x853C_49E6_748F_EA9Bu64;
        let v1 = next_rand(&mut seed);
        let v2 = next_rand(&mut seed);
        let v3 = next_rand(&mut seed);
        assert_ne!(v1, v2, "consecutive next_rand calls must produce distinct values");
        assert_ne!(v2, v3, "consecutive next_rand calls must produce distinct values");
    }

    // ── Step Randomness (Stream B) ───────────────────────────────────────────

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
        // step_rand = 0 → all enabled steps always fire (existing behaviour preserved).
        let mut state = SequencerState::default();
        state.playing = true;
        state.paused = false;
        state.step_rand = 0;
        // Enable all 16 steps.
        for step in state.steps.iter_mut() {
            step.enabled = true;
        }
        state.playhead = 15; // will wrap to 0 on first tick
        let mut fires = 0u32;
        for _ in 0..1000 {
            if state.tick().is_some() {
                fires += 1;
            }
        }
        assert_eq!(fires, 1000, "step_rand=0 must fire on every enabled step (got {fires})");
    }

    #[test]
    fn test_step_rand_hundred_never_fires() {
        // step_rand = 100 → no enabled steps fire (all probabilistically muted).
        let mut state = SequencerState::default();
        state.playing = true;
        state.paused = false;
        state.step_rand = 100;
        for step in state.steps.iter_mut() {
            step.enabled = true;
        }
        for _ in 0..1000 {
            assert!(
                state.tick().is_none(),
                "step_rand=100 must never fire"
            );
        }
    }

    #[test]
    fn test_step_rand_fifty_statistical() {
        // step_rand = 50 → over 1 000 ticks, between 40% and 60% of enabled steps fire.
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
}

