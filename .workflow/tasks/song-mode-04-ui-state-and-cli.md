Name: ui-state-and-cli
Type: coder
Status: pending
Repo: /home/whinchman/experiments/midi-man-mk3
Parallel Group: 4
Feature Branch: feature/song-mode
Branch: feature/song-mode/ui-state-and-cli
Base Branch: feature/song-mode
Goal: Add song fields to UiState, wire F9/F10 translate_key handling, song panel key navigation, and all new CLI pattern/song commands with HELP_ENTRIES updates.

Context:
  Files to modify:
    engine/src/ui.rs  (all changes for both plan step 5 and step 6)

  NOTE: Plan steps 5 and 6 are merged here because they both touch only ui.rs.

  ## 1. Imports to add at the top of ui.rs
  ```rust
  use std::sync::{Arc, RwLock};
  use crate::pattern::{
      pattern_dir, song_dir,
      save_pattern, load_pattern,
      save_song, load_song,
      pattern_from_state, apply_pattern_to_state,
      Song, PatternRef,
  };
  use crate::state::PlayMode;
  ```

  ## 2. Add fields to UiState struct
  After `midi_channel_display: u8` and before `start_time: Instant`:
  ```rust
  pub play_mode: PlayMode,
  pub song: Option<Song>,
  pub song_cursor: usize,
  ```

  ## 3. Initialize new fields in UiState::new()
  ```rust
  play_mode: PlayMode::Pattern,
  song: None,
  song_cursor: 0,
  ```

  ## 4. Wire F9/F10 in translate_key (hw-io feature gate)
  translate_key is the function that maps crossterm KeyEvents to InputCommands.
  It contains a `to_simple` inner conversion from crossterm KeyCode to KeyCodeSimple.

  In the `to_simple` match arm (crossterm → KeyCodeSimple), add after existing F4 arm:
  ```rust
  KeyCode::F(9)  => KeyCodeSimple::F9,
  KeyCode::F(10) => KeyCodeSimple::F10,
  KeyCode::Delete => KeyCodeSimple::Delete,
  ```

  After calling `global_key_to_command(simple)`, if Some(cmd) is returned, send it and
  also update ui.play_mode:
  ```rust
  if let Some(cmd) = crate::input::global_key_to_command(simple) {
      match cmd {
          InputCommand::SwitchToPatternMode => { ui.play_mode = PlayMode::Pattern; }
          InputCommand::SwitchToSongMode    => { ui.play_mode = PlayMode::Song; }
          _ => {}
      }
      let _ = cmd_tx.try_send(cmd);
      return;
  }
  ```
  This must run BEFORE the existing panel-specific routing so F9/F10 are always global.

  ## 5. Song panel key navigation in translate_key
  When `ui.focus == FocusPanel::Sequencer && ui.play_mode == PlayMode::Song`, map:
  - Up arrow   → update `ui.song_cursor` (decrement, clamp to 0), send SongSlotCursorUp
  - Down arrow → update `ui.song_cursor` (increment, clamp to slots.len().saturating_sub(1)), send SongSlotCursorDown
  - `d` or Delete → send SongSlotDelete; if ui.song.is_some(), remove slot at cursor and clamp cursor
  - `[` → send SongSlotMoveUp
  - `]` → send SongSlotMoveDown

  The cursor clamping for Up/Down uses `ui.song.as_ref().map(|s| s.slots.len()).unwrap_or(0)`.
  The local cursor update in ui.song_cursor mirrors what the engine state will do, keeping
  the UI cursor in sync without a round-trip.

  ## 6. Extend handle_cli_submit with pattern/song commands
  Add a new private helper `handle_cli_pattern_cmd` and `handle_cli_song_cmd`.
  Call them from `handle_cli_submit` before the existing command dispatch:
  ```rust
  if parts[0] == "pattern" { return handle_cli_pattern_cmd(parts, ui, state, cmd_tx, arc_song); }
  if parts[0] == "song"    { return handle_cli_song_cmd(parts, ui, state, cmd_tx, arc_song); }
  ```

  NOTE: `arc_song: &Arc<RwLock<Option<Song>>>` is a new parameter added to both
  `handle_cli_submit` and `run_ui`. All existing callers in test code pass a
  newly-constructed `Arc::new(RwLock::new(None))`.

  ### handle_cli_pattern_cmd — commands:
  | CLI input | Action |
  |-----------|--------|
  | `pattern save <name>` | `pattern_from_state(state, name)` → `save_pattern(&data, &format!("{name}.pat.toml"))` → push LogTag::Cmd on success |
  | `pattern load <name>` | `load_pattern(&format!("{name}.pat.toml"))` → `apply_pattern_to_state(&data, state)` → send state update → push Cmd/Err |
  | `pattern list` | `std::fs::read_dir(pattern_dir())` → log each `.pat.toml` filename with LogTag::Info |

  ### handle_cli_song_cmd — commands:
  | CLI input | Action |
  |-----------|--------|
  | `song new <name>` | Set `ui.song = Some(Song { name, slots: vec![] })`, write to arc_song; push Cmd |
  | `song load <name>` | `load_song(&format!("{name}.song.toml"))` → set ui.song, write to arc_song; push Cmd/Err |
  | `song save <name>` | `save_song(ui.song.as_ref().unwrap_or_err, ...)` → push Cmd/Err |
  | `song list` | `std::fs::read_dir(song_dir())` → log each `.song.toml` filename |
  | `song add <filename>` | Append `PatternRef { filename: format!("{filename}.pat.toml"), repeats: 1 }` to ui.song; write arc_song |
  | `song remove <n>` | Remove slot at 1-indexed position n from ui.song; write arc_song |
  | `song set-repeats <n> <r>` | Set slot n's repeat count to r; write arc_song |

  "write arc_song" means: `*arc_song.write().unwrap() = ui.song.clone();`

  Errors (missing song, bad index, parse failure, disk error) push `LogTag::Err`.

  ## 7. Update HELP_ENTRIES
  Extend the existing `HELP_ENTRIES` constant with:
  ```rust
  ("pattern save <name>", "save current pattern to <name>.pat.toml"),
  ("pattern load <name>", "load pattern from <name>.pat.toml into current state"),
  ("pattern list", "list saved pattern files"),
  ("song new <name>", "create a new empty song"),
  ("song load <name>", "load song from <name>.song.toml"),
  ("song save <name>", "save current song to <name>.song.toml"),
  ("song list", "list saved song files"),
  ("song add <filename>", "append a pattern slot to the current song"),
  ("song remove <n>", "remove slot at 1-indexed position n"),
  ("song set-repeats <n> <r>", "set repeat count for slot n to r"),
  ```

  ## Unit tests
  Add to the `#[cfg(test)] mod tests` block in ui.rs (use the existing make_channels()
  pattern already present in ui.rs tests):

  - `pattern_save_pushes_cmd_log`: construct a UiState and a default SequencerState,
    call handle_cli_pattern_cmd with parts=["pattern","save","test-pat"], assert the
    log contains a LogTag::Cmd entry. Use a temp dir via `std::env::temp_dir()` by
    setting HOME env var or mocking pattern_dir to /tmp/... — simplest: the test can
    call `toml::to_string` directly on a PatternData without disk I/O if you refactor
    the save into a helper that accepts a PathBuf; otherwise just let the test write to /tmp.
  - `song_new_creates_empty_song`: call handle_cli_song_cmd with ["song","new","my-song"],
    assert ui.song.is_some() and ui.song.unwrap().slots.is_empty().
  - `song_add_appends_slot`: call song new then song add "verse-A", assert slots.len()==1
    and slots[0].filename=="verse-A.pat.toml".
  - `song_remove_removes_slot`: add two slots, remove slot 1, assert len==1.
  - `unknown_pattern_cmd_pushes_err`: call handle_cli_pattern_cmd with ["pattern","frobnicate"],
    assert LogTag::Err in log.

  ## run_ui signature note (hw-io gated)
  run_ui already takes `Arc<RwLock<SequencerState>>` and `SyncSender<InputCommand>`.
  Add `arc_song: Arc<RwLock<Option<Song>>>` as a new parameter and thread it into
  the translate_key / handle_cli_submit calls. In main.rs, construct the arc and pass it:
    `let arc_song: Arc<RwLock<Option<Song>>> = Arc::new(RwLock::new(None));`
  Pass `Arc::clone(&arc_song)` to run_ui. The same arc will be passed to the
  command-processor in the song-mode-wiring task.

Acceptance Criteria:
  - [ ] UiState has play_mode, song, song_cursor fields with correct defaults
  - [ ] translate_key maps F(9)→KeyCodeSimple::F9 and F(10)→KeyCodeSimple::F10
  - [ ] global_key_to_command is called before panel routing; F9/F10 update ui.play_mode
  - [ ] Up/Down/d/Delete/[/] navigate song slots when Sequencer focus + Song mode
  - [ ] handle_cli_submit routes "pattern" and "song" to their helpers
  - [ ] All 10 CLI commands described above are handled (success and error paths)
  - [ ] HELP_ENTRIES contains all 10 new entries
  - [ ] All 5 unit tests pass
  - [ ] `cargo test -p engine` passes with no regressions
  - [ ] arc_song is threaded through run_ui and main.rs

Dependencies: state-and-input
