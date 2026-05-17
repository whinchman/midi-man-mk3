Name: main-wiring
Type: coder
Status: pending
Repo: /home/whinchman/experiments/midi-man-mk3
Parallel Group: 5
Feature Branch: feature/song-mode
Branch: feature/song-mode/main-wiring
Base Branch: feature/song-mode
Goal: Wire the Arc<RwLock<Option<Song>>> through main.rs, extend the command-processor to handle SongAdvance, and verify the full thread graph compiles and runs.

Context:
  Files to modify:
    engine/src/main.rs

  This task assumes all previous tasks are merged into feature/song-mode:
  - arc_song already exists in run_ui signature (from ui-state-and-cli task)
  - run_clock already accepts cmd_tx: SyncSender<InputCommand> (from state-and-input task)
  - apply_pattern_to_state is available from pattern.rs (from pattern-module task)

  ## 1. Construct arc_song in main()
  After the `let state = Arc::new(...)` line, add:
  ```rust
  use engine::pattern::Song;
  let arc_song: std::sync::Arc<std::sync::RwLock<Option<Song>>> =
      std::sync::Arc::new(std::sync::RwLock::new(None));
  ```

  ## 2. Pass arc_song to run_ui (hw-io gate)
  The run_ui call currently is:
  ```rust
  engine::ui::run_ui(ui_state, ui_cmd_tx, ui_notify_rx, ui_ctrl_tx, midi_log_rx)
  ```
  Change to:
  ```rust
  engine::ui::run_ui(ui_state, ui_cmd_tx, ui_notify_rx, ui_ctrl_tx, midi_log_rx, Arc::clone(&arc_song))
  ```

  ## 3. Pass arc_song clone to the command-processor thread
  Capture a clone of arc_song before the cmd-processor spawn:
  ```rust
  let cmd_arc_song = Arc::clone(&arc_song);
  ```
  Move it into the closure.

  ## 4. Extend the command-processor loop to handle SongAdvance
  Current command-processor loop:
  ```rust
  while let Ok(cmd) = cmd_rx.recv() {
      {
          let mut s = cmd_state.write()...;
          s.apply_command(cmd);
      }
      let _ = cmd_notify.try_send(());
  }
  ```

  Replace with:
  ```rust
  while let Ok(cmd) = cmd_rx.recv() {
      match &cmd {
          InputCommand::SongAdvance => {
              // Read the current song and slot state under separate locks.
              let (slot_index, slot_repeat) = {
                  let s = cmd_state.read().expect("cmd-processor: state RwLock poisoned");
                  (s.song_slot_index, s.song_slot_repeat)
              };
              let next_action = {
                  let song_guard = cmd_arc_song.read().expect("cmd-processor: song arc poisoned");
                  song_guard.as_ref().and_then(|song| {
                      let slot = song.slots.get(slot_index)?;
                      Some((slot.filename.clone(), slot.repeats, song.slots.len()))
                  })
              };
              if let Some((filename, repeats, total_slots)) = next_action {
                  // Determine whether to advance to next slot or repeat.
                  let new_repeat = slot_repeat + 1;
                  if new_repeat >= repeats {
                      // Exhausted repeats: advance slot index (wrap around).
                      let next_slot = (slot_index + 1) % total_slots.max(1);
                      // Look up next slot's filename.
                      let next_filename = {
                          let sg = cmd_arc_song.read().expect("cmd-processor: song arc poisoned");
                          sg.as_ref()
                              .and_then(|s| s.slots.get(next_slot))
                              .map(|p| p.filename.clone())
                      };
                      if let Some(nf) = next_filename {
                          match engine::pattern::load_pattern(&nf) {
                              Ok(data) => {
                                  let mut s = cmd_state.write().expect("cmd-processor: state RwLock poisoned");
                                  s.song_slot_index = next_slot;
                                  s.song_slot_repeat = 0;
                                  let _ = engine::pattern::apply_pattern_to_state(&data, &mut s);
                              }
                              Err(e) => {
                                  eprintln!("cmd-processor: SongAdvance load error: {e}");
                              }
                          }
                      }
                  } else {
                      // Still repeating: increment repeat counter only.
                      let mut s = cmd_state.write().expect("cmd-processor: state RwLock poisoned");
                      s.song_slot_repeat = new_repeat;
                  }
              }
          }
          _ => {
              let mut s = cmd_state.write().expect("cmd-processor: state RwLock poisoned");
              s.apply_command(cmd);
          }
      }
      let _ = cmd_notify.try_send(());
  }
  ```

  Note: `apply_command` for SongAdvance is a no-op in SequencerState (as established in
  state-and-input task), so the match arm above handles it entirely in main.rs.

  ## 5. Verify clock thread still compiles
  The clock thread spawn (hw-io gate) already passes cmd_tx.clone() from the
  state-and-input task. Confirm it still matches the new run_clock signature:
  ```rust
  engine::clock::run_clock(clock_state, clock_midi_tx, cmd_tx.clone())
  ```

  ## No new unit tests required in this task
  The integration test task (song-mode-integration-tests) covers end-to-end verification.
  Smoke test: `cargo build -p engine --features hw-io` must compile without errors.
  `cargo test -p engine` must pass (no new test failures).

Acceptance Criteria:
  - [ ] arc_song is constructed in main() as Arc<RwLock<Option<Song>>>
  - [ ] arc_song clone is passed to run_ui (hw-io) and to the command-processor thread
  - [ ] Command-processor handles SongAdvance: increments repeat, advances slot, loads pattern from disk
  - [ ] Slot index wraps around when the last slot finishes
  - [ ] `cargo build -p engine --features hw-io` compiles without errors
  - [ ] `cargo test -p engine` passes with no regressions
  - [ ] Pattern load errors during SongAdvance are logged to stderr and do not panic

Dependencies: ui-state-and-cli, ui-render
