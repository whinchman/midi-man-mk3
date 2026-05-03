# UI Refactor Plan — Issue #78

**Status:** Ready for implementation  
**Author:** Architect agent  
**Date:** 2026-05-03  

---

## 1. Architecture Overview

### Goal

Complete visual and interaction overhaul of the ratatui TUI. Replace the
current minimal "top bar + step rows + overlay" layout with a full 7-zone
vertical stack, a 4-panel focus model (F1–F4), a runtime CLI for MIDI port
selection, and a cyberpunk color palette (#0a0a0a / #00ffff / #ff007f).

### High-Level Structure

```
main.rs           — startup, thread wiring (no pre-TUI prompts)
state.rs          — SequencerState additions (rand_seed u32, midi_device_name)
input.rs          — FocusPanel enum, new InputCommand variants (BpmDelta,
                    CliSubmit, CliChar, CliBackspace, PortChange, ChannelChange)
ui.rs             — UiState rewrite (focus, CLI line buffer, log ring buffer)
ui_render.rs      — full rewrite of render_frame (7-zone layout)
midi_out.rs       — runtime port-change command support via MidiCtrlMsg channel
```

### Data Flow

```
Keyboard → ui.rs (translate_key per focus panel)
                → cmd_tx: SyncSender<InputCommand>
                       → cmd-processor thread → state.apply_command()
                                               → ui_notify_tx.try_send(())
                       → midi_ctrl_tx: SyncSender<MidiCtrlMsg>  [new]
                                    → midi_out thread (port swap)
```

The CLI path (F4 input) is the only path that writes to `midi_ctrl_tx`
directly from `ui.rs`. All other commands flow through the existing
`cmd_tx → state.apply_command()` pipeline unchanged.

### Key Decisions

1. **Runtime port change via a separate control channel** (`MidiCtrlMsg`
   enum, `SyncSender<MidiCtrlMsg>`). This avoids coupling `InputCommand`
   (which is also consumed by the HID thread and state processor) with MIDI
   infrastructure concerns. The midi_out thread listens on both the existing
   `midi_rx: Receiver<MidiEvent>` and a new `ctrl_rx: Receiver<MidiCtrlMsg>`
   using `select!` semantics (crossbeam-channel `select!` macro, or a simpler
   non-blocking `try_recv` polling approach — see §6).

2. **CLI line buffer and log ring buffer live in `UiState` (UI thread only)**.
   They are not part of `SequencerState`. The log is rendered from `UiState`
   on each frame; command responses are appended to the log from within
   `ui.rs` after the CLI command is processed. This avoids locking the shared
   state for UI-local text display.

3. **Focus enum in `UiState` (UI thread only)**. Panel focus is not stored in
   `SequencerState`. It drives key dispatch in `translate_key` and border
   highlighting in `render_frame`.

4. **Full rewrite of `ui_render.rs`**. The existing render logic is not worth
   preserving incrementally — the layout zones, color strategy, and widget
   hierarchy are all being replaced.

5. **`rng_seed` stays as `u64` in `SequencerState`** but a `rand_seed: u32`
   display field is added as a separate `SequencerState` field, seeded from
   the lower 32 bits of `rng_seed` and updated whenever a `SeedSet` command
   is applied. The CLI `seed 0xXXXX` command writes both fields.

6. **No `crossbeam-channel` dependency**. Use two `std::sync::mpsc` channels
   (`midi_rx` and `ctrl_rx`) with non-blocking `try_recv` in a tight loop
   inside `run_midi_out`, polling `ctrl_rx` each iteration before blocking
   on `midi_rx.recv()`. This avoids adding a new crate and stays on `std`.

7. **`ratatui::style::Color::Rgb(r, g, b)`** is used directly for the
   custom palette. Ratatui 0.30 (already in `Cargo.toml`) fully supports
   `Color::Rgb`. No palette wrapper struct is needed.

8. **Startup**: `choose_midi_port()` and `choose_midi_channel()` are removed.
   `midi_ctrl_tx` is passed to `run_ui` so F4 CLI commands can send port/channel
   change requests without going through shared state.

---

## 2. New Data Structures

### 2.1 `FocusPanel` (in `input.rs`)

```rust
/// Which panel currently holds keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPanel {
    Sequencer,    // F1 — step select, space, enter
    SeqParams,    // F2 — ←/→ param select, ↑/↓ adjust
    RandParams,   // F3 — ←/→ param select, ↑/↓ adjust
    Cli,          // F4 — text input mode
}
```

### 2.2 `MidiCtrlMsg` (in `midi_out.rs`)

```rust
/// Runtime control messages for the MIDI output thread.
pub enum MidiCtrlMsg {
    /// Switch to the port whose name contains this substring (case-insensitive).
    ChangePort(String),
    /// Set the MIDI channel (1-indexed; stored 0-indexed in state).
    ChangeChannel(u8),
}
```

### 2.3 New `InputCommand` variants (in `input.rs`)

```rust
/// Adjust BPM by signed delta (always active, no focus required).
BpmDelta(i8),
/// F4 CLI: append a character to the line buffer.
CliChar(char),
/// F4 CLI: delete last character.
CliBackspace,
/// F4 CLI: submit the current line buffer.
CliSubmit,
/// Focus a panel by pressing F1–F4.
FocusPanel(FocusPanel),
/// F2/F3: select param by absolute index in the active panel.
PanelParamSelect(u8),
/// F2/F3: adjust selected param by delta.
PanelParamDelta(i8),
```

Note: `CliChar`, `CliBackspace`, `CliSubmit` are handled entirely inside
`ui.rs` and never sent on `cmd_tx` — they mutate `UiState.cli_line` directly.
`CliSubmit` triggers CLI parsing in `ui.rs`, then either sends a
`MidiCtrlMsg` or an `InputCommand` (e.g. `SeedSet`) on the appropriate channel.

### 2.4 `UiState` additions (in `ui.rs`)

```rust
struct UiState {
    focus: FocusPanel,
    // F1
    selected_step: usize,
    // F2 / F3
    seq_param_idx: u8,
    rand_param_idx: u8,
    // F4 CLI
    cli_line: String,       // current input line, max 256 chars
    cli_log: VecDeque<LogEntry>, // ring buffer, max CLI_LOG_CAPACITY entries
    // MIDI runtime info (echoed from MidiCtrlMsg responses)
    midi_device_name: String,
    midi_channel_display: u8,
}

const CLI_LOG_CAPACITY: usize = 200;

struct LogEntry {
    timestamp_ms: u64,   // millis since startup (from Instant)
    tag: LogTag,
    text: String,
}

enum LogTag { Info, Midi, Err, Cmd }
```

### 2.5 `SequencerState` additions (in `state.rs`)

```rust
/// User-facing random seed (lower 32 bits of rng_seed, settable via CLI).
pub rand_seed: u32,
/// Name of the connected MIDI output port (for title bar display).
pub midi_device_name: String,
```

---

## 3. Module-by-Module Change Breakdown

### 3.1 `engine/src/state.rs`

**Changes:**
- Add `rand_seed: u32` field, default `0x853C_49E6` (lower 32 of current
  `rng_seed` default).
- Add `midi_device_name: String` field, default `String::new()` (empty = no
  device selected).
- Add `InputCommand::BpmDelta(i8)` handling in `apply_command`:
  ```rust
  InputCommand::BpmDelta(d) => {
      self.tempo_bpm = (self.tempo_bpm as i32 + d as i32).clamp(20, 300) as u16;
  }
  ```
- Add `InputCommand::SeedSet(u32)` handling:
  ```rust
  InputCommand::SeedSet(seed) => {
      self.rand_seed = seed;
      self.rng_seed = seed as u64 | ((seed as u64) << 32);
  }
  ```
- Add `InputCommand::MidiDeviceName(String)` handling (for title bar sync):
  ```rust
  InputCommand::MidiDeviceName(name) => {
      self.midi_device_name = name;
  }
  ```
- `Default::default()`: initialize new fields.

**No changes to existing fields or methods.**

### 3.2 `engine/src/input.rs`

**Changes:**
- Add `FocusPanel` enum (see §2.1).
- Add new `InputCommand` variants: `BpmDelta(i8)`, `SeedSet(u32)`,
  `MidiDeviceName(String)`, `FocusPanel(FocusPanel)`,
  `PanelParamSelect(u8)`, `PanelParamDelta(i8)`.
  (CLI variants `CliChar`, `CliBackspace`, `CliSubmit` are UI-local and NOT
  added to `InputCommand` — see §2.3 rationale.)
- Rewrite `root_key_to_command` to be a no-op for F1–F4 (those now set
  focus via the UI layer, not via `InputCommand`).
- Remove `OpenOverlay`, `CloseOverlay`, `ParamSelect`, `ParamSelectDelta`,
  `ParamValueDelta` from `InputCommand` — these are superseded by the new
  focus model.
  > **WARNING**: This is a breaking change for the HID thread (`hid.rs`).
  > See §3.6.
- Add `KeyCodeSimple::F3`, `KeyCodeSimple::F4`, `KeyCodeSimple::Plus`,
  `KeyCodeSimple::Minus`, `KeyCodeSimple::Backspace` variants.
- New pure function `panel_key_to_command(key: KeyCodeSimple, focus: FocusPanel) -> Option<InputCommand>`.

### 3.3 `engine/src/ui.rs`

**Changes (significant rewrite of `UiState` and `run_ui`):**

- Expand `UiState` with fields from §2.4.
- `run_ui` signature gains `midi_ctrl_tx: SyncSender<MidiCtrlMsg>` parameter.
- Rewrite `translate_key` to dispatch based on `ui.focus`:
  - Any panel: `F1` → `FocusPanel(Sequencer)`, `F2` → `FocusPanel(SeqParams)`,
    etc.
  - Any panel: `+` / `-` → `BpmDelta(+1)` / `BpmDelta(-1)` sent on `cmd_tx`.
  - `FocusPanel::Sequencer`: `←/→` → `StepSelectDelta`, `Space` →
    `ToggleStep`, `Enter` → `Confirm`, `↑/↓` → `NoteDelta`.
  - `FocusPanel::SeqParams`: `←/→` → adjust `ui.seq_param_idx`, `↑/↓` →
    send `PanelParamDelta`.
  - `FocusPanel::RandParams`: same but for `ui.rand_param_idx`.
  - `FocusPanel::Cli`: all printable chars append to `ui.cli_line`; Enter
    calls `handle_cli_submit`; Backspace trims last char.
- New `handle_cli_submit(ui: &mut UiState, cmd_tx, midi_ctrl_tx)`:
  - Parse `ui.cli_line` content:
    - `port <name>` → `midi_ctrl_tx.send(MidiCtrlMsg::ChangePort(name))`
      + append log entry + send `InputCommand::MidiDeviceName(name)` on `cmd_tx`.
    - `channel <n>` → `midi_ctrl_tx.send(MidiCtrlMsg::ChangeChannel(n))`
      + update `ui.midi_channel_display` + append log.
    - `seed <hex>` → parse hex → `cmd_tx.send(InputCommand::SeedSet(v))`
      + append log.
    - Unknown → append error to log.
  - Clear `ui.cli_line` after submit.
- Remove overlay state (`overlay`, `selected_param`) from `UiState`.

### 3.4 `engine/src/ui_render.rs`

**Full rewrite of `render_frame`. Keep helper functions that remain valid.**

**New `render_frame` signature:**
```rust
pub fn render_frame(
    frame: &mut Frame,
    state: &SequencerState,
    ui: &UiLocalSnapshot,
)
```

Where `UiLocalSnapshot` is a new struct (or the same `UiState` passed by
reference) carrying: `focus: FocusPanel`, `selected_step: usize`,
`seq_param_idx: u8`, `rand_param_idx: u8`, `cli_line: &str`,
`cli_log: &VecDeque<LogEntry>`, `midi_device_name: &str`,
`midi_channel_display: u8`.

> To avoid leaking the full `UiState` into `ui_render.rs` (which has no
> `hw-io` gate), define `UiLocalSnapshot` as a public struct in `ui_render.rs`
> that `ui.rs` populates before calling `render_frame`. This preserves the
> existing compile-time separation.

**Layout (7 zones, `Direction::Vertical`):**

```
Constraint::Length(1)   // [0] Title bar
Constraint::Length(1)   // [1] Transport bar
Constraint::Min(5)      // [2] F1·SEQ panel (step cards, min 5 rows)
Constraint::Length(3)   // [3] F2·SEQ PARAMS panel (border + 1 content row)
Constraint::Length(3)   // [4] F3·RANDOM PARAMS panel
Constraint::Min(5)      // [5] F4·CLI panel (log area, min 5 rows)
Constraint::Length(1)   // [6] Bottom keybind bar
```

Total fixed rows = 1+1+3+3+1 = 9. The Min(5) zones expand to fill terminal
height. If the terminal is very short (<20 rows), both Min(5) zones shrink
equally — this is acceptable for the MVP.

**Color palette constants (top of file):**

```rust
const BG:       Color = Color::Rgb(10,   10,  10);  // #0a0a0a
const CYAN:     Color = Color::Rgb(0,   255, 255);  // #00ffff
const MAGENTA:  Color = Color::Rgb(255,   0, 127);  // #ff007f
const FUCHSIA:  Color = Color::Rgb(255,   0, 255);  // #ff00ff
const DIM_CYAN: Color = Color::Rgb(0,    64,  64);  // ~25% cyan
const GREEN:    Color = Color::Rgb(0,   200,  80);  // PLAYING
const GRAY:     Color = Color::Rgb(136, 136, 136);  // #888888
```

**Zone rendering functions (all `fn`, no heap on hot path):**

- `render_title_bar(frame, state, ui, area)`:
  - Left: `"▶ 217 Industries / midi-man-mk3"` — "midi-man-mk3" in `FUCHSIA`
  - Right: `"MIDI OUT <device> CH:<n>"` — assembled from `ui.midi_device_name`
    and `ui.midi_channel_display`

- `render_transport_bar(frame, state, area)`:
  - Single line: `"BPM <n>  KEY <k>  MODE <m>  STEP <s>  STATUS ► <state>"`
  - Status text colored: `GREEN` for PLAYING, `CYAN` for PAUSED, default for STOPPED

- `render_seq_panel(frame, state, ui, area)`:
  - Border block with title `"F1 · SEQ"`, border `CYAN` when focused, dim otherwise
  - Inner area split into 16 equal columns
  - Each column: 3 rows (step number top-left in small text, note name center, enabled indicator bottom)
  - Step color logic:
    - Playhead step: border + text `MAGENTA`, background tinted
    - Enabled non-playhead: `CYAN`
    - Disabled: `DIM_CYAN`
    - Selected (cursor): outlined in `MAGENTA`
  - If terminal columns < 80, render compact 2-char note names; if < 48, render
    step numbers only (graceful degradation note — not MVP-blocking)

- `render_seq_params_panel(frame, state, ui, area)`:
  - Bordered block `"F2 · SEQ PARAMS"`, border highlight when focused
  - Single content row: `"KEY <v>  MODE <v>  SWING <v>  STEP <v>  L.IN <v>  L.OUT <v>  PAUSE <v>  PLAY <v>"`
  - Selected param (ui.seq_param_idx) rendered in `MAGENTA` + bold

- `render_rand_params_panel(frame, state, ui, area)`:
  - Bordered block `"F3 · RANDOM PARAMS"`, border highlight when focused
  - Single content row: `"N.RND <v>  T.RND <v>  ROLL <v>  V.MAX <v>  T.TYPE <v>  S.RND <v>  S.QUANT <v>  SEED 0x<hex>"`
  - SEED formatted as `format!("0x{:04X}", state.rand_seed)`
  - Selected param highlighted in `MAGENTA`

- `render_cli_panel(frame, ui, area)`:
  - Bordered block `"F4 · CLI"`, border highlight when focused
  - Inner area: top portion = scrollable log; bottom 1 row = `"> <cli_line>_"`
  - Log entries rendered with timestamp in `GRAY`, tag in `CYAN`/`GREEN`/red,
    message in default color
  - Scrolling: always show the last N lines that fit in the available height

- `render_keybind_bar(frame, area)`:
  - Static line: `"F1-F4 focus | P play | +/- BPM | ←/→ param | ↑/↓ adjust | space toggle | enter confirm | esc cancel | ^C quit"`
  - All in `DIM_CYAN`

**Keep from existing `ui_render.rs`:**
- `key_name`, `mode_name`, `step_size_label`, `status_label`
- `shift_param_value_string`, `shift_pending_param_value_string`
- `param_value_string`, `pending_param_value_string`
- `tempo_roll_point_name`, `tempo_rand_type_name`

**Remove from existing `ui_render.rs`:**
- `SHIFT_PARAMS`, `REGULAR_PARAMS` constants (replaced by inline panel rendering)
- `render_overlay`, `render_steps` (replaced by new panel functions)

### 3.5 `engine/src/midi_out.rs`

**Changes:**

- Add `MidiCtrlMsg` enum (see §2.2).
- Change `run_midi_out` signature:
  ```rust
  pub fn run_midi_out(
      rx: Receiver<MidiEvent>,
      ctrl_rx: Receiver<MidiCtrlMsg>,
      port_name: Option<String>,
  )
  ```
- Internal loop — replace the simple `while let Ok(event) = rx.recv()` with
  a polling loop that checks `ctrl_rx` between MIDI events:
  ```rust
  loop {
      // Non-blocking ctrl check first.
      match ctrl_rx.try_recv() {
          Ok(MidiCtrlMsg::ChangePort(name)) => {
              // Drop old sender, open new port.
              sender = open_port(Some(&name));
          }
          Ok(MidiCtrlMsg::ChangeChannel(_)) => {
              // Channel is applied at the MidiEvent level via state.midi_channel.
              // This message is informational only for the midi_out thread.
          }
          Err(TryRecvError::Empty) => {}
          Err(TryRecvError::Disconnected) => break,
      }
      // Blocking recv on MIDI events (50 ms timeout via recv_timeout).
      match rx.recv_timeout(Duration::from_millis(50)) {
          Ok(event) => {
              if let Some(ref mut s) = sender {
                  dispatch(s, event);
              }
          }
          Err(RecvTimeoutError::Timeout) => continue,
          Err(RecvTimeoutError::Disconnected) => break,
      }
  }
  ```
  This is still safe Rust, no heap on the hot path, and requires no new crate.

- Remove `choose_midi_port()` and `choose_midi_channel()` (or gate them
  `#[cfg(test)]` if existing tests reference them — check first).
- `open_port` changes: make it callable with `Option<&str>` filter (already
  the case) and make it non-fatal when no port is found (return `Option<Box<dyn MidiSender>>`
  — already the case).

### 3.6 `engine/src/main.rs`

**Changes:**

- Remove `selected_midi_port` / `selected_midi_channel` / `choose_midi_port()`
  / `choose_midi_channel()` calls.
- Add `let (midi_ctrl_tx, midi_ctrl_rx) = mpsc::sync_channel::<MidiCtrlMsg>(16);`
- Pass `midi_ctrl_rx` to `run_midi_out` thread.
- Pass `midi_ctrl_tx` clone to `run_ui`.
- Remove `state.write().midi_channel = selected_midi_channel` initialization.
- Thread 5 (`run_ui`) spawning: add `midi_ctrl_tx` argument.

**No changes to thread shutdown order.**

### 3.7 `engine/src/hid.rs`

**Changes (minimal, for compatibility):**

The HID thread currently sends `InputCommand::OpenOverlay`, `CloseOverlay`,
`ParamSelect`, `ParamSelectDelta`, `ParamValueDelta`. These are being removed
from `InputCommand`.

Options:
- **Option A (recommended)**: Replace the HID overlay-related sends with the
  new `PanelParamSelect(u8)` and `PanelParamDelta(i8)` variants. Remove
  `OpenOverlay`/`CloseOverlay` sends from `hid.rs` — focus switching via
  hardware is not in scope for this refactor.
- **Option B**: Keep the old variants in `InputCommand` as deprecated no-ops
  in `apply_command`, leaving `hid.rs` unchanged. Remove in a follow-up.

**Recommendation: Option A** — cleaner, prevents dead code. The HID thread
should still compile; it just sends fewer command types.

> **Risk**: HID module is large (21K). Assign a separate coder task for HID
> compatibility changes, or leave as Option B until the HID refactor issue is
> filed.

---

## 4. Cross-Module Interfaces

### 4.1 CLI Command Flow (F4 → state/midi_out)

```
User types in F4 panel
    → chars accumulate in ui.cli_line (UiState, UI thread only)
    → Enter key calls handle_cli_submit(ui, cmd_tx, midi_ctrl_tx)
        parse "port <name>"    → midi_ctrl_tx.send(ChangePort(name))
                               → cmd_tx.send(MidiDeviceName(name))   [state sync]
        parse "channel <n>"   → midi_ctrl_tx.send(ChangeChannel(n))
                               → cmd_tx.send(BpmDelta(0))            [noop, triggers notify]
                               (midi_channel in state updated on next NoteOn via channel param)
        parse "seed <hex>"    → cmd_tx.send(SeedSet(v))
        unknown               → append error to ui.cli_log
    → append response to ui.cli_log
    → clear ui.cli_line
```

For `channel <n>`: the MIDI channel in `SequencerState.midi_channel` must also
be updated so the clock thread's NoteOn events use the new channel. Send
`InputCommand::ChannelSet(u8)` through `cmd_tx`:

```rust
// New InputCommand variant:
ChannelSet(u8),

// In apply_command:
InputCommand::ChannelSet(ch) => {
    self.midi_channel = ch.saturating_sub(1); // 1-indexed → 0-indexed
}
```

### 4.2 Focus Model Key Dispatch

```
ui.rs::translate_key(event, &ui) -> Option<Action>

enum Action {
    SendCmd(InputCommand),       // goes to cmd_tx
    SendCtrl(MidiCtrlMsg),       // goes to midi_ctrl_tx (CLI submit only)
    LocalOnly,                   // handled in UiState, no channel send
}
```

This avoids making `cmd_tx.send` and `midi_ctrl_tx.send` calls deep in match
arms — `translate_key` returns the action, and the caller dispatches it.

### 4.3 `UiLocalSnapshot` Interface

`ui_render.rs` defines:
```rust
pub struct UiLocalSnapshot<'a> {
    pub focus: FocusPanel,
    pub selected_step: usize,
    pub seq_param_idx: u8,
    pub rand_param_idx: u8,
    pub cli_line: &'a str,
    pub cli_log: &'a VecDeque<LogEntry>,
    pub midi_device_name: &'a str,
    pub midi_channel_display: u8,
}
```

`ui.rs` constructs this from `UiState` fields on each render cycle:
```rust
let snapshot = UiLocalSnapshot {
    focus: ui.focus,
    selected_step: ui.selected_step,
    seq_param_idx: ui.seq_param_idx,
    rand_param_idx: ui.rand_param_idx,
    cli_line: &ui.cli_line,
    cli_log: &ui.cli_log,
    midi_device_name: &ui.midi_device_name,
    midi_channel_display: ui.midi_channel_display,
};
```

`LogEntry` and `LogTag` are also defined in `ui_render.rs` (always compiled)
so they can be referenced without the `hw-io` feature.

---

## 5. Runtime MIDI Port Changes

### Approach: Dual-channel polling in `run_midi_out`

The midi_out thread receives `MidiEvent` on `rx` (existing) and
`MidiCtrlMsg` on `ctrl_rx` (new). It polls `ctrl_rx` non-blockingly on each
loop iteration, then does a 50ms `recv_timeout` on `rx`.

**Port-swap procedure inside run_midi_out:**
1. On `MidiCtrlMsg::ChangePort(name)`: call `open_port(Some(&name))`.
2. If successful: replace `sender` with the new connection.
3. If `open_port` returns `None`: log to stderr, keep old sender (or None).
4. The old `MidiOutputConnection` is dropped when the `Box<dyn MidiSender>`
   is replaced — this sends any in-flight note-offs on the old port before
   the connection closes. ALSA's midir connection cleanup handles this.

**Channel change:** The channel byte is carried in each `MidiEvent::NoteOn`
already (via `state.midi_channel`). The `MidiCtrlMsg::ChangeChannel` message
is consumed by `run_midi_out` as a no-op (the real update happens via
`InputCommand::ChannelSet` in the state processor). This keeps the architecture
clean — midi_out never needs to read from `SequencerState`.

---

## 6. Ratatui Layout and Color Strategy

### Ratatui 0.30 API Notes

- `Color::Rgb(r, g, b)` — available since ratatui 0.20, stable in 0.30.
- `Block::new().borders(Borders::ALL).border_style(Style::default().fg(CYAN))`
  for focused panels.
- `Block::new().borders(Borders::ALL).border_style(Style::default().fg(DIM_CYAN))`
  for unfocused panels.
- The 16 step cards in F1 use horizontal `Layout::horizontal()` with 16
  `Constraint::Ratio(1, 16)` columns inside the F1 panel's inner area.
- `Paragraph::new(lines).block(step_block)` for each step card.
- Background color: ratatui `Style::default().bg(BG)` on the outermost
  `Block` or applied to the terminal via `terminal.backend_mut()` if needed.
  The terminal's default background is typically black, which is close enough
  to `#0a0a0a` without needing explicit BG fills on every widget.

### Color Reference

| Token     | Hex      | `Color::Rgb`          | Use                              |
|-----------|----------|-----------------------|----------------------------------|
| BG        | #0a0a0a  | Rgb(10, 10, 10)       | Terminal background               |
| CYAN      | #00ffff  | Rgb(0, 255, 255)      | Borders, enabled steps, accents  |
| MAGENTA   | #ff007f  | Rgb(255, 0, 127)      | Playhead step, selected cursor   |
| FUCHSIA   | #ff00ff  | Rgb(255, 0, 255)      | Project name in title bar        |
| DIM_CYAN  | ~#004040 | Rgb(0, 64, 64)        | Disabled steps, unfocused borders|
| GREEN     | #00c850  | Rgb(0, 200, 80)       | PLAYING status                   |
| GRAY      | #888888  | Rgb(136, 136, 136)    | Log timestamps, dim text         |
| WHITE     | #ffffff  | Rgb(255, 255, 255)    | Playhead note name               |

### Step Card Layout (per card, within a Constraint::Ratio(1,16) column)

```
┌──────┐   ← CYAN border (enabled), MAGENTA (playhead), DIM_CYAN (disabled)
│01    │   ← step number, small, top-left, GRAY
│  C4  │   ← note name, centered, CYAN/MAGENTA/DIM_CYAN
│  ●   │   ← enabled indicator, bottom-center, same color as border
└──────┘
```

If terminal width < 80 columns, 16 cards at `Ratio(1,16)` may be too narrow
for the note name. Minimum viable card width is 4 columns (e.g. `C#4`).
At 80 columns = 5 columns per card = acceptable. At 64 columns = 4 per card.
Below 64 columns, note names may be truncated — acceptable for MVP.

---

## 7. Acceptance Criteria

- [ ] TUI starts immediately without any stdin prompts
- [ ] Title bar shows `▶ 217 Industries / midi-man-mk3` left, `MIDI OUT <device> CH:<n>` right
- [ ] "midi-man-mk3" in title bar renders in #ff00ff (fuchsia/magenta)
- [ ] Transport bar shows `BPM <n>  KEY <k>  MODE <m>  STEP <s>  STATUS ► <state>`
- [ ] STATUS value is green when PLAYING, default when STOPPED/PAUSED
- [ ] F1 panel shows 16 step cards in a horizontal row
- [ ] Active playhead step card renders with #ff007f (magenta) border/text
- [ ] Enabled non-playhead steps render with #00ffff (cyan)
- [ ] Disabled steps render with dim cyan (~#004040)
- [ ] F1 focus: ←/→ moves step selection, space toggles enable, enter opens note edit
- [ ] F2 panel always visible with flat param bar (KEY MODE SWING STEP L.IN L.OUT PAUSE PLAY)
- [ ] F2 focus: ←/→ moves between params, ↑/↓ adjusts selected param
- [ ] F3 panel always visible with flat random params bar
- [ ] F3 SEED shown as `0xXXXX` hex format
- [ ] F3 focus: same interaction model as F2
- [ ] F4 CLI panel shows log area + `> ` input line
- [ ] F4 `port <name>` command switches MIDI port at runtime (no restart)
- [ ] F4 `channel <n>` command switches MIDI channel at runtime (1–16)
- [ ] F4 `seed <hex>` command sets random seed and displays new value in F3
- [ ] Unknown F4 command shows error in log
- [ ] `+` / `-` keys adjust BPM from any panel (no focus required)
- [ ] `P` key toggles play/stop from any panel
- [ ] Focused panel has highlighted (#00ffff) border
- [ ] Unfocused panels show dim border
- [ ] Bottom keybind bar shows all key hints
- [ ] `cargo test -p engine` passes (existing tests must not regress)
- [ ] `cargo clippy -p engine --features hw-io` passes clean
- [ ] `cargo build -p engine --features hw-io --release` succeeds

---

## 8. Trade-offs and Alternatives

### A. Runtime port change: separate `MidiCtrlMsg` channel vs. `InputCommand` variant

**Chosen: separate channel.** `InputCommand` is consumed by the state
processor and HID thread — routing MIDI infrastructure commands through it
creates awkward coupling. A dedicated `SyncSender<MidiCtrlMsg>` to the
midi_out thread is architecturally cleaner and avoids modifying
`apply_command` for MIDI-only concerns.

**Alternative: `InputCommand::PortChange(String)` + state processor forwards
to midi_out.** Would require adding a second output channel to the command
processor, complicating its logic. Rejected.

### B. Log buffer in `UiState` vs. `SequencerState`

**Chosen: `UiState` (UI thread only).** Log entries are display-only; they
don't affect sequencer behavior. Storing them in `SequencerState` would
require locking the RwLock on every log append, adding latency on the write
path. The UI thread is the only consumer, so keeping the log local is natural.

**Alternative: `SequencerState.cli_log: VecDeque<LogEntry>`.** Would allow
the state processor to log responses, but at the cost of RwLock contention.
Rejected.

### C. Focus model: `FocusPanel` in `UiState` vs. `SequencerState`

**Chosen: `UiState` (UI thread only).** Focus is a UI concern that affects
key dispatch and border rendering but not sequencer behavior. The HID thread
does not need to know about focus.

**Alternative: in `SequencerState`.** The HID thread could read focus to
route its encoder events. Deferred — if the HID thread needs panel-aware
routing, add it as a follow-up.

### D. 16 step cards: `Constraint::Ratio(1,16)` vs. fixed `Constraint::Length(5)`

**Chosen: `Ratio(1,16)`.** Adapts to terminal width automatically. At 80
columns = 5 wide per card, acceptable. Fixed Length(5) would overflow on
narrow terminals.

**Risk: very narrow terminals.** Note names will be truncated below ~64
columns. Acceptable for a hardware-targeted application.

---

## 9. Dependencies and Prerequisites

**No new Cargo dependencies required.** All needed APIs exist in:
- `ratatui 0.30` (already in `Cargo.toml`) — `Color::Rgb`, `Constraint::Ratio`,
  `Layout::horizontal`, `VecDeque` rendering
- `std::sync::mpsc` — dual-channel polling with `recv_timeout` + `try_recv`
- `std::collections::VecDeque` — log ring buffer (stdlib)
- `std::time::Instant` — log timestamps

**Prerequisites:**
- None. All changes are within the single `engine/` crate.

---

## 10. Implementation Order

Tasks are ordered by dependency. Items with the same step number can run in
parallel on separate worktrees.

### Step 1 — Foundation (no parallelism, everything depends on this)

**Task 1.1: `state.rs` additions**
- Add `rand_seed: u32`, `midi_device_name: String` fields
- Add `InputCommand::BpmDelta(i8)`, `SeedSet(u32)`, `ChannelSet(u8)`,
  `MidiDeviceName(String)` variants
- Add handlers in `apply_command`
- Update `Default::default()`
- Tests: `bpm_delta_clamps_to_range`, `seed_set_updates_both_fields`,
  `channel_set_converts_1_indexed`
- **Agent**: coder

### Step 2 — Parallel (depends on Step 1)

**Task 2.1: `input.rs` refactor**
- Add `FocusPanel` enum
- Add `PanelParamSelect(u8)`, `PanelParamDelta(i8)` to `InputCommand`
- Add `KeyCodeSimple::F3`, `F4`, `Plus`, `Minus`, `Backspace`
- Add `panel_key_to_command` pure function
- Remove `OpenOverlay`, `CloseOverlay`, `ParamSelect`, `ParamSelectDelta`,
  `ParamValueDelta` from `InputCommand` (or keep as dead-code stubs if hid.rs
  compatibility is deferred — see §3.7)
- Update existing key translation tests
- **Agent**: coder

**Task 2.2: `midi_out.rs` changes**
- Add `MidiCtrlMsg` enum
- Change `run_midi_out` signature to accept `ctrl_rx: Receiver<MidiCtrlMsg>`
- Rewrite event loop with `try_recv` / `recv_timeout` dual-channel polling
- Remove `choose_midi_port` and `choose_midi_channel` (or gate `#[cfg(test)]`)
- Tests: `ctrl_rx_port_change_swaps_sender`, `ctrl_rx_disconnect_exits_loop`
- **Agent**: coder

### Step 3 — Parallel (depends on Step 2)

**Task 3.1: `ui_render.rs` full rewrite**
- Define `UiLocalSnapshot`, `LogEntry`, `LogTag` structs
- Implement all 7 zone render functions (see §3.4)
- Implement color palette constants
- Port retained helper functions
- Tests with `TestBackend`:
  - `render_frame_does_not_panic_on_empty_state`
  - `title_bar_contains_project_name`
  - `step_cards_playhead_has_magenta_style`
  - `cli_panel_shows_log_entries`
- **Agent**: coder

**Task 3.2: `hid.rs` compatibility**
- Replace `OpenOverlay`/`CloseOverlay`/`ParamSelectDelta`/`ParamValueDelta`
  sends with `PanelParamSelect`/`PanelParamDelta` equivalents
- Keep HID encoder behavior semantically equivalent
- Tests: existing HID tests must still pass
- **Agent**: coder (lower priority, can be Option B stub until separate issue)

### Step 4 — Sequential (depends on Steps 2+3)

**Task 4.1: `ui.rs` rewrite**
- Expand `UiState` with all new fields
- Add `handle_cli_submit` function
- Rewrite `translate_key` with focus-aware dispatch
- Update `run_ui` signature and body
- Tests:
  - `cli_submit_port_sends_midi_ctrl_msg`
  - `cli_submit_channel_sends_channel_set_cmd`
  - `cli_submit_unknown_appends_error_to_log`
  - `bpm_plus_key_sends_bpm_delta_from_any_focus`
- **Agent**: coder

### Step 5 — Final wiring (depends on Step 4)

**Task 5.1: `main.rs` cleanup**
- Remove pre-TUI prompt calls
- Wire `midi_ctrl_tx`/`midi_ctrl_rx` channels
- Pass `midi_ctrl_tx` to `run_ui`
- Smoke test: binary starts, TUI renders without crash
- **Agent**: coder

### Step 6 — QA (depends on Step 5)

**Task 6.1: Integration test**
- Build with `--features hw-io`
- Verify full 7-zone layout renders on an 80×24 terminal
- Verify F1–F4 focus switching works
- Verify `+`/`-` BPM changes from any panel
- Verify CLI `port`, `channel`, `seed` commands
- Verify `cargo clippy` clean
- **Agent**: qa

---

## 11. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| HID thread sends removed `InputCommand` variants | Medium | Compile error | Step 2.2 task explicitly handles hid.rs; Option B fallback available |
| 16 step cards too narrow on small terminals | High | Visual clipping | Accepted for MVP; note names truncate gracefully |
| `recv_timeout` polling loop burns CPU if midi_tx is silent | Low | High CPU idle | 50ms timeout is the same as the UI render budget; acceptable |
| `MidiOutputConnection` drop on port swap causes note stuck | Low | Audio artifact | ALSA sends all buffered data on drop; midir handles this correctly |
| TestBackend tests for 7-zone layout are fragile | Medium | Flaky CI | Test for widget presence/style, not exact character positions |
| Removing `choose_midi_port`/`choose_midi_channel` breaks existing `#[cfg(test)]` tests | Low | Compile error | Check `midi_out.rs` tests before deletion; gate or adapt them |

---

## 12. Parallel Execution Summary

```
Step 1 (foundation) → must complete before Step 2
Step 2.1 + 2.2      → can run in parallel (separate worktrees)
Step 3.1 + 3.2      → can run in parallel with each other; depend on Step 2
Step 4.1            → depends on 2.1, 2.2, 3.1
Step 5.1            → depends on 4.1 and 3.2
Step 6.1            → depends on 5.1
```

Maximum parallelism: 2 coder agents at Steps 2 and 3. Total sequential depth:
6 phases.
