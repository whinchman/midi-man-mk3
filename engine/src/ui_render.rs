//! Ratatui render logic for the sequencer UI.
//!
//! This module contains the pure rendering function (`render_frame`) which
//! takes a `SequencerState` snapshot and an `Option<OverlayMode>` and draws
//! into any ratatui `Backend`.  Because it only depends on `ratatui` (not on
//! `crossterm` or any real terminal), it compiles without the `hw-io` feature
//! and can be exercised by unit tests using `TestBackend`.
//!
//! The companion module `ui` (hw-io gated) owns the blocking event loop and
//! calls `render_frame` on each paint cycle.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::input::OverlayMode;
use crate::music_theory::note_name;
use crate::state::{PendingEdit, SequencerState, StepSize};
use crate::music_theory::{Key, Mode};

/// Regular overlay parameter names (index 0–6).
pub const REGULAR_PARAMS: [&str; 7] = [
    "Key",
    "Mode",
    "Swing",
    "Step Size",
    "Loop",
    "Pause",
    "Stop/Start",
];

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
        Mode::NaturalMinor => "Natural Minor",
        Mode::Dorian => "Dorian",
        Mode::Phrygian => "Phrygian",
        Mode::Lydian => "Lydian",
        Mode::Mixolydian => "Mixolydian",
        Mode::Locrian => "Locrian",
        Mode::HarmonicMinor => "Harmonic Minor",
        Mode::MelodicMinor => "Melodic Minor",
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

/// Return the playback status string.
fn status_label(playing: bool, paused: bool) -> &'static str {
    if !playing {
        "STOPPED"
    } else if paused {
        "PAUSED"
    } else {
        "PLAYING"
    }
}

/// Render one frame of the sequencer UI into `frame`.
///
/// `state` is a cloned snapshot — no lock is held during rendering.
/// `overlay` is the current overlay mode tracked by the UI thread locally.
/// `selected_param` is the locally-tracked selected param index in the overlay.
pub fn render_frame(
    frame: &mut Frame,
    state: &SequencerState,
    overlay: Option<OverlayMode>,
    selected_param: u8,
) {
    let area = frame.area();

    // Vertical split: top bar | step rows | info row | overlay panel (if active)
    let overlay_height = if overlay.is_some() { 3u16 } else { 0u16 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // top bar
            Constraint::Length(3),  // step rows (note + indicator + playhead marker)
            Constraint::Length(1),  // info row (swing, loop, status)
            Constraint::Length(overlay_height), // overlay panel
            Constraint::Min(0),     // remaining space
        ])
        .split(area);

    // ── Top bar ──────────────────────────────────────────────────────────────
    let top_text = format!(
        " BPM: {}  Key: {}  Mode: {}  Step: {}  Status: {} ",
        state.tempo_bpm,
        key_name(state.key),
        mode_name(state.mode),
        step_size_label(state.step_size),
        status_label(state.playing, state.paused),
    );
    let top_bar = Paragraph::new(top_text)
        .style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(top_bar, chunks[0]);

    // ── Step rows ─────────────────────────────────────────────────────────────
    // We render three lines inside a 3-row area:
    //   Line 0: note names (with pending-note preview in selected column)
    //   Line 1: enabled indicators (● / ○)
    //   Line 2: playhead / selected markers
    render_steps(frame, state, chunks[1]);

    // ── Info row ──────────────────────────────────────────────────────────────
    let swing_str = if state.swing >= 0 {
        format!("Swing: +{}%", state.swing)
    } else {
        format!("Swing: {}%", state.swing)
    };
    let loop_str = if state.loop_active {
        format!("  Loop: {}–{}", state.loop_in, state.loop_out)
    } else {
        String::new()
    };
    let info_text = format!("{}{}", swing_str, loop_str);
    let info_bar = Paragraph::new(info_text);
    frame.render_widget(info_bar, chunks[2]);

    // ── Overlay panel ─────────────────────────────────────────────────────────
    if overlay_height > 0 {
        render_overlay(frame, state, overlay, selected_param, chunks[3]);
    }
}

