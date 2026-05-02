/// Input command abstraction — InputCommand and OverlayMode enums.
pub mod input;
/// USB HID report structures for the MIDI controller.
pub mod hid;
/// Music theory primitives: keys, modes, scale tables, and note navigation.
pub mod music_theory;
/// Sequencer state — the shared truth for all threads.
pub mod state;
/// Sequencer module — higher-level engine wiring and re-exports.
pub mod sequencer;
/// Real-time clock thread driving the sequencer forward.
pub mod clock;
/// MIDI output thread — sends NoteOn/NoteOff via midir.
#[cfg(feature = "hw-io")]
pub mod midi_out;
/// Ratatui render logic — pure rendering, no crossterm dependency.
pub mod ui_render;
/// Terminal UI — ratatui render loop with keyboard event handling.
#[cfg(feature = "hw-io")]
pub mod ui;
/// Unit tests for the terminal UI render logic (TestBackend, no real terminal).
pub mod ui_tests;
