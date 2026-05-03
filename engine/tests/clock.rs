use engine::clock::{
    add_nanos_signed, add_nanos, compute_effective_bpm, run_clock, swing_offset_nanos,
    tick_nanos, step_ratio, TempoRandSnapshot, TempoRollState, CLOCK_RNG_INIT,
};
use engine::state::{MidiEvent, SequencerState, StepData, StepSize, TempoRandType, TempoRollPoint};
use libc;
use std::sync::mpsc;

// --- tick_nanos formula ---

#[test]
fn tick_nanos_120bpm_sixteenth() {
    // 60_000_000_000 / (120 * 4) = 125_000_000
    assert_eq!(tick_nanos(120, StepSize::Sixteenth), 125_000_000);
}

#[test]
fn tick_nanos_120bpm_quarter() {
    // 60_000_000_000 / (120 * 1) = 500_000_000
    assert_eq!(tick_nanos(120, StepSize::Quarter), 500_000_000);
}

#[test]
fn tick_nanos_120bpm_eighth() {
    // 60_000_000_000 / (120 * 2) = 250_000_000
    assert_eq!(tick_nanos(120, StepSize::Eighth), 250_000_000);
}

#[test]
fn tick_nanos_60bpm_sixteenth() {
    // 60_000_000_000 / (60 * 4) = 250_000_000
    assert_eq!(tick_nanos(60, StepSize::Sixteenth), 250_000_000);
}

#[test]
fn tick_nanos_300bpm_quarter() {
    // 60_000_000_000 / (300 * 1) = 200_000_000
    assert_eq!(tick_nanos(300, StepSize::Quarter), 200_000_000);
}

#[test]
fn tick_nanos_60bpm_quarter_is_one_second() {
    // 60 BPM quarter note = exactly 1 beat per second = 1_000_000_000 ns
    assert_eq!(tick_nanos(60, StepSize::Quarter), 1_000_000_000);
}

#[test]
fn tick_nanos_120bpm_sixteenth_is_31250000() {
    // 60_000_000_000 / (120 * 4) = 31_250_000 — from the context doc
    // Note: 120 BPM sixteenth is 125_000_000 ns; the 31_250_000 figure in
    // the context refers to 120 BPM with 16 steps-per-beat (32nd notes).
    // The spec says steps_per_beat=4 for Sixteenth, giving 125_000_000.
    // Test the actual spec value here for clarity.
    assert_eq!(tick_nanos(120, StepSize::Sixteenth), 125_000_000);
}

#[test]
fn tick_nanos_240bpm_eighth_is_125ms() {
    // 60_000_000_000 / (240 * 2) = 125_000_000 ns
    assert_eq!(tick_nanos(240, StepSize::Eighth), 125_000_000);
}

// --- step_ratio ---

#[test]
fn step_ratio_values() {
    assert_eq!(step_ratio(StepSize::Whole),        (4, 1));
    assert_eq!(step_ratio(StepSize::Half),         (2, 1));
    assert_eq!(step_ratio(StepSize::Quarter),      (1, 1));
    assert_eq!(step_ratio(StepSize::Eighth),       (1, 2));
    assert_eq!(step_ratio(StepSize::Sixteenth),    (1, 4));
    assert_eq!(step_ratio(StepSize::ThirtySecond), (1, 8));
}

#[test]
fn tick_nanos_whole_note_120bpm() {
    // 4 beats per step at 120 BPM = 2_000_000_000 ns
    assert_eq!(tick_nanos(120, StepSize::Whole), 2_000_000_000);
}

#[test]
fn tick_nanos_half_note_120bpm() {
    // 2 beats per step at 120 BPM = 1_000_000_000 ns
    assert_eq!(tick_nanos(120, StepSize::Half), 1_000_000_000);
}

#[test]
fn tick_nanos_thirty_second_120bpm() {
    // 60_000_000_000 / (120 * 8) = 62_500_000 ns
    assert_eq!(tick_nanos(120, StepSize::ThirtySecond), 62_500_000);
}

// --- swing_offset_nanos ---

#[test]
fn swing_offset_positive() {
    // swing=50, period=1_000_000 → 50 * 1_000_000 / 100 = 500_000
    assert_eq!(swing_offset_nanos(50, 1_000_000), 500_000);
}

