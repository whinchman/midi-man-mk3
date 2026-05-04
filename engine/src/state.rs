//! Sequencer state — the single shared truth for all threads.
//!
//! Wrap in `Arc<RwLock<SequencerState>>` at the call site (Step 9).
//! All mutating methods take `&mut self`; callers hold the write lock.

use crate::input::InputCommand;
use crate::music_theory::{Key, Mode};

// Re-export OverlayMode from input so that existing code importing it from
// state (e.g. sequencer.rs) continues to compile without modification.
pub use crate::input::OverlayMode;

/// When the tempo randomness roll fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TempoRollPoint {
    /// Tempo randomness disabled.
    Off,
    /// Roll fires on every step.
    Step,
    /// Roll fires on every beat (4 steps at 1/16 resolution).
    Beat,
    /// Roll fires on every sequence loop.
    Seq,
}

impl TempoRollPoint {
    /// Number of TempoRollPoint variants.
    pub const COUNT: usize = 4;

    /// Convert a zero-based index (mod 4) to the corresponding TempoRollPoint variant.
    pub fn from_index(i: usize) -> Self {
        match i % Self::COUNT {
            0 => TempoRollPoint::Off,
            1 => TempoRollPoint::Step,
            2 => TempoRollPoint::Beat,
            _ => TempoRollPoint::Seq,
        }
    }

    /// Return the zero-based index of this TempoRollPoint variant.
    pub fn to_index(self) -> usize {
        match self {
            TempoRollPoint::Off => 0,
            TempoRollPoint::Step => 1,
            TempoRollPoint::Beat => 2,
            TempoRollPoint::Seq => 3,
        }
    }
}

/// Shape of the tempo randomness curve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TempoRandType {
    /// Completely random within the variance window.
    Random,
    /// Bias toward increasing tempo.
    Up,
    /// Bias toward decreasing tempo.
    Down,
    /// Slow oscillation between min and max (sine-like).
    Breathe,
    /// Bounce back and forth between min and max.
    PingPong,
}

impl TempoRandType {
    /// Number of TempoRandType variants.
    pub const COUNT: usize = 5;

    /// Convert a zero-based index (mod 5) to the corresponding TempoRandType variant.
    pub fn from_index(i: usize) -> Self {
        match i % Self::COUNT {
            0 => TempoRandType::Random,
            1 => TempoRandType::Up,
            2 => TempoRandType::Down,
            3 => TempoRandType::Breathe,
            _ => TempoRandType::PingPong,
        }
    }

    /// Return the zero-based index of this TempoRandType variant.
    pub fn to_index(self) -> usize {
        match self {
            TempoRandType::Random => 0,
            TempoRandType::Up => 1,
            TempoRandType::Down => 2,
            TempoRandType::Breathe => 3,
            TempoRandType::PingPong => 4,
        }
    }
}

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
    Param {
        overlay: OverlayMode,
        index: u8,
        value: i64,
    },
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
        Self {
            enabled: false,
            midi_note: 60,
            velocity: 100,
        }
    }
}

/// MIDI event produced by the sequencer or clock.
///
/// `NoteOn` carries `duration_nanos` so that `midi_out.rs` (Step 5) can
/// schedule the matching `NoteOff` without involving the sequencer again.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MidiEvent {
    /// Note-on with embedded duration for NoteOff scheduling.
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
        duration_nanos: u64,
    },
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

    // --- Randomness ---
    /// Probability (0–100) that the tempo randomness roll fires.
    pub tempo_rand: u8,
    /// When the tempo randomness roll fires.
    pub tempo_roll_point: TempoRollPoint,
    /// Maximum tempo variance as a percentage of the base BPM (1–99).
    pub tempo_variance_max: u8,
    /// Shape of the tempo randomness curve.
    pub tempo_rand_type: TempoRandType,
    /// When true, outgoing notes are quantised to the current scale.
    pub scale_quant: bool,

    // --- Shift modifiers ---
    /// Semitone offset applied to every NoteOn. 0 = off.
    ///
    /// ParamValueDelta steps ±1 while `abs(value) ≤ 12`, then ±12 (one octave)
    /// beyond that. Maximum ±96 (8 octaves).
    pub note_modifier: i8,
    /// When true, every step is muted at play time.
    pub skip_modifier: bool,
    /// Velocity offset applied to every NoteOn (-127..=127). 0 = off.
    ///
    /// Clamped to 0–127 at emit time.
    pub velocity_modifier: i8,
    /// User-facing random seed (lower 32 bits of rng_seed, settable via CLI).
    pub rand_seed: u32,
    /// Name of the connected MIDI output port (for title bar display).
    pub midi_device_name: String,
    /// Index of the currently highlighted random parameter (0–7, F3 panel).
    pub selected_rand_param: u8,
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
            tempo_rand: 0,
            tempo_roll_point: TempoRollPoint::Off,
            tempo_variance_max: 10,
            tempo_rand_type: TempoRandType::Random,
            scale_quant: false,
            note_modifier: 0,
            skip_modifier: false,
            velocity_modifier: 0,
            rand_seed: 0x853C_49E6,
            midi_device_name: String::new(),
            selected_rand_param: 0,
        }
    }
}

