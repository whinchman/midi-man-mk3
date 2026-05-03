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
                let new_note = crate::music_theory::next_note(current_note, self.key, self.mode, d);
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
}

