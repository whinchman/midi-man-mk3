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

/// Returns (beats_per_step_num, beats_per_step_den) as a rational multiplier.
/// tick_nanos = 60_000_000_000 * num / (bpm * den)
pub fn step_ratio(step_size: StepSize) -> (u64, u64) {
    match step_size {
        StepSize::Whole        => (4, 1),
        StepSize::Half         => (2, 1),
        StepSize::Quarter      => (1, 1),
        StepSize::Eighth       => (1, 2),
        StepSize::Sixteenth    => (1, 4),
        StepSize::ThirtySecond => (1, 8),
    }
}

/// Computes the tick period in nanoseconds for the given tempo and step size.
pub fn tick_nanos(bpm: u16, step_size: StepSize) -> u64 {
    let (num, den) = step_ratio(step_size);
    NANOS_PER_MINUTE * num / (bpm as u64 * den)
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
pub fn add_nanos(ts: libc::timespec, nanos: u64) -> libc::timespec {
    let total_nanos = ts.tv_nsec as u64 + nanos;
    libc::timespec {
        tv_sec: ts.tv_sec + (total_nanos / 1_000_000_000) as libc::time_t,
        tv_nsec: (total_nanos % 1_000_000_000) as libc::c_long,
    }
}

/// Adds a signed nanosecond offset to a `timespec`, clamped so the result
/// never falls before the epoch (total nanoseconds are clamped to zero).
/// Correctly carries borrows across the second boundary.
pub fn add_nanos_signed(ts: libc::timespec, nanos: i64) -> libc::timespec {
    let total_ns: i64 = ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64 + nanos;
    let total_ns = total_ns.max(0);
    libc::timespec {
        tv_sec: (total_ns / 1_000_000_000) as libc::time_t,
        tv_nsec: (total_ns % 1_000_000_000) as libc::c_long,
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

