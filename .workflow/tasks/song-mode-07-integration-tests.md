Name: integration-tests
Type: coder
Status: pending
Repo: /home/whinchman/experiments/midi-man-mk3
Parallel Group: 6
Feature Branch: feature/song-mode
Branch: feature/song-mode/integration-tests
Base Branch: feature/song-mode
Goal: Write engine/tests/song_mode.rs with integration tests covering pattern roundtrip, song roundtrip, tick PatternEnd behavior, and F9/F10 key mapping — no hardware required.

Context:
  File to create:
    engine/tests/song_mode.rs  (new integration test file)

  All tests in this file are no_std-hostile (require std) but do not need MIDI
  hardware or a real terminal. They run with `cargo test -p engine`.

  ## Imports
  ```rust
  use engine::input::{global_key_to_command, InputCommand, KeyCodeSimple};
  use engine::pattern::{
      apply_pattern_to_state, pattern_from_state, PatternRef, Song,
  };
  use engine::state::{PlayMode, SequencerState, TickResult};
  ```

  ## Test 1: pattern_roundtrip
  ```
  - Create a default SequencerState.
  - Mutate a few fields: tempo_bpm=140, loop_out=7.
  - Enable steps 0 and 3, set step 3 midi_note=72.
  - Call pattern_from_state(&state, "test-pattern") → PatternData.
  - Serialize: toml::to_string(&data).expect("serialize").
  - Deserialize: toml::from_str::<engine::pattern::PatternData>(&toml_str).expect("deserialize").
  - Apply to a fresh SequencerState with apply_pattern_to_state.
  - Assert: tempo_bpm==140, loop_out==7, steps[3].midi_note==72, steps[3].enabled==true, steps.len()→16 steps.
  ```

  ## Test 2: song_roundtrip
  ```
  - Construct a Song with name="test-song" and three PatternRef slots:
      ("verse-A.pat.toml", repeats=2), ("chorus.pat.toml", repeats=1), ("bridge.pat.toml", repeats=1).
  - Serialize: toml::to_string(&song).expect("serialize song").
  - Deserialize: toml::from_str::<Song>(&toml_str).expect("deserialize song").
  - Assert: song.slots.len()==3, song.slots[0].filename=="verse-A.pat.toml", song.slots[0].repeats==2.
  ```

  ## Test 3: tick_returns_pattern_end_in_song_mode
  ```
  - Create a SequencerState with:
      playing=true, paused=false, play_mode=PlayMode::Song,
      loop_active=false, loop_out=15.
  - Set playhead=14 (step before the wrap at 15→0).
  - Enable step 15 (so it fires on tick).
  - Call tick() once: expect TickResult::Note(...) or TickResult::Idle (step 15 fires or doesn't).
  - Set playhead=15 manually (simulate being at the last step).
  - Call tick() again: the next tick should advance playhead to 0 and return TickResult::PatternEnd.
  - Assert the result is TickResult::PatternEnd.
  ```

  Note on tick() semantics: PatternEnd fires on the tick where the playhead wraps.
  The exact detection depends on the implementation in state-and-input. Adjust the
  test to match the actual behavior (wrap detection before or after step fires) — the
  important assertion is that `TickResult::PatternEnd` is returned after the playhead
  wraps in song mode.

  ## Test 4: f9_f10_global_key
  ```
  - Assert global_key_to_command(KeyCodeSimple::F9) == Some(InputCommand::SwitchToPatternMode).
  - Assert global_key_to_command(KeyCodeSimple::F10) == Some(InputCommand::SwitchToSongMode).
  - Assert global_key_to_command(KeyCodeSimple::F1) == None.
  ```

  ## Test 5: apply_song_mode_command_resets_slot
  ```
  - Create a SequencerState.
  - Set song_slot_index=3, song_slot_repeat=2.
  - Apply InputCommand::SwitchToSongMode via apply_command.
  - Assert song_slot_index==0, song_slot_repeat==0, play_mode==PlayMode::Song.
  ```

  ## Test 6: switch_to_pattern_mode_sets_mode
  ```
  - Create a SequencerState, apply SwitchToSongMode, then apply SwitchToPatternMode.
  - Assert play_mode==PlayMode::Pattern.
  ```

  These 6 tests collectively cover AC-3, AC-4, AC-7, and the core data model.
  None require disk I/O or a terminal.

Acceptance Criteria:
  - [ ] engine/tests/song_mode.rs exists with all 6 tests
  - [ ] All 6 tests pass under `cargo test -p engine song_mode`
  - [ ] No test touches the filesystem (use toml::to_string/from_str in-memory)
  - [ ] `cargo test -p engine` passes (no regressions in other test modules)
  - [ ] TickResult::PatternEnd test correctly distinguishes Song mode from Pattern mode

Dependencies: main-wiring
