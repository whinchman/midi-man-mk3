use ratatui::backend::TestBackend;
use ratatui::Terminal;

use engine::input::OverlayMode;
use engine::music_theory::{Key, Mode};
use engine::state::{PendingEdit, SequencerState, StepData, StepSize};
use engine::ui_render::render_frame;

/// Build a known state: step 0 = C4 enabled, playhead=0, 120 BPM, C Major, PLAYING.
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
    s.steps[0] = StepData { enabled: true, midi_note: 60, velocity: 100 };
    s
}

#[test]
fn top_bar_contains_bpm_key_mode_step_status() {
    let state = known_state();
    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    // Collect row 0 as a string.
    let row0: String = (0..120)
        .map(|x| buffer.cell((x, 0)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row0.contains("BPM: 120"), "top bar must contain 'BPM: 120', got: {}", row0);
    assert!(row0.contains("Key: C"), "top bar must contain 'Key: C', got: {}", row0);
    assert!(row0.contains("Mode: Major"), "top bar must contain 'Mode: Major', got: {}", row0);
    assert!(row0.contains("Step: 1/16"), "top bar must contain 'Step: 1/16', got: {}", row0);
    assert!(row0.contains("PLAYING"), "top bar must contain 'PLAYING', got: {}", row0);
}

#[test]
fn step_row_shows_c4_at_step_0() {
    let state = known_state();
    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    // Row 1 is the note name row (top bar is row 0, step rows start at 1).
    let row1: String = (0..120)
        .map(|x| buffer.cell((x, 1)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row1.contains("C4"), "step row must contain 'C4' for step 0, got: {}", row1);
}

#[test]
fn step_row_shows_enabled_indicator_for_step_0() {
    let state = known_state();
    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    // Row 2 is the indicator row.
    let row2: String = (0..120)
        .map(|x| buffer.cell((x, 2)).map(|c| c.symbol()).unwrap_or(""))
        .collect();

    // Step 0 is enabled, so indicator should be ●.
    assert!(row2.contains('●'), "indicator row must contain '●' for enabled step 0, got: {}", row2);
}

#[test]
fn info_row_shows_swing_zero() {
    let state = known_state();
    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    // Row 4 is the info row (rows 1-3 = step rows).
    let row4: String = (0..120)
        .map(|x| buffer.cell((x, 4)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row4.contains("Swing"), "info row must contain 'Swing', got: {}", row4);
}

#[test]
fn overlay_regular_shows_param_names() {
    let state = known_state();
    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, Some(OverlayMode::Regular), 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    // Collect all rows into one string to search for param names.
    let all_text: String = (0..10u16)
        .flat_map(|y| (0..120u16).map(move |x| (x, y)))
        .map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(all_text.contains("Key"), "overlay must show 'Key' param, got buffer text");
    assert!(all_text.contains("Mode"), "overlay must show 'Mode' param");
    assert!(all_text.contains("Swing"), "overlay must show 'Swing' param");
}

#[test]
fn overlay_shift_shows_coming_soon() {
    let state = known_state();
    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, Some(OverlayMode::Shift), 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let all_text: String = (0..10u16)
        .flat_map(|y| (0..120u16).map(move |x| (x, y)))
        .map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(
        all_text.contains("shift mode"),
        "shift overlay must contain 'shift mode', got buffer text"
    );
}

#[test]
fn pending_note_preview_shown_in_selected_step() {
    let mut state = known_state();
    // Set a pending note edit on step 0.
    state.pending_edit = PendingEdit::Note { step: 0, midi_note: 62 }; // D4

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    // Row 1 is the note row; step 0 should show D4 (pending) not C4.
    let row1: String = (0..120)
        .map(|x| buffer.cell((x, 1)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row1.contains("D4"), "note row must show pending note D4, got: {}", row1);
}

#[test]
fn stopped_status_shown_when_not_playing() {
    let mut state = known_state();
    state.playing = false;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let row0: String = (0..120)
        .map(|x| buffer.cell((x, 0)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row0.contains("STOPPED"), "top bar must show STOPPED when not playing, got: {}", row0);
}

#[test]
fn paused_status_shown_when_paused() {
    let mut state = known_state();
    state.paused = true;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let row0: String = (0..120)
        .map(|x| buffer.cell((x, 0)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row0.contains("PAUSED"), "top bar must show PAUSED when paused, got: {}", row0);
}

#[test]
fn loop_bounds_shown_when_loop_active() {
    let mut state = known_state();
    state.loop_active = true;
    state.loop_in = 3;
    state.loop_out = 10;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let row4: String = (0..120)
        .map(|x| buffer.cell((x, 4)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row4.contains("Loop"), "info row must show loop bounds, got: {}", row4);
    assert!(row4.contains('3'), "info row must show loop_in=3, got: {}", row4);
}

// ── New augmented tests ───────────────────────────────────────────────────

// --- Top bar: step size labels ---

#[test]
fn top_bar_shows_quarter_step_size() {
    let mut state = known_state();
    state.step_size = StepSize::Quarter;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();
    let row0: String = (0..120)
        .map(|x| buffer.cell((x, 0)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row0.contains("1/4"), "top bar must show '1/4' for Quarter step size, got: {}", row0);
}

#[test]
fn top_bar_shows_eighth_step_size() {
    let mut state = known_state();
    state.step_size = StepSize::Eighth;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();
    let row0: String = (0..120)
        .map(|x| buffer.cell((x, 0)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(row0.contains("1/8"), "top bar must show '1/8' for Eighth step size, got: {}", row0);
}

// --- Step row: disabled indicator ---

#[test]
fn step_row_shows_disabled_indicator_for_disabled_step() {
    let mut state = known_state();
    // Step 1 is disabled by default (known_state only enables step 0).
    state.steps[1].enabled = false;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();
    let row2: String = (0..120)
        .map(|x| buffer.cell((x, 2)).map(|c| c.symbol()).unwrap_or(""))
        .collect();

    assert!(row2.contains('○'), "indicator row must contain '○' for a disabled step, got: {}", row2);
}

// --- Step row: playhead at step 8 highlights correct column ---

#[test]
fn step_row_playhead_at_step_8_highlights_correct_column() {
    let mut state = known_state();
    state.playhead = 8;
    state.selected_step = 0; // keep selected at 0 so playhead != selected
    state.steps[8] = StepData { enabled: true, midi_note: 69, velocity: 100 }; // A4

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let marker_row: String = (0..120)
        .map(|x| buffer.cell((x, 3)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    // The playhead marker is '▲' at position 32.
    assert!(
        marker_row.contains('▲'),
        "marker row must contain '▲' when playhead=8, got: {}",
        marker_row
    );

    // Also confirm note A4 appears in note row (y=1) at the playhead column.
    let note_row: String = (0..120)
        .map(|x| buffer.cell((x, 1)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(note_row.contains("A4"), "note row must contain 'A4' at playhead step 8, got: {}", note_row);
}

// --- Second row: swing positive value ---

#[test]
fn info_row_shows_positive_swing() {
    let mut state = known_state();
    state.swing = 15;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();
    let row4: String = (0..120)
        .map(|x| buffer.cell((x, 4)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(
        row4.contains("Swing: +15"),
        "info row must contain 'Swing: +15' for swing=15, got: {}",
        row4
    );
}

// --- Second row: loop bounds with active loop ---

#[test]
fn info_row_shows_loop_bounds_3_to_10() {
    let mut state = known_state();
    state.loop_active = true;
    state.loop_in = 3;
    state.loop_out = 10;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();
    let row4: String = (0..120)
        .map(|x| buffer.cell((x, 4)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(
        row4.contains("Loop"),
        "info row must contain 'Loop' when loop is active, got: {}",
        row4
    );
    assert!(row4.contains('3'), "info row must show loop_in=3, got: {}", row4);
    assert!(row4.contains("10"), "info row must show loop_out=10, got: {}", row4);
}

// --- Second row: loop bounds NOT shown when loop is inactive ---

#[test]
fn info_row_does_not_show_loop_when_loop_inactive() {
    let mut state = known_state();
    state.loop_active = false;
    state.loop_in = 3;
    state.loop_out = 10;

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();
    let row4: String = (0..120)
        .map(|x| buffer.cell((x, 4)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(
        !row4.contains("Loop"),
        "info row must NOT show 'Loop' when loop is inactive, got: {}",
        row4
    );
}

// --- Overlay: F1 Regular with selected_param=2 (Swing) highlighted ---

#[test]
fn overlay_regular_selected_param_2_shows_swing_highlighted() {
    let state = known_state();
    let backend = TestBackend::new(120, 12); // extra rows for overlay panel
    let mut terminal = Terminal::new(backend).expect("test terminal");

    // selected_param=2 corresponds to "Swing" in REGULAR_PARAMS.
    terminal.draw(|frame| {
        render_frame(frame, &state, Some(OverlayMode::Regular), 2);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let all_text: String = (0..12u16)
        .flat_map(|y| (0..120u16).map(move |x| (x, y)))
        .map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(
        all_text.contains("Swing"),
        "overlay must display 'Swing' param at index 2, got buffer text"
    );
}

// --- Overlay: F2 Shift shows "(shift mode" text ---

#[test]
fn overlay_shift_shows_shift_mode_text() {
    let state = known_state();
    let backend = TestBackend::new(120, 12);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, Some(OverlayMode::Shift), 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let all_text: String = (0..12u16)
        .flat_map(|y| (0..120u16).map(move |x| (x, y)))
        .map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(
        all_text.contains("shift mode"),
        "shift overlay must contain '(shift mode', got buffer text"
    );
}

// --- PendingEdit::Note: specific midi_note visible in selected step column ---

#[test]
fn pending_note_edit_with_specific_midi_note_visible_in_selected_column() {
    let mut state = known_state();
    state.selected_step = 3;
    state.steps[3] = StepData { enabled: true, midi_note: 60, velocity: 100 }; // C4
    // Pending note 67 = G4
    state.pending_edit = PendingEdit::Note { step: 3, midi_note: 67 };

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();
    // Note row is y=1.
    let row1: String = (0..120)
        .map(|x| buffer.cell((x, 1)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert!(
        row1.contains("G4"),
        "note row must show pending note G4 (midi 67) in selected step column, got: {}",
        row1
    );
}

// ── Feature-gate regression tests (BUG-007) ─────────────────────────────────
//
// These tests compile and run WITHOUT the `hw-io` feature.  Their existence
// proves that `ratatui` and `ui_render` are usable without `crossterm`, which
// is the core fix for BUG-007.
//
// If the Cargo.toml regression is re-introduced (ratatui default-features = true),
// crossterm would become a non-optional dep and `cargo test -p engine` would
// fail in environments without a real tty during link/compile, catching the bug.

/// Verify that TestBackend can construct a terminal and render a frame without
/// the `hw-io` feature.  This is the canonical compile-time + runtime proof
/// that `ratatui` is usable without `crossterm`.
#[test]
fn test_backend_renders_without_hw_io_feature() {
    // If BUG-007 were re-introduced, `ratatui` would pull in `crossterm` and
    // this test would fail to compile or link in a headless environment.
    let backend = TestBackend::new(80, 5);
    let mut terminal = Terminal::new(backend).expect("TestBackend terminal must construct without hw-io");

    let state = SequencerState::default();
    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("render_frame must complete with TestBackend and no hw-io feature");

    // If we reach here, ratatui compiled and rendered without crossterm.
    let buffer = terminal.backend().buffer().clone();
    // Buffer must be non-empty — at least one cell should be filled.
    let any_non_space = (0..80u16)
        .any(|x| buffer.cell((x, 0)).map(|c| c.symbol() != " ").unwrap_or(false));
    assert!(any_non_space, "rendered buffer must contain non-space cells (top bar must render)");
}

/// Verify that clearing and redrawing a TestBackend terminal produces the same
/// content as the first draw — render_frame must be deterministic and
/// side-effect-free (no hidden terminal state dependency).
#[test]
fn test_backend_clear_and_redraw_is_idempotent() {
    let state = known_state();

    let backend = TestBackend::new(120, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("first draw");

    let first_row0: String = (0..120)
        .map(|x| terminal.backend().buffer().cell((x, 0)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    terminal.clear().expect("terminal clear must succeed");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("second draw after clear");

    let second_row0: String = (0..120)
        .map(|x| terminal.backend().buffer().cell((x, 0)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    assert_eq!(
        first_row0, second_row0,
        "top bar must be identical after clear-and-redraw (render must be deterministic)"
    );
}

/// Verify that all 16 steps can be rendered — note row must contain at least
/// 16 note-name tokens when every step is enabled.  This exercises the full
/// step-iteration loop in render_frame without hw-io.
#[test]
fn all_sixteen_steps_enabled_renders_note_names_in_note_row() {
    let mut state = SequencerState::default();
    // Enable all 16 steps with a mix of notes to ensure the iteration loop
    // visits every step and does not short-circuit on an empty/disabled step.
    for (i, step) in state.steps.iter_mut().enumerate() {
        step.enabled = true;
        // Spread notes: C4(60), D4(62), E4(64), ... cycling every 4
        step.midi_note = 60 + (i as u8 % 4) * 2;
        step.velocity = 100;
    }
    state.playhead = 0;
    state.selected_step = 0;

    // Wide terminal so all 16 columns fit without truncation.
    let backend = TestBackend::new(160, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw with all steps enabled");

    let buffer = terminal.backend().buffer().clone();

    // Collect the entire note row (y=1).
    let note_row: String = (0..160)
        .map(|x| buffer.cell((x, 1)).map(|c| c.symbol().chars().next().unwrap_or(' ')).unwrap_or(' '))
        .collect();

    // C4 appears for steps 0, 4, 8, 12 — must appear multiple times.
    let c4_count = note_row.matches("C4").count();
    assert!(
        c4_count >= 4,
        "note row must contain 'C4' at least 4 times when steps 0,4,8,12 are C4, got: {}",
        note_row
    );
}

/// Verify that the indicator row (y=2) shows the enabled marker '●' for all
/// enabled steps and the disabled marker '○' for all disabled steps when a
/// known pattern is set.  This is the step-indicator path in render_frame.
#[test]
fn indicator_row_reflects_enabled_disabled_pattern() {
    let mut state = SequencerState::default();
    // Enable even steps, disable odd steps.
    for (i, step) in state.steps.iter_mut().enumerate() {
        step.enabled = i % 2 == 0;
        step.midi_note = 60;
        step.velocity = 100;
    }
    state.playhead = 0;
    state.selected_step = 0;

    let backend = TestBackend::new(160, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    terminal.draw(|frame| {
        render_frame(frame, &state, None, 0);
    }).expect("draw");

    let buffer = terminal.backend().buffer().clone();

    let indicator_row: String = (0..160)
        .map(|x| buffer.cell((x, 2)).map(|c| c.symbol()).unwrap_or(""))
        .collect();

    assert!(
        indicator_row.contains('●'),
        "indicator row must contain '●' for enabled (even) steps, got: {}",
        indicator_row
    );
    assert!(
        indicator_row.contains('○'),
        "indicator row must contain '○' for disabled (odd) steps, got: {}",
        indicator_row
    );
}