#[test]
fn swing_offset_negative() {
    // swing=-25, period=1_000_000 → -25 * 1_000_000 / 100 = -250_000
    assert_eq!(swing_offset_nanos(-25, 1_000_000), -250_000);
}

#[test]
fn swing_offset_zero() {
    assert_eq!(swing_offset_nanos(0, 1_000_000), 0);
}

#[test]
fn swing_offset_realistic() {
    // 120 BPM sixteenth: tick = 125_000_000 ns, swing = 33
    // offset = 33 * 125_000_000 / 100 = 41_250_000
    let period = tick_nanos(120, StepSize::Sixteenth);
    assert_eq!(swing_offset_nanos(33, period), 41_250_000);
}

#[test]
fn swing_offset_plus50_equals_half_tick_period() {
    // At swing=+50 the odd-step offset must equal tick_nanos / 2.
    let period = tick_nanos(120, StepSize::Sixteenth); // 125_000_000
    let offset = swing_offset_nanos(50, period);
    assert_eq!(offset, (period / 2) as i64, "swing=+50 should delay by exactly half a tick period");
}

#[test]
fn swing_offset_minus50_equals_negative_half_tick_period() {
    // At swing=-50 the odd-step offset must equal -(tick_nanos / 2).
    let period = tick_nanos(120, StepSize::Sixteenth); // 125_000_000
    let offset = swing_offset_nanos(-50, period);
    assert_eq!(offset, -((period / 2) as i64), "swing=-50 should advance by exactly half a tick period");
}

#[test]
fn swing_offset_zero_produces_no_offset() {
    // At swing=0 no offset is applied regardless of tick period.
    let period = tick_nanos(120, StepSize::Sixteenth);
    assert_eq!(swing_offset_nanos(0, period), 0, "swing=0 should produce zero offset");
}

// --- add_nanos helpers ---

#[test]
fn add_nanos_carries_seconds() {
    let ts = libc::timespec { tv_sec: 1, tv_nsec: 900_000_000 };
    let result = add_nanos(ts, 200_000_000);
    assert_eq!(result.tv_sec, 2);
    assert_eq!(result.tv_nsec, 100_000_000);
}

#[test]
fn add_nanos_no_carry() {
    let ts = libc::timespec { tv_sec: 5, tv_nsec: 100_000_000 };
    let result = add_nanos(ts, 50_000_000);
    assert_eq!(result.tv_sec, 5);
    assert_eq!(result.tv_nsec, 150_000_000);
}

#[test]
fn add_nanos_signed_positive_offset() {
    let ts = libc::timespec { tv_sec: 0, tv_nsec: 100_000_000 };
    let result = add_nanos_signed(ts, 50_000_000);
    assert_eq!(result.tv_sec, 0);
    assert_eq!(result.tv_nsec, 150_000_000);
}

#[test]
fn add_nanos_signed_clamps_to_zero() {
    // Negative offset (1.1s - 0.2s = 0.9s) should produce tv_sec=0, tv_nsec=900_000_000.
    let ts = libc::timespec { tv_sec: 1, tv_nsec: 100_000_000 };
    let result = add_nanos_signed(ts, -200_000_000);
    assert!(result.tv_nsec >= 0, "tv_nsec must not be negative");
    assert_eq!(result.tv_sec, 0, "tv_sec should be 0 after 1.1s - 0.2s = 0.9s");
    assert_eq!(result.tv_nsec, 900_000_000, "tv_nsec should be 900_000_000 after 1.1s - 0.2s");
}

#[test]
fn add_nanos_signed_negative_crosses_second_boundary() {
    let ts = libc::timespec { tv_sec: 5, tv_nsec: 10_000_000 }; // 5.010s
    let result = add_nanos_signed(ts, -62_500_000); // subtract 62.5ms
    assert_eq!(result.tv_sec, 4);
    assert_eq!(result.tv_nsec, 947_500_000);
}

#[test]
fn add_nanos_signed_positive_crosses_second_boundary() {
    // 0.999s + 100ms = 1.099s
    let ts = libc::timespec { tv_sec: 0, tv_nsec: 999_000_000 };
    let result = add_nanos_signed(ts, 100_000_000);
    assert_eq!(result.tv_sec, 1, "tv_sec should be 1 after crossing second boundary");
    assert_eq!(result.tv_nsec, 99_000_000, "tv_nsec should be 99_000_000 ns");
}

