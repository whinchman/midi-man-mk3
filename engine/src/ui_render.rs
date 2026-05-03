//! Ratatui render logic for the sequencer UI — 7-zone cyberpunk layout.
//!
//! This module is always compiled (no `hw-io` feature gate) so that
//! `UiLocalSnapshot`, `LogEntry`, and `LogTag` are available to `ui.rs`
//! without feature gating.  `TestBackend` tests live at the bottom.

use std::collections::VecDeque;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::input::FocusPanel;
use crate::music_theory::note_name;
use crate::music_theory::{Key, Mode};
use crate::state::{SequencerState, StepSize, TempoRandType, TempoRollPoint};

// ── Color palette ─────────────────────────────────────────────────────────────

const BG: Color = Color::Rgb(10, 10, 10);
const CYAN: Color = Color::Rgb(0, 255, 255);
const MAGENTA: Color = Color::Rgb(255, 0, 127);
const FUCHSIA: Color = Color::Rgb(255, 0, 255);
const DIM_CYAN: Color = Color::Rgb(0, 64, 64);
const GREEN: Color = Color::Rgb(0, 200, 80);
const GRAY: Color = Color::Rgb(136, 136, 136);

// ── Public structs (no hw-io gate) ────────────────────────────────────────────

/// Snapshot of UI-local state passed to `render_frame` each frame.
///
/// Defined here (no `hw-io` gate) so `ui.rs` can import without feature gating.
pub struct UiLocalSnapshot<'a> {
    /// Which panel has keyboard focus.
    pub focus: FocusPanel,
    /// Currently selected step index (0–15) in F1.
    pub selected_step: usize,
    /// Currently selected param index (0–7) in F2.
    pub seq_param_idx: u8,
    /// Currently selected param index (0–7) in F3.
    pub rand_param_idx: u8,
    /// Current contents of the F4 CLI input line.
    pub cli_line: &'a str,
    /// Ring buffer of log entries for the F4 CLI log area.
    pub cli_log: &'a VecDeque<LogEntry>,
    /// Name of the active MIDI output device (empty = none selected).
    pub midi_device_name: &'a str,
    /// MIDI channel display value (1-indexed).
    pub midi_channel_display: u8,
}

/// A single log entry in the F4 CLI panel.
pub struct LogEntry {
    /// Milliseconds since startup.
    pub timestamp_ms: u64,
    /// Severity / source tag.
    pub tag: LogTag,
    /// Human-readable message text.
    pub text: String,
}

/// Tag controlling how a log entry is coloured in the CLI panel.
pub enum LogTag {
    /// Informational message.
    Info,
    /// MIDI event or device message.
    Midi,
    /// Error message.
    Err,
    /// User-submitted CLI command echo.
    Cmd,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Render one complete frame of the sequencer TUI into `frame`.
///
/// `state` is a cloned snapshot — no lock is held during rendering.
/// `ui` carries UI-local state that is not part of `SequencerState`.
pub fn render_frame(frame: &mut Frame, state: &SequencerState, ui: &UiLocalSnapshot<'_>) {
    let area = frame.area();

    // 7-zone vertical layout.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // [0] title bar
            Constraint::Length(1), // [1] transport bar
            Constraint::Min(5),    // [2] F1 · SEQ panel
            Constraint::Length(3), // [3] F2 · SEQ PARAMS panel
            Constraint::Length(3), // [4] F3 · RANDOM PARAMS panel
            Constraint::Min(5),    // [5] F4 · CLI panel
            Constraint::Length(1), // [6] bottom keybind bar
        ])
        .split(area);

    render_title_bar(frame, state, ui, chunks[0]);
    render_transport_bar(frame, state, chunks[1]);
    render_seq_panel(frame, state, ui, chunks[2]);
    render_seq_params_panel(frame, state, ui, chunks[3]);
    render_rand_params_panel(frame, state, ui, chunks[4]);
    render_cli_panel(frame, ui, chunks[5]);
    render_keybind_bar(frame, chunks[6]);
}

