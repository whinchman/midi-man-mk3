pub mod hid;
/// Music theory primitives: keys, modes, scale tables, and note navigation.
pub mod music_theory;
/// Sequencer state — the shared truth for all threads.
pub mod state;
/// Sequencer module — higher-level engine wiring and re-exports.
pub mod sequencer;