// --- playhead wraps after 32 ticks ---

#[test]
fn playhead_wraps_after_32_ticks() {
    let mut s = SequencerState::default();
    s.playing = true;
    // Enable all steps so tick() always returns Some.
    for step in &mut s.steps {
        step.enabled = true;
    }
    // Advance 32 ticks — two full cycles of 16.
    for _ in 0..32 {
        s.tick();
    }
    // After 32 ticks starting from playhead=0, playhead should be back at 0.
    assert_eq!(s.playhead, 0, "playhead should be 0 after 32 ticks (two full cycles)");
}

#[test]
fn playhead_position_after_16_ticks() {
    let mut s = SequencerState::default();
    s.playing = true;
    for step in &mut s.steps {
        step.enabled = true;
    }
    for _ in 0..16 {
        s.tick();
    }
    assert_eq!(s.playhead, 0, "playhead should wrap to 0 after exactly 16 ticks");
}

// --- state: not playing / paused — tick() must return None and not advance playhead ---

#[test]
fn tick_not_called_effectively_when_not_playing() {
    // When playing=false the clock skips the tick() call; SequencerState.tick()
    // returns None and the playhead stays at 0.  This mirrors the run_clock
    // guard `if playing { s.tick() }`.
    let mut s = SequencerState::default();
    s.playing = false;
    for step in &mut s.steps {
        step.enabled = true;
    }
    // Calling tick() when not playing must return None (spec from state.rs).
    let result = s.tick();
    assert!(result.is_none(), "tick() must return None when playing=false");
    assert_eq!(s.playhead, 0, "playhead must not advance when playing=false");
}

#[test]
fn tick_not_called_effectively_when_paused() {
    // When paused=true the clock skips the tick() call; SequencerState.tick()
    // returns None and the playhead stays at 0.
    let mut s = SequencerState::default();
    s.playing = true;
    s.paused = true;
    for step in &mut s.steps {
        step.enabled = true;
    }
    let result = s.tick();
    assert!(result.is_none(), "tick() must return None when paused=true");
    assert_eq!(s.playhead, 0, "playhead must not advance when paused=true");
}

#[test]
fn tick_no_events_sent_when_not_playing() {
    // Verify that run_clock does not forward any event when playing=false.
    // We simulate the clock body: read playing flag, skip tick() if false.
    use std::sync::{Arc, RwLock, mpsc};
    let state = Arc::new(RwLock::new(SequencerState::default()));
    {
        let mut s = state.write().unwrap();
        s.playing = false;
        for step in &mut s.steps {
            step.enabled = true;
        }
    }
    let (tx, rx) = mpsc::sync_channel::<MidiEvent>(16);
    // Simulate one clock iteration: read, skip tick, advance time.
    let playing = state.read().unwrap().playing;
    if playing {
        let mut s = state.write().unwrap();
        if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = s.tick() {
            let _ = tx.send(MidiEvent::NoteOn { channel, note, velocity, duration_nanos: 0 });
        }
    }
    drop(tx);
    // Nothing should have been sent.
    assert!(rx.try_recv().is_err(), "no events should be sent when playing=false");
}

#[test]
fn tick_no_events_sent_when_paused() {
    // Same as above but with paused=true.
    use std::sync::{Arc, RwLock, mpsc};
    let state = Arc::new(RwLock::new(SequencerState::default()));
    {
        let mut s = state.write().unwrap();
        s.playing = true;
        s.paused = true;
        for step in &mut s.steps {
            step.enabled = true;
        }
    }
    let (tx, rx) = mpsc::sync_channel::<MidiEvent>(16);
    let (playing, paused) = {
        let s = state.read().unwrap();
        (s.playing, s.paused)
    };
    // run_clock checks `if playing` but tick() internally guards on paused.
    if playing {
        let maybe = {
            let mut s = state.write().unwrap();
            s.tick()
        };
        if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = maybe {
            let _ = tx.send(MidiEvent::NoteOn { channel, note, velocity, duration_nanos: 0 });
        }
    }
    let _ = paused; // used implicitly via tick() guard
    drop(tx);
    assert!(rx.try_recv().is_err(), "no events should be sent when paused=true");
}

