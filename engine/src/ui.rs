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

use std::io;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::input::{InputCommand, KeyCodeSimple, OverlayMode};
use crate::input::{overlay_key_to_command, root_key_to_command};
use crate::state::SequencerState;
use crate::ui_render::render_frame;

/// RAII guard that restores the terminal on drop.
///
/// Using Drop ensures the terminal is cleaned up even if a panic unwinds the
/// stack, preventing a broken terminal state for the user.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup — ignore errors because we may already be panicking.
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
    }
}

/// Local UI state — lives entirely in the UI thread.
struct UiState {
    /// Active overlay, if any.  Tracked locally; not read from shared state.
    overlay: Option<OverlayMode>,
    /// Selected param index (0–6) — tracked locally for rendering.
    selected_param: u8,
}

impl UiState {
    fn new() -> Self {
        Self { overlay: None, selected_param: 0 }
    }
}

/// Convert a crossterm `KeyCode` into our portable `KeyCodeSimple`.
fn to_simple(code: KeyCode) -> KeyCodeSimple {
    match code {
        KeyCode::Left => KeyCodeSimple::Left,
        KeyCode::Right => KeyCodeSimple::Right,
        KeyCode::Up => KeyCodeSimple::Up,
        KeyCode::Down => KeyCodeSimple::Down,
        KeyCode::Char(' ') => KeyCodeSimple::Space,
        KeyCode::Enter => KeyCodeSimple::Enter,
        KeyCode::Esc => KeyCodeSimple::Esc,
        KeyCode::F(1) => KeyCodeSimple::F1,
        KeyCode::F(2) => KeyCodeSimple::F2,
        _ => KeyCodeSimple::Other,
    }
}

/// Translate a crossterm `KeyEvent` into an `InputCommand`, given the current
/// UI state (overlay open or not).  Returns `None` for unmapped keys.
fn translate_key(event: KeyEvent, ui: &UiState) -> Option<InputCommand> {
    let simple = to_simple(event.code);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);

    match ui.overlay {
        None => root_key_to_command(simple, shift),
        Some(_) => overlay_key_to_command(simple),
    }
}

/// Apply overlay side-effects in the UI thread when a command is about to be sent.
///
/// The UI thread needs to track the overlay locally so subsequent key events
/// are translated in the right mode.
fn update_local_overlay(ui: &mut UiState, cmd: &InputCommand) {
    match cmd {
        InputCommand::OpenOverlay(mode) => {
            ui.overlay = Some(*mode);
        }
        InputCommand::CloseOverlay => {
            ui.overlay = None;
        }
        InputCommand::ParamSelectDelta(d) => {
            let current = ui.selected_param as i32;
            let next = ((current + *d as i32).rem_euclid(7)) as u8;
            ui.selected_param = next;
        }
        InputCommand::ParamSelect(n) => {
            ui.selected_param = *n;
        }
        _ => {}
    }
}

/// Run the terminal UI event loop.
///
/// Blocks until the user presses Ctrl-C or the `notify` channel is closed.
///
/// # Parameters
///
/// - `state`   — shared sequencer state; read lock is acquired briefly per frame.
/// - `notify`  — wakeup channel; the clock and HID threads send `()` after each
///               state mutation.  A 50 ms timeout fires if no wakeup arrives.
/// - `cmd_tx`  — command channel to the state processor.
///
/// # Termination
///
/// On exit the terminal is restored via the `TerminalGuard` Drop impl.
/// The caller is responsible for stopping the sequencer (sending
/// `MidiEvent::Stop`) and joining all other threads after this returns.
pub fn run_ui(
    state: Arc<RwLock<SequencerState>>,
    notify: Receiver<()>,
    cmd_tx: SyncSender<InputCommand>,
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
        let snapshot = {
            state.read().expect("run_ui: state RwLock poisoned").clone()
        };
        let overlay = ui.overlay;
        let selected_param = ui.selected_param;
        if let Err(e) = terminal.draw(|frame| {
            render_frame(frame, &snapshot, overlay, selected_param);
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

                if let Some(cmd) = translate_key(key_event, &ui) {
                    update_local_overlay(&mut ui, &cmd);
                    // Best-effort send; if the receiver is gone we exit.
                    if cmd_tx.send(cmd).is_err() {
                        break;
                    }
                }
            }
        }

        // ── Notify drain ─────────────────────────────────────────────────────
        // Drain any pending wakeups so we don't fall behind if the clock fires
        // faster than we render.  `try_recv` returns Err when the channel is
        // empty; `Disconnected` means all senders have dropped — exit.
        loop {
            match notify.try_recv() {
                Ok(_) => continue,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // All notify senders gone — exit the outer loop on next iteration.
                    // We set a flag here and break the inner loop.
                    return; // exit run_ui immediately.
                }
            }
        }
    }
    // TerminalGuard Drop restores the terminal.
}