/// Advance seed and return a pseudo-random u64 (Xorshift64).
pub fn next_rand(seed: &mut u64) -> u64 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *seed = x;
    x
}

/// Returns true with probability `chance/100`. `chance` is clamped to 0–100.
pub fn prob_hit(seed: &mut u64, chance: u8) -> bool {
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
            // 1. Skip modifier: mute the step entirely.
            if self.skip_modifier {
                return None;
            }

            // 2. Compute base note.
            let mut note = step.midi_note;

            // 3. Note modifier + Note Randomness gate.
            //    Apply note_modifier first; then gate on note_rand probability.
            //    If the prob roll misses, revert to the original note.
            //    note_rand == 0  → prob_hit returns false → modifier never applied.
            //    note_rand == 100 → prob_hit returns true → modifier always applied.
            if self.note_modifier != 0 {
                let modified = (note as i16 + self.note_modifier as i16).clamp(0, 127) as u8;
                if prob_hit(&mut self.rng_seed, self.note_rand) {
                    note = modified;
                }
            }

            // 4. Scale Quantization: snap to key after note_modifier is applied.
            //    Apply note_modifier first, then snap_to_key. If the modifier pushes
            //    the note out of key, quantization corrects it.
            if self.scale_quant {
                note = crate::music_theory::snap_to_key(note, self.key, self.mode);
            }

            // 5. Velocity modifier: clamped to 0–127 at emit time.
            let velocity =
                (step.velocity as i16 + self.velocity_modifier as i16).clamp(0, 127) as u8;

            Some(MidiEvent::NoteOn {
                channel: self.midi_channel,
                note,
                velocity,
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
                if matches!(
                    self.pending_edit,
                    PendingEdit::Note { .. } | PendingEdit::Velocity { .. }
                ) {
                    self.pending_edit = PendingEdit::None;
                }
            }
            InputCommand::StepSelectDelta(d) => {
                // Wrapping arithmetic modulo 16.
                let current = self.selected_step as i32;
                let next = ((current + d as i32).rem_euclid(16)) as usize;
                self.selected_step = next;
                if matches!(
                    self.pending_edit,
                    PendingEdit::Note { .. } | PendingEdit::Velocity { .. }
                ) {
                    self.pending_edit = PendingEdit::None;
                }
            }
            InputCommand::NoteDelta(d) => {
                let step = self.selected_step;
                // BUG-035: apply immediately — the new focus model has no overlay/cancel.
                // Repeated presses accumulate on the committed note value.
                let new_note = crate::music_theory::next_note(
                    self.steps[step].midi_note,
                    self.key,
                    self.mode,
                    d,
                );
                self.steps[step].midi_note = new_note;
                // Clear stale pending note edit for this step so Confirm is a no-op.
                if matches!(self.pending_edit, PendingEdit::Note { step: ps, .. } if ps == step) {
                    self.pending_edit = PendingEdit::None;
                }
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
                    PendingEdit::Param {
                        overlay,
                        index,
                        value,
                    } => {
                        // Route to the correct overlay-specific apply method.
                        match overlay {
                            OverlayMode::Regular => self.apply_param_value(index, value),
                            OverlayMode::Shift => self.shift_apply_param_value(index, value),
                        }
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
                self.pending_edit = PendingEdit::Velocity {
                    step,
                    velocity: new_vel,
                };
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
                // Seed from the current committed state value so the pending value
                // is always in the same unit space as the state field.
                let current_value = match self.pending_edit {
                    PendingEdit::Param {
                        index: pi, value, ..
                    } if pi == index => value,
                    _ => match overlay {
                        OverlayMode::Regular => self.committed_param_value(index),
                        OverlayMode::Shift => self.shift_committed_param_value(index),
                    },
                };
                let new_value = match overlay {
                    OverlayMode::Regular => {
                        self.clamped_param_value(index, current_value + d as i64)
                    }
                    OverlayMode::Shift => {
                        self.shift_clamped_param_value(index, current_value + d as i64)
                    }
                };
                self.pending_edit = PendingEdit::Param {
                    overlay,
                    index,
                    value: new_value,
                };
            }
            InputCommand::PlayStop => {
                if self.playing {
                    self.playing = false;
                    self.paused = false;
                } else {
                    self.playing = true;
                }
            }
            InputCommand::NoteModifierSet(s) => {
                self.note_modifier = s;
            }
            InputCommand::SkipModifierToggle => {
                self.skip_modifier = !self.skip_modifier;
            }
            InputCommand::VelocityModifierSet(v) => {
                self.velocity_modifier = v;
            }
            InputCommand::GenerateRandomSequence => {
                self.generate_random_sequence();
            }
            InputCommand::BpmDelta(d) => {
                self.tempo_bpm = (self.tempo_bpm as i32 + d as i32).clamp(20, 300) as u16;
            }
            InputCommand::SeedSet(seed) => {
                self.rand_seed = seed;
                // Xorshift64 with seed 0 is a zero-fixed-point; substitute a
                // known nonzero fallback constant so the RNG is never stuck.
                self.rng_seed = if seed == 0 {
                    0x853C_49E6_853C_49E6u64
                } else {
                    seed as u64 | ((seed as u64) << 32)
                };
            }
            InputCommand::ChannelSet(ch) => {
                self.midi_channel = ch.saturating_sub(1).min(15); // 1-indexed → 0-indexed, clamped to 0–15
            }
            InputCommand::MidiDeviceName(name) => {
                self.midi_device_name = name;
            }
            // SetFocus is handled at the UI layer; state doesn't track focus.
            InputCommand::SetFocus(_) => {}
            // PanelParamSelect: jump to an absolute param index (clamped to 0–7).
            InputCommand::PanelParamSelect(n) => {
                self.selected_param = n.min(7);
            }
            // PanelParamDelta: adjust selected param value immediately (no pending edit).
            // Maps to the F2 (SEQ PARAMS) regular-param map. The hardware param knob
            // also emits this variant since it has no panel context.
            InputCommand::PanelParamDelta(d) => {
                let current = self.committed_param_value(self.selected_param);
                let new_val = self.clamped_param_value(self.selected_param, current + d as i64);
                self.apply_param_value(self.selected_param, new_val);
            }
            // RandParamSelect: jump to an absolute rand-param index (clamped to 0–7).
            InputCommand::RandParamSelect(n) => {
                self.selected_rand_param = n.min(7);
            }
            // RandParamDelta: adjust selected rand-param value via shift param map.
            InputCommand::RandParamDelta(d) => {
                let current = self.shift_committed_param_value(self.selected_rand_param);
                let new_val =
                    self.shift_clamped_param_value(self.selected_rand_param, current + d as i64);
                self.shift_apply_param_value(self.selected_rand_param, new_val);
            }
            // RandAll: randomise notes in-key then randomise velocities.
            InputCommand::RandAll => {
                self.randomise_all();
            }
            // RandVelocities: randomise velocities only; notes unchanged.
            InputCommand::RandVelocities => {
                self.randomise_velocities();
            }
            // NoteSet: set note and velocity for a specific step (no-op if out of range).
            InputCommand::NoteSet { step, midi_note, velocity } => {
                if step < 16 {
                    self.steps[step].midi_note = midi_note;
                    self.steps[step].velocity = velocity;
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
    pub fn committed_param_value(&self, index: u8) -> i64 {
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

    /// Return the current committed state value for shift param `index` as an i64.
    ///
    /// Shift overlay param index map:
    /// 0=note_rand (Stream B — stub returns 0), 1=tempo_rand, 2=tempo_roll_point,
    /// 3=tempo_variance_max, 4=tempo_rand_type, 5=step_rand (Stream B — stub returns 0),
    /// 6=scale_quant, 7=reserved (returns 0).
    pub fn shift_committed_param_value(&self, index: u8) -> i64 {
        match index {
            // 0: note_rand — owned by Stream B; stub until B merges.
            0 => 0,
            1 => self.tempo_rand as i64,
            2 => self.tempo_roll_point.to_index() as i64,
            3 => self.tempo_variance_max as i64,
            4 => self.tempo_rand_type.to_index() as i64,
            // 5: step_rand — owned by Stream B; stub until B merges.
            5 => 0,
            6 => self.scale_quant as i64,
            // 7: reserved — always 0.
            _ => 0,
        }
    }

    /// Clamp or wrap `value` into the valid range for shift param `index`.
    ///
    /// Shift overlay param index map:
    /// 0=note_rand (0–100), 1=tempo_rand (0–100), 2=tempo_roll_point (wraps),
    /// 3=tempo_variance_max (1–99), 4=tempo_rand_type (wraps),
    /// 5=step_rand (0–100), 6=scale_quant (0–1), 7=reserved (no-op).
    pub fn shift_clamped_param_value(&self, index: u8, value: i64) -> i64 {
        match index {
            // 0: note_rand — owned by Stream B; passthrough stub.
            0 => value.clamp(0, 100),
            1 => value.clamp(0, 100),
            2 => value.rem_euclid(TempoRollPoint::COUNT as i64),
            3 => value.clamp(1, 99),
            4 => value.rem_euclid(TempoRandType::COUNT as i64),
            // 5: step_rand — owned by Stream B; passthrough stub.
            5 => value.clamp(0, 100),
            6 => value.clamp(0, 1),
            // 7: reserved — no-op, return value unchanged.
            _ => value,
        }
    }

    /// Write the resolved `value` for shift param `index` back to the matching state field.
    ///
    /// Shift overlay param index map:
    /// 0=note_rand (Stream B — no-op stub), 1=tempo_rand, 2=tempo_roll_point,
    /// 3=tempo_variance_max, 4=tempo_rand_type, 5=step_rand (Stream B — no-op stub),
    /// 6=scale_quant, 7=reserved (no-op).
    pub fn shift_apply_param_value(&mut self, index: u8, value: i64) {
        match index {
            // 0: note_rand — owned by Stream B; no-op until B merges.
            0 => {}
            1 => self.tempo_rand = value as u8,
            2 => self.tempo_roll_point = TempoRollPoint::from_index(value as usize),
            3 => self.tempo_variance_max = value as u8,
            4 => self.tempo_rand_type = TempoRandType::from_index(value as usize),
            // 5: step_rand — owned by Stream B; no-op until B merges.
            5 => {}
            6 => self.scale_quant = value != 0,
            // 7: reserved — no-op.
            _ => {}
        }
    }

    /// Randomise all 16 steps' notes to in-key values within MIDI range 48–84.
    ///
    /// Uses `next_rand(&mut self.rng_seed)` for each step. Enabled flags are
    /// left unchanged — only `midi_note` is updated.
    /// Generated note range: 48–84 (C3–C6, 3 octaves). The raw random value
    /// is mapped to this range, then snapped to the current key/mode via
    /// `music_theory::snap_to_key`.
    fn generate_random_sequence(&mut self) {
        for step in self.steps.iter_mut() {
            let raw = next_rand(&mut self.rng_seed);
            let note_in_range = (raw % 37) as u8 + 48; // 48..=84
            step.midi_note = crate::music_theory::snap_to_key(note_in_range, self.key, self.mode);
        }
    }

    /// Randomise all 16 step velocities to a value in 40..=127 using `rng_seed`.
    ///
    /// Notes and enabled flags are left unchanged.
    /// Velocity formula: `(raw % 88) as u8 + 40`.
    fn randomise_velocities(&mut self) {
        for step in self.steps.iter_mut() {
            let raw = next_rand(&mut self.rng_seed);
            step.velocity = (raw % 88) as u8 + 40;
        }
    }

    /// Randomise all 16 step notes in-key then randomise all velocities.
    ///
    /// Calls `generate_random_sequence()` first (notes), then `randomise_velocities()`
    /// so both operations share the same `rng_seed` chain.
    fn randomise_all(&mut self) {
        self.generate_random_sequence();
        self.randomise_velocities();
    }

    /// Re-snap all 16 step notes to the nearest note in the current key and mode.
    ///
    /// Called immediately after `self.key` or `self.mode` is updated.
    /// No heap allocation: operates on the fixed-size `steps` array in place.
    fn snap_all_steps_to_key(&mut self) {
        for step in self.steps.iter_mut() {
            step.midi_note = crate::music_theory::snap_to_key(step.midi_note, self.key, self.mode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputCommand;

    #[test]
    fn bpm_delta_clamps_to_range() {
        let mut state = SequencerState::default();
        state.tempo_bpm = 120;

        // Delta that would go below minimum
        state.apply_command(InputCommand::BpmDelta(-127));
        assert_eq!(state.tempo_bpm, 20, "BPM should clamp to 20 at minimum");

        // Delta that would go above maximum
        state.apply_command(InputCommand::BpmDelta(127));
        assert_eq!(state.tempo_bpm, 147, "BPM should accumulate from 20");

        // Reset and verify upper clamp
        state.tempo_bpm = 290;
        state.apply_command(InputCommand::BpmDelta(20));
        assert_eq!(state.tempo_bpm, 300, "BPM should clamp to 300 at maximum");
    }

    #[test]
    fn seed_set_updates_both_fields() {
        let mut state = SequencerState::default();
        let seed: u32 = 0xABCD;
        state.apply_command(InputCommand::SeedSet(seed));

        assert_eq!(
            state.rand_seed, 0xABCD,
            "rand_seed must match the seed value"
        );
        let expected_rng = seed as u64 | ((seed as u64) << 32);
        assert_eq!(
            state.rng_seed, expected_rng,
            "rng_seed must be seed | (seed << 32)"
        );
    }

    #[test]
    fn channel_set_converts_1_indexed() {
        let mut state = SequencerState::default();

        // 1-indexed input 1 → 0-indexed 0
        state.apply_command(InputCommand::ChannelSet(1));
        assert_eq!(
            state.midi_channel, 0,
            "ChannelSet(1) should store midi_channel = 0"
        );

        // 1-indexed input 16 → 0-indexed 15
        state.apply_command(InputCommand::ChannelSet(16));
        assert_eq!(
            state.midi_channel, 15,
            "ChannelSet(16) should store midi_channel = 15"
        );

        // Saturating sub: ChannelSet(0) stays at 0 (not underflows)
        state.apply_command(InputCommand::ChannelSet(0));
        assert_eq!(state.midi_channel, 0, "ChannelSet(0) should saturate to 0");
    }

    #[test]
    fn midi_device_name_set() {
        let mut state = SequencerState::default();
        assert_eq!(
            state.midi_device_name, "",
            "midi_device_name should be empty by default"
        );

        state.apply_command(InputCommand::MidiDeviceName("IAC Driver".to_string()));
        assert_eq!(state.midi_device_name, "IAC Driver");

        state.apply_command(InputCommand::MidiDeviceName(String::new()));
        assert_eq!(state.midi_device_name, "");
    }

    #[test]
    fn rand_seed_default_value() {
        let state = SequencerState::default();
        assert_eq!(state.rand_seed, 0x853C_49E6);
    }

    #[test]
    fn seed_set_zero_uses_fallback_nonzero_rng_seed() {
        let mut state = SequencerState::default();
        state.apply_command(InputCommand::SeedSet(0));

        assert_eq!(
            state.rand_seed, 0,
            "rand_seed must store the raw seed value"
        );
        assert_ne!(
            state.rng_seed, 0,
            "rng_seed must be nonzero when seed=0 to avoid xorshift64 zero-fixed-point"
        );
        assert_eq!(
            state.rng_seed, 0x853C_49E6_853C_49E6u64,
            "rng_seed must use the fallback constant when seed=0"
        );
    }

    #[test]
    fn channel_set_clamps_above_16() {
        let mut state = SequencerState::default();

        // Values above 16 must clamp to channel 15 (0-indexed)
        state.apply_command(InputCommand::ChannelSet(17));
        assert_eq!(
            state.midi_channel, 15,
            "ChannelSet(17) should clamp to midi_channel = 15"
        );

        state.apply_command(InputCommand::ChannelSet(255));
        assert_eq!(
            state.midi_channel, 15,
            "ChannelSet(255) should clamp to midi_channel = 15"
        );
    }

    #[test]
    fn bpm_delta_boundary_at_minimum() {
        let mut state = SequencerState::default();
        // Start exactly at the minimum boundary.
        state.tempo_bpm = 20;

        // Negative delta at minimum stays at 20.
        state.apply_command(InputCommand::BpmDelta(-1));
        assert_eq!(state.tempo_bpm, 20, "BPM already at 20 must not go below");

        state.apply_command(InputCommand::BpmDelta(-127));
        assert_eq!(
            state.tempo_bpm, 20,
            "Large negative delta at 20 must clamp to 20"
        );
    }

    #[test]
    fn bpm_delta_boundary_at_maximum() {
        let mut state = SequencerState::default();
        // Start exactly at the maximum boundary.
        state.tempo_bpm = 300;

        // Positive delta at maximum stays at 300.
        state.apply_command(InputCommand::BpmDelta(1));
        assert_eq!(
            state.tempo_bpm, 300,
            "BPM already at 300 must not exceed 300"
        );

        state.apply_command(InputCommand::BpmDelta(127));
        assert_eq!(
            state.tempo_bpm, 300,
            "Large positive delta at 300 must clamp to 300"
        );
    }

    #[test]
    fn bpm_delta_arithmetic() {
        let mut state = SequencerState::default();
        state.tempo_bpm = 100;

        // Positive delta within range.
        state.apply_command(InputCommand::BpmDelta(25));
        assert_eq!(state.tempo_bpm, 125, "100 + 25 = 125");

        // Negative delta within range.
        state.apply_command(InputCommand::BpmDelta(-25));
        assert_eq!(state.tempo_bpm, 100, "125 - 25 = 100");

        // Zero delta leaves BPM unchanged.
        state.apply_command(InputCommand::BpmDelta(0));
        assert_eq!(state.tempo_bpm, 100, "delta 0 must leave BPM unchanged");
    }

    #[test]
    fn seed_set_nonzero_rng_seed_formula() {
        let mut state = SequencerState::default();

        // Verify formula: rng_seed = lo | (lo << 32) for a variety of nonzero seeds.
        for &seed in &[1u32, 0xFFFF_FFFFu32, 0x1234_5678u32] {
            state.apply_command(InputCommand::SeedSet(seed));
            assert_eq!(
                state.rand_seed, seed,
                "rand_seed must equal the supplied seed"
            );
            let lo = seed as u64;
            let expected = lo | (lo << 32);
            assert_eq!(
                state.rng_seed, expected,
                "rng_seed for seed={seed:#010x} must be lo | (lo << 32)"
            );
        }
    }

    #[test]
    fn midi_device_name_overwrite_existing() {
        let mut state = SequencerState::default();

        state.apply_command(InputCommand::MidiDeviceName("First Device".to_string()));
        assert_eq!(state.midi_device_name, "First Device");

        // Overwrite with a different non-empty name.
        state.apply_command(InputCommand::MidiDeviceName("Second Device".to_string()));
        assert_eq!(
            state.midi_device_name, "Second Device",
            "MidiDeviceName must overwrite the previously stored name"
        );
    }

    #[test]
    fn midi_device_name_empty_string() {
        let mut state = SequencerState::default();

        state.apply_command(InputCommand::MidiDeviceName("IAC Driver".to_string()));
        assert_eq!(state.midi_device_name, "IAC Driver");

        // Overwrite with empty string.
        state.apply_command(InputCommand::MidiDeviceName(String::new()));
        assert_eq!(
            state.midi_device_name, "",
            "MidiDeviceName with empty string must clear the stored name"
        );
    }

    // ── BUG-031: PanelParamSelect and PanelParamDelta ────────────────────────

    #[test]
    fn panel_param_select_sets_selected_param() {
        let mut state = SequencerState::default();
        state.apply_command(InputCommand::PanelParamSelect(3));
        assert_eq!(state.selected_param, 3, "PanelParamSelect(3) should set selected_param to 3");
    }

    #[test]
    fn panel_param_select_clamps_to_7() {
        let mut state = SequencerState::default();
        state.apply_command(InputCommand::PanelParamSelect(255));
        assert_eq!(
            state.selected_param, 7,
            "PanelParamSelect(255) should clamp selected_param to 7"
        );
    }

    #[test]
    fn panel_param_delta_adjusts_swing() {
        // Param index 2 = swing (-50..=50).
        let mut state = SequencerState::default();
        state.swing = 10;
        // Select param index 2 (Swing).
        state.apply_command(InputCommand::PanelParamSelect(2));
        // Apply a delta of +5.
        state.apply_command(InputCommand::PanelParamDelta(5));
        assert_eq!(
            state.swing, 15,
            "PanelParamDelta(5) with Swing selected should increase swing by 5"
        );
    }

    #[test]
    fn panel_param_delta_clamps_at_boundary() {
        // Swing at max (50) + delta(10) must stay at 50.
        let mut state = SequencerState::default();
        state.swing = 50;
        state.apply_command(InputCommand::PanelParamSelect(2));
        state.apply_command(InputCommand::PanelParamDelta(10));
        assert_eq!(
            state.swing, 50,
            "PanelParamDelta(10) when swing is already at 50 should clamp to 50"
        );
    }

    // ── RandParamSelect and RandParamDelta (Finding C) ───────────────────────

    #[test]
    fn rand_param_select_sets_selected_rand_param() {
        let mut state = SequencerState::default();
        state.apply_command(InputCommand::RandParamSelect(2));
        assert_eq!(
            state.selected_rand_param, 2,
            "RandParamSelect(2) should set selected_rand_param to 2"
        );
    }

    #[test]
    fn rand_param_select_clamps_to_7() {
        let mut state = SequencerState::default();
        state.apply_command(InputCommand::RandParamSelect(255));
        assert_eq!(
            state.selected_rand_param, 7,
            "RandParamSelect(255) should clamp selected_rand_param to 7"
        );
    }

    #[test]
    fn rand_param_delta_adjusts_tempo_rand() {
        // Shift param index 1 = tempo_rand (0–100).
        let mut state = SequencerState::default();
        state.tempo_rand = 10;
        state.apply_command(InputCommand::RandParamSelect(1));
        state.apply_command(InputCommand::RandParamDelta(3));
        assert_eq!(
            state.tempo_rand, 13,
            "RandParamDelta(3) with tempo_rand=10 should produce tempo_rand=13"
        );
    }

    #[test]
    fn rand_param_delta_does_not_touch_regular_params() {
        // Adjusting a rand param must not affect regular params (key, swing, etc.).
        let mut state = SequencerState::default();
        let original_key = state.key;
        let original_swing = state.swing;
        state.apply_command(InputCommand::RandParamSelect(1));
        state.apply_command(InputCommand::RandParamDelta(1));
        assert_eq!(state.key, original_key, "RandParamDelta must not change key");
        assert_eq!(state.swing, original_swing, "RandParamDelta must not change swing");
    }

    #[test]
    fn panel_param_delta_does_not_touch_rand_params() {
        // Adjusting a regular param (index 2 = swing) must not affect rand params.
        let mut state = SequencerState::default();
        let original_tempo_rand = state.tempo_rand;
        state.apply_command(InputCommand::PanelParamSelect(2));
        state.apply_command(InputCommand::PanelParamDelta(1));
        assert_eq!(
            state.tempo_rand, original_tempo_rand,
            "PanelParamDelta must not change tempo_rand"
        );
    }

    // ── BUG-035: NoteDelta must apply immediately without Confirm ────────────

    #[test]
    fn note_delta_applies_immediately_without_confirm() {
        let mut state = SequencerState::default();
        let original_note = state.steps[0].midi_note;
        // Apply NoteDelta — note must be written immediately, no Confirm needed.
        state.apply_command(InputCommand::NoteDelta(1));
        assert_ne!(
            state.steps[0].midi_note, original_note,
            "NoteDelta(1) must update steps[selected_step].midi_note immediately"
        );
    }

    #[test]
    fn note_delta_clears_pending_note_edit() {
        let mut state = SequencerState::default();
        // Manually install a stale PendingEdit::Note for the current step.
        state.pending_edit = PendingEdit::Note {
            step: 0,
            midi_note: 42,
        };
        state.apply_command(InputCommand::NoteDelta(1));
        assert_eq!(
            state.pending_edit,
            PendingEdit::None,
            "NoteDelta must clear a stale PendingEdit::Note for the same step"
        );
    }

    #[test]
    fn note_delta_does_not_affect_other_steps() {
        let mut state = SequencerState::default();
        // selected_step is 0 by default; record step 1's note.
        let step1_note = state.steps[1].midi_note;
        state.apply_command(InputCommand::NoteDelta(1));
        assert_eq!(
            state.steps[1].midi_note, step1_note,
            "NoteDelta must only modify the selected step, not other steps"
        );
    }

    // ── RandAll ─────────────────────────────────────────────────────────────

    #[test]
    fn rand_all_sets_notes_in_range_and_velocities_in_range() {
        let mut state = SequencerState::default();
        let seed_before = state.rng_seed;
        state.apply_command(InputCommand::RandAll);

        // rng_seed must have advanced
        assert_ne!(state.rng_seed, seed_before, "RandAll must advance rng_seed");

        for (i, step) in state.steps.iter().enumerate() {
            assert!(
                (48..=84).contains(&step.midi_note),
                "step {i} midi_note {} out of 48..=84",
                step.midi_note
            );
            assert!(
                (40..=127).contains(&step.velocity),
                "step {i} velocity {} out of 40..=127",
                step.velocity
            );
        }
    }

    // ── RandVelocities ───────────────────────────────────────────────────────

    #[test]
    fn rand_velocities_changes_velocities_only() {
        let mut state = SequencerState::default();
        // Record original notes
        let original_notes: [u8; 16] = core::array::from_fn(|i| state.steps[i].midi_note);

        state.apply_command(InputCommand::RandVelocities);

        // Notes must be unchanged
        for (i, step) in state.steps.iter().enumerate() {
            assert_eq!(
                step.midi_note, original_notes[i],
                "RandVelocities must not change step {i} midi_note"
            );
            assert!(
                (40..=127).contains(&step.velocity),
                "step {i} velocity {} out of 40..=127",
                step.velocity
            );
        }
    }

    // ── NoteSet ──────────────────────────────────────────────────────────────

    #[test]
    fn note_set_step_3_sets_correct_fields() {
        let mut state = SequencerState::default();
        state.apply_command(InputCommand::NoteSet { step: 3, midi_note: 72, velocity: 100 });

        assert_eq!(state.steps[3].midi_note, 72, "NoteSet must write midi_note to step 3");
        assert_eq!(state.steps[3].velocity, 100, "NoteSet must write velocity to step 3");

        // Other steps must be unchanged
        for i in 0..16 {
            if i == 3 {
                continue;
            }
            assert_eq!(
                state.steps[i].midi_note,
                StepData::default().midi_note,
                "NoteSet must not alter step {i} midi_note"
            );
            assert_eq!(
                state.steps[i].velocity,
                StepData::default().velocity,
                "NoteSet must not alter step {i} velocity"
            );
        }
    }

    #[test]
    fn note_set_out_of_range_is_noop() {
        let mut state = SequencerState::default();
        let original: [StepData; 16] = state.steps;

        // step = 16 is out of range — must be a no-op (no panic)
        state.apply_command(InputCommand::NoteSet { step: 16, midi_note: 99, velocity: 99 });

        for i in 0..16 {
            assert_eq!(
                state.steps[i].midi_note, original[i].midi_note,
                "NoteSet(step=16) must not modify step {i} midi_note"
            );
            assert_eq!(
                state.steps[i].velocity, original[i].velocity,
                "NoteSet(step=16) must not modify step {i} velocity"
            );
        }
    }

    // ── RNG seed chain interaction ───────────────────────────────────────────

    /// RandVelocities must advance rng_seed — it calls next_rand 16 times.
    #[test]
    fn rand_velocities_advances_rng_seed() {
        let mut state = SequencerState::default();
        let seed_before = state.rng_seed;
        state.apply_command(InputCommand::RandVelocities);
        assert_ne!(
            state.rng_seed, seed_before,
            "RandVelocities must advance rng_seed"
        );
    }

    /// RandAll followed by RandVelocities must produce different velocities than
    /// RandVelocities alone, because the seed position differs after RandAll
    /// consumed 32 next_rand calls (16 for notes + 16 for velocities).
    #[test]
    fn rand_all_then_rand_velocities_differs_from_rand_velocities_alone() {
        // Scenario A: RandVelocities only
        let mut state_a = SequencerState::default();
        state_a.apply_command(InputCommand::RandVelocities);
        let velocities_a: [u8; 16] = core::array::from_fn(|i| state_a.steps[i].velocity);

        // Scenario B: RandAll then RandVelocities — seed chain is further advanced
        let mut state_b = SequencerState::default();
        state_b.apply_command(InputCommand::RandAll);
        state_b.apply_command(InputCommand::RandVelocities);
        let velocities_b: [u8; 16] = core::array::from_fn(|i| state_b.steps[i].velocity);

        assert_ne!(
            velocities_a, velocities_b,
            "RandAll advances rng_seed by 32 calls so the subsequent \
             RandVelocities seed position must differ"
        );
    }

    /// RandAll consumes the seed chain in order: notes first, then velocities.
    /// Verify by replicating the expected seed advancement manually: after
    /// RandAll the seed must equal the state obtained by calling next_rand 32
    /// times on the initial seed (16 note calls + 16 velocity calls).
    #[test]
    fn rand_all_seed_chain_order_notes_then_velocities() {
        let mut state = SequencerState::default();
        let mut expected_seed = state.rng_seed;

        // Advance expected_seed by 32 steps (matches generate_random_sequence + randomise_velocities)
        for _ in 0..32 {
            next_rand(&mut expected_seed);
        }

        state.apply_command(InputCommand::RandAll);

        assert_eq!(
            state.rng_seed, expected_seed,
            "After RandAll, rng_seed must equal seed advanced by exactly 32 next_rand calls"
        );
    }

    /// Velocity range boundary check: formula (raw % 88) as u8 + 40 must always
    /// produce a value in 40..=127. This is an arithmetic property; verify it
    /// holds across the extremes of the modulus: 0 and 87.
    #[test]
    fn velocity_formula_bounds_are_40_to_127() {
        // min: 0 % 88 = 0  → 0 + 40 = 40
        let min_vel = (0u64 % 88) as u8 + 40;
        // max: 87 % 88 = 87 → 87 + 40 = 127
        let max_vel = (87u64 % 88) as u8 + 40;
        assert_eq!(min_vel, 40, "velocity formula minimum must be 40");
        assert_eq!(max_vel, 127, "velocity formula maximum must be 127");
        assert!((40..=127).contains(&min_vel));
        assert!((40..=127).contains(&max_vel));
    }

    /// NoteSet with step = usize::MAX must be a no-op (no panic).
    #[test]
    fn note_set_usize_max_is_noop() {
        let mut state = SequencerState::default();
        let original: [StepData; 16] = state.steps;

        state.apply_command(InputCommand::NoteSet {
            step: usize::MAX,
            midi_note: 60,
            velocity: 80,
        });

        for i in 0..16 {
            assert_eq!(
                state.steps[i].midi_note, original[i].midi_note,
                "NoteSet(step=usize::MAX) must not modify step {i} midi_note"
            );
            assert_eq!(
                state.steps[i].velocity, original[i].velocity,
                "NoteSet(step=usize::MAX) must not modify step {i} velocity"
            );
        }
    }

    /// NoteSet at step 0 and step 15 (boundary steps) must write correctly.
    #[test]
    fn note_set_boundary_steps_0_and_15() {
        let mut state = SequencerState::default();

        state.apply_command(InputCommand::NoteSet { step: 0, midi_note: 48, velocity: 40 });
        assert_eq!(state.steps[0].midi_note, 48, "NoteSet step 0 must set midi_note");
        assert_eq!(state.steps[0].velocity, 40, "NoteSet step 0 must set velocity");

        state.apply_command(InputCommand::NoteSet { step: 15, midi_note: 84, velocity: 127 });
        assert_eq!(state.steps[15].midi_note, 84, "NoteSet step 15 must set midi_note");
        assert_eq!(state.steps[15].velocity, 127, "NoteSet step 15 must set velocity");
    }
}