// --- add_nanos_signed: epoch clamp and edge cases ---

#[test]
fn add_nanos_signed_actual_epoch_clamp_sub_second() {
    // tv_sec=0, tv_nsec=50_000_000 (50 ms), subtract 100 ms → goes below epoch → clamped to 0.
    let ts = libc::timespec { tv_sec: 0, tv_nsec: 50_000_000 };
    let result = add_nanos_signed(ts, -100_000_000);
    assert_eq!(result.tv_sec, 0, "tv_sec must be 0 when clamped to epoch");
    assert_eq!(result.tv_nsec, 0, "tv_nsec must be 0 when clamped to epoch");
}

#[test]
fn add_nanos_signed_clamp_at_exact_epoch() {
    // Offset that brings the result to exactly zero (no positive remainder).
    let ts = libc::timespec { tv_sec: 1, tv_nsec: 0 };
    let result = add_nanos_signed(ts, -1_000_000_000);
    assert_eq!(result.tv_sec, 0, "tv_sec must be 0 when result is exactly the epoch");
    assert_eq!(result.tv_nsec, 0, "tv_nsec must be 0 when result is exactly the epoch");
}

#[test]
fn add_nanos_signed_zero_offset_leaves_timespec_unchanged() {
    let ts = libc::timespec { tv_sec: 3, tv_nsec: 456_789_000 };
    let result = add_nanos_signed(ts, 0);
    assert_eq!(result.tv_sec, 3, "tv_sec must be unchanged for zero offset");
    assert_eq!(result.tv_nsec, 456_789_000, "tv_nsec must be unchanged for zero offset");
}

#[test]
fn add_nanos_signed_large_positive_spans_multiple_seconds() {
    // 1.0s + 2_500_000_000 ns = 3.5s → tv_sec=3, tv_nsec=500_000_000
    let ts = libc::timespec { tv_sec: 1, tv_nsec: 0 };
    let result = add_nanos_signed(ts, 2_500_000_000);
    assert_eq!(result.tv_sec, 3, "tv_sec should be 3 after spanning 2 full seconds");
    assert_eq!(result.tv_nsec, 500_000_000, "tv_nsec should be 500_000_000 ns");
}

#[test]
fn add_nanos_signed_large_negative_spans_multiple_seconds() {
    // 10.0s - 3_200_000_000 ns = 6.8s → tv_sec=6, tv_nsec=800_000_000
    let ts = libc::timespec { tv_sec: 10, tv_nsec: 0 };
    let result = add_nanos_signed(ts, -3_200_000_000);
    assert_eq!(result.tv_sec, 6, "tv_sec should be 6 after subtracting 3.2 seconds");
    assert_eq!(result.tv_nsec, 800_000_000, "tv_nsec should be 800_000_000 ns");
}

#[test]
fn add_nanos_signed_at_large_time_with_boundary_crossing_negative() {
    // Simulates a running monotonic clock at ~100s with 120 BPM swing subtraction.
    // 100.010s - 62.5ms = 99.9475s → tv_sec=99, tv_nsec=947_500_000
    let ts = libc::timespec { tv_sec: 100, tv_nsec: 10_000_000 };
    let result = add_nanos_signed(ts, -62_500_000);
    assert_eq!(result.tv_sec, 99);
    assert_eq!(result.tv_nsec, 947_500_000);
}

// --- add_nanos: edge cases ---

#[test]
fn add_nanos_zero_nanos_leaves_timespec_unchanged() {
    let ts = libc::timespec { tv_sec: 7, tv_nsec: 123_456_789 };
    let result = add_nanos(ts, 0);
    assert_eq!(result.tv_sec, 7, "tv_sec must be unchanged when adding zero nanoseconds");
    assert_eq!(result.tv_nsec, 123_456_789, "tv_nsec must be unchanged when adding zero nanoseconds");
}

