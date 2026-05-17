Name: state-and-input
Type: coder
Status: pending
Repo: /home/whinchman/experiments/midi-man-mk3
Parallel Group: 3
Feature Branch: feature/song-mode
Branch: feature/song-mode/state-and-input
Base Branch: feature/song-mode
Goal: Add PlayMode enum, TickResult enum, new SequencerState fields, new InputCommand variants (including F9/F10 KeyCodeSimple variants), and update all call sites of tick().

Context:
  Files to modify:
    engine/src/state.rs
    engine/src/input.rs
    engine/src/clock.rs   (update tick() call site + accept cmd_tx for SongAdvance)

  NOTE: Steps 3 and 4 from the plan are merged here because they both touch
  input.rs and have identical dependencies.

  ## Changes to engine/src/state.rs

  ### Add PlayMode enum (near the top, before SequencerState)
  ```rust
  #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
  pub enum PlayMode {
      #[default]
      Pattern,
      Song,
  }
  ```

  ### Add TickResult enum (near MidiEvent)
  ```rust
  pub enum TickResult {
      Idle,
      Note(MidiEvent),
      PatternEnd,   // song mode only: pattern wrapped, advance to next slot
  }
  ```

  ### Add fields to SequencerState
  ```rust
  pub play_mode: PlayMode,
  pub song_slot_index: usize,
  pub song_slot_repeat: u8,
  ```
  All three default to their zero values (PlayMode::Pattern, 0, 0).

  ### Change tick() return type: Option<MidiEvent> -> TickResult
  Current signature (line 367 of state.rs):
    pub fn tick(&mut self) -> Option<MidiEvent>

  New signature:
    pub fn tick(&mut self) -> TickResult

  Conversion rules:
    - When not playing or paused: was `return None;` → now `return TickResult::Idle;`
    - When a NoteOn fires: was `Some(MidiEvent::NoteOn { ... })` → `TickResult::Note(MidiEvent::NoteOn { ... })`
    - After playhead wraps AND `self.play_mode == PlayMode::Song`: instead of
      resetting playhead and continuing normally, reset the playhead to loop_in (or 0)
      and `return TickResult::PatternEnd;`
    - All other `None` returns → `TickResult::Idle`

  The pattern-end detection point is: after computing the new playhead position,
  if play_mode is Song and the playhead just wrapped (i.e. the new playhead equals
  loop_in when loop_active, or 0 when not loop_active), emit PatternEnd before
  the step fires. Specifically: detect the wrap condition BEFORE processing the step,
  so the new slot loads before its first step fires.

  ### Add apply_command arms
  In the existing `apply_command` match:
  ```rust
  InputCommand::SwitchToPatternMode => {
      self.play_mode = PlayMode::Pattern;
  }
  InputCommand::SwitchToSongMode => {
      self.play_mode = PlayMode::Song;
      self.song_slot_index = 0;
      self.song_slot_repeat = 0;
  }
  // SongAdvance is handled by the command processor (has Song access); no-op here.
  InputCommand::SongAdvance => {}
  InputCommand::SongSlotCursorUp => {}     // UI-only; state ignores
  InputCommand::SongSlotCursorDown => {}
  InputCommand::SongSlotDelete => {}
  InputCommand::SongSlotMoveUp => {}
  InputCommand::SongSlotMoveDown => {}
  InputCommand::SongSlotInsert(_) => {}
  ```

  ## Changes to engine/src/input.rs

  ### Extend KeyCodeSimple enum
  After the existing `F4` variant, add:
  ```rust
  /// F9 — switch to pattern mode.
  F9,
  /// F10 — switch to song mode.
  F10,
  /// Delete key.
  Delete,
  ```

  ### Add new InputCommand variants
  After the existing `NoteSet` variant, add:
  ```rust
  /// Switch to pattern mode (F9).
  SwitchToPatternMode,
  /// Switch to song mode (F10).
  SwitchToSongMode,
  /// Advance song to next slot (sent by clock on PatternEnd).
  SongAdvance,
  /// Song slot list cursor: move up.
  SongSlotCursorUp,
  /// Song slot list cursor: move down.
  SongSlotCursorDown,
  /// Insert a pattern slot at cursor position (filename only, no path).
  SongSlotInsert(String),
  /// Delete the slot at cursor.
  SongSlotDelete,
  /// Swap cursor slot with the slot above it.
  SongSlotMoveUp,
  /// Swap cursor slot with the slot below it.
  SongSlotMoveDown,
  ```

  ### Add global_key_to_command function (new, in input.rs)
  ```rust
  /// Translate a key event that is always active regardless of focus or overlay.
  /// Currently handles F9/F10 mode switching.
  pub fn global_key_to_command(key: KeyCodeSimple) -> Option<InputCommand> {
      match key {
          KeyCodeSimple::F9  => Some(InputCommand::SwitchToPatternMode),
          KeyCodeSimple::F10 => Some(InputCommand::SwitchToSongMode),
          _ => None,
      }
  }
  ```

  ## Changes to engine/src/clock.rs

  ### Update run_clock signature
  Old: `pub fn run_clock(state: Arc<RwLock<SequencerState>>, midi_tx: SyncSender<MidiEvent>)`
  New: `pub fn run_clock(state: Arc<RwLock<SequencerState>>, midi_tx: SyncSender<MidiEvent>, cmd_tx: SyncSender<InputCommand>)`

  Add `use crate::input::InputCommand;` to the imports in clock.rs.

  ### Update the tick() call site
  Old (lines 377-410, roughly):
  ```rust
  let maybe_event = {
      let mut s = state.write()...;
      s.tick()
  };
  if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = maybe_event {
      ...
  }
  ```

  New:
  ```rust
  let tick_result = {
      let mut s = state.write().expect("clock: state RwLock poisoned");
      s.tick()
  };
  match tick_result {
      crate::state::TickResult::Note(MidiEvent::NoteOn { channel, note, velocity, .. }) => {
          // retrigger + send NoteOn — same logic as before
          ...
          last_note = Some((channel, note));
      }
      crate::state::TickResult::PatternEnd => {
          if cmd_tx.send(InputCommand::SongAdvance).is_err() {
              break;
          }
      }
      _ => {}
  }
  ```

  ## Unit tests

  Add to the `#[cfg(test)] mod tests` block in state.rs:

  - `tick_in_pattern_mode_returns_idle_when_not_playing`: default state (playing=false),
    call tick(), assert matches TickResult::Idle.
  - `tick_in_song_mode_returns_pattern_end_on_wrap`: set play_mode=Song, playing=true,
    loop_active=false, set playhead=14 (one before wrap at 15), call tick() twice —
    second call should return TickResult::PatternEnd.
  - `switch_to_song_mode_resets_slot_index`: apply SwitchToSongMode, assert
    song_slot_index==0 and song_slot_repeat==0 and play_mode==Song.

  Add to input.rs tests:

  - `global_key_f9_maps_to_switch_to_pattern_mode`: assert global_key_to_command(F9) ==
    Some(SwitchToPatternMode).
  - `global_key_f10_maps_to_switch_to_song_mode`: assert global_key_to_command(F10) ==
    Some(SwitchToSongMode).
  - `global_key_other_returns_none`: assert global_key_to_command(F1) == None.

  ## main.rs update (small)
  In engine/src/main.rs the clock spawn passes only two args:
    `engine::clock::run_clock(clock_state, clock_midi_tx)`
  This must become:
    `engine::clock::run_clock(clock_state, clock_midi_tx, cmd_tx.clone())`
  Add this change in this task since clock.rs signature changes here.
  The Song Arc wiring from the plan's Step 8 is deferred to the song-mode-wiring task.

Acceptance Criteria:
  - [ ] PlayMode and TickResult enums exist and are pub in state.rs
  - [ ] SequencerState has play_mode, song_slot_index, song_slot_repeat fields with correct defaults
  - [ ] tick() returns TickResult in all branches; no remaining Option<MidiEvent> return sites
  - [ ] KeyCodeSimple has F9, F10, Delete variants
  - [ ] global_key_to_command function exists in input.rs and is pub
  - [ ] All new InputCommand variants exist (SwitchToPatternMode, SwitchToSongMode, SongAdvance, SongSlotCursorUp/Down/Insert/Delete/MoveUp/MoveDown)
  - [ ] run_clock signature accepts cmd_tx: SyncSender<InputCommand> as third arg
  - [ ] main.rs passes cmd_tx.clone() to run_clock
  - [ ] All 6 new unit tests pass
  - [ ] `cargo test -p engine` passes with no regressions

Dependencies: pattern-module
