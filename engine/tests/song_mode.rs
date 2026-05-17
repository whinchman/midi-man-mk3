//! Integration tests for song-mode data model, tick behaviour, and key mapping.
//!
//! All tests are in-memory — no disk I/O, no MIDI hardware, no terminal.
//! Run with: `cargo test -p engine song_mode`

use engine::input::{global_key_to_command, InputCommand, KeyCodeSimple};
use engine::pattern::{apply_pattern_to_state, pattern_from_state, PatternRef, Song};
use engine::state::{PlayMode, SequencerState, TickResult};

// ── Test 1: pattern_roundtrip ─────────────────────────────────────────────────

/// Verify that a SequencerState snapshot survives a full serialize/deserialize
/// roundtrip through TOML (in-memory) with custom field values preserved.
#[test]
fn pattern_roundtrip() {
    let mut state = SequencerState::default();

    // Mutate a few fields so we test more than defaults.
    state.tempo_bpm = 140;
    state.loop_out = 7;

    // Enable steps 0 and 3; set step 3 to a non-default midi_note.
    state.steps[0].enabled = true;
    state.steps[3].enabled = true;
    state.steps[3].midi_note = 72;

    // Capture a PatternData snapshot.
    let data = pattern_from_state(&state, "test-pattern");

    // Serialize to TOML (no disk I/O).
    let toml_str = toml::to_string(&data).expect("serialize PatternData to TOML");

    // Deserialize back.
    let data2: engine::pattern::PatternData =
        toml::from_str(&toml_str).expect("deserialize PatternData from TOML");

    // Apply to a fresh state.
    let mut restored = SequencerState::default();
    apply_pattern_to_state(&data2, &mut restored).expect("apply pattern to state");

    assert_eq!(restored.tempo_bpm, 140, "tempo_bpm must survive roundtrip");
    assert_eq!(restored.loop_out, 7, "loop_out must survive roundtrip");
    assert_eq!(restored.steps[3].midi_note, 72, "step 3 midi_note must survive roundtrip");
    assert!(restored.steps[3].enabled, "step 3 enabled must survive roundtrip");
    assert_eq!(data2.steps.len(), 16, "PatternData must have exactly 16 steps");
}

// ── Test 2: song_roundtrip ────────────────────────────────────────────────────

/// Verify that a Song with named slots serializes and deserializes correctly
/// through TOML without hitting the filesystem.
#[test]
fn song_roundtrip() {
    let song = Song {
        name: "test-song".to_string(),
        slots: vec![
            PatternRef { filename: "verse-A.pat.toml".to_string(), repeats: 2 },
            PatternRef { filename: "chorus.pat.toml".to_string(),  repeats: 1 },
            PatternRef { filename: "bridge.pat.toml".to_string(),  repeats: 1 },
        ],
    };

    // Serialize to TOML (in-memory).
    let toml_str = toml::to_string(&song).expect("serialize Song to TOML");

    // Deserialize back.
    let song2: Song = toml::from_str(&toml_str).expect("deserialize Song from TOML");

    assert_eq!(song2.slots.len(), 3, "Song must have 3 slots after roundtrip");
    assert_eq!(
        song2.slots[0].filename, "verse-A.pat.toml",
        "slot 0 filename must survive roundtrip"
    );
    assert_eq!(song2.slots[0].repeats, 2, "slot 0 repeats must survive roundtrip");
}

// ── Test 3: tick_returns_pattern_end_in_song_mode ────────────────────────────