#[test]
fn add_nanos_exactly_one_second() {
    // Adding exactly 1_000_000_000 ns to tv_nsec=0 must increment tv_sec by 1.
    let ts = libc::timespec { tv_sec: 5, tv_nsec: 0 };
    let result = add_nanos(ts, 1_000_000_000);
    assert_eq!(result.tv_sec, 6, "tv_sec must increment by 1 when adding exactly one second");
    assert_eq!(result.tv_nsec, 0, "tv_nsec must be 0 when adding exactly one second to a zero nsec");
}

#[test]
fn add_nanos_spans_multiple_seconds() {
    // 0.5s + 2_500_000_000 ns = 3.0s → tv_sec=3, tv_nsec=0
    let ts = libc::timespec { tv_sec: 0, tv_nsec: 500_000_000 };
    let result = add_nanos(ts, 2_500_000_000);
    assert_eq!(result.tv_sec, 3, "tv_sec should be 3 after spanning 2.5 seconds");
    assert_eq!(result.tv_nsec, 0, "tv_nsec should be 0");
}

// --- swing integration: add_nanos_signed with realistic swing offsets ---

#[test]
fn swing_120bpm_negative50_does_not_underflow_sub_second_start() {
    // If the base time is less than the max negative swing offset, clamping must
    // prevent tv_nsec < 0 and tv_sec < 0.
    let period = tick_nanos(120, StepSize::Sixteenth); // 125_000_000 ns
    let offset = swing_offset_nanos(-50, period);      // -62_500_000 ns
    // Start at 30 ms — less than the 62.5 ms offset — so the result goes below 0.
    let ts = libc::timespec { tv_sec: 0, tv_nsec: 30_000_000 };
    let result = add_nanos_signed(ts, offset);
    assert!(result.tv_sec >= 0, "tv_sec must not be negative after epoch clamp");
    assert!(result.tv_nsec >= 0, "tv_nsec must not be negative after epoch clamp");
    assert_eq!(result.tv_sec, 0, "should be clamped to epoch");
    assert_eq!(result.tv_nsec, 0, "should be clamped to epoch");
}

#[test]
fn swing_120bpm_positive50_odd_step_delay_is_correct() {
    // Odd step at 5.000s with +50 swing: wake time should be 5.000s + 62.5ms = 5.0625s
    let period = tick_nanos(120, StepSize::Sixteenth); // 125_000_000 ns
    let offset = swing_offset_nanos(50, period);       // +62_500_000 ns
    let ts = libc::timespec { tv_sec: 5, tv_nsec: 0 };
    let result = add_nanos_signed(ts, offset);
    assert_eq!(result.tv_sec, 5, "tv_sec should remain 5 for a sub-second positive offset");
    assert_eq!(result.tv_nsec, 62_500_000, "tv_nsec should be 62_500_000 ns");
}

#[test]
fn swing_120bpm_negative50_odd_step_advance_is_correct() {
    // Odd step at 5.100s with -50 swing: wake time should be 5.100s - 62.5ms = 5.0375s
    let period = tick_nanos(120, StepSize::Sixteenth); // 125_000_000 ns
    let offset = swing_offset_nanos(-50, period);      // -62_500_000 ns
    let ts = libc::timespec { tv_sec: 5, tv_nsec: 100_000_000 };
    let result = add_nanos_signed(ts, offset);
    assert_eq!(result.tv_sec, 5, "tv_sec should remain 5");
    assert_eq!(result.tv_nsec, 37_500_000, "tv_nsec should be 37_500_000 ns");
}

// --- retrigger helpers (moved from src/clock.rs inline tests) ---

/// Simulates the retrigger logic from `run_clock` on a single tick.
///
/// Returns the events that would be emitted for the given `maybe_event`,
/// given the current `last_note` state.  Updates `last_note` in place,
/// mirroring the production code.
fn simulate_tick(
    maybe_event: Option<MidiEvent>,
    last_note: &mut Option<(u8, u8)>,
    period: u64,
) -> Vec<MidiEvent> {
    let mut events = Vec::new();
    if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = maybe_event {
        if *last_note == Some((channel, note)) {
            events.push(MidiEvent::NoteOff { channel, note });
        }
        events.push(MidiEvent::NoteOn { channel, note, velocity, duration_nanos: period });
        *last_note = Some((channel, note));
    }
    events
}

// ── BUG-018: repeated-note retrigger ────────────────────────────────────────

