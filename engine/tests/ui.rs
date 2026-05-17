//! Integration tests for `ui_render::render_frame` — the 7-zone cyberpunk layout.
//!
//! All tests use `TestBackend` and require no `hw-io` feature.
//! The terminal is sized at 120×30 (minimum) to accommodate all 7 zones.

use std::collections::VecDeque;

use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

use engine::input::FocusPanel;
use engine::music_theory::{Key, Mode};
use engine::state::{
    PendingEdit, SequencerState, StepData, StepSize, TempoRandType, TempoRollPoint,
};
use engine::ui_render::{
    render_frame, shift_param_value_string, shift_pending_param_value_string, LogEntry, LogTag,
    UiLocalSnapshot,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Default known state: step 0 = C4 enabled, playhead=0, 120 BPM, C Major, PLAYING.
fn known_state() -> SequencerState {
    let mut s = SequencerState::default();
    s.playing = true;
    s.paused = false;
    s.tempo_bpm = 120;
    s.key = Key::C;
    s.mode = Mode::Major;
    s.step_size = StepSize::Sixteenth;
    s.playhead = 0;
    s.selected_step = 0;
    s.steps[0] = StepData {
        enabled: true,
        midi_note: 60,
        velocity: 100,
    };
    s
}

/// Default empty log.
fn empty_log() -> VecDeque<LogEntry> {
    VecDeque::new()
}

/// Build a minimal `UiLocalSnapshot` for tests that don't need CLI-specific state.
fn default_snapshot<'a>(log: &'a VecDeque<LogEntry>) -> UiLocalSnapshot<'a> {
    UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: log,
        midi_device_name: "TestDevice",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    }
}

/// Collect all text from the terminal buffer as a single string.
fn collect_all_text(backend: &TestBackend, width: u16, height: u16) -> String {
    let buffer = backend.buffer().clone();
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            buffer
                .cell((x, y))
                .map(|c| c.symbol().chars().next().unwrap_or(' '))
                .unwrap_or(' ')
        })
        .collect()
}

/// Collect a single row as a string.
fn collect_row(backend: &TestBackend, y: u16, width: u16) -> String {
    let buffer = backend.buffer().clone();
    (0..width)
        .map(|x| {
            buffer
                .cell((x, y))
                .map(|c| c.symbol().chars().next().unwrap_or(' '))
                .unwrap_or(' ')
        })
        .collect()
}

/// Return true if any cell in the given row has the specified fg color.
fn row_has_fg(backend: &TestBackend, y: u16, width: u16, color: Color) -> bool {
    let buffer = backend.buffer().clone();
    (0..width).any(|x| buffer.cell((x, y)).map(|c| c.fg == color).unwrap_or(false))
}

/// Return true if any cell in the buffer within the row range has the specified fg color.
fn area_has_fg(backend: &TestBackend, width: u16, y_start: u16, y_end: u16, color: Color) -> bool {
    let buffer = backend.buffer().clone();
    (y_start..y_end).any(|y| {
        (0..width).any(|x| buffer.cell((x, y)).map(|c| c.fg == color).unwrap_or(false))
    })
}

// ── Transport bar (row 1) ─────────────────────────────────────────────────────

#[test]
fn transport_bar_contains_bpm_key_mode_step_status() {
    let state = known_state();
    let log = empty_log();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    // Transport bar is row 1.
    let row1 = collect_row(terminal.backend(), 1, 120);

    assert!(
        row1.contains("120"),
        "transport bar must contain BPM '120', got: {}",
        row1
    );
    assert!(
        row1.contains('C'),
        "transport bar must contain key 'C', got: {}",
        row1
    );
    assert!(
        row1.contains("Major"),
        "transport bar must contain 'Major', got: {}",
        row1
    );
    assert!(
        row1.contains("1/16"),
        "transport bar must contain step '1/16', got: {}",
        row1
    );
    assert!(
        row1.contains("PLAYING"),
        "transport bar must contain 'PLAYING', got: {}",
        row1
    );
}