// ── Zone render functions (private) ───────────────────────────────────────────

/// Render the title bar.
///
/// Left: `"▶ 217 Industries / midi-man-mk3"` with the project name in FUCHSIA.
/// Right: `"MIDI OUT <device> CH:<n>"`.
fn render_title_bar(
    frame: &mut Frame,
    _state: &SequencerState,
    ui: &UiLocalSnapshot<'_>,
    area: Rect,
) {
    let left_prefix = Span::styled("▶ 217 Industries / ", Style::default().fg(GRAY));
    let left_name = Span::styled(
        "midi-man-mk3",
        Style::default().fg(FUCHSIA).add_modifier(Modifier::BOLD),
    );
    let device = if ui.midi_device_name.is_empty() {
        "—"
    } else {
        ui.midi_device_name
    };
    let right_text = format!("  MIDI OUT {} CH:{}", device, ui.midi_channel_display);
    let right = Span::styled(right_text, Style::default().fg(GRAY));

    let line = Line::from(vec![left_prefix, left_name, right]);
    let para = Paragraph::new(line).style(Style::default().bg(BG));
    frame.render_widget(para, area);
}

/// Render the transport bar.
///
/// Format: `" BPM <n>  KEY <k>  MODE <m>  STEP <s>  STATUS ► <state>"`
fn render_transport_bar(frame: &mut Frame, state: &SequencerState, area: Rect) {
    let stat = status_label(state.playing, state.paused);
    let stat_color = if state.playing && !state.paused {
        GREEN
    } else if state.paused {
        CYAN
    } else {
        Color::Reset
    };

    let prefix = Span::styled(
        format!(
            " BPM {}  KEY {}  MODE {}  STEP {}  STATUS ► ",
            state.tempo_bpm,
            key_name(state.key),
            mode_name(state.mode),
            step_size_label(state.step_size),
        ),
        Style::default().fg(GRAY),
    );
    let status_span = Span::styled(
        stat,
        Style::default().fg(stat_color).add_modifier(Modifier::BOLD),
    );

    let line = Line::from(vec![prefix, status_span]);
    let para = Paragraph::new(line).style(Style::default().bg(BG));
    frame.render_widget(para, area);
}

/// Render the F1 sequencer step panel with 16 equal-width step cards.
fn render_seq_panel(
    frame: &mut Frame,
    state: &SequencerState,
    ui: &UiLocalSnapshot<'_>,
    area: Rect,
) {
    let focused = ui.focus == FocusPanel::Sequencer;
    let border_color = if focused { CYAN } else { DIM_CYAN };

    let block = Block::default()
        .title("F1 · SEQ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Split inner area into 16 equal columns.
    let col_constraints = [Constraint::Ratio(1, 16); 16];
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints)
        .split(inner);

    for i in 0usize..16 {
        let step = &state.steps[i];
        let is_playhead = state.playhead as usize == i;
        let is_selected = ui.selected_step == i;

        // Determine step color.
        let step_color = if is_playhead {
            MAGENTA
        } else if step.enabled {
            CYAN
        } else {
            DIM_CYAN
        };

        let border_style = if is_selected {
            Style::default().fg(MAGENTA)
        } else {
            Style::default().fg(step_color)
        };

        let card_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(Style::default().bg(BG));

        let card_inner = card_block.inner(cols[i]);
        frame.render_widget(card_block, cols[i]);

        if card_inner.width == 0 || card_inner.height == 0 {
            continue;
        }

        // Row 0: step number (1-indexed), top-left, GRAY.
        let num_line = Line::from(Span::styled(
            format!("{:02}", i + 1),
            Style::default().fg(GRAY),
        ));

        // Row 1: note name, centered.
        let note_str = note_name(step.midi_note);
        let note_line =
            Line::from(Span::styled(note_str, Style::default().fg(step_color))).centered();

        // Row 2: enabled indicator.
        let dot = if step.enabled { "●" } else { "○" };
        let dot_line = Line::from(Span::styled(dot, Style::default().fg(step_color))).centered();

        let lines = vec![num_line, note_line, dot_line];
        let para = Paragraph::new(lines);
        frame.render_widget(para, card_inner);
    }
}

