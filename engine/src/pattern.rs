//! Pattern and song data model with TOML serialization and file I/O.
//!
//! `PatternData` is the serializable snapshot of a `SequencerState`.
//! `Song` is an ordered list of `PatternRef` slots.
//!
//! File layout:
//!   patterns → `~/.config/midi-man-mk3/patterns/<name>.pat.toml`
//!   songs    → `~/.config/midi-man-mk3/songs/<name>.song.toml`

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::music_theory::{Key, Mode};
use crate::state::{SequencerState, StepSize, TempoRandType, TempoRollPoint};

// ── Structs ──────────────────────────────────────────────────────────────────

/// Serializable snapshot of a single sequencer step.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StepDataSerial {
    pub enabled: bool,
    pub midi_note: u8,
    pub velocity: u8,
}

/// Serializable snapshot of a full `SequencerState`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PatternData {
    pub name: String,
    /// Exactly 16 elements.
    pub steps: Vec<StepDataSerial>,
    /// Key as a string, e.g. "C", "Cs".
    pub key: String,
    /// Mode as a string, e.g. "Major", "NaturalMinor".
    pub mode: String,
    pub tempo_bpm: u16,
    pub swing: i8,
    /// Step size as a string, e.g. "1/16", "1/8".
    pub step_size: String,
    pub loop_in: u8,
    pub loop_out: u8,
    pub loop_active: bool,
    pub midi_channel: u8,
    pub scale_quant: bool,
    pub note_modifier: i8,
    pub velocity_modifier: i8,
    pub skip_modifier: bool,
    pub tempo_rand: u8,
    /// TempoRollPoint as a string, e.g. "Off", "Step".
    pub tempo_roll_point: String,
    pub tempo_variance_max: u8,
    /// TempoRandType as a string, e.g. "Random", "PingPong".
    pub tempo_rand_type: String,
    pub step_rand: u8,
    pub note_rand: u8,
}

/// A reference to a pattern file within a song, with a repeat count.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PatternRef {
    /// Filename only, e.g. "verse-A.pat.toml".
    pub filename: String,
    /// Number of times to play the pattern (1 = play once).
    pub repeats: u8,
}

/// An ordered list of pattern references forming a song.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Song {
    pub name: String,
    pub slots: Vec<PatternRef>,
}

// ── Directory helpers ─────────────────────────────────────────────────────────

/// Returns `~/.config/midi-man-mk3/patterns/`.
pub fn pattern_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".config")
        .join("midi-man-mk3")
        .join("patterns")
}

/// Returns `~/.config/midi-man-mk3/songs/`.
pub fn song_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".config")
        .join("midi-man-mk3")
        .join("songs")
}

// ── File I/O helpers ──────────────────────────────────────────────────────────

/// Serialize `data` to TOML and write it to `<pattern_dir>/<filename>`.
///
/// Creates the directory if it does not exist.
pub fn save_pattern(data: &PatternData, filename: &str) -> Result<(), String> {
    let dir = pattern_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(filename);
    let content = toml::to_string(data).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

/// Read `<pattern_dir>/<filename>` and deserialize it as a `PatternData`.
///
/// Creates the directory if it does not exist.
pub fn load_pattern(filename: &str) -> Result<PatternData, String> {
    let dir = pattern_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(filename);
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str::<PatternData>(&content).map_err(|e| e.to_string())
}

/// Serialize `song` to TOML and write it to `<song_dir>/<filename>`.
///
/// Creates the directory if it does not exist.
pub fn save_song(song: &Song, filename: &str) -> Result<(), String> {
    let dir = song_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(filename);
    let content = toml::to_string(song).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

/// Read `<song_dir>/<filename>` and deserialize it as a `Song`.
///
/// Creates the directory if it does not exist.
pub fn load_song(filename: &str) -> Result<Song, String> {
    let dir = song_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(filename);
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str::<Song>(&content).map_err(|e| e.to_string())
}

// ── Enum conversion helpers ───────────────────────────────────────────────────

fn key_to_str(key: Key) -> &'static str {
    match key {
        Key::C => "C",
        Key::Cs => "Cs",
        Key::D => "D",
        Key::Ds => "Ds",
        Key::E => "E",
        Key::F => "F",
        Key::Fs => "Fs",
        Key::G => "G",
        Key::Gs => "Gs",
        Key::A => "A",
        Key::As => "As",
        Key::B => "B",
    }
}

fn str_to_key(s: &str) -> Result<Key, String> {
    match s {
        "C" => Ok(Key::C),
        "Cs" => Ok(Key::Cs),
        "D" => Ok(Key::D),
        "Ds" => Ok(Key::Ds),
        "E" => Ok(Key::E),
        "F" => Ok(Key::F),
        "Fs" => Ok(Key::Fs),
        "G" => Ok(Key::G),
        "Gs" => Ok(Key::Gs),
        "A" => Ok(Key::A),
        "As" => Ok(Key::As),
        "B" => Ok(Key::B),
        other => Err(format!("unrecognized key: {}", other)),
    }
}

fn mode_to_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Major => "Major",
        Mode::NaturalMinor => "NaturalMinor",
        Mode::Dorian => "Dorian",
        Mode::Phrygian => "Phrygian",
        Mode::Lydian => "Lydian",
        Mode::Mixolydian => "Mixolydian",
        Mode::Locrian => "Locrian",
        Mode::HarmonicMinor => "HarmonicMinor",
        Mode::MelodicMinor => "MelodicMinor",
    }
}