/// Verify that tick() returns TickResult::PatternEnd exactly when the playhead
/// wraps from 15 to 0 in Song mode.
///
/// tick() semantics (from state.rs): PatternEnd is returned on the tick where
/// the playhead wraps (before the step at position 0 fires). Two ticks are
/// needed:
///   Tick 1 — playhead 14 → 15: no wrap, not PatternEnd.
///   Tick 2 — playhead 15 → 0:  wrap,    PatternEnd returned.
#[test]
fn tick_returns_pattern_end_in_song_mode() {
    let mut state = SequencerState::default();
    state.playing = true;
    state.paused = false;
    state.play_mode = PlayMode::Song;
    state.loop_active = false;
    // loop_out is 15 by default; no loop active so the full 16-step pattern wraps.

    // Position playhead at 14 so the next tick advances to 15 (no wrap yet).
    state.playhead = 14;

    // Enable step 15 — it may or may not fire on tick 1 depending on playhead
    // advancement; the important assertion is the absence of PatternEnd here.
    state.steps[15].enabled = true;

    // Tick 1: playhead 14 → 15. The step at 15 is enabled and should fire a Note
    // (or Idle if probabilistically muted), but must NOT be PatternEnd.
    let result1 = state.tick();
    assert!(
        !matches!(result1, TickResult::PatternEnd),
        "tick advancing playhead 14→15 must not return PatternEnd"
    );

    // Manually set playhead to 15 (simulate being at the last step before wrap).
    // This is valid: the external caller (clock thread) may reposition the
    // playhead between ticks.
    state.playhead = 15;

    // Tick 2: playhead 15 → 16 (wraps to 0) → PatternEnd returned immediately.
    let result2 = state.tick();
    assert!(
        matches!(result2, TickResult::PatternEnd),
        "tick advancing playhead 15→0 in Song mode must return PatternEnd"
    );
}

// ── Test 4: f9_f10_global_key ─────────────────────────────────────────────────

/// Verify global_key_to_command correctly maps F9 → SwitchToPatternMode,
/// F10 → SwitchToSongMode, and returns None for unmapped keys.
#[test]
fn f9_f10_global_key() {
    let cmd_f9 = global_key_to_command(KeyCodeSimple::F9);
    assert!(
        matches!(cmd_f9, Some(InputCommand::SwitchToPatternMode)),
        "F9 must map to SwitchToPatternMode"
    );

    let cmd_f10 = global_key_to_command(KeyCodeSimple::F10);
    assert!(
        matches!(cmd_f10, Some(InputCommand::SwitchToSongMode)),
        "F10 must map to SwitchToSongMode"
    );

    let cmd_f1 = global_key_to_command(KeyCodeSimple::F1);
    assert!(cmd_f1.is_none(), "F1 must not map to any global command");
}

// ── Test 5: apply_song_mode_command_resets_slot ───────────────────────────────

/// Verify that applying SwitchToSongMode resets song_slot_index and
/// song_slot_repeat to 0 and sets play_mode to Song.
#[test]
fn apply_song_mode_command_resets_slot() {
    let mut state = SequencerState::default();

    // Simulate mid-song state.
    state.song_slot_index = 3;
    state.song_slot_repeat = 2;

    state.apply_command(InputCommand::SwitchToSongMode);

    assert_eq!(
        state.play_mode,
        PlayMode::Song,
        "SwitchToSongMode must set play_mode to Song"
    );
    assert_eq!(
        state.song_slot_index, 0,
        "SwitchToSongMode must reset song_slot_index to 0"
    );
    assert_eq!(
        state.song_slot_repeat, 0,
        "SwitchToSongMode must reset song_slot_repeat to 0"
    );
}

// ── Test 6: switch_to_pattern_mode_sets_mode ─────────────────────────────────

/// Verify that SwitchToPatternMode sets play_mode back to Pattern after Song
/// mode has been activated.
#[test]
fn switch_to_pattern_mode_sets_mode() {
    let mut state = SequencerState::default();

    state.apply_command(InputCommand::SwitchToSongMode);
    assert_eq!(state.play_mode, PlayMode::Song, "pre-condition: Song mode active");

    state.apply_command(InputCommand::SwitchToPatternMode);
    assert_eq!(
        state.play_mode,
        PlayMode::Pattern,
        "SwitchToPatternMode must set play_mode to Pattern"
    );
}