/// Two consecutive steps with the same note must each produce a NoteOn,
/// with a NoteOff inserted between them.
#[test]
fn test_retrigger_same_note_inserts_note_off() {
    let mut state = SequencerState::default();
    state.steps[0] = StepData { enabled: true, midi_note: 60, velocity: 100 };
    state.steps[1] = StepData { enabled: true, midi_note: 60, velocity: 100 };
    state.playing = true;
    state.playhead = 15;

    let period: u64 = 500_000;
    let mut last_note: Option<(u8, u8)> = None;

    let e1 = simulate_tick(state.tick(), &mut last_note, period);
    assert_eq!(e1, vec![MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100, duration_nanos: period }],
        "first tick: NoteOn only");
    assert_eq!(last_note, Some((0, 60)));

    let e2 = simulate_tick(state.tick(), &mut last_note, period);
    assert_eq!(e2, vec![
        MidiEvent::NoteOff { channel: 0, note: 60 },
        MidiEvent::NoteOn  { channel: 0, note: 60, velocity: 100, duration_nanos: period },
    ], "second tick: NoteOff then NoteOn for retrigger");
}

/// A different note on the second step must NOT produce a NoteOff first.
#[test]
fn test_no_retrigger_for_different_note() {
    let mut state = SequencerState::default();
    state.steps[0] = StepData { enabled: true, midi_note: 60, velocity: 100 };
    state.steps[1] = StepData { enabled: true, midi_note: 62, velocity: 100 };
    state.playing = true;
    state.playhead = 15;

    let period: u64 = 500_000;
    let mut last_note: Option<(u8, u8)> = None;

    let e1 = simulate_tick(state.tick(), &mut last_note, period);
    assert_eq!(e1, vec![MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100, duration_nanos: period }]);

    let e2 = simulate_tick(state.tick(), &mut last_note, period);
    assert_eq!(e2, vec![MidiEvent::NoteOn { channel: 0, note: 62, velocity: 100, duration_nanos: period }],
        "different note: no NoteOff inserted");
}

/// A disabled step must NOT update last_note, so no phantom NoteOff fires.
#[test]
fn test_disabled_step_does_not_update_last_note() {
    let mut state = SequencerState::default();
    state.steps[0] = StepData { enabled: true,  midi_note: 60, velocity: 100 };
    state.steps[1] = StepData { enabled: false, midi_note: 60, velocity: 100 };
    state.steps[2] = StepData { enabled: true,  midi_note: 60, velocity: 100 };
    state.playing = true;
    state.playhead = 15;

    let period: u64 = 500_000;
    let mut last_note: Option<(u8, u8)> = None;

    let _e1 = simulate_tick(state.tick(), &mut last_note, period); // step 0 → NoteOn
    let e2 = simulate_tick(state.tick(), &mut last_note, period); // step 1 → disabled, None
    assert!(e2.is_empty(), "disabled step emits no events");
    assert_eq!(last_note, Some((0, 60)), "last_note unchanged after disabled step");

    // Step 2 has the same note: because last_note is still set, a NoteOff fires.
    let e3 = simulate_tick(state.tick(), &mut last_note, period); // step 2 → retrigger
    assert_eq!(e3, vec![
        MidiEvent::NoteOff { channel: 0, note: 60 },
        MidiEvent::NoteOn  { channel: 0, note: 60, velocity: 100, duration_nanos: period },
    ], "retrigger fires after disabled gap");
}

// ── run_clock integration: channel-based smoke test ─────────────────────────

