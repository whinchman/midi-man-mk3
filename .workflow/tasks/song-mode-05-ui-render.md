Name: ui-render
Type: coder
Status: pending
Repo: /home/whinchman/experiments/midi-man-mk3
Parallel Group: 4
Feature Branch: feature/song-mode
Branch: feature/song-mode/ui-render
Base Branch: feature/song-mode
Goal: Add song-mode fields to UiLocalSnapshot, implement render_song_panel, update render_keybind_bar with play_mode-aware hints, and add [PAT]/[SONG] label to the title bar.

Context:
  Files to modify:
    engine/src/ui_render.rs   (primary changes)
    engine/src/ui.rs          (update UiLocalSnapshot assembly in run_ui)

  ## 1. New imports in ui_render.rs
  ```rust
  use crate::pattern::PatternRef;
  use crate::state::PlayMode;
  ```

  ## 2. Extend UiLocalSnapshot<'a>
  Current struct ends with `midi_channel_display: u8`. Add:
  ```rust
  pub play_mode: PlayMode,
  pub song_slots: &'a [PatternRef],     // empty slice when no song loaded
  pub song_cursor: usize,               // which slot the cursor is on
  pub song_active_slot: usize,          // from SequencerState::song_slot_index
  ```

  ## 3. Update render_frame to branch on play_mode
  In `render_frame`, locate the call to `render_seq_panel(frame, snap, state, chunks[2])`.
  Replace with:
  ```rust
  match snap.play_mode {
      PlayMode::Pattern => render_seq_panel(frame, snap, state, chunks[2]),
      PlayMode::Song    => render_song_panel(frame, snap, chunks[2]),
  }
  ```

  ## 4. Implement render_song_panel
  ```rust
  fn render_song_panel(frame: &mut Frame, snap: &UiLocalSnapshot, area: Rect) {
      // ...
  }
  ```

  Layout: a single `Block` with border and title "F1 · SONG" (or "F1 · SEQ" when
  pattern mode — title bar label is handled in render_keybind_bar, not here).

  Each slot renders as one `Line`:
  ```
   [01] verse-A.pat.toml               ×2   ◀ playing
   [02] chorus.pat.toml                ×1   ◀
  ```
  Format: `" [{:02}] {:<30} ×{:<3} {}"`, where:
  - Index is 1-based
  - Filename is truncated to 30 chars (use `chars().take(30).collect::<String>()`)
  - Repeat count
  - Indicator: "◀ playing" if this slot == song_active_slot; empty otherwise

  Row coloring (using the existing palette consts already in ui_render.rs):
  - Cursor row (song_cursor == index): style fg=MAGENTA
  - Active playing row (song_active_slot == index): style fg=CYAN
  - Other rows: style fg=DIM_CYAN

  A row can be both cursor and active — cursor takes precedence for color.

  If snap.song_slots is empty, render a single dim row: "  (no song loaded)"

  ## 5. Update render_keybind_bar
  The existing function renders a static hint string. Accept `play_mode: PlayMode`
  and switch the hint string. Update its signature:
  ```rust
  fn render_keybind_bar(frame: &mut Frame, snap: &UiLocalSnapshot, area: Rect)
  ```
  (It already receives snap; add a branch inside the function body that reads snap.play_mode.)

  Pattern mode hint (existing or adjusted): show existing F1-F4 focus hints.
  Song mode hint (new): "F1:SONG  ↑↓:cursor  d:delete  [:move↑  ]:move↓  F9:PAT  F10:SONG"

  ## 6. Title bar [PAT] / [SONG] label
  Locate the title bar render (it currently contains "▶ 217 Industries / midi-man-mk3").
  After the project name, append:
  - `[PAT]` when `snap.play_mode == PlayMode::Pattern`
  - `[SONG]` when `snap.play_mode == PlayMode::Song`

  ## 7. Update UiLocalSnapshot assembly in ui.rs
  In `run_ui` (hw-io gate), where `UiLocalSnapshot { ... }` is constructed each frame,
  add the new fields:
  ```rust
  play_mode: ui.play_mode,
  song_slots: ui.song.as_ref().map(|s| s.slots.as_slice()).unwrap_or(&[]),
  song_cursor: ui.song_cursor,
  song_active_slot: state_snap.song_slot_index,  // read from SequencerState snapshot
  ```
  The state snapshot is the cloned SequencerState already read under a read lock each frame.

  ## Unit tests
  Add tests using `ratatui::backend::TestBackend` exactly as the existing render tests do:
  - `render_song_panel_empty_slots_does_not_panic`: construct a UiLocalSnapshot with
    song_slots=&[], call render_frame, assert no panic.
  - `render_song_panel_three_slots_renders_all_numbers`: construct snap with 3 PatternRef
    slots (filenames "a.pat.toml", "b.pat.toml", "c.pat.toml"), render, capture output
    as string, assert "[01]", "[02]", "[03]" all appear.
  - `render_song_panel_cursor_row_differs`: with cursor=1, assert the rendered buffer
    for row 1 has a different style than row 0 (or just assert the output contains "◀"
    if that is simpler with TestBackend).

  To construct a minimal UiLocalSnapshot for tests, use:
  ```rust
  UiLocalSnapshot {
      focus: FocusPanel::Sequencer,
      selected_step: 0,
      seq_param_idx: 0,
      rand_param_idx: 0,
      cli_line: "",
      cli_log: &VecDeque::new(),
      midi_device_name: "",
      midi_channel_display: 1,
      play_mode: PlayMode::Song,
      song_slots: &[PatternRef { filename: "a.pat.toml".into(), repeats: 1 }],
      song_cursor: 0,
      song_active_slot: 0,
  }
  ```

Acceptance Criteria:
  - [ ] UiLocalSnapshot has play_mode, song_slots, song_cursor, song_active_slot fields
  - [ ] render_frame calls render_song_panel when play_mode==Song
  - [ ] render_song_panel renders slot number, filename (truncated to 30 chars), repeat count
  - [ ] Cursor row is colored MAGENTA; active slot is colored CYAN
  - [ ] Empty slot list renders without panic and shows "(no song loaded)"
  - [ ] render_keybind_bar shows song-mode hints when play_mode==Song
  - [ ] Title bar shows [PAT] or [SONG]
  - [ ] UiLocalSnapshot assembly in run_ui populates all four new fields
  - [ ] All 3 unit tests pass
  - [ ] `cargo test -p engine` passes with no regressions

Dependencies: state-and-input
