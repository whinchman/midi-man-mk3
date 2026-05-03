//! Terminal UI event loop — ratatui render loop + crossterm keyboard handling.
//!
//! This module requires the `hw-io` feature because it uses the crossterm
//! backend which needs a real terminal.  Render logic lives in `ui_render`
//! (always compiled) so it can be unit-tested with `TestBackend`.
//!
//! # Threading model
//!
//! `run_ui` blocks the calling thread.  It is designed to be the main thread's
//! blocking point (Step 9 joins on it).  When the function returns, the process
//! should exit.
//!
//! # Exit behaviour
//!
//! On Ctrl-C (or when the notify channel closes), `run_ui` exits cleanly.
//! It does *not* send a `MidiEvent::Stop` — the caller (Step 9 / main.rs) is
//! responsible for stopping the sequencer and joining the clock/MIDI threads
//! after `run_ui` returns.  This keeps the UI thread free of back-channels into
//! the clock domain.
//!
//! # Render timing
//!
//! The render loop wakes on either:
//! - A message on the `notify` channel (sent by the clock or HID thread after
//!   each state mutation), or
//! - A forced 50 ms timeout (~20 FPS) for playhead animation.
//!
//! The read lock is acquired, state is cloned, and the lock is released *before*
//! rendering starts.  The render never holds the lock.

use std::collections::VecDeque;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use crate::input::{FocusPanel, InputCommand};
use crate::midi_out::MidiCtrlMsg;
use crate::ui_render::{LogEntry, LogTag};

// ── UiState ───────────────────────────────────────────────────────────────────

/// Local UI state — lives entirely in the UI thread, never shared.
// Fields are used by run_ui (hw-io) and by tests; silence dead-code warnings in
// non-hw-io lib builds where run_ui is not compiled.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) struct UiState {
    /// Which panel currently holds keyboard focus.
    pub focus: FocusPanel,
    /// Currently selected step index (0–15) for F1 panel navigation.
    pub selected_step: usize,
    /// Currently selected param index (0–7) for the F2 · SEQ PARAMS panel.
    pub seq_param_idx: u8,
    /// Currently selected param index (0–7) for the F3 · RANDOM PARAMS panel.
    pub rand_param_idx: u8,
    /// Current contents of the F4 CLI input line (max 256 chars).
    pub cli_line: String,
    /// Ring buffer of CLI log entries (max `CLI_LOG_CAPACITY`).
    pub cli_log: VecDeque<LogEntry>,
    /// Name of the connected MIDI output port (echoed from `port` CLI command).
    pub midi_device_name: String,
    /// MIDI channel display value (1-indexed, echoed from `channel` CLI command).
    pub midi_channel_display: u8,
    /// Startup instant used for log entry timestamps.
    pub start_time: Instant,
}

#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) const CLI_LOG_CAPACITY: usize = 200;

impl UiState {
    /// Create a new `UiState` with default values.
    #[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
    pub(crate) fn new() -> Self {
        Self {
            focus: FocusPanel::Sequencer,
            selected_step: 0,
            seq_param_idx: 0,
            rand_param_idx: 0,
            cli_line: String::new(),
            cli_log: VecDeque::with_capacity(CLI_LOG_CAPACITY),
            midi_device_name: String::new(),
            midi_channel_display: 1,
            start_time: Instant::now(),
        }
    }
}

// ── CLI helpers ────────────────────────────────────────────────────────────────

/// Push a log entry to `log`, dropping the oldest entry if at capacity.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) fn push_log(log: &mut VecDeque<LogEntry>, timestamp_ms: u64, tag: LogTag, text: String) {
    if log.len() >= CLI_LOG_CAPACITY {
        log.pop_front();
    }
    log.push_back(LogEntry {
        timestamp_ms,
        tag,
        text,
    });
}