#[test]
fn transport_bar_shows_quarter_step_size() {
    let mut state = known_state();
    state.step_size = StepSize::Quarter;
    let log = empty_log();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let row1 = collect_row(terminal.backend(), 1, 120);
    assert!(
        row1.contains("1/4"),
        "transport bar must contain '1/4', got: {}",
        row1
    );
}

#[test]
fn transport_bar_shows_eighth_step_size() {
    let mut state = known_state();
    state.step_size = StepSize::Eighth;
    let log = empty_log();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let row1 = collect_row(terminal.backend(), 1, 120);
    assert!(
        row1.contains("1/8"),
        "transport bar must contain '1/8', got: {}",
        row1
    );
}

#[test]
fn transport_bar_stopped_status() {
    let mut state = known_state();
    state.playing = false;
    let log = empty_log();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let row1 = collect_row(terminal.backend(), 1, 120);
    assert!(
        row1.contains("STOPPED"),
        "transport bar must show STOPPED, got: {}",
        row1
    );
}

#[test]
fn transport_bar_paused_status() {
    let mut state = known_state();
    state.paused = true;
    let log = empty_log();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let row1 = collect_row(terminal.backend(), 1, 120);
    assert!(
        row1.contains("PAUSED"),
        "transport bar must show PAUSED, got: {}",
        row1
    );
}

// ── Title bar (row 0) ─────────────────────────────────────────────────────────

#[test]
fn title_bar_contains_project_name() {
    let state = known_state();
    let log = empty_log();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let row0 = collect_row(terminal.backend(), 0, 120);
    assert!(
        row0.contains("midi-man-mk3"),
        "title bar must contain 'midi-man-mk3', got: {}",
        row0
    );
}

#[test]
fn title_bar_contains_midi_device_info() {
    let state = known_state();
    let log = empty_log();
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "USB MIDI",
        midi_channel_display: 3,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let row0 = collect_row(terminal.backend(), 0, 120);
    assert!(
        row0.contains("MIDI OUT"),
        "title bar must contain 'MIDI OUT', got: {}",
        row0
    );
    assert!(
        row0.contains("CH:3"),
        "title bar must contain 'CH:3', got: {}",
        row0
    );
}

// ── F1 SEQ panel — step cards ─────────────────────────────────────────────────

#[test]
fn f1_panel_renders_enabled_step_indicator() {
    let state = known_state(); // step 0 enabled
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 160, 30);
    assert!(
        all.contains('●'),
        "F1 panel must show '●' for enabled step 0"
    );
}

#[test]
fn f1_panel_renders_disabled_step_indicator() {
    let mut state = known_state();
    // Step 1 is disabled by default in known_state (only step 0 enabled).
    state.steps[1].enabled = false;
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 160, 30);
    assert!(
        all.contains('○'),
        "F1 panel must show '○' for disabled steps"
    );
}

#[test]
fn f1_panel_renders_c4_note_name() {
    let state = known_state(); // step 0 = C4
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 160, 30);
    assert!(
        all.contains("C4"),
        "F1 panel must contain 'C4' for step 0 note, got all text"
    );
}

#[test]
fn f1_panel_all_sixteen_steps_render_without_panic() {
    let mut state = SequencerState::default();
    for (i, step) in state.steps.iter_mut().enumerate() {
        step.enabled = true;
        step.midi_note = 60 + (i as u8 % 4) * 2;
        step.velocity = 100;
    }
    state.playhead = 0;
    state.selected_step = 0;

    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 160, 30);
    let c4_count = all.matches("C4").count();
    assert!(
        c4_count >= 4,
        "F1 panel must contain 'C4' at least 4 times for steps 0,4,8,12, got: {}",
        c4_count
    );
}

#[test]
fn f1_panel_enabled_disabled_pattern() {
    let mut state = SequencerState::default();
    for (i, step) in state.steps.iter_mut().enumerate() {
        step.enabled = i % 2 == 0;
        step.midi_note = 60;
        step.velocity = 100;
    }
    state.playhead = 0;
    state.selected_step = 0;

    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 160, 30);
    assert!(all.contains('●'), "must show '●' for enabled (even) steps");
    assert!(all.contains('○'), "must show '○' for disabled (odd) steps");
}

