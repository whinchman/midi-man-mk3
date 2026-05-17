/// CLI argument parsing — exposed for integration tests.
pub mod cli;
/// Real-time clock thread driving the sequencer forward.
pub mod clock;
/// USB HID report structures for the MIDI controller.
pub mod hid;
/// Input command abstraction — InputCommand and OverlayMode enums.
pub mod input;
/// MIDI output thread — sends NoteOn/NoteOff via midir.
/// Always compiled; hw-io–only items are individually gated within the module.
pub mod midi_out;
/// Music theory primitives: keys, modes, scale tables, and note navigation.
pub mod music_theory;
/// Sequencer module — higher-level engine wiring and re-exports.
pub mod sequencer;
/// Pattern and song data model with TOML serialization and file I/O.
pub mod pattern;
/// Sequencer state — the shared truth for all threads.
pub mod state;
/// Terminal UI — ratatui render loop with keyboard event handling.
/// Pure helpers (UiState, handle_cli_submit, push_log) are always compiled.
/// hw-io–only items (run_ui, TerminalGuard, crossterm glue) are gated inside.
pub mod ui;
/// Ratatui render logic — pure rendering, no crossterm dependency.
pub mod ui_render;