fn str_to_mode(s: &str) -> Result<Mode, String> {
    match s {
        "Major" => Ok(Mode::Major),
        "NaturalMinor" => Ok(Mode::NaturalMinor),
        "Dorian" => Ok(Mode::Dorian),
        "Phrygian" => Ok(Mode::Phrygian),
        "Lydian" => Ok(Mode::Lydian),
        "Mixolydian" => Ok(Mode::Mixolydian),
        "Locrian" => Ok(Mode::Locrian),
        "HarmonicMinor" => Ok(Mode::HarmonicMinor),
        "MelodicMinor" => Ok(Mode::MelodicMinor),
        other => Err(format!("unrecognized mode: {}", other)),
    }
}

fn step_size_to_str(sz: StepSize) -> &'static str {
    match sz {
        StepSize::Whole => "1/1",
        StepSize::Half => "1/2",
        StepSize::Quarter => "1/4",
        StepSize::Eighth => "1/8",
        StepSize::Sixteenth => "1/16",
        StepSize::ThirtySecond => "1/32",
    }
}

fn str_to_step_size(s: &str) -> Result<StepSize, String> {
    match s {
        "1/1" => Ok(StepSize::Whole),
        "1/2" => Ok(StepSize::Half),
        "1/4" => Ok(StepSize::Quarter),
        "1/8" => Ok(StepSize::Eighth),
        "1/16" => Ok(StepSize::Sixteenth),
        "1/32" => Ok(StepSize::ThirtySecond),
        other => Err(format!("unrecognized step_size: {}", other)),
    }
}

fn tempo_roll_point_to_str(trp: TempoRollPoint) -> &'static str {
    match trp {
        TempoRollPoint::Off => "Off",
        TempoRollPoint::Step => "Step",
        TempoRollPoint::Beat => "Beat",
        TempoRollPoint::Seq => "Seq",
    }
}

fn str_to_tempo_roll_point(s: &str) -> Result<TempoRollPoint, String> {
    match s {
        "Off" => Ok(TempoRollPoint::Off),
        "Step" => Ok(TempoRollPoint::Step),
        "Beat" => Ok(TempoRollPoint::Beat),
        "Seq" => Ok(TempoRollPoint::Seq),
        other => Err(format!("unrecognized tempo_roll_point: {}", other)),
    }
}

fn tempo_rand_type_to_str(trt: TempoRandType) -> &'static str {
    match trt {
        TempoRandType::Random => "Random",
        TempoRandType::Up => "Up",
        TempoRandType::Down => "Down",
        TempoRandType::Breathe => "Breathe",
        TempoRandType::PingPong => "PingPong",
    }
}

fn str_to_tempo_rand_type(s: &str) -> Result<TempoRandType, String> {
    match s {
        "Random" => Ok(TempoRandType::Random),
        "Up" => Ok(TempoRandType::Up),
        "Down" => Ok(TempoRandType::Down),
        "Breathe" => Ok(TempoRandType::Breathe),
        "PingPong" => Ok(TempoRandType::PingPong),
        other => Err(format!("unrecognized tempo_rand_type: {}", other)),
    }
}

// ── Conversion helpers ────────────────────────────────────────────────────────

/// Build a `PatternData` snapshot from the current `SequencerState`.
pub fn pattern_from_state(state: &SequencerState, name: &str) -> PatternData {
    let steps = state
        .steps
        .iter()
        .map(|s| StepDataSerial {
            enabled: s.enabled,
            midi_note: s.midi_note,
            velocity: s.velocity,
        })
        .collect();

    PatternData {
        name: name.to_string(),
        steps,
        key: key_to_str(state.key).to_string(),
        mode: mode_to_str(state.mode).to_string(),
        tempo_bpm: state.tempo_bpm,
        swing: state.swing,
        step_size: step_size_to_str(state.step_size).to_string(),
        loop_in: state.loop_in,
        loop_out: state.loop_out,
        loop_active: state.loop_active,
        midi_channel: state.midi_channel,
        scale_quant: state.scale_quant,
        note_modifier: state.note_modifier,
        velocity_modifier: state.velocity_modifier,
        skip_modifier: state.skip_modifier,
        tempo_rand: state.tempo_rand,
        tempo_roll_point: tempo_roll_point_to_str(state.tempo_roll_point).to_string(),
        tempo_variance_max: state.tempo_variance_max,
        tempo_rand_type: tempo_rand_type_to_str(state.tempo_rand_type).to_string(),
        step_rand: state.step_rand,
        note_rand: state.note_rand,
    }
}