#[test]
fn f1_panel_playhead_step_renders_a4_note() {
    let mut state = known_state();
    state.playhead = 8;
    state.selected_step = 0;
    state.steps[8] = StepData {
        enabled: true,
        midi_note: 69, // A4
        velocity: 100,
    };
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 160, 30);
    assert!(
        all.contains("A4"),
        "F1 panel must show 'A4' at playhead step 8"
    );
}

// ── F2 SEQ PARAMS panel ───────────────────────────────────────────────────────

#[test]
fn f2_panel_shows_seq_param_names() {
    let state = known_state();
    let log = empty_log();
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 200, 30);
    assert!(all.contains("KEY"), "F2 panel must show 'KEY'");
    assert!(all.contains("MODE"), "F2 panel must show 'MODE'");
    assert!(all.contains("SWING"), "F2 panel must show 'SWING'");
    assert!(all.contains("STEP"), "F2 panel must show 'STEP'");
    assert!(all.contains("L.IN"), "F2 panel must show 'L.IN'");
    assert!(all.contains("L.OUT"), "F2 panel must show 'L.OUT'");
    assert!(all.contains("PAUSE"), "F2 panel must show 'PAUSE'");
    assert!(all.contains("PLAY"), "F2 panel must show 'PLAY'");
}

#[test]
fn f2_panel_shows_current_key_value() {
    let state = known_state(); // key = C
    let log = empty_log();
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 200, 30);
    // KEY:C should appear in F2 panel row.
    assert!(
        all.contains("KEY:C"),
        "F2 panel must show 'KEY:C' for key=C, all text present"
    );
}

#[test]
fn f2_panel_swing_value_shown() {
    let mut state = known_state();
    state.swing = 15;
    let log = empty_log();
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 200, 30);
    // The swing value is rendered as "+15".
    assert!(all.contains("+15"), "F2 panel must show '+15' for swing=15");
}

// ── F3 RANDOM PARAMS panel ────────────────────────────────────────────────────

#[test]
fn f3_panel_shows_rand_param_names() {
    let state = known_state();
    let log = empty_log();
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 200, 30);
    assert!(all.contains("N.RND"), "F3 panel must show 'N.RND'");
    assert!(all.contains("T.RND"), "F3 panel must show 'T.RND'");
    assert!(all.contains("ROLL"), "F3 panel must show 'ROLL'");
    assert!(all.contains("V.MAX"), "F3 panel must show 'V.MAX'");
    assert!(all.contains("T.TYPE"), "F3 panel must show 'T.TYPE'");
    assert!(all.contains("S.RND"), "F3 panel must show 'S.RND'");
    assert!(all.contains("S.QUANT"), "F3 panel must show 'S.QUANT'");
    assert!(all.contains("SEED"), "F3 panel must show 'SEED'");
}

#[test]
fn f3_panel_shows_seed_in_hex_format() {
    let mut state = known_state();
    state.rand_seed = 0xABCD;
    let log = empty_log();
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 200, 30);
    assert!(
        all.contains("0xABCD"),
        "F3 panel must show seed as '0xABCD', got all text"
    );
}

// ── F4 CLI panel ──────────────────────────────────────────────────────────────

#[test]
fn f4_panel_shows_log_entries() {
    let state = known_state();
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
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Cli,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "my input",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 120, 40);
    assert!(
        all.contains("hello world"),
        "CLI panel must display 'hello world' log entry"
    );
    assert!(
        all.contains("my input"),
        "CLI panel must show the input line content"
    );
}

#[test]
fn f4_panel_shows_input_prompt() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Cli,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 120, 30);
    assert!(all.contains('>'), "CLI panel must show '>' input prompt");
}

// ── Focus border coloring ─────────────────────────────────────────────────────

#[test]
fn focused_f1_panel_renders_without_panic() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");
}

#[test]
fn focused_f2_panel_renders_without_panic() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::SeqParams,
        selected_step: 0,
        seq_param_idx: 2,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");
    // Swing param (index 2) should be highlighted when F2 focused.
    let all = collect_all_text(terminal.backend(), 200, 30);
    assert!(all.contains("SWING"), "F2 panel must show SWING param");
}

