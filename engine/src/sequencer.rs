//! Sequencer module — re-exports the core state types and serves as the
//! integration point for higher-level engine wiring (Step 9).

pub use crate::state::{MidiEvent, OverlayMode, PendingEdit, SequencerState, StepData, StepSize};