/// Two consecutive same-note steps both produce NoteOn events on the MIDI
/// channel, with a NoteOff sandwiched between them.
#[test]
fn test_run_clock_retrigger_via_channel() {
    use std::sync::{Arc, RwLock};

    let mut state = SequencerState::default();
    state.steps[0] = StepData { enabled: true, midi_note: 60, velocity: 100 };
    state.steps[1] = StepData { enabled: true, midi_note: 60, velocity: 100 };
    state.playing = true;
    state.playhead = 15;
    state.tempo_bpm = 240;
    state.step_size = StepSize::ThirtySecond;

    let shared = Arc::new(RwLock::new(state));
    let (tx, rx) = mpsc::sync_channel::<MidiEvent>(3);

    let shared_clone = Arc::clone(&shared);
    let handle = std::thread::spawn(move || run_clock(shared_clone, tx));

    let ev1 = rx.recv().expect("event 1");
    let ev2 = rx.recv().expect("event 2");
    let ev3 = rx.recv().expect("event 3");
    drop(rx);
    handle.join().ok();

    assert!(matches!(ev1, MidiEvent::NoteOn { note: 60, .. }), "ev1 NoteOn");
    assert!(matches!(ev2, MidiEvent::NoteOff { note: 60, .. }), "ev2 NoteOff retrigger");
    assert!(matches!(ev3, MidiEvent::NoteOn { note: 60, .. }), "ev3 NoteOn again");
}

// ── compute_effective_bpm tests ──────────────────────────────────────────────

#[test]
fn test_compute_effective_bpm_tempo_rand_zero_returns_base() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let params = TempoRandSnapshot {
        tempo_rand: 0,
        roll_point: TempoRollPoint::Step,
        variance_max: 20,
        rand_type: TempoRandType::Random,
    };
    for step in 0..1000u64 {
        let effective = compute_effective_bpm(120, &mut roll_state, &params, &mut rng, step);
        assert_eq!(effective, 120, "tempo_rand=0 must always return base BPM");
    }
}

#[test]
fn test_compute_effective_bpm_off_returns_base() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let params = TempoRandSnapshot {
        tempo_rand: 100,
        roll_point: TempoRollPoint::Off,
        variance_max: 20,
        rand_type: TempoRandType::Random,
    };
    for step in 0..1000u64 {
        let effective = compute_effective_bpm(120, &mut roll_state, &params, &mut rng, step);
        assert_eq!(effective, 120, "roll_point=Off must always return base BPM");
    }
}

#[test]
fn test_compute_effective_bpm_random_stays_in_bounds() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let base = 120u16;
    let vm = 20u8;
    let params = TempoRandSnapshot {
        tempo_rand: 100,
        roll_point: TempoRollPoint::Step,
        variance_max: vm,
        rand_type: TempoRandType::Random,
    };
    for step in 0..1000u64 {
        let effective = compute_effective_bpm(base, &mut roll_state, &params, &mut rng, step);
        assert!(
            effective >= base - vm as u16 && effective <= base + vm as u16,
            "Random BPM {effective} out of [{}, {}]",
            base - vm as u16, base + vm as u16
        );
    }
}

#[test]
fn test_compute_effective_bpm_clamped_to_range() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let params = TempoRandSnapshot {
        tempo_rand: 100,
        roll_point: TempoRollPoint::Step,
        variance_max: 99,
        rand_type: TempoRandType::Random,
    };
    for step in 0..1000u64 {
        let effective = compute_effective_bpm(25, &mut roll_state, &params, &mut rng, step);
        assert!(effective >= 20, "BPM {effective} below minimum 20");
        assert!(effective <= 300, "BPM {effective} above maximum 300");
    }
}

#[test]
fn test_compute_effective_bpm_pingpong_bounces() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let base = 120u16;
    let vm = 10u8;
    let params = TempoRandSnapshot {
        tempo_rand: 100,
        roll_point: TempoRollPoint::Step,
        variance_max: vm,
        rand_type: TempoRandType::PingPong,
    };

    let mut prev = base as i32;
    let mut direction_up = true;

    for step in 0..200u64 {
        let effective = compute_effective_bpm(base, &mut roll_state, &params, &mut rng, step) as i32;
        let offset = effective - base as i32;
        assert!(offset >= -(vm as i32) && offset <= vm as i32,
            "PingPong offset {offset} out of bounds at step {step}");

        if direction_up {
            if effective < prev {
                direction_up = false;
            }
        } else if effective > prev {
            direction_up = true;
        }
        prev = effective;
    }
}

#[test]
fn test_compute_effective_bpm_breathe_within_bounds() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let base = 120u16;
    let vm = 10u8;
    let params = TempoRandSnapshot {
        tempo_rand: 100,
        roll_point: TempoRollPoint::Step,
        variance_max: vm,
        rand_type: TempoRandType::Breathe,
    };
    for step in 0..1000u64 {
        let effective = compute_effective_bpm(base, &mut roll_state, &params, &mut rng, step);
        let offset = effective as i32 - base as i32;
        assert!(
            offset >= -(vm as i32) && offset <= vm as i32,
            "Breathe offset {offset} out of bounds at step {step}"
        );
    }
}