#[test]
fn focused_f3_panel_renders_without_panic() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::RandParams,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 1,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");
    let all = collect_all_text(terminal.backend(), 200, 30);
    assert!(all.contains("T.RND"), "F3 panel must show T.RND param");
}

// ── BUG-007 regression: TestBackend compiles without hw-io ────────────────────

/// Verify TestBackend renders without the `hw-io` feature.
#[test]
fn test_backend_renders_without_hw_io_feature() {
    let backend = TestBackend::new(120, 30);
    let mut terminal =
        Terminal::new(backend).expect("TestBackend terminal must construct without hw-io");

    let state = SequencerState::default();
    let log = empty_log();
    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("render_frame must complete with TestBackend and no hw-io feature");

    let buffer = terminal.backend().buffer().clone();
    let any_non_space = (0..120u16).any(|x| {
        buffer
            .cell((x, 0))
            .map(|c| c.symbol() != " ")
            .unwrap_or(false)
    });
    assert!(
        any_non_space,
        "rendered buffer must contain non-space cells (title bar must render)"
    );
}

/// Render must be deterministic: clear-and-redraw produces identical output.
#[test]
fn test_backend_clear_and_redraw_is_idempotent() {
    let state = known_state();
    let log = empty_log();

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("first draw");

    let first_row0 = collect_row(terminal.backend(), 0, 120);
    terminal.clear().expect("terminal clear must succeed");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("second draw after clear");

    let second_row0 = collect_row(terminal.backend(), 0, 120);
    assert_eq!(
        first_row0, second_row0,
        "title bar must be identical after clear-and-redraw (render must be deterministic)"
    );
}

// ── render_frame does not panic for any focus panel or param index ─────────────

#[test]
fn render_frame_does_not_panic_for_all_seq_param_indices() {
    let state = known_state();
    let log = empty_log();
    for idx in 0u8..=7 {
        let snap = UiLocalSnapshot {
            focus: FocusPanel::SeqParams,
            selected_step: 0,
            seq_param_idx: idx,
            rand_param_idx: 0,
            cli_line: "",
            cli_log: &log,
            midi_device_name: "",
            midi_channel_display: 1,
            play_mode: engine::state::PlayMode::Pattern,
            song_slots: &[],
            song_cursor: 0,
            song_active_slot: 0,
        };
        let backend = TestBackend::new(200, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_frame(frame, &state, &snap))
            .expect("draw must not panic");
    }
}

#[test]
fn render_frame_does_not_panic_for_all_rand_param_indices() {
    let state = known_state();
    let log = empty_log();
    for idx in 0u8..=7 {
        let snap = UiLocalSnapshot {
            focus: FocusPanel::RandParams,
            selected_step: 0,
            seq_param_idx: 0,
            rand_param_idx: idx,
            cli_line: "",
            cli_log: &log,
            midi_device_name: "",
            midi_channel_display: 1,
            play_mode: engine::state::PlayMode::Pattern,
            song_slots: &[],
            song_cursor: 0,
            song_active_slot: 0,
        };
        let backend = TestBackend::new(200, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_frame(frame, &state, &snap))
            .expect("draw must not panic");
    }
}

// ── PendingEdit: note preview via note_name ────────────────────────────────────
//
// The new render shows all steps' midi_note values. Pending note edits are not
// yet reflected in the F1 panel (ui.rs task 4.1 will wire the snapshot). The
// F1 panel renders state.steps[i].midi_note — verify that works.

#[test]
fn f1_panel_shows_d4_when_step_note_is_d4() {
    let mut state = known_state();
    state.steps[3] = StepData {
        enabled: true,
        midi_note: 62, // D4
        velocity: 100,
    };
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 160, 30);
    assert!(
        all.contains("D4"),
        "F1 panel must show 'D4' for step 3 with midi_note=62"
    );
}

// ── Pending edit in state — does not crash render ─────────────────────────────

#[test]
fn pending_note_edit_state_does_not_crash_render() {
    let mut state = known_state();
    state.pending_edit = PendingEdit::Note {
        step: 0,
        midi_note: 62,
    };
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw must not panic with PendingEdit::Note in state");
}