/// Render the F2 SEQ PARAMS panel.
fn render_seq_params_panel(
    frame: &mut Frame,
    state: &SequencerState,
    ui: &UiLocalSnapshot<'_>,
    area: Rect,
) {
    let focused = ui.focus == FocusPanel::SeqParams;
    let border_color = if focused { CYAN } else { DIM_CYAN };

    let block = Block::default()
        .title("F2 · SEQ PARAMS")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Param labels and their current values.
    let param_labels = [
        "KEY", "MODE", "SWING", "STEP", "L.IN", "L.OUT", "PAUSE", "PLAY",
    ];
    let mut spans: Vec<Span> = Vec::with_capacity(param_labels.len() * 3);

    for (i, label) in param_labels.iter().enumerate() {
        let idx = i as u8;
        let value = param_value_string(state, idx);
        let text = format!(" {}:{} ", label, value);
        let is_selected = focused && idx == ui.seq_param_idx;
        let style = if is_selected {
            Style::default().fg(MAGENTA).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GRAY)
        };
        spans.push(Span::styled(text, style));
        if i < param_labels.len() - 1 {
            spans.push(Span::styled("|", Style::default().fg(DIM_CYAN)));
        }
    }

    let line = Line::from(spans);
    let para = Paragraph::new(line);
    frame.render_widget(para, inner);
}

/// Render the F3 RANDOM PARAMS panel.
fn render_rand_params_panel(
    frame: &mut Frame,
    state: &SequencerState,
    ui: &UiLocalSnapshot<'_>,
    area: Rect,
) {
    let focused = ui.focus == FocusPanel::RandParams;
    let border_color = if focused { CYAN } else { DIM_CYAN };

    let block = Block::default()
        .title("F3 · RANDOM PARAMS")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let param_labels = [
        "N.RND", "T.RND", "ROLL", "V.MAX", "T.TYPE", "S.RND", "S.QUANT", "SEED",
    ];
    let mut spans: Vec<Span> = Vec::with_capacity(param_labels.len() * 3);

    for (i, label) in param_labels.iter().enumerate() {
        let idx = i as u8;
        // Override SEED (index 7) display with hex format.
        let value = if idx == 7 {
            format!("0x{:04X}", state.rand_seed)
        } else {
            shift_param_value_string(state, idx)
        };
        let text = format!(" {}:{} ", label, value);
        let is_selected = focused && idx == ui.rand_param_idx;
        let style = if is_selected {
            Style::default().fg(MAGENTA).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GRAY)
        };
        spans.push(Span::styled(text, style));
        if i < param_labels.len() - 1 {
            spans.push(Span::styled("|", Style::default().fg(DIM_CYAN)));
        }
    }

    let line = Line::from(spans);
    let para = Paragraph::new(line);
    frame.render_widget(para, inner);
}