/// Process the current `cli_line`, dispatch commands, append log entries, clear input.
///
/// Handles:
/// - `port <name>`   → `MidiCtrlMsg::ChangePort` + `InputCommand::MidiDeviceName`
/// - `channel <n>`   → `MidiCtrlMsg::ChangeChannel` + `InputCommand::ChannelSet`
/// - `seed <hex>`    → `InputCommand::SeedSet`
/// - unknown         → error log entry
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) fn handle_cli_submit(
    ui: &mut UiState,
    cmd_tx: &SyncSender<InputCommand>,
    midi_ctrl_tx: &SyncSender<MidiCtrlMsg>,
) {
    let line = ui.cli_line.trim().to_string();
    ui.cli_line.clear();
    let ts = ui.start_time.elapsed().as_millis() as u64;

    if line.is_empty() {
        return;
    }

    if let Some(name) = line.strip_prefix("port ") {
        let name = name.trim().to_string();
        let _ = midi_ctrl_tx.send(MidiCtrlMsg::ChangePort(name.clone()));
        let _ = cmd_tx.send(InputCommand::MidiDeviceName(name.clone()));
        ui.midi_device_name = name.clone();
        push_log(&mut ui.cli_log, ts, LogTag::Midi, format!("port → {name}"));
    } else if let Some(rest) = line.strip_prefix("channel ") {
        if let Ok(n) = rest.trim().parse::<u8>() {
            if (1..=16).contains(&n) {
                let _ = midi_ctrl_tx.send(MidiCtrlMsg::ChangeChannel(n));
                let _ = cmd_tx.send(InputCommand::ChannelSet(n));
                ui.midi_channel_display = n;
                push_log(&mut ui.cli_log, ts, LogTag::Midi, format!("channel → {n}"));
            } else {
                push_log(
                    &mut ui.cli_log,
                    ts,
                    LogTag::Err,
                    "channel must be 1–16".into(),
                );
            }
        } else {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Err,
                format!("invalid channel: {rest}"),
            );
        }
    } else if let Some(rest) = line.strip_prefix("seed ") {
        let hex = rest
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            let _ = cmd_tx.send(InputCommand::SeedSet(v));
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Cmd,
                format!("seed → 0x{v:04X}"),
            );
        } else {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Err,
                format!("invalid hex: {rest}"),
            );
        }
    } else {
        push_log(
            &mut ui.cli_log,
            ts,
            LogTag::Err,
            format!("unknown command: {line}"),
        );
    }
}

// ── hw-io–only items (crossterm, terminal, run_ui) ────────────────────────────

#[cfg(feature = "hw-io")]
use std::io;
#[cfg(feature = "hw-io")]
use std::sync::mpsc::Receiver;
#[cfg(feature = "hw-io")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "hw-io")]
use std::time::Duration;

#[cfg(feature = "hw-io")]
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
#[cfg(feature = "hw-io")]
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
#[cfg(feature = "hw-io")]
use crossterm::ExecutableCommand;
#[cfg(feature = "hw-io")]
use ratatui::backend::CrosstermBackend;
#[cfg(feature = "hw-io")]
use ratatui::Terminal;

#[cfg(feature = "hw-io")]
use crate::input::{panel_key_to_command, KeyCodeSimple};
#[cfg(feature = "hw-io")]
use crate::state::SequencerState;
#[cfg(feature = "hw-io")]
use crate::ui_render::{render_frame, UiLocalSnapshot};

/// RAII guard that restores the terminal on drop.
///
/// Using Drop ensures the terminal is cleaned up even if a panic unwinds the
/// stack, preventing a broken terminal state for the user.
#[cfg(feature = "hw-io")]
struct TerminalGuard;

#[cfg(feature = "hw-io")]
impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        Ok(Self)
    }
}

#[cfg(feature = "hw-io")]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup — ignore errors because we may already be panicking.
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
    }
}