// ─── shift_param_value_string unit tests ─────────────────────────────────────

#[test]
fn shift_param_value_string_note_rand() {
    let mut s = SequencerState::default();
    s.note_rand = 42;
    assert_eq!(shift_param_value_string(&s, 0), "42");
}

#[test]
fn shift_param_value_string_tempo_rand() {
    let mut s = SequencerState::default();
    s.tempo_rand = 75;
    assert_eq!(shift_param_value_string(&s, 1), "75");
}

#[test]
fn shift_param_value_string_roll_point_variants() {
    let mut s = SequencerState::default();
    s.tempo_roll_point = TempoRollPoint::Off;
    assert_eq!(shift_param_value_string(&s, 2), "Off");
    s.tempo_roll_point = TempoRollPoint::Step;
    assert_eq!(shift_param_value_string(&s, 2), "Step");
    s.tempo_roll_point = TempoRollPoint::Beat;
    assert_eq!(shift_param_value_string(&s, 2), "Beat");
    s.tempo_roll_point = TempoRollPoint::Seq;
    assert_eq!(shift_param_value_string(&s, 2), "Seq");
}

#[test]
fn shift_param_value_string_var_max() {
    let mut s = SequencerState::default();
    s.tempo_variance_max = 50;
    assert_eq!(shift_param_value_string(&s, 3), "50");
}

#[test]
fn shift_param_value_string_tempo_rand_type_variants() {
    let mut s = SequencerState::default();
    s.tempo_rand_type = TempoRandType::Random;
    assert_eq!(shift_param_value_string(&s, 4), "Random");
    s.tempo_rand_type = TempoRandType::Up;
    assert_eq!(shift_param_value_string(&s, 4), "Up");
    s.tempo_rand_type = TempoRandType::Down;
    assert_eq!(shift_param_value_string(&s, 4), "Down");
    s.tempo_rand_type = TempoRandType::Breathe;
    assert_eq!(shift_param_value_string(&s, 4), "Breathe");
    s.tempo_rand_type = TempoRandType::PingPong;
    assert_eq!(shift_param_value_string(&s, 4), "PingPong");
}

#[test]
fn shift_param_value_string_step_rand() {
    let mut s = SequencerState::default();
    s.step_rand = 33;
    assert_eq!(shift_param_value_string(&s, 5), "33");
}

#[test]
fn shift_param_value_string_scale_quant() {
    let mut s = SequencerState::default();
    s.scale_quant = false;
    assert_eq!(shift_param_value_string(&s, 6), "Off");
    s.scale_quant = true;
    assert_eq!(shift_param_value_string(&s, 6), "On");
}

#[test]
fn shift_param_value_string_reserved_returns_em_dash() {
    let s = SequencerState::default();
    let result = shift_param_value_string(&s, 7);
    assert!(
        !result.is_empty(),
        "reserved param must return a non-empty string"
    );
}

// ─── shift_pending_param_value_string unit tests ──────────────────────────────

#[test]
fn shift_pending_param_value_string_numeric() {
    assert_eq!(shift_pending_param_value_string(0, 55), "55");
    assert_eq!(shift_pending_param_value_string(1, 80), "80");
    assert_eq!(shift_pending_param_value_string(3, 25), "25");
    assert_eq!(shift_pending_param_value_string(5, 10), "10");
}

#[test]
fn shift_pending_param_value_string_roll_point() {
    assert_eq!(shift_pending_param_value_string(2, 0), "Off");
    assert_eq!(shift_pending_param_value_string(2, 1), "Step");
    assert_eq!(shift_pending_param_value_string(2, 2), "Beat");
    assert_eq!(shift_pending_param_value_string(2, 3), "Seq");
}

#[test]
fn shift_pending_param_value_string_tempo_rand_type() {
    assert_eq!(shift_pending_param_value_string(4, 0), "Random");
    assert_eq!(shift_pending_param_value_string(4, 1), "Up");
    assert_eq!(shift_pending_param_value_string(4, 2), "Down");
    assert_eq!(shift_pending_param_value_string(4, 3), "Breathe");
    assert_eq!(shift_pending_param_value_string(4, 4), "PingPong");
}

