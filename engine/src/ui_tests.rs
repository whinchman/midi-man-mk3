//! Unit tests for the terminal UI render logic.
//!
//! These tests use `ratatui::backend::TestBackend` so they run under
//! `cargo test -p engine` without the `hw-io` feature (no real terminal
//! required).  The render logic lives in `ui_render::render_frame`.

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::input::OverlayMode;
    use crate::music_theory::{Key, Mode};
    use crate::state::{PendingEdit, SequencerState, StepData, StepSize};
    use crate::ui_render::render_frame;

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
}