/// Render the F4 CLI panel with scrolling log and input line.
fn render_cli_panel(frame: &mut Frame, ui: &UiLocalSnapshot<'_>, area: Rect) {
    let focused = ui.focus == FocusPanel::Cli;
    let border_color = if focused { CYAN } else { DIM_CYAN };

    let block = Block::default()
        .title("F4 · CLI")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    // Reserve the last row for the input line.
    let log_height = inner.height.saturating_sub(1) as usize;
    let log_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    let input_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };

    // Build log lines — take the last `log_height` entries.
    let log_len = ui.cli_log.len();
    let skip = log_len.saturating_sub(log_height);

    let mut log_lines: Vec<Line> = Vec::with_capacity(log_height);
    for entry in ui.cli_log.iter().skip(skip) {
        let ts = Span::styled(
            format!("[{:>8}ms] ", entry.timestamp_ms),
            Style::default().fg(GRAY),
        );
        let tag_span = match entry.tag {
            LogTag::Cmd => Span::styled("[CMD] ", Style::default().fg(CYAN)),
            LogTag::Info => Span::styled("[INFO]", Style::default().fg(Color::White)),
            LogTag::Err => Span::styled("[ERR] ", Style::default().fg(Color::Red)),
            LogTag::Midi => Span::styled("[MIDI]", Style::default().fg(CYAN)),
        };
        let text = Span::raw(format!(" {}", entry.text));
        log_lines.push(Line::from(vec![ts, tag_span, text]));
    }

    if log_area.height > 0 {
        let log_para = Paragraph::new(log_lines);
        frame.render_widget(log_para, log_area);
    }

    // Input line.
    let input_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(CYAN)),
        Span::raw(ui.cli_line),
        Span::styled("_", Style::default().fg(CYAN)),
    ]);
    let input_para = Paragraph::new(input_line);
    frame.render_widget(input_para, input_area);
}

/// Render the bottom keybind hint bar.
fn render_keybind_bar(frame: &mut Frame, area: Rect) {
    let hints = "F1-F4 focus | P play | +/- BPM | \u{2190}/\u{2192} param | \u{2191}/\u{2193} adjust | space toggle | enter confirm | esc cancel | ^C quit";
    let para = Paragraph::new(hints).style(Style::default().fg(DIM_CYAN).bg(BG));
    frame.render_widget(para, area);
}

// ── Helper functions (retained from previous ui_render.rs) ────────────────────

/// Return a human-readable string for a `Key`.
fn key_name(key: Key) -> &'static str {
    match key {
        Key::C => "C",
        Key::Cs => "C#",
        Key::D => "D",
        Key::Ds => "D#",
        Key::E => "E",
        Key::F => "F",
        Key::Fs => "F#",
        Key::G => "G",
        Key::Gs => "G#",
        Key::A => "A",
        Key::As => "A#",
        Key::B => "B",
    }
}

/// Return a human-readable string for a `Mode`.
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Major => "Major",
        Mode::NaturalMinor => "NatMin",
        Mode::Dorian => "Dorian",
        Mode::Phrygian => "Phryg",
        Mode::Lydian => "Lydian",
        Mode::Mixolydian => "Mixo",
        Mode::Locrian => "Locrian",
        Mode::HarmonicMinor => "HarMin",
        Mode::MelodicMinor => "MelMin",
    }
}

/// Return a human-readable string for a `StepSize`.
fn step_size_label(sz: StepSize) -> &'static str {
    match sz {
        StepSize::Whole => "1/1",
        StepSize::Half => "1/2",
        StepSize::Quarter => "1/4",
        StepSize::Eighth => "1/8",
        StepSize::Sixteenth => "1/16",
        StepSize::ThirtySecond => "1/32",
    }
}

/// Return the playback status label string.
fn status_label(playing: bool, paused: bool) -> &'static str {
    if !playing {
        "STOPPED"
    } else if paused {
        "PAUSED"
    } else {
        "PLAYING"
    }
}

/// Return a human-readable string for a `TempoRollPoint`.
fn tempo_roll_point_name(trp: TempoRollPoint) -> &'static str {
    match trp {
        TempoRollPoint::Off => "Off",
        TempoRollPoint::Step => "Step",
        TempoRollPoint::Beat => "Beat",
        TempoRollPoint::Seq => "Seq",
    }
}

/// Return a human-readable string for a `TempoRandType`.
fn tempo_rand_type_name(trt: TempoRandType) -> &'static str {
    match trt {
        TempoRandType::Random => "Random",
        TempoRandType::Up => "Up",
        TempoRandType::Down => "Down",
        TempoRandType::Breathe => "Breathe",
        TempoRandType::PingPong => "PingPong",
    }
}