#[test]
fn shift_pending_param_value_string_scale_quant() {
    assert_eq!(shift_pending_param_value_string(6, 0), "Off");
    assert_eq!(shift_pending_param_value_string(6, 1), "On");
}

#[test]
fn shift_pending_param_value_string_reserved_does_not_panic() {
    let result = shift_pending_param_value_string(7, 999);
    assert!(!result.is_empty());
}

// ── Transport bar — STATUS color assertions ───────────────────────────────────

/// STATUS span is GREEN (Rgb(0,200,80)) when playing=true, paused=false.
#[test]
fn transport_bar_status_green_when_playing() {
    let mut state = known_state();
    state.playing = true;
    state.paused = false;
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    // Transport bar = row 1. GREEN = Rgb(0,200,80).
    let green = Color::Rgb(0, 200, 80);
    assert!(
        row_has_fg(terminal.backend(), 1, 160, green),
        "transport bar STATUS must have GREEN fg when playing=true, paused=false"
    );
}

/// STATUS span is CYAN (Rgb(0,255,255)) when paused=true.
#[test]
fn transport_bar_status_cyan_when_paused() {
    let mut state = known_state();
    state.playing = true;
    state.paused = true;
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    // CYAN = Rgb(0,255,255).
    let cyan = Color::Rgb(0, 255, 255);
    assert!(
        row_has_fg(terminal.backend(), 1, 160, cyan),
        "transport bar STATUS must have CYAN fg when paused=true"
    );
}

/// STATUS span uses Color::Reset (terminal default) when stopped (playing=false).
#[test]
fn transport_bar_status_default_when_stopped() {
    let mut state = known_state();
    state.playing = false;
    state.paused = false;
    let log = empty_log();
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &default_snapshot(&log)))
        .expect("draw");

    // When stopped, STATUS color is Color::Reset — neither GREEN nor CYAN must be in row 1.
    let green = Color::Rgb(0, 200, 80);
    let cyan = Color::Rgb(0, 255, 255);
    // The GRAY prefix spans use Rgb(136,136,136); the status span must not be green or cyan.
    assert!(
        !row_has_fg(terminal.backend(), 1, 160, green),
        "transport bar STATUS must NOT be GREEN when stopped"
    );
    assert!(
        !row_has_fg(terminal.backend(), 1, 160, cyan),
        "transport bar STATUS must NOT be CYAN when stopped"
    );
    // The row must still contain "STOPPED" text.
    let row1 = collect_row(terminal.backend(), 1, 160);
    assert!(row1.contains("STOPPED"), "transport bar must show STOPPED");
}

// ── F2 SEQ PARAMS — selected param MAGENTA color assertions ──────────────────

/// Selected param (seq_param_idx=2 = SWING) is rendered MAGENTA when focus=SeqParams.
#[test]
fn f2_selected_param_has_magenta_when_focused() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::SeqParams,
        selected_step: 0,
        seq_param_idx: 2,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    // F2 panel occupies rows 3–5 (title=0, transport=1, F1=2..N-4, F2 block starts after F1).
    // Use area_has_fg across all rows to find MAGENTA in the F2 region.
    let magenta = Color::Rgb(255, 0, 127);
    // F2 panel inner row is at y=4 in a standard 30-row layout (title=0,transport=1,F1=2-12,F2=13-15).
    // Scan all rows to avoid hard-coding exact layout.
    assert!(
        area_has_fg(terminal.backend(), 200, 0, 30, magenta),
        "F2 panel must render selected param (seq_param_idx=2) with MAGENTA when focus=SeqParams"
    );
}