/// Apply a `PatternData` snapshot back onto a `SequencerState`.
///
/// Returns `Err` if any string-encoded enum field is unrecognized.
pub fn apply_pattern_to_state(data: &PatternData, state: &mut SequencerState) -> Result<(), String> {
    let key = str_to_key(&data.key)?;
    let mode = str_to_mode(&data.mode)?;
    let step_size = str_to_step_size(&data.step_size)?;
    let tempo_roll_point = str_to_tempo_roll_point(&data.tempo_roll_point)?;
    let tempo_rand_type = str_to_tempo_rand_type(&data.tempo_rand_type)?;

    // Write non-enum fields first.
    state.tempo_bpm = data.tempo_bpm;
    state.swing = data.swing;
    state.loop_in = data.loop_in;
    state.loop_out = data.loop_out;
    state.loop_active = data.loop_active;
    state.midi_channel = data.midi_channel;
    state.scale_quant = data.scale_quant;
    state.note_modifier = data.note_modifier;
    state.velocity_modifier = data.velocity_modifier;
    state.skip_modifier = data.skip_modifier;
    state.tempo_rand = data.tempo_rand;
    state.tempo_variance_max = data.tempo_variance_max;
    state.step_rand = data.step_rand;
    state.note_rand = data.note_rand;

    // Write enum fields.
    state.key = key;
    state.mode = mode;
    state.step_size = step_size;
    state.tempo_roll_point = tempo_roll_point;
    state.tempo_rand_type = tempo_rand_type;

    // Restore steps (up to 16; extra entries are ignored, missing ones are left as-is).
    for (i, step_serial) in data.steps.iter().enumerate().take(16) {
        state.steps[i].enabled = step_serial.enabled;
        state.steps[i].midi_note = step_serial.midi_note;
        state.steps[i].velocity = step_serial.velocity;
    }

    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Full pattern roundtrip: state → PatternData → TOML string → PatternData → state.
    #[test]
    fn pattern_roundtrip() {
        let original = SequencerState::default();
        let data = pattern_from_state(&original, "test-pattern");

        // Serialize to TOML string (no disk I/O).
        let toml_str = toml::to_string(&data).expect("serialize to TOML");

        // Deserialize back.
        let data2: PatternData = toml::from_str(&toml_str).expect("deserialize from TOML");

        // Apply to a fresh state.
        let mut restored = SequencerState::default();
        apply_pattern_to_state(&data2, &mut restored).expect("apply pattern to state");

        assert_eq!(restored.tempo_bpm, original.tempo_bpm, "tempo_bpm must survive roundtrip");
        assert_eq!(data2.key, "C", "key string must be 'C' for default state");
        assert_eq!(data2.steps.len(), 16, "must have exactly 16 steps");
        assert_eq!(
            restored.steps[0].midi_note,
            original.steps[0].midi_note,
            "step 0 midi_note must survive roundtrip"
        );
    }

    /// Song roundtrip: build a Song with 3 slots, serialize, deserialize, check.
    #[test]
    fn song_roundtrip() {
        let song = Song {
            name: "my-song".to_string(),
            slots: vec![
                PatternRef { filename: "intro.pat.toml".to_string(), repeats: 2 },
                PatternRef { filename: "verse.pat.toml".to_string(), repeats: 4 },
                PatternRef { filename: "outro.pat.toml".to_string(), repeats: 1 },
            ],
        };

        let toml_str = toml::to_string(&song).expect("serialize Song to TOML");
        let song2: Song = toml::from_str(&toml_str).expect("deserialize Song from TOML");

        assert_eq!(song2.slots.len(), 3, "must have 3 slots after roundtrip");
        assert_eq!(song2.slots[0].filename, "intro.pat.toml");
        assert_eq!(song2.slots[1].filename, "verse.pat.toml");
        assert_eq!(song2.slots[2].filename, "outro.pat.toml");
    }

    /// Unknown key string must produce an Err.
    #[test]
    fn unknown_key_returns_err() {
        let result = str_to_key("ZZZZ");
        assert!(result.is_err(), "str_to_key('ZZZZ') must return Err");
    }
}
