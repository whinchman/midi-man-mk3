# Plan: Song Mode

## Architecture Overview

Song mode chains saved patterns into a linear playlist. The engine continues to
use a single `SequencerState` for live playback; song mode provides a layer
above it that swaps the active pattern when the playhead wraps. Patterns are
stored on disk as TOML files; a song is a separate TOML file that references
them by filename.

### Key decisions

1. **Pattern files live in `~/.config/midi-man-mk3/patterns/`**; song files in
   `~/.config/midi-man-mk3/songs/`. The directory is created on first save.
   Assumption: the operator will not run the engine as root. Flag for review if
   XDG_CONFIG_HOME must be respected on all target platforms.

2. **Pattern file extension:** `.pat.toml`. Song file extension: `.song.toml`.
   These are intentionally human-readable so users can hand-edit them.

3. **Song mode does not stop the clock.** When `PlayMode` is `Song`, the clock
   thread keeps ticking; the command processor detects a pattern boundary and
   calls a new method `SequencerState::load_pattern_slot(slot: usize)` which
   swaps steps/key/mode in place. No thread restart is needed.

4. **Song-mode UI replaces the F1 · SEQ panel**, not a new zone. In pattern
   mode F1 shows the 16-step grid. In song mode F1 shows the vertical slot
   list. F9/F10 toggle the mode; the label on the F1 zone border changes
   accordingly. This avoids adding a new layout zone (which would break the
   existing 7-zone height constraints).

5. **Pattern data serialized includes every user-visible field** in
   `SequencerState` that describes the pattern itself: steps, key, mode,
   step_size, swing, loop_in, loop_out, loop_active, tempo_bpm, midi_channel,
   scale_quant, note_modifier, velocity_modifier, skip_modifier. Fields that
   are runtime-only (playhead, playing, paused, rng_seed, rand_seed,
   midi_device_name, pending_edit, active_overlay, selected_step,
   selected_param, selected_rand_param) are excluded from serialization.
   The randomness params (tempo_rand, tempo_roll_point, etc.) are included
   because they shape the sound of the pattern. Flag for operator review:
   should per-step randomness params be per-pattern or global?

6. **No `toml` crate is currently in the workspace.** `serde` 1.0.228 is
   transitively present (via ratatui) but not a direct dependency and does not
   have `derive` features enabled. Both `serde` (with `derive`) and `toml` must
   be added to `engine/Cargo.toml`.

7. **Song state lives in `UiState`**, not `SequencerState`. The song slot list
   (`Vec<PatternSlot>`) and cursor are UI concerns. The sequencer only cares
   about: "am I in song mode?" (a `PlayMode` flag in state) and "which slot am
   I on?" (a `usize` in state). The pattern data itself is loaded into
   `SequencerState` on slot transitions.

---

## Data Model

### `StepData` — serializable (no change to struct, derive added)

```
engine/src/state.rs  — StepData
  enabled: bool
  midi_note: u8
  velocity: u8
```

### `PatternData` — new serializable struct (separate from runtime `SequencerState`)