/// Return the display string for shift (F3 random) param `index` given current state.
///
/// Index map: 0=note_rand, 1=tempo_rand, 2=tempo_roll_point, 3=tempo_variance_max,
/// 4=tempo_rand_type, 5=step_rand, 6=scale_quant, 7=rand_seed (formatted by caller).
pub fn shift_param_value_string(state: &SequencerState, index: u8) -> String {
    match index {
        0 => state.note_rand.to_string(),
        1 => state.tempo_rand.to_string(),
        2 => tempo_roll_point_name(state.tempo_roll_point).to_string(),
        3 => state.tempo_variance_max.to_string(),
        4 => tempo_rand_type_name(state.tempo_rand_type).to_string(),
        5 => state.step_rand.to_string(),
        6 => {
            if state.scale_quant {
                "On".to_string()
            } else {
                "Off".to_string()
            }
        }
        _ => "\u{2014}".to_string(), // em dash
    }
}

/// Return the display string for a pending shift param edit.
///
/// `v` is the raw `i64` from `PendingEdit::Param { value, .. }`.
pub fn shift_pending_param_value_string(index: u8, v: i64) -> String {
    match index {
        0 | 1 | 3 | 5 => format!("{}", v),
        2 => tempo_roll_point_name(TempoRollPoint::from_index(v as usize)).to_string(),
        4 => tempo_rand_type_name(TempoRandType::from_index(v as usize)).to_string(),
        6 => {
            if v != 0 {
                "On".to_string()
            } else {
                "Off".to_string()
            }
        }
        _ => "\u{2014}".to_string(),
    }
}

/// Return a short string for seq (F2) param `index` current value.
///
/// Index map: 0=Key, 1=Mode, 2=Swing, 3=StepSize, 4=loop_in, 5=loop_out,
/// 6=paused, 7=playing.
fn param_value_string(state: &SequencerState, index: u8) -> String {
    match index {
        0 => key_name(state.key).to_string(),
        1 => mode_name(state.mode).to_string(),
        2 => format!("{:+}", state.swing),
        3 => step_size_label(state.step_size).to_string(),
        4 => state.loop_in.to_string(),
        5 => state.loop_out.to_string(),
        6 => if state.paused { "on" } else { "off" }.to_string(),
        7 => if state.playing { "playing" } else { "stopped" }.to_string(),
        _ => "?".to_string(),
    }
}

