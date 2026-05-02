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
/// Keyboard UI event loop — translates crossterm KeyEvent to InputCommand.
#[cfg(feature = "hw-io")]
pub mod ui;