/// Convert a crossterm `KeyCode` into our portable `KeyCodeSimple`.
#[cfg(feature = "hw-io")]
fn to_simple(code: KeyCode) -> KeyCodeSimple {
    match code {
        KeyCode::Left => KeyCodeSimple::Left,
        KeyCode::Right => KeyCodeSimple::Right,
        KeyCode::Up => KeyCodeSimple::Up,
        KeyCode::Down => KeyCodeSimple::Down,
        KeyCode::Char(' ') => KeyCodeSimple::Space,
        KeyCode::Char('+') | KeyCode::Char('=') => KeyCodeSimple::Plus,
        KeyCode::Char('-') => KeyCodeSimple::Minus,
        KeyCode::Char(c) => KeyCodeSimple::Char(c),
        KeyCode::Enter => KeyCodeSimple::Enter,
        KeyCode::Esc => KeyCodeSimple::Esc,
        KeyCode::Backspace => KeyCodeSimple::Backspace,
        KeyCode::F(1) => KeyCodeSimple::F1,
        KeyCode::F(2) => KeyCodeSimple::F2,
        KeyCode::F(3) => KeyCodeSimple::F3,
        KeyCode::F(4) => KeyCodeSimple::F4,
        _ => KeyCodeSimple::Other,
    }
}

/// Dispatch a key event to the appropriate handler based on current focus.
///
/// Global keys (F1–F4, +/-, P) are handled first regardless of focus.
/// Focus-specific keys are then dispatched via `panel_key_to_command` or inline CLI logic.
#[cfg(feature = "hw-io")]
fn translate_key(
    event: KeyEvent,
    ui: &mut UiState,
    cmd_tx: &SyncSender<InputCommand>,
    midi_ctrl_tx: &SyncSender<MidiCtrlMsg>,
) {
    let simple = to_simple(event.code);

    // ── Global keys (active in any focus) ─────────────────────────────────────
    match simple {
        KeyCodeSimple::F1 => {
            ui.focus = FocusPanel::Sequencer;
            return;
        }
        KeyCodeSimple::F2 => {
            ui.focus = FocusPanel::SeqParams;
            return;
        }
        KeyCodeSimple::F3 => {
            ui.focus = FocusPanel::RandParams;
            return;
        }
        KeyCodeSimple::F4 => {
            ui.focus = FocusPanel::Cli;
            return;
        }
        KeyCodeSimple::Plus => {
            let _ = cmd_tx.send(InputCommand::BpmDelta(1));
            return;
        }
        KeyCodeSimple::Minus => {
            let _ = cmd_tx.send(InputCommand::BpmDelta(-1));
            return;
        }
        KeyCodeSimple::Char('p') | KeyCodeSimple::Char('P') => {
            let _ = cmd_tx.send(InputCommand::PlayStop);
            return;
        }
        _ => {}
    }

    // ── Focus-specific keys ────────────────────────────────────────────────────
    match ui.focus {
        FocusPanel::Sequencer => {
            if let Some(cmd) = panel_key_to_command(simple, FocusPanel::Sequencer) {
                let _ = cmd_tx.send(cmd);
            }
        }
        FocusPanel::SeqParams => match simple {
            KeyCodeSimple::Left => {
                ui.seq_param_idx = ui.seq_param_idx.saturating_sub(1);
                let _ = cmd_tx.send(InputCommand::PanelParamSelect(ui.seq_param_idx));
            }
            KeyCodeSimple::Right => {
                ui.seq_param_idx = (ui.seq_param_idx + 1).min(7);
                let _ = cmd_tx.send(InputCommand::PanelParamSelect(ui.seq_param_idx));
            }
            KeyCodeSimple::Up => {
                let _ = cmd_tx.send(InputCommand::PanelParamDelta(1));
            }
            KeyCodeSimple::Down => {
                let _ = cmd_tx.send(InputCommand::PanelParamDelta(-1));
            }
            _ => {}
        },
        FocusPanel::RandParams => match simple {
            KeyCodeSimple::Left => {
                ui.rand_param_idx = ui.rand_param_idx.saturating_sub(1);
                let _ = cmd_tx.send(InputCommand::PanelParamSelect(ui.rand_param_idx));
            }
            KeyCodeSimple::Right => {
                ui.rand_param_idx = (ui.rand_param_idx + 1).min(7);
                let _ = cmd_tx.send(InputCommand::PanelParamSelect(ui.rand_param_idx));
            }
            KeyCodeSimple::Up => {
                let _ = cmd_tx.send(InputCommand::PanelParamDelta(1));
            }
            KeyCodeSimple::Down => {
                let _ = cmd_tx.send(InputCommand::PanelParamDelta(-1));
            }
            _ => {}
        },
        FocusPanel::Cli => match simple {
            KeyCodeSimple::Enter => {
                handle_cli_submit(ui, cmd_tx, midi_ctrl_tx);
            }
            KeyCodeSimple::Backspace => {
                ui.cli_line.pop();
            }
            KeyCodeSimple::Char(c) if ui.cli_line.len() < 256 => {
                ui.cli_line.push(c);
            }
            _ => {}
        },
    }
}