/// Return a human-readable string for a pending seq param value `v` at `index`.
pub fn pending_param_value_string(index: u8, v: i64) -> String {
    match index {
        0 => key_name(Key::from_index(v as usize)).to_string(),
        1 => mode_name(Mode::from_index(v as usize)).to_string(),
        2 => format!("{:+}", v as i8),
        3 => step_size_label(StepSize::from_index(v as usize)).to_string(),
        4 | 5 => format!("{}", v),
        6 => {
            if v != 0 {
                "on".to_string()
            } else {
                "off".to_string()
            }
        }
        7 => {
            if v != 0 {
                "playing".to_string()
            } else {
                "stopped".to_string()
            }
        }
        _ => "?".to_string(),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn make_snapshot<'a>(
        cli_log: &'a VecDeque<LogEntry>,
        cli_line: &'a str,
    ) -> UiLocalSnapshot<'a> {
        UiLocalSnapshot {
            focus: FocusPanel::Sequencer,
            selected_step: 0,
            seq_param_idx: 0,
            rand_param_idx: 0,
            cli_line,
            cli_log,
            midi_device_name: "TestDevice",
            midi_channel_display: 1,
        }
    }

    #[test]
    fn render_frame_does_not_panic_on_empty_state() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let state = SequencerState::default();
        let log = VecDeque::new();
        let ui = make_snapshot(&log, "");
        terminal
            .draw(|frame| render_frame(frame, &state, &ui))
            .expect("draw must not panic");
    }

    #[test]
    fn title_bar_contains_project_name_fuchsia() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let state = SequencerState::default();
        let log = VecDeque::new();
        let ui = make_snapshot(&log, "");
        terminal
            .draw(|frame| render_frame(frame, &state, &ui))
            .expect("draw");

        // Row 0 is the title bar. Check it contains "midi-man-mk3".
        let buf = terminal.backend().buffer().clone();
        let row0: String = (0..buf.area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
                    .unwrap_or(' ')
            })
            .collect();
        assert!(
            row0.contains("midi-man-mk3"),
            "title row should contain 'midi-man-mk3', got: {:?}",
            row0
        );
        // Check that at least one cell in row 0 has FUCHSIA fg.
        let fuchsia_found =
            (0..buf.area.width).any(|x| buf.cell((x, 0)).map(|c| c.fg == FUCHSIA).unwrap_or(false));
        assert!(
            fuchsia_found,
            "title bar should have at least one FUCHSIA cell"
        );
    }

    #[test]
    fn step_cards_16_columns_rendered() {
        let backend = TestBackend::new(160, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut state = SequencerState::default();
        state.steps[0].enabled = true;
        state.steps[3].enabled = true;
        state.playhead = 2;
        let log = VecDeque::new();
        let ui = make_snapshot(&log, "");
        terminal
            .draw(|frame| render_frame(frame, &state, &ui))
            .expect("draw");

        // F1 panel starts at row 2 (title=0, transport=1, seq_panel=2...).
        let buf = terminal.backend().buffer().clone();
        let row2: String = (0..buf.area.width)
            .map(|x| {
                buf.cell((x, 2))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
                    .unwrap_or(' ')
            })
            .collect();
        assert!(!row2.trim().is_empty(), "F1 panel row should not be empty");
    }

    #[test]
    fn cli_panel_shows_log_entries() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let state = SequencerState::default();
        let mut log: VecDeque<LogEntry> = VecDeque::new();
        log.push_back(LogEntry {
            timestamp_ms: 1234,
            tag: LogTag::Info,
            text: "hello world".to_string(),
        });
        log.push_back(LogEntry {
            timestamp_ms: 5678,
            tag: LogTag::Cmd,
            text: "port test".to_string(),
        });
        let snapshot = UiLocalSnapshot {
            focus: FocusPanel::Cli,
            selected_step: 0,
            seq_param_idx: 0,
            rand_param_idx: 0,
            cli_line: "my input",
            cli_log: &log,
            midi_device_name: "",
            midi_channel_display: 1,
        };
        terminal
            .draw(|frame| render_frame(frame, &state, &snapshot))
            .expect("draw");

        // Scan all cells for the log text.
        let buf = terminal.backend().buffer().clone();
        let w = buf.area.width;
        let h = buf.area.height;
        let all_text: String = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                buf.cell((x, y))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
                    .unwrap_or(' ')
            })
            .collect();
        assert!(
            all_text.contains("hello world"),
            "CLI panel should display log entry text"
        );
        assert!(
            all_text.contains("my input"),
            "CLI panel should show the input line"
        );
    }

    #[test]
    fn transport_bar_shows_bpm_and_key() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut state = SequencerState::default();
        state.tempo_bpm = 140;
        state.playing = true;
        let log = VecDeque::new();
        let ui = make_snapshot(&log, "");
        terminal
            .draw(|frame| render_frame(frame, &state, &ui))
            .expect("draw");

        let buf = terminal.backend().buffer().clone();
        let row1: String = (0..buf.area.width)
            .map(|x| {
                buf.cell((x, 1))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
                    .unwrap_or(' ')
            })
            .collect();
        assert!(
            row1.contains("140"),
            "transport bar should contain BPM value 140, got: {:?}",
            row1
        );
        assert!(
            row1.contains('C'),
            "transport bar should contain key 'C', got: {:?}",
            row1
        );
    }
}
