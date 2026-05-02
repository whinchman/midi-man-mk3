//! Keyboard UI event loop.
//!
//! Uses crossterm for raw terminal key events and ratatui for rendering.
//! This module is gated behind the `hw-io` feature because crossterm and
//! ratatui are optional dependencies.
//!
//! # Design: overlay state split
//!
//! `Option<OverlayMode>` and `selected_param: u8` live **in this UI thread
//! only** — they are presentation state, not shared state.  The `PendingEdit`
//! lives in `SequencerState` (shared) so the HID thread can read it.
//!
//! When the user presses F1/F2 the UI thread:
//! 1. Sends `InputCommand::OpenOverlay(mode)` on the shared channel (so
//!    `SequencerState::apply_command` records `active_overlay`).
//! 2. Flips its own `overlay: Option<OverlayMode>` field to switch the key
//!    mapping for subsequent events.
//!
//! This avoids a secondary back-channel from state → UI; the UI thread is the
//! sole authority on the current overlay for key-mapping purposes.

use std::sync::mpsc::SyncSender;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;

use crate::input::{InputCommand, KeyCodeSimple, OverlayMode};
use crate::input::{overlay_key_to_command, root_key_to_command};

/// Regular overlay parameter names (index 0–6).
const REGULAR_PARAMS: [&str; 7] = [
    "Key",
    "Mode",
    "Swing",
    "Step Size",
    "Loop",
    "Pause",
    "Stop/Start",
];

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

/// Render the current state into the terminal.
///
/// This is a stub render that shows the overlay panel when active.
fn render<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    ui: &UiState,
) -> std::io::Result<()> {
    terminal.draw(|frame| {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(5)])
            .split(area);

        // Top area — placeholder sequencer view.
        let main_block = Block::default().title("MIDI-Man MK3").borders(Borders::ALL);
        frame.render_widget(main_block, chunks[0]);

        // Bottom area — overlay panel.
        match ui.overlay {
            None => {
                let help = Paragraph::new("F1: Regular overlay  F2: Shift overlay  Space: Toggle  Enter: Confirm")
                    .block(Block::default().title("Controls").borders(Borders::ALL));
                frame.render_widget(help, chunks[1]);
            }
            Some(OverlayMode::Regular) => {
                let items: Vec<ListItem> = REGULAR_PARAMS
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        if i as u8 == ui.selected_param {
                            ListItem::new(Span::styled(
                                format!("> {name}"),
                                Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED),
                            ))
                        } else {
                            ListItem::new(Span::raw(format!("  {name}")))
                        }
                    })
                    .collect();
                let list = List::new(items)
                    .block(Block::default().title("Regular Overlay (Esc to close)").borders(Borders::ALL));
                frame.render_widget(list, chunks[1]);
            }
            Some(OverlayMode::Shift) => {
                let placeholder = Paragraph::new("(shift mode — coming soon)")
                    .block(Block::default().title("Shift Overlay (Esc to close)").borders(Borders::ALL));
                frame.render_widget(placeholder, chunks[1]);
            }
        }
    })?;
    Ok(())
}

/// Run the keyboard event loop.
///
/// Blocks until the user presses `q` or `Ctrl+C`.
///
/// `tx` is the shared command channel; both this function and the HID thread
/// (Step 7) send `InputCommand` values on it.
///
/// Uses `crossterm::event::poll` with a 50 ms timeout so the render loop
/// fires at ~20 FPS between key events.
pub fn run(tx: SyncSender<InputCommand>) -> std::io::Result<()> {
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
    use crossterm::ExecutableCommand;
    use std::io;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut ui = UiState::new();
    let mut running = true;

    while running {
        render(&mut terminal, &ui)?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key_event) = event::read()? {
                // Quit on 'q' or Ctrl+C.
                if key_event.code == KeyCode::Char('q')
                    || (key_event.code == KeyCode::Char('c')
                        && key_event.modifiers.contains(KeyModifiers::CONTROL))
                {
                    running = false;
                    continue;
                }

                if let Some(cmd) = translate_key(key_event, &ui) {
                    update_local_overlay(&mut ui, &cmd);
                    // Best-effort send; if the receiver is gone we exit cleanly.
                    if tx.send(cmd).is_err() {
                        running = false;
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout.execute(LeaveAlternateScreen)?;
    Ok(())
}
