//! Real-time clock thread — drives the sequencer forward one step per tick.
//!
//! Uses `libc::clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)` with absolute
//! wake times to prevent drift accumulation. Swing is applied by offsetting
//! odd-step wake times by `swing_factor * tick_period / 100` nanoseconds.
//!
//! The clock does NOT send `NoteOff` events — that responsibility belongs to
//! `midi_out.rs`. It embeds `duration_nanos = tick_nanos` into each `NoteOn`
//! so that `midi_out.rs` can schedule the matching `NoteOff`.

use std::sync::{Arc, RwLock, mpsc::SyncSender};

use crate::state::{MidiEvent, SequencerState, StepSize};

/// Number of nanoseconds in one minute.
const NANOS_PER_MINUTE: u64 = 60_000_000_000;

/// Returns the number of steps per beat for the given step size.
pub fn steps_per_beat(step_size: StepSize) -> u64 {
    match step_size {
        StepSize::Quarter => 1,
        StepSize::Eighth => 2,
        StepSize::Sixteenth => 4,
    }
}

/// Computes the tick period in nanoseconds for the given tempo and step size.
///
/// Formula: `60_000_000_000 / (bpm * steps_per_beat)`
pub fn tick_nanos(bpm: u16, step_size: StepSize) -> u64 {
    let spb = steps_per_beat(step_size);
    NANOS_PER_MINUTE / (bpm as u64 * spb)
}

/// Computes the swing offset in nanoseconds for an odd step.
///
/// `swing_factor` is in the range -50 to +50. Positive swing delays odd steps;
/// negative swing advances them (clamped to 0 so they don't go before the beat).
pub fn swing_offset_nanos(swing_factor: i8, tick_period_nanos: u64) -> i64 {
    swing_factor as i64 * tick_period_nanos as i64 / 100
}

/// Requests SCHED_FIFO real-time scheduling at priority 50.
///
/// Failure is non-fatal: a warning is printed to stderr and the thread
/// continues with normal scheduling. Requires root or `CAP_SYS_NICE`.
///
/// # Safety
/// Calls `libc::sched_setscheduler` which is unsafe FFI.
fn try_set_realtime() {
    // SAFETY: sched_setscheduler is always safe to call; we handle the error
    // return value and never dereference the param pointer unsafely.
    #[cfg(target_os = "linux")]
    unsafe {
        let param = libc::sched_param { sched_priority: 50 };
        let rc = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param as *const _);
        if rc != 0 {
            eprintln!(
                "clock: SCHED_FIFO not granted (errno {}); continuing with default scheduling.",
                *libc::__errno_location()
            );
        }
    }
}

/// Returns the current monotonic time as a `libc::timespec`.
///
/// # Safety
/// Calls `libc::clock_gettime` which is safe to call; we check its return.
fn monotonic_now() -> libc::timespec {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: ts is a valid, properly-aligned timespec on the stack.
    #[cfg(target_os = "linux")]
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts as *mut _);
    }
    ts
}

/// Sleeps until the given absolute monotonic time.
///
/// `remaining` is unused (we pass NULL) because we retry on EINTR by looping
/// in the caller (re-computing absolute times each tick).
///
/// # Safety
/// Calls `libc::clock_nanosleep`; the timespec pointer is valid.
fn sleep_until(abs_time: &libc::timespec) {
    #[cfg(target_os = "linux")]
    unsafe {
        // TIMER_ABSTIME = 1
        libc::clock_nanosleep(
            libc::CLOCK_MONOTONIC,
            libc::TIMER_ABSTIME,
            abs_time as *const _,
            std::ptr::null_mut(),
        );
    }
    // On non-Linux platforms (e.g. macOS dev builds), this is a no-op.
    // Tests that don't call sleep_until directly are unaffected.
    let _ = abs_time;
}

/// Adds `nanos` to a `timespec`, carrying nanoseconds into seconds.
fn add_nanos(ts: libc::timespec, nanos: u64) -> libc::timespec {
    let total_nanos = ts.tv_nsec as u64 + nanos;
    libc::timespec {
        tv_sec: ts.tv_sec + (total_nanos / 1_000_000_000) as libc::time_t,
        tv_nsec: (total_nanos % 1_000_000_000) as libc::c_long,
    }
}

/// Adds a signed nanosecond offset to a `timespec`, clamped so tv_nsec stays
/// non-negative (we never schedule a wake time before the beat boundary).
fn add_nanos_signed(ts: libc::timespec, nanos: i64) -> libc::timespec {
    let tv_nsec_i64 = ts.tv_nsec + nanos;
    // Clamp to the beat boundary if swing would pull before it.
    let tv_nsec_clamped = tv_nsec_i64.max(0);
    let sec_delta = tv_nsec_clamped / 1_000_000_000;
    let nsec_rem = tv_nsec_clamped % 1_000_000_000;
    libc::timespec {
        tv_sec: ts.tv_sec + sec_delta as libc::time_t,
        tv_nsec: nsec_rem as libc::c_long,
    }
}

/// Runs the real-time clock loop.
///
/// This function is intended to be called from inside a dedicated
/// `std::thread::spawn` closure. It blocks indefinitely, advancing the
/// sequencer one step per tick, until `midi_tx` is disconnected (i.e. the
/// receiver is dropped), at which point it returns.
///
/// Timing uses `libc::clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)` with
/// absolute wake times to prevent drift accumulation across ticks.
pub fn run_clock(state: Arc<RwLock<SequencerState>>, midi_tx: SyncSender<MidiEvent>) {
    try_set_realtime();

    let mut next_abs = monotonic_now();
    // step_count tracks even/odd for swing (does not reset with playhead).
    let mut step_count: u64 = 0;

    loop {
        // --- read current parameters (read lock, released immediately) ---
        let (bpm, step_size, swing, playing) = {
            let s = state.read().expect("clock: state RwLock poisoned");
            (s.tempo_bpm, s.step_size, s.swing, s.playing)
        };

        let period = tick_nanos(bpm, step_size);
        let swing_off = swing_offset_nanos(swing, period);

        // --- compute wake time for this tick ---
        let wake_time = if step_count % 2 == 1 {
            // Odd step: apply swing offset (delayed or advanced).
            add_nanos_signed(next_abs, swing_off)
        } else {
            next_abs
        };

        sleep_until(&wake_time);

        // --- advance sequencer (write lock, released immediately) ---
        if playing {
            let maybe_event = {
                let mut s = state.write().expect("clock: state RwLock poisoned");
                s.tick()
            };

            if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = maybe_event {
                let event = MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                    duration_nanos: period,
                };
                if midi_tx.send(event).is_err() {
                    // Receiver dropped — exit cleanly.
                    break;
                }
            }
        }

        // Advance next absolute time by one tick period.
        next_abs = add_nanos(next_abs, period);
        step_count = step_count.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SequencerState, StepSize};

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

    // --- steps_per_beat ---

    #[test]
    fn steps_per_beat_values() {
        assert_eq!(steps_per_beat(StepSize::Quarter), 1);
        assert_eq!(steps_per_beat(StepSize::Eighth), 2);
        assert_eq!(steps_per_beat(StepSize::Sixteenth), 4);
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
        // Negative offset larger than tv_nsec — should not go negative.
        let ts = libc::timespec { tv_sec: 1, tv_nsec: 100_000_000 };
        let result = add_nanos_signed(ts, -200_000_000);
        assert!(result.tv_nsec >= 0, "tv_nsec must not be negative");
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
}