/// Run the terminal UI event loop.
///
/// Blocks until the user presses Ctrl-C or the `ui_notify_rx` channel closes.
///
/// # Parameters
///
/// - `state`         — shared sequencer state; read lock is acquired briefly per frame.
/// - `cmd_tx`        — command channel to the state processor.
/// - `ui_notify_rx`  — wakeup channel; the clock and HID threads send `()` after each
///                     state mutation.  A 50 ms timeout fires if no wakeup arrives.
/// - `midi_ctrl_tx`  — control channel to the MIDI output thread (port/channel changes).
///
/// # Termination
///
/// On exit the terminal is restored via the `TerminalGuard` Drop impl.
/// The caller is responsible for stopping the sequencer (sending
/// `MidiEvent::Stop`) and joining all other threads after this returns.
#[cfg(feature = "hw-io")]
pub fn run_ui(
    state: Arc<RwLock<SequencerState>>,
    cmd_tx: SyncSender<InputCommand>,
    ui_notify_rx: Receiver<()>,
    midi_ctrl_tx: SyncSender<MidiCtrlMsg>,
) {
    let _guard = match TerminalGuard::enter() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("run_ui: failed to enter raw mode: {e}");
            return;
        }
    };

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("run_ui: failed to create terminal: {e}");
            return;
        }
    };

    let mut ui = UiState::new();

    loop {
        // ── Render ───────────────────────────────────────────────────────────
        // Acquire read lock, clone state, release lock, then render.
        let state_snapshot = { state.read().expect("run_ui: state RwLock poisoned").clone() };
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
        if let Err(e) = terminal.draw(|frame| {
            render_frame(frame, &state_snapshot, &snapshot);
        }) {
            eprintln!("run_ui: render error: {e}");
            break;
        }

        // ── Input ────────────────────────────────────────────────────────────
        // Poll for a key event with 50 ms timeout.
        let has_event = match event::poll(Duration::from_millis(50)) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("run_ui: poll error: {e}");
                break;
            }
        };

        if has_event {
            let ev = match event::read() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("run_ui: read error: {e}");
                    break;
                }
            };

            if let Event::Key(key_event) = ev {
                // Ctrl-C exits the UI thread cleanly.
                if key_event.code == KeyCode::Char('c')
                    && key_event.modifiers.contains(KeyModifiers::CONTROL)
                {
                    break;
                }

                translate_key(key_event, &mut ui, &cmd_tx, &midi_ctrl_tx);
            }
        }

        // ── Notify drain ─────────────────────────────────────────────────────
        // Drain any pending wakeups so we don't fall behind if the clock fires
        // faster than we render.  `try_recv` returns Err when the channel is
        // empty; `Disconnected` means all senders have dropped — exit.
        loop {
            match ui_notify_rx.try_recv() {
                Ok(_) => continue,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return; // exit run_ui immediately.
                }
            }
        }
    }
    // TerminalGuard Drop restores the terminal.
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn make_channels() -> (
        mpsc::SyncSender<InputCommand>,
        mpsc::Receiver<InputCommand>,
        mpsc::SyncSender<MidiCtrlMsg>,
        mpsc::Receiver<MidiCtrlMsg>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::sync_channel(16);
        let (ctrl_tx, ctrl_rx) = mpsc::sync_channel(16);
        (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx)
    }

    // ── handle_cli_submit tests ───────────────────────────────────────────────

    #[test]
    fn cli_submit_port_sends_midi_ctrl_msg() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        ui.cli_line = "port MyDevice".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx);

        // MidiCtrlMsg::ChangePort should be sent.
        let ctrl_msg = ctrl_rx.try_recv().expect("expected MidiCtrlMsg");
        assert!(matches!(ctrl_msg, MidiCtrlMsg::ChangePort(ref n) if n == "MyDevice"));

        // InputCommand::MidiDeviceName should be sent.
        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::MidiDeviceName(ref n) if n == "MyDevice"));

        // cli_line should be cleared.
        assert!(ui.cli_line.is_empty());

        // A log entry should be appended.
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Midi));
    }

    #[test]
    fn cli_submit_channel_sends_channel_set_cmd() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        ui.cli_line = "channel 5".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx);

        // MidiCtrlMsg::ChangeChannel(5) should be sent.
        let ctrl_msg = ctrl_rx.try_recv().expect("expected MidiCtrlMsg");
        assert!(matches!(ctrl_msg, MidiCtrlMsg::ChangeChannel(5)));

        // InputCommand::ChannelSet(5) should be sent.
        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::ChannelSet(5)));

        // ui state updated.
        assert_eq!(ui.midi_channel_display, 5);
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Midi));
    }

    #[test]
    fn cli_submit_channel_out_of_range_appends_error() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        ui.cli_line = "channel 0".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx);

        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
    }

    #[test]
    fn cli_submit_unknown_appends_error_to_log() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        ui.cli_line = "foo bar baz".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx);

        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
        assert!(ui.cli_log[0].text.contains("foo bar baz"));
    }

    #[test]
    fn cli_submit_seed_hex_sends_seed_set() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        ui.cli_line = "seed 0xDEAD".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx);

        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::SeedSet(0xDEAD)));
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
    }

    #[test]
    fn cli_submit_empty_line_is_noop() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        ui.cli_line = "   ".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx);

        assert!(cmd_rx.try_recv().is_err());
        assert!(ctrl_rx.try_recv().is_err());
        assert!(ui.cli_log.is_empty());
    }

    #[test]
    fn cli_log_capacity_is_respected() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();

        // Submit CLI_LOG_CAPACITY + 5 unknown commands to fill the log.
        for i in 0..(CLI_LOG_CAPACITY + 5) {
            ui.cli_line = format!("unknowncmd{i}");
            handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx);
        }

        assert_eq!(ui.cli_log.len(), CLI_LOG_CAPACITY);
    }

    // ── push_log tests ────────────────────────────────────────────────────────

    #[test]
    fn push_log_evicts_oldest_when_at_capacity() {
        let mut log: VecDeque<LogEntry> = VecDeque::new();
        for i in 0..CLI_LOG_CAPACITY {
            push_log(&mut log, i as u64, LogTag::Info, format!("entry {i}"));
        }
        assert_eq!(log.len(), CLI_LOG_CAPACITY);
        assert_eq!(log[0].text, "entry 0");

        push_log(&mut log, 999, LogTag::Info, "new entry".into());
        assert_eq!(log.len(), CLI_LOG_CAPACITY);
        assert_eq!(log[0].text, "entry 1");
        assert_eq!(log[log.len() - 1].text, "new entry");
    }
}