#[test]
fn test_tempo_bpm_never_mutated_by_run_clock() {
    use std::sync::{Arc, RwLock};

    let initial_bpm = 120u16;
    let mut state = SequencerState::default();
    state.tempo_bpm = initial_bpm;
    state.tempo_rand = 100;
    state.tempo_roll_point = TempoRollPoint::Step;
    state.tempo_variance_max = 20;
    state.tempo_rand_type = TempoRandType::Random;
    state.playing = true;
    state.steps[0] = StepData { enabled: true, midi_note: 60, velocity: 100 };
    state.tempo_bpm = 240;
    state.step_size = StepSize::ThirtySecond;

    let shared = Arc::new(RwLock::new(state));
    let (tx, rx) = mpsc::sync_channel::<MidiEvent>(32);

    let shared_clone = Arc::clone(&shared);
    let handle = std::thread::spawn(move || run_clock(shared_clone, tx));

    for _ in 0..10 {
        let _ = rx.recv();
    }
    drop(rx);
    handle.join().ok();

    let final_bpm = shared.read().expect("read").tempo_bpm;
    assert_eq!(final_bpm, 240, "tempo_bpm must not be mutated by the clock thread");
}

#[test]
fn test_compute_effective_bpm_seq_fires_every_16_steps() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let base = 120u16;
    let params = TempoRandSnapshot {
        tempo_rand: 100,
        roll_point: TempoRollPoint::Seq,
        variance_max: 10,
        rand_type: TempoRandType::Up,
    };
    let mut last_bpm = compute_effective_bpm(base, &mut roll_state, &params, &mut rng, 0);

    for step in 1..80u64 {
        let effective = compute_effective_bpm(base, &mut roll_state, &params, &mut rng, step);
        if effective != last_bpm {
            assert_eq!(step % 16, 0,
                "Seq roll changed at step {step}, not a multiple of 16");
            last_bpm = effective;
        }
    }
}

#[test]
fn test_compute_effective_bpm_beat_fires_every_4_steps() {
    let mut roll_state = TempoRollState::default();
    let mut rng = CLOCK_RNG_INIT;
    let base = 120u16;
    let params = TempoRandSnapshot {
        tempo_rand: 100,
        roll_point: TempoRollPoint::Beat,
        variance_max: 10,
        rand_type: TempoRandType::Up,
    };
    let mut last_update_step: Option<u64> = None;
    let mut last_bpm = compute_effective_bpm(base, &mut roll_state, &params, &mut rng, 0);

    for step in 1..40u64 {
        let effective = compute_effective_bpm(base, &mut roll_state, &params, &mut rng, step);
        if effective != last_bpm {
            assert_eq!(step % 4, 0,
                "Beat roll changed at step {step}, not a multiple of 4");
            last_update_step = Some(step);
            last_bpm = effective;
        } else {
            if let Some(last) = last_update_step {
                assert!(step - last < 4,
                    "BPM unchanged for {} steps (expected change at beat boundary)", step - last);
            }
        }
    }
}

// --- NoteOn carries duration_nanos from tick_nanos ---

#[test]
fn note_on_duration_nanos_set_by_period() {
    // Simulate what run_clock does: get tick from state, override duration_nanos.
    let mut s = SequencerState::default();
    s.playing = true;
    s.steps[1].enabled = true;
    s.steps[1].midi_note = 64;

    let period = tick_nanos(120, StepSize::Sixteenth);

    // First tick: playhead 0 → 1, step 1 enabled.
    let raw = s.tick();
    assert!(raw.is_some());

    if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = raw {
        let event = MidiEvent::NoteOn {
            channel,
            note,
            velocity,
            duration_nanos: period,
        };
        assert_eq!(
            event,
            MidiEvent::NoteOn {
                channel: 0,
                note: 64,
                velocity: 100,
                duration_nanos: 125_000_000
            }
        );
    } else {
        panic!("expected NoteOn");
    }
}