/// Render the 16-step grid into `area`.
fn render_steps(frame: &mut Frame, state: &SequencerState, area: Rect) {
    // Build three lines: notes, indicators, markers.
    let mut note_spans: Vec<Span> = Vec::with_capacity(16 * 2);
    let mut indicator_spans: Vec<Span> = Vec::with_capacity(16 * 2);
    let mut marker_spans: Vec<Span> = Vec::with_capacity(16 * 2);

    for i in 0..16usize {
        let step = &state.steps[i];
        let is_playhead = state.playhead as usize == i;
        let is_selected = state.selected_step == i;

        // Determine display note: pending preview overrides for selected step.
        let pending_in_this_step = match state.pending_edit {
            PendingEdit::Note { step: s, midi_note } if s == i => Some(midi_note),
            _ => None,
        };

        // Build note span style.
        let note_style = if is_playhead && is_selected {
            Style::default()
                .add_modifier(Modifier::BOLD | Modifier::REVERSED | Modifier::UNDERLINED)
        } else if is_playhead {
            Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if is_selected {
            if pending_in_this_step.is_some() {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().add_modifier(Modifier::UNDERLINED)
            }
        } else {
            Style::default()
        };

        let note_str = if let Some(pn) = pending_in_this_step {
            format!("{:<4}", note_name(pn))
        } else {
            format!("{:<4}", note_name(step.midi_note))
        };

        note_spans.push(Span::styled(note_str, note_style));

        // Enabled indicator.
        let ind_char = if step.enabled { "● " } else { "○ " };
        let ind_style = if is_playhead {
            Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if is_selected {
            Style::default().add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default()
        };
        indicator_spans.push(Span::styled(ind_char, ind_style));
        // Pad to 4 chars to align with note column.
        indicator_spans.push(Span::raw("  "));

        // Playhead / selected marker row.
        let marker = if is_playhead && is_selected {
            "▲●  "
        } else if is_playhead {
            "▲   "
        } else if is_selected {
            "*   "
        } else {
            "    "
        };
        marker_spans.push(Span::raw(marker));
    }

    let lines = vec![
        Line::from(note_spans),
        Line::from(indicator_spans),
        Line::from(marker_spans),
    ];

    let para = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(para, area);
}

/// Render the overlay panel into `area`.
fn render_overlay(
    frame: &mut Frame,
    state: &SequencerState,
    overlay: Option<OverlayMode>,
    selected_param: u8,
    area: Rect,
) {
    match overlay {
        None => {}
        Some(OverlayMode::Shift) => {
            let para = Paragraph::new("(shift mode — coming soon)")
                .block(Block::default().title("Shift Overlay").borders(Borders::ALL));
            frame.render_widget(para, area);
        }
        Some(OverlayMode::Regular) => {
            // Horizontal list of 7 params with current values.
            let pending_param_value: Option<(u8, i64)> = match state.pending_edit {
                PendingEdit::Param { index, value, .. } => Some((index, value)),
                _ => None,
            };

            let mut spans: Vec<Span> = Vec::with_capacity(7 * 3);
            for (i, name) in REGULAR_PARAMS.iter().enumerate() {
                let idx = i as u8;
                let is_highlighted = idx == selected_param;

                // Build current value string.
                let value_str = param_value_string(state, idx);
                let display = if let Some((pi, pv)) = pending_param_value {
                    if pi == idx {
                        format!(" {}[{}→{}] ", name, value_str, pv)
                    } else {
                        format!(" {}:{} ", name, value_str)
                    }
                } else {
                    format!(" {}:{} ", name, value_str)
                };

                let style = if is_highlighted {
                    Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(display, style));
                if i < 6 {
                    spans.push(Span::raw("|"));
                }
            }

            let line = Line::from(spans);
            let para = Paragraph::new(line)
                .block(Block::default().title("Regular Overlay (Esc to close)").borders(Borders::ALL));
            frame.render_widget(para, area);
        }
    }
}

/// Return a short string representation of parameter `index` current value.
fn param_value_string(state: &SequencerState, index: u8) -> String {
    match index {
        0 => key_name(state.key).to_string(),
        1 => mode_name(state.mode).to_string(),
        2 => format!("{:+}", state.swing),
        3 => step_size_label(state.step_size).to_string(),
        4 => {
            if state.loop_active {
                format!("{}–{}", state.loop_in, state.loop_out)
            } else {
                "off".to_string()
            }
        }
        5 => {
            if state.paused { "on" } else { "off" }.to_string()
        }
        6 => {
            if state.playing { "playing" } else { "stopped" }.to_string()
        }
        _ => "?".to_string(),
    }
}