New file: `engine/src/pattern.rs`

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StepDataSerial {
    pub enabled: bool,
    pub midi_note: u8,
    pub velocity: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PatternData {
    pub name: String,            // user-visible label, e.g. "verse-A"
    pub steps: Vec<StepDataSerial>,  // exactly 16 elements
    pub key: String,             // "C", "C#", "D" … serialized as string
    pub mode: String,            // "Major", "NaturalMinor" … serialized as string
    pub tempo_bpm: u16,
    pub swing: i8,
    pub step_size: String,       // "1/16", "1/8" …
    pub loop_in: u8,
    pub loop_out: u8,
    pub loop_active: bool,
    pub midi_channel: u8,        // 0-indexed
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
```

String-encoded enums are chosen over integer indices to keep TOML files
human-readable and stable against future reordering.

**TOML schema example:**

```toml
name = "verse-A"
tempo_bpm = 120
swing = 0
step_size = "1/16"
key = "C"
mode = "Major"
loop_in = 0
loop_out = 15
loop_active = false
midi_channel = 0
scale_quant = false
note_modifier = 0
velocity_modifier = 0
skip_modifier = false
tempo_rand = 0
tempo_roll_point = "Off"
tempo_variance_max = 10
tempo_rand_type = "Random"
step_rand = 0
note_rand = 0

[[steps]]
enabled = true
midi_note = 60
velocity = 100

[[steps]]
enabled = false
midi_note = 60
velocity = 100
# … 14 more step entries
```

### `PatternRef` and `Song`

Also in `engine/src/pattern.rs`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PatternRef {
    pub filename: String,   // just the filename, not a full path: "verse-A.pat.toml"
    pub repeats: u8,        // 1 = play once, 2 = play twice, etc.  Default: 1.
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Song {
    pub name: String,
    pub slots: Vec<PatternRef>,  // unbounded
}
```

**TOML schema example:**

```toml
name = "my-song"

[[slots]]
filename = "verse-A.pat.toml"
repeats = 2

[[slots]]
filename = "chorus.pat.toml"
repeats = 1
```

### File I/O helpers (in `engine/src/pattern.rs`)

```rust
pub fn pattern_dir() -> PathBuf
pub fn song_dir() -> PathBuf

pub fn save_pattern(data: &PatternData, filename: &str) -> Result<(), String>
pub fn load_pattern(filename: &str) -> Result<PatternData, String>

pub fn save_song(song: &Song, filename: &str) -> Result<(), String>
pub fn load_song(filename: &str) -> Result<Song, String>

pub fn pattern_from_state(state: &SequencerState, name: &str) -> PatternData
pub fn apply_pattern_to_state(data: &PatternData, state: &mut SequencerState) -> Result<(), String>
```

`apply_pattern_to_state` deserializes string enum fields (key, mode, etc.) back
to their enum variants, returning `Err` with a human-readable message if any
string is unrecognized.

---

## State Machine Changes

### `PlayMode` enum (new, in `engine/src/state.rs`)

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PlayMode {
    #[default]
    Pattern,
    Song,
}
```

### New fields on `SequencerState`

```rust
pub play_mode: PlayMode,
pub song_slot_index: usize,  // current slot position in the song
pub song_slot_repeat: u8,    // how many times the current slot has been played
```

### `SequencerState::tick()` change

When `play_mode == PlayMode::Song`, after `playhead` wraps past `loop_out`
(or past step 15 when loop is off), instead of resetting `playhead` to
`loop_in`/0, the method emits a sentinel: it sets a new
`pub song_advance_pending: bool` flag on state and resets the playhead. The
command processor (or a new song-advance thread hook — see trade-offs) polls
this flag.

**Simpler alternative chosen:** `tick()` returns a new `TickResult` enum
instead of `Option<MidiEvent>`:

```rust
pub enum TickResult {
    Idle,
    Note(MidiEvent),
    PatternEnd,          // song mode only: wrap completed, advance to next slot
}
```

The clock thread handles `TickResult::PatternEnd` by sending a new
`InputCommand::SongAdvance` on `cmd_tx`. The command processor applies it
under the write lock: increments `song_slot_repeat`; if repeats are exhausted,
increments `song_slot_index` and calls `load_pattern_slot` which reads the
next pattern file from disk and applies it to state. This keeps disk I/O off
the hot clock path (I/O happens in the command-processor thread, not the
clock thread).

Assumption: disk latency for a small TOML file on a local filesystem is
negligible (<1 ms) and does not audibly disrupt playback. Flag for operator
review if this causes glitches in practice; the fallback is pre-loading all
patterns into memory when a song is loaded.

### New `InputCommand` variants

```rust
// Toggle between pattern and song mode (F9/F10).
SwitchToPatternMode,
SwitchToSongMode,

// Advance the song to the next slot (sent by clock on PatternEnd).
SongAdvance,

// Song slot list cursor navigation (sent by UI in song mode).
SongSlotCursorUp,
SongSlotCursorDown,
SongSlotInsert(String),   // filename to insert at cursor
SongSlotDelete,            // delete slot at cursor
SongSlotMoveUp,            // swap cursor slot with one above
SongSlotMoveDown,          // swap cursor slot with one below
```

### `apply_command` additions (in `SequencerState`)

```
SwitchToPatternMode  → state.play_mode = PlayMode::Pattern
SwitchToSongMode     → state.play_mode = PlayMode::Song; state.song_slot_index = 0; state.song_slot_repeat = 0
SongAdvance          → (handled by command processor which has access to the Song struct, not in SequencerState directly)
```

`SongAdvance` must be handled outside `apply_command` because it needs access
to the `Song` (stored in UI/song layer, not `SequencerState`). The command
processor receives `SongAdvance`, looks up the song's next slot, loads the
pattern file, then calls `apply_pattern_to_state` under the write lock.

This means the song data (`Song` struct) must live somewhere the command
processor can access. Two options:

- **Option A (chosen):** Wrap `Song` in a second `Arc<RwLock<Option<Song>>>` passed
  to the command-processor thread. When `SongAdvance` arrives, the processor
  reads the song, determines the next slot, loads the pattern, and applies it.
- **Option B:** Inline the song Vec into `SequencerState`. Rejected because it
  mixes concerns (sequencer state vs. song arrangement) and adds heap allocation
  to an otherwise allocation-free hot path.

---

## TUI Changes

### F9/F10 mode switching

In `translate_key` (`engine/src/ui.rs`), in the `to_simple` crossterm→simple
conversion, add:

```rust
KeyCode::F(9)  => KeyCodeSimple::F9,
KeyCode::F(10) => KeyCodeSimple::F10,
```

And to `KeyCodeSimple` enum:
```rust
F9,
F10,
```

In `global_key_to_command`:
```rust
KeyCodeSimple::F9  => Some(InputCommand::SwitchToPatternMode),
KeyCodeSimple::F10 => Some(InputCommand::SwitchToSongMode),
```

`UiState` gains a field mirroring the active mode:
```rust
pub play_mode: PlayMode,
```
Updated in the `SwitchToPatternMode` / `SwitchToSongMode` arms of
`translate_key` (similar to how `SetFocus` updates `ui.focus`).

### Song panel (replaces F1 step grid in song mode)

`render_frame` in `engine/src/ui_render.rs` checks `ui.play_mode`:
- `PlayMode::Pattern` → existing `render_seq_panel` (no change)
- `PlayMode::Song` → new `render_song_panel`

`UiLocalSnapshot` gains:
```rust
pub play_mode: PlayMode,
pub song_slots: &'a [PatternRef],   // borrow from ui.song (loaded song's slots)
pub song_cursor: usize,              // which slot the cursor is on
pub song_active_slot: usize,         // which slot is currently playing (from SequencerState)
```

`UiState` gains:
```rust
pub play_mode: PlayMode,
pub song: Option<Song>,
pub song_cursor: usize,
```

### `render_song_panel` layout

```
┌ F1 · SONG ──────────────────────────────────────────────────────────────────┐
│  [01] verse-A.pat.toml    ×2   ◀ playing                                    │
│  [02] chorus.pat.toml     ×1   ◀ cursor                                     │
│  [03] bridge.pat.toml     ×1                                                 │
│  [04] chorus.pat.toml     ×1                                                 │
│  [05] outro.pat.toml      ×1                                                 │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

Each row: slot number (1-indexed), filename (truncated to 30 chars), repeat
count (`×N`), and an indicator if it is the currently-playing slot or the
cursor position.

Colors follow the existing palette:
- Cursor row: MAGENTA border style (matches playhead highlight convention)
- Active playing row: CYAN
- All others: DIM_CYAN/GRAY

### Song-mode key bindings (when F1 has focus and play_mode == Song)

| Key | Action |
|-----|--------|
| Up | `SongSlotCursorUp` |
| Down | `SongSlotCursorDown` |
| Delete / `d` | `SongSlotDelete` |
| `i` | (prompt in CLI: `song insert <filename>`) |
| `[` | `SongSlotMoveUp` |
| `]` | `SongSlotMoveDown` |

Insertion is done via the CLI panel (F4) to avoid needing an inline text
editor in the song panel.

### Key bind bar update

The existing `render_keybind_bar` is a static string. In song mode it should
show song-specific hints. Two approaches:
- **Option A (chosen):** make `render_keybind_bar` accept `play_mode` and
  switch the hint string.
- **Option B:** always show pattern hints. Simpler but confusing.

### F9/F10 status in title bar

The title bar currently shows `▶ 217 Industries / midi-man-mk3`. Add a mode
indicator: `[PAT]` or `[SONG]` after the project name.

---

## CLI Additions

New commands added to `handle_cli_submit` in `engine/src/ui.rs`:

| Command | Action |
|---------|--------|
| `pattern save <name>` | Serialize current state to `<name>.pat.toml` |
| `pattern load <name>` | Load `<name>.pat.toml` into current state |
| `pattern list` | List `.pat.toml` files in pattern dir, log each |
| `song new <name>` | Create empty song, set as active song |
| `song load <name>` | Load `<name>.song.toml` |
| `song save <name>` | Save current `ui.song` to `<name>.song.toml` |
| `song list` | List `.song.toml` files in song dir |
| `song add <filename>` | Append `<filename>.pat.toml` slot to current song |
| `song remove <n>` | Remove slot at 1-indexed position `n` |
| `song set-repeats <n> <r>` | Set repeat count for slot `n` to `r` |

These are handled entirely within `handle_cli_submit` (or a new helper
`handle_cli_song_cmd`). They produce `LogTag::Cmd` on success and `LogTag::Err`
on failure. Pattern/song disk operations are performed synchronously in the UI
thread (not via `InputCommand`) because they produce log output and the UI
thread already owns `UiState`.

`HELP_ENTRIES` in `engine/src/ui.rs` is extended with all new commands.

---

## Step-by-Step Implementation Plan

### Step 1 — Dependencies (prerequisite for all)

**File:** `engine/Cargo.toml`

Add:
```toml
serde = { version = "1", features = ["derive"] }
toml = "0.8"
```

`toml` 0.8.x is the current stable release (as of 2025) with full
serialize/deserialize support. No other steps can proceed until this builds.

Agent type: Coder. Expected test: `cargo build -p engine` succeeds.

### Step 2 — `pattern.rs` module (data model + file I/O)

**File:** `engine/src/pattern.rs` (new)
**File:** `engine/src/lib.rs` — add `pub mod pattern;`

Implement:
- `StepDataSerial`, `PatternData`, `PatternRef`, `Song` with full
  `Serialize`/`Deserialize` derives
- `pattern_dir()`, `song_dir()` using `dirs::home_dir()` or `std::env::var("HOME")`
  — no `dirs` crate needed; use `std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())`
- `save_pattern`, `load_pattern`, `save_song`, `load_song`
- `pattern_from_state`, `apply_pattern_to_state`
- Helper functions: `key_to_str/str_to_key`, `mode_to_str/str_to_mode`,
  `step_size_to_str/str_to_step_size`, `tempo_roll_point_to_str/str_to_tempo_roll_point`,
  `tempo_rand_type_to_str/str_to_tempo_rand_type`

Unit tests (no disk I/O): roundtrip `pattern_from_state` → serialize to TOML
string → deserialize → `apply_pattern_to_state`, assert key fields survive.
Use `toml::to_string` / `toml::from_str` directly in tests (no filesystem).

Dependency: Step 1.

### Step 3 — State changes (`state.rs`)

**File:** `engine/src/state.rs`

- Add `PlayMode` enum with `Default`
- Add fields to `SequencerState`: `play_mode`, `song_slot_index`,
  `song_slot_repeat`
- Change `tick()` return type from `Option<MidiEvent>` to `TickResult` enum
- Add `SwitchToPatternMode`, `SwitchToSongMode` variants to `InputCommand`
  (in `engine/src/input.rs`)
- Add `apply_command` arms for the two new mode-switch commands
- Update `SequencerState::default()` for new fields

`TickResult::PatternEnd` is emitted when `play_mode == Song` and the playhead
wraps. `tick()` still returns `TickResult::Note(event)` for normal steps.
For pattern mode, the existing `None` → `TickResult::Idle` and
`Some(event)` → `TickResult::Note(event)`.

Update all call sites of `tick()`:
- `engine/src/clock.rs` — the primary consumer; handle `PatternEnd` by sending
  `InputCommand::SongAdvance` on a new `cmd_tx` reference the clock holds.

**Clock thread signature change:** `run_clock` must also accept a
`SyncSender<InputCommand>` to forward `SongAdvance`. Update `main.rs` to
pass a clone of `cmd_tx` to `run_clock`.

Dependency: Steps 1–2.

Unit tests: `tick()` in pattern mode returns `Idle`/`Note` unchanged;
in song mode returns `PatternEnd` on wrap.

### Step 4 — `KeyCodeSimple` + `global_key_to_command` (F9/F10)

**File:** `engine/src/input.rs`

- Add `F9`, `F10` variants to `KeyCodeSimple`
- Add `F9`/`F10` arms to `to_simple()` in `engine/src/ui.rs` (crossterm mapping)
- Add arms in `global_key_to_command`:
  ```rust
  KeyCodeSimple::F9  => Some(InputCommand::SwitchToPatternMode),
  KeyCodeSimple::F10 => Some(InputCommand::SwitchToSongMode),
  ```

- Add `SongSlotCursorUp`, `SongSlotCursorDown`, `SongSlotDelete`,
  `SongSlotMoveUp`, `SongSlotMoveDown` to `InputCommand` enum.

Unit tests: `global_key_to_command(F9)` → `SwitchToPatternMode`,
`global_key_to_command(F10)` → `SwitchToSongMode`.

Dependency: Step 3.

### Step 5 — `UiState` + song state in UI layer

**File:** `engine/src/ui.rs`

- Add `play_mode: PlayMode`, `song: Option<Song>`, `song_cursor: usize` to
  `UiState`
- `UiState::new()` defaults: `play_mode = PlayMode::Pattern`, `song = None`,
  `song_cursor = 0`
- In `translate_key`, handle `SwitchToPatternMode` / `SwitchToSongMode` by
  updating `ui.play_mode` (mirrors how `SetFocus` updates `ui.focus`)
- In `translate_key`, when `FocusPanel::Sequencer` and
  `ui.play_mode == PlayMode::Song`, map Up/Down arrow to `SongSlotCursorUp` /
  `SongSlotCursorDown` (updating `ui.song_cursor` locally); map `d` to
  `SongSlotDelete`; map `[`/`]` to move commands.

**Song Arc**: In `main.rs`, create
`Arc<RwLock<Option<Song>>>` and pass it to both the command processor thread
and (as a read reference) to the UI via a channel or direct arc clone. The
simplest approach: UI thread owns `Option<Song>` directly (no arc needed for
the list display); song state for the command processor is delivered via
`InputCommand` — specifically `SongAdvance` carries enough info because the
processor looks up the next slot index from the arc.

Revised approach for simplicity: the `Song` is owned by `UiState`; the
command processor receives `InputCommand::SongLoadSlot { filename: String }`
instead of `SongAdvance`. The clock sends `SongAdvance`; the command processor
retrieves the next filename by reading `Arc<RwLock<Option<Song>>>`, then loads
the pattern and applies it. This cleanly separates concerns.

**`Arc<RwLock<Option<Song>>>`** passed to:
1. Command-processor thread (to look up slot on `SongAdvance`)
2. `run_ui` (to update when song is loaded/changed via CLI)

Dependency: Steps 3–4.

### Step 6 — CLI song/pattern commands

**File:** `engine/src/ui.rs`

Extend `handle_cli_submit` with all new `pattern *` and `song *` commands.
Add `handle_cli_pattern_cmd` and `handle_cli_song_cmd` private helpers matching
the style of `handle_cli_note_set`.

Each successful disk operation pushes a `LogTag::Cmd` entry; errors push
`LogTag::Err`.

Update `HELP_ENTRIES` with all new commands.

Write unit tests for each new branch using the same `make_channels()` / `UiState`
pattern already in the test module, using `toml::to_string` mocks rather than
actual disk I/O (pass a custom pattern dir via a test env var, or abstract the
dir resolver behind a trait — simplest: just call `toml::from_str`/`to_string`
directly in unit tests without touching disk).

Dependency: Steps 2, 5.

### Step 7 — `UiLocalSnapshot` + `render_song_panel`

**File:** `engine/src/ui_render.rs`

- Add `play_mode`, `song_slots`, `song_cursor`, `song_active_slot` fields to
  `UiLocalSnapshot`
- In `render_frame`, branch on `ui.play_mode`:
  - `Pattern` → existing `render_seq_panel(…, chunks[2])`
  - `Song` → new `render_song_panel(…, chunks[2])`
- Implement `render_song_panel`: vertical list, each slot one row, cursor/active
  highlights, slot number, filename, repeat count
- Update `render_keybind_bar` to accept `PlayMode` and switch hint string
- Update title bar to append `[PAT]` or `[SONG]`

`UiLocalSnapshot` is assembled in `run_ui` each frame; update the assembly in
`engine/src/ui.rs` to populate the new fields from `ui.play_mode`, `ui.song`,
`ui.song_cursor`, and the state snapshot's `song_slot_index`.

Unit tests: `render_song_panel` does not panic with empty slot list; with 3
slots renders all slot numbers; cursor row contains a distinguishable marker.
Use `TestBackend` exactly as existing render tests do.

Dependency: Steps 3–5.

### Step 8 — `SongAdvance` command-processor wiring in `main.rs`

**File:** `engine/src/main.rs`

- Add `Arc<RwLock<Option<Song>>>` construction and sharing
- Pass `cmd_tx.clone()` to `run_clock` (clock needs to send `SongAdvance`)
- Pass the song arc to the command-processor closure and to `run_ui`
- In the command-processor loop, handle `InputCommand::SongAdvance`:
  1. Read the song arc (read lock)
  2. Look up next slot (accounting for `song_slot_index` and `song_slot_repeat`)
  3. Load pattern from disk
  4. Acquire write lock on state, call `apply_pattern_to_state`
  5. Update `song_slot_index` / `song_slot_repeat` in state

Dependency: Steps 3–7.

### Step 9 — Integration test + acceptance verification

**File:** `engine/tests/song_mode.rs` (new)

Tests (no hardware):
- `pattern_roundtrip`: create `SequencerState`, call `pattern_from_state`,
  serialize to TOML string, deserialize, apply to a new `SequencerState`,
  verify key fields.
- `song_roundtrip`: create `Song` with 3 slots, serialize, deserialize,
  verify slot count and filenames.
- `tick_returns_pattern_end_in_song_mode`: set `play_mode = Song`,
  advance playhead to loop_out, call tick, expect `TickResult::PatternEnd`.
- `f9_f10_global_key`: `global_key_to_command(F9)` → `SwitchToPatternMode`.

Dependency: Steps 1–8.

---

## Acceptance Criteria

### AC-1: Pattern save/load
- [ ] `pattern save verse-A` creates `~/.config/midi-man-mk3/patterns/verse-A.pat.toml`
- [ ] File is valid TOML parseable by hand
- [ ] `pattern load verse-A` restores all step data, key, mode, tempo to previous values

### AC-2: Song file
- [ ] `song new my-song` creates an empty song
- [ ] `song add verse-A` appends a slot; `song save my-song` writes the file
- [ ] `song load my-song` restores the slot list
- [ ] `song remove 2` removes the second slot (1-indexed)

### AC-3: Song playback
- [ ] In song mode, when a pattern finishes its repeats, the next slot loads and plays without silence gap longer than one tick
- [ ] Song wraps to slot 0 after the last slot plays
- [ ] `song_slot_index` in state advances correctly

### AC-4: F9/F10 switching
- [ ] F9 transitions to pattern mode; F1 panel shows the 16-step grid
- [ ] F10 transitions to song mode; F1 panel shows the slot list
- [ ] Title bar shows `[PAT]` / `[SONG]` accordingly
- [ ] Switching mode does not stop playback

### AC-5: Song panel TUI
- [ ] Vertical slot list renders without panic for 0, 1, and 50+ slots
- [ ] Cursor is visually distinct (MAGENTA row highlight)
- [ ] Active playing slot is highlighted (CYAN)
- [ ] Up/Down arrows move the cursor when F1 has focus in song mode

### AC-6: CLI commands
- [ ] All new commands appear in `help` output
- [ ] Unknown patterns or songs produce `LogTag::Err` entries
- [ ] `pattern list` and `song list` log available files

### AC-7: No regressions
- [ ] All existing tests pass (`cargo test -p engine`)
- [ ] Pattern mode behavior (F9 or default) is identical to pre-song-mode behavior
- [ ] Tick timing is unaffected by song mode when in pattern mode

---

## Trade-offs

### Pattern transition: in-place state mutation vs. thread restart

**Option A (chosen): apply pattern to running state under write lock.**
The clock thread is not paused; the pattern swap happens in the command
processor on `SongAdvance`. There is a possible one-tick gap: the clock
advances the playhead before the pattern is loaded, producing a stale step.
Acceptable for a sequencer (hardware grooveboxes do the same). Mitigation: the
command processor is higher priority than `thread::spawn` default; the gap is
sub-millisecond on a modern system.

**Option B: pause the clock, swap state, resume.** Eliminates the stale-step
risk but adds significant complexity (a third channel back from the command
processor to the clock, or a `Condvar`). Overkill for this use case.

### Song data storage: UI-owned vs. state-owned

**Option A (chosen): Song in `UiState` + `Arc<RwLock<Option<Song>>>` for
cross-thread access.** Keeps `SequencerState` free of heap-allocated song data.
The command processor reads the arc on `SongAdvance`.

**Option B: inline `Song` into `SequencerState`.** Simpler thread wiring
(no second arc) but adds `Vec` allocation to a struct that is otherwise
`Clone`-by-copy-fields. Also makes the song changeable via `apply_command`
which conflates sequencer concerns with song management concerns. Rejected.

### File format: TOML vs. JSON vs. binary

TOML is confirmed by operator. JSON would be equally achievable with the same
dependencies (serde). Binary (e.g. bincode) would be smaller but not
human-editable. TOML wins on readability and is standard in the Rust ecosystem.

### Slot cursor: UiState-local vs. state-tracked

The slot cursor (which slot the user is navigating in the song panel) is UI-
only. It does not need to be in `SequencerState` because no other thread
reads it. However, `song_slot_index` (which slot is currently playing) must
be in `SequencerState` so the clock/render can read it under the read lock.
These are distinct fields and must not be conflated.

---

## Dependencies and Prerequisites

### New crate dependencies

Add to `engine/Cargo.toml` `[dependencies]`:

```toml
serde = { version = "1", features = ["derive"] }
toml = "0.8"
```

`serde` 1.x and `toml` 0.8.x are the current stable series. `toml` 0.8
depends on `serde` 1; there is no version conflict with the transitively-
present `serde` 1.0.228. After adding, run `cargo build -p engine` to verify
the lockfile resolves cleanly.

No other new external crates are needed. Filesystem access uses `std::fs` and
`std::path::PathBuf` from std. The `HOME` environment variable is used to
derive the config directory; no platform-specific crate is required.

### Environment / config directory

The directory `~/.config/midi-man-mk3/` is created automatically by
`pattern_dir()` / `song_dir()` via `std::fs::create_dir_all`. No migration
is needed for existing users (there are no existing files). No `.env` changes.

### Source files created or modified

| File | Change |
|------|--------|
| `engine/Cargo.toml` | Add `serde`, `toml` dependencies |
| `engine/src/pattern.rs` | New — data model and file I/O |
| `engine/src/lib.rs` | Add `pub mod pattern;` |
| `engine/src/state.rs` | Add `PlayMode`, `TickResult`, new state fields, new `InputCommand` variants |
| `engine/src/input.rs` | Add `F9`, `F10` to `KeyCodeSimple`; add new `InputCommand` variants |
| `engine/src/ui.rs` | Add song fields to `UiState`; extend `translate_key`, `handle_cli_submit`, `HELP_ENTRIES`; extend `UiLocalSnapshot` assembly |
| `engine/src/ui_render.rs` | Add `play_mode`/song fields to `UiLocalSnapshot`; add `render_song_panel`; update `render_keybind_bar` and title bar |
| `engine/src/clock.rs` | Accept `SyncSender<InputCommand>`; handle `TickResult::PatternEnd` |
| `engine/src/main.rs` | Wire song arc; pass `cmd_tx` clone to clock; extend command processor |
| `engine/tests/song_mode.rs` | New — integration tests |

---

## Summary: Key Decisions, Affected Files, Recommended Order

**Key decisions:**
- Pattern and song files are TOML in `~/.config/midi-man-mk3/patterns/` and
  `.../songs/`
- Song mode replaces the F1 zone; it does not add a new layout zone
- `tick()` return type changes to `TickResult` to signal pattern-end cleanly
- The clock thread receives a `cmd_tx` clone to forward `SongAdvance`
- Song data lives in `UiState` + `Arc<RwLock<Option<Song>>>` for
  cross-thread slot lookup; it is not in `SequencerState`
- Pattern transitions happen in the command-processor thread under a write lock
  with no clock pause (accepts sub-tick jitter)

**Most affected source files (highest change density):**
1. `engine/src/state.rs` — new enum, new fields, return type change
2. `engine/src/ui.rs` — song UiState fields, translate_key, CLI commands
3. `engine/src/ui_render.rs` — song panel render, snapshot fields
4. `engine/src/main.rs` — arc wiring, clock signature
5. `engine/src/pattern.rs` — new file (all new)

**Recommended implementation order (each step is a separately committable unit):**
1. Step 1 — Cargo.toml dependencies
2. Step 2 — `pattern.rs` data model + roundtrip tests
3. Step 3 — `state.rs` + `input.rs` changes (TickResult, PlayMode, new commands)
4. Step 4 — `KeyCodeSimple` F9/F10 + global key mapping
5. Step 5 — `UiState` song fields + translate_key song navigation
6. Step 6 — CLI pattern/song commands
7. Step 7 — `ui_render.rs` song panel
8. Step 8 — `main.rs` wiring
9. Step 9 — Integration tests