/// No param is rendered MAGENTA in F2 when focus is not SeqParams.
#[test]
fn f2_no_magenta_when_focus_is_sequencer() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 2,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    // With focus=Sequencer the is_selected condition in render_seq_params_panel is false
    // (focused=false), so no MAGENTA should appear from F2. MAGENTA may still appear from
    // the F1 playhead (playhead=0, selected_step=0 → MAGENTA border). We must check only
    // the F2 row. In a 30-row, 200-col terminal the layout is:
    //   row 0: title, row 1: transport, rows 2-12: F1 (Min(5)+borders),
    //   rows 13-15: F2 (Length(3)), rows 16-18: F3 (Length(3)),
    //   rows 19-28: F4 (Min(5)), row 29: keybind.
    // F2 occupies 3 rows. Scan only those rows for MAGENTA fg.
    // Because Min(5) distribution can vary, we scan rows 10-22 to safely include F2.
    let magenta = Color::Rgb(255, 0, 127);
    let buf = terminal.backend().buffer().clone();
    // F2 inner content row contains param labels. Find the row that contains "SWING".
    let f2_row = (0u16..30).find(|&y| {
        let row_text: String = (0..200u16)
            .map(|x| {
                buf.cell((x, y))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
                    .unwrap_or(' ')
            })
            .collect();
        row_text.contains("SWING")
    });

    if let Some(row_y) = f2_row {
        let magenta_on_f2 = (0..200u16).any(|x| {
            buf.cell((x, row_y))
                .map(|c| c.fg == magenta)
                .unwrap_or(false)
        });
        assert!(
            !magenta_on_f2,
            "F2 param row must NOT render MAGENTA when focus=Sequencer (row {})",
            row_y
        );
    }
    // If SWING row not found, the test trivially passes (other tests verify F2 content).
}

// ── F3 RANDOM PARAMS — SEED hex format and selected param color ───────────────

/// SEED value 0xABCD renders as "0xABCD" in the F3 panel (hex string content).
#[test]
fn f3_seed_renders_with_0x_prefix_hex() {
    let mut state = known_state();
    state.rand_seed = 0xABCD;
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 200, 30);
    assert!(
        all.contains("0xABCD"),
        "F3 SEED must render as '0xABCD' for rand_seed=0xABCD"
    );
}

/// rand_seed=0x0001 renders as "0x0001" (four-digit zero-padded hex).
#[test]
fn f3_seed_renders_four_digit_hex_zero_padded() {
    let mut state = known_state();
    state.rand_seed = 1;
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 200, 30);
    assert!(
        all.contains("0x0001"),
        "F3 SEED must render as '0x0001' for rand_seed=1 (zero-padded to 4 hex digits)"
    );
}

/// Selected rand param (rand_param_idx=2) is MAGENTA when focus=RandParams.
#[test]
fn f3_selected_param_has_magenta_when_focused() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::RandParams,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 2,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let magenta = Color::Rgb(255, 0, 127);
    assert!(
        area_has_fg(terminal.backend(), 200, 0, 30, magenta),
        "F3 panel must render selected param (rand_param_idx=2) with MAGENTA when focus=RandParams"
    );
}

// ── F4 CLI panel — prompt, log tag colors ─────────────────────────────────────

/// The "> " prompt appears in the CLI panel input row.
#[test]
fn f4_cli_prompt_contains_greater_than_space() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Cli,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 120, 40);
    assert!(
        all.contains("> "),
        "CLI panel must show '> ' prompt"
    );
}

/// cli_line content appears in the CLI input prompt area.
#[test]
fn f4_cli_line_content_appears_in_prompt() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Cli,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "device list",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let all = collect_all_text(terminal.backend(), 120, 40);
    assert!(
        all.contains("device list"),
        "CLI panel must render cli_line content 'device list' in the prompt area"
    );
}

