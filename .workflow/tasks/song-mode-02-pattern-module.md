Name: pattern-module
Type: coder
Status: pending
Repo: /home/whinchman/experiments/midi-man-mk3
Parallel Group: 2
Feature Branch: feature/song-mode
Branch: feature/song-mode/pattern-module
Base Branch: feature/song-mode
Goal: Create engine/src/pattern.rs with all serializable data structs, TOML file I/O helpers, and roundtrip unit tests.

Context:
  Files to create/modify:
    engine/src/pattern.rs  (new file — all content below)
    engine/src/lib.rs      (add `pub mod pattern;` after the existing `pub mod state;` line)

  ## Structs to implement

  All structs derive `Serialize, Deserialize, Clone, Debug` from serde.

  ```rust
  pub struct StepDataSerial {
      pub enabled: bool,
      pub midi_note: u8,
      pub velocity: u8,
  }

  pub struct PatternData {
      pub name: String,
      pub steps: Vec<StepDataSerial>,   // exactly 16 elements
      pub key: String,                  // e.g. "C", "C#"
      pub mode: String,                 // e.g. "Major", "NaturalMinor"
      pub tempo_bpm: u16,
      pub swing: i8,
      pub step_size: String,            // e.g. "1/16", "1/8"
      pub loop_in: u8,
      pub loop_out: u8,
      pub loop_active: bool,
      pub midi_channel: u8,
      pub scale_quant: bool,
      pub note_modifier: i8,
      pub velocity_modifier: i8,
      pub skip_modifier: bool,
      pub tempo_rand: u8,
      pub tempo_roll_point: String,
      pub tempo_variance_max: u8,
      pub tempo_rand_type: String,
      pub step_rand: u8,
      pub note_rand: u8,
  }

  pub struct PatternRef {
      pub filename: String,   // just filename, e.g. "verse-A.pat.toml"
      pub repeats: u8,        // 1 = play once; default 1
  }

  pub struct Song {
      pub name: String,
      pub slots: Vec<PatternRef>,
  }
  ```

  ## File I/O helpers

  Use `std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())` to derive the
  config root. Do NOT add the `dirs` crate.

  ```rust
  pub fn pattern_dir() -> PathBuf  // ~/.config/midi-man-mk3/patterns/
  pub fn song_dir() -> PathBuf     // ~/.config/midi-man-mk3/songs/

  pub fn save_pattern(data: &PatternData, filename: &str) -> Result<(), String>
  pub fn load_pattern(filename: &str) -> Result<PatternData, String>
  pub fn save_song(song: &Song, filename: &str) -> Result<(), String>
  pub fn load_song(filename: &str) -> Result<Song, String>
  ```

  All four I/O helpers must call `std::fs::create_dir_all` on the relevant dir
  before any file operation. Errors are mapped to `String` via `.map_err(|e| e.to_string())`.

  ## Conversion helpers

  ```rust
  pub fn pattern_from_state(state: &SequencerState, name: &str) -> PatternData
  pub fn apply_pattern_to_state(data: &PatternData, state: &mut SequencerState) -> Result<(), String>
  ```

  `pattern_from_state` reads every serializable field from `SequencerState` and converts
  enum variants to their string forms.

  `apply_pattern_to_state` parses string-encoded fields back to their enum variants.
  Return `Err("unrecognized key: <value>")` (and similar) if any string is unknown.

  Private helpers for each enum conversion (all must be exhaustive):
    key_to_str / str_to_key  (crate::music_theory::Key — see existing variants in state.rs)
    mode_to_str / str_to_mode (crate::music_theory::Mode)
    step_size_to_str / str_to_step_size (crate::state::StepSize — Whole/Half/Quarter/Eighth/Sixteenth/ThirtySecond)
    tempo_roll_point_to_str / str_to_tempo_roll_point (Off/Step/Beat/Seq)
    tempo_rand_type_to_str / str_to_tempo_rand_type (Random/Up/Down/Breathe/PingPong)

  ## TOML file extensions
    Pattern files: `<name>.pat.toml` (pattern_dir / <filename>)
    Song files:    `<name>.song.toml` (song_dir / <filename>)

  ## Unit tests (no disk I/O)
  Tests go in a `#[cfg(test)] mod tests` block at the bottom of pattern.rs.
  Use `toml::to_string` / `toml::from_str` directly — no filesystem calls.

  Test cases required:
  - `pattern_roundtrip`: build a default `SequencerState`, call `pattern_from_state`,
    serialize to TOML string with `toml::to_string`, deserialize back with
    `toml::from_str::<PatternData>`, call `apply_pattern_to_state` on a fresh state,
    assert tempo_bpm, key string, step count (16), and a step's midi_note all survive.
  - `song_roundtrip`: construct a `Song` with 3 `PatternRef` slots, serialize and
    deserialize, assert `slots.len() == 3` and filenames are preserved.
  - `unknown_key_returns_err`: call `str_to_key("ZZZZ")` and assert it returns `Err`.

  ## Existing enum locations (for imports)
  - `crate::music_theory::{Key, Mode}` — Key and Mode enums
  - `crate::state::{SequencerState, StepSize, TempoRollPoint, TempoRandType}`

Acceptance Criteria:
  - [ ] engine/src/pattern.rs exists with all four structs, all I/O helpers, all conversion helpers
  - [ ] engine/src/lib.rs has `pub mod pattern;`
  - [ ] `cargo test -p engine pattern` passes (all three unit tests in pattern.rs)
  - [ ] `cargo build -p engine` succeeds with no warnings about unused items in pattern.rs
  - [ ] All 16 steps survive the roundtrip test (count check)
  - [ ] Disk I/O functions use `create_dir_all` before writing

Dependencies: cargo-deps