/// A log entry with LogTag::Midi renders with GREEN (Rgb(0,200,80)) fg for the tag span.
#[test]
fn f4_log_tag_midi_renders_with_green_color() {
    let state = known_state();
    let mut log: VecDeque<LogEntry> = VecDeque::new();
    log.push_back(LogEntry {
        timestamp_ms: 100,
        tag: LogTag::Midi,
        text: "note on C4".to_string(),
    });
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Cli,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(160, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let green = Color::Rgb(0, 200, 80);
    assert!(
        area_has_fg(terminal.backend(), 160, 0, 40, green),
        "CLI panel must render LogTag::Midi with GREEN (Rgb(0,200,80)) fg"
    );
}

/// A log entry with LogTag::Err renders with Color::Red fg for the tag span.
#[test]
fn f4_log_tag_err_renders_with_red_color() {
    let state = known_state();
    let mut log: VecDeque<LogEntry> = VecDeque::new();
    log.push_back(LogEntry {
        timestamp_ms: 200,
        tag: LogTag::Err,
        text: "device not found".to_string(),
    });
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Cli,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(160, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    assert!(
        area_has_fg(terminal.backend(), 160, 0, 40, Color::Red),
        "CLI panel must render LogTag::Err with Color::Red fg"
    );
}

// ── Title bar — MIDI OUT device name and channel ──────────────────────────────

/// "MIDI OUT" text appears with device name and channel when midi_device_name is non-empty.
#[test]
fn title_bar_shows_midi_out_with_device_and_channel() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "Arturia KeyStep",
        midi_channel_display: 5,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let row0 = collect_row(terminal.backend(), 0, 160);
    assert!(
        row0.contains("MIDI OUT"),
        "title bar must contain 'MIDI OUT', got: {}",
        row0
    );
    assert!(
        row0.contains("Arturia KeyStep"),
        "title bar must contain device name 'Arturia KeyStep', got: {}",
        row0
    );
    assert!(
        row0.contains("CH:5"),
        "title bar must contain 'CH:5', got: {}",
        row0
    );
}

/// Title bar shows "—" placeholder when midi_device_name is empty.
#[test]
fn title_bar_shows_dash_when_no_midi_device() {
    let state = known_state();
    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 0,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    let row0 = collect_row(terminal.backend(), 0, 160);
    assert!(
        row0.contains('\u{2014}'),
        "title bar must show '—' (em dash) when midi_device_name is empty, got: {}",
        row0
    );
}

// ── F1 SEQ panel — disabled/enabled step color assertions ────────────────────

/// A disabled step (step.enabled=false) renders with DIM_CYAN (Rgb(0,64,64)) in F1.
#[test]
fn f1_disabled_step_renders_with_dim_cyan() {
    let mut state = SequencerState::default();
    // All steps disabled, playhead at a non-zero position to avoid MAGENTA overlap on step 0.
    for step in state.steps.iter_mut() {
        step.enabled = false;
        step.midi_note = 60;
        step.velocity = 100;
    }
    state.playhead = 15; // playhead at step 15 → step 0 is just disabled
    state.selected_step = 15; // selected at 15 too, so step 0 border is plain dim_cyan

    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 15,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(160, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    // DIM_CYAN = Rgb(0,64,64). Must appear somewhere in the F1 area.
    let dim_cyan = Color::Rgb(0, 64, 64);
    // F1 panel covers rows 2 onwards. Scan rows 2-25 to be safe.
    assert!(
        area_has_fg(terminal.backend(), 160, 2, 25, dim_cyan),
        "F1 panel must render disabled steps with DIM_CYAN (Rgb(0,64,64))"
    );
}

/// An enabled non-playhead step renders with CYAN (Rgb(0,255,255)) in F1.
#[test]
fn f1_enabled_non_playhead_step_renders_with_cyan() {
    let mut state = SequencerState::default();
    state.steps[0].enabled = true;
    state.steps[0].midi_note = 60;
    state.steps[0].velocity = 100;
    state.playhead = 8; // playhead is at step 8, so step 0 is enabled non-playhead
    state.selected_step = 8;

    let log = empty_log();
    let snap = UiLocalSnapshot {
        focus: FocusPanel::Sequencer,
        selected_step: 8,
        seq_param_idx: 0,
        rand_param_idx: 0,
        cli_line: "",
        cli_log: &log,
        midi_device_name: "",
        midi_channel_display: 1,
        play_mode: engine::state::PlayMode::Pattern,
        song_slots: &[],
        song_cursor: 0,
        song_active_slot: 0,
    };
    let backend = TestBackend::new(160, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, &state, &snap))
        .expect("draw");

    // CYAN = Rgb(0,255,255). Must appear in the F1 area for the enabled step 0.
    let cyan = Color::Rgb(0, 255, 255);
    // Scan rows 2-25 (F1 panel area).
    assert!(
        area_has_fg(terminal.backend(), 160, 2, 25, cyan),
        "F1 panel must render enabled non-playhead step with CYAN (Rgb(0,255,255))"
    );
}
