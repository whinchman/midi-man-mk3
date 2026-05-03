//! Real-time clock thread — drives the sequencer forward one step per tick.
//!
//! Uses `libc::clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME)` with absolute
//! wake times to prevent drift accumulation. Swing is applied by offsetting
//! odd-step wake times by `swing_factor * tick_period / 100` nanoseconds.
//!
//! The clock sends a `NoteOff` immediately before a `NoteOn` only when the
//! same (channel, note) pair repeats on consecutive steps (retrigger). All
//! other `NoteOff` scheduling is delegated to `midi_out.rs`, which uses
//! `duration_nanos` embedded in each `NoteOn`.

use std::sync::{Arc, RwLock, mpsc::SyncSender};

use crate::state::{MidiEvent, SequencerState, StepSize, TempoRandType, TempoRollPoint};

/// Number of nanoseconds in one minute.
const NANOS_PER_MINUTE: u64 = 60_000_000_000;

/// Minimum allowed effective BPM.
const BPM_MIN: u16 = 20;
/// Maximum allowed effective BPM.
const BPM_MAX: u16 = 300;

/// Initial seed for the clock-local Xorshift64 RNG (separate from `SequencerState::rng_seed`).
pub const CLOCK_RNG_INIT: u64 = 0xA24B_AED4_963D_37C5;

// ── Clock-local tempo jitter types ──────────────────────────────────────────

/// Clock-local tempo jitter state.
///
/// Not stored in `SequencerState` — no lock needed.
pub struct TempoRollState {
    /// Step counter used for phase-based roll points (Beat, Seq).
    phase: u64,
    /// Current signed direction for PingPong (+1 or -1).
    direction: i8,
    /// Last computed BPM offset (carried between steps for smooth curves).
    current_offset: i16,
}

impl Default for TempoRollState {
    fn default() -> Self {
        Self { phase: 0, direction: 1, current_offset: 0 }
    }
}

/// Snapshot of tempo randomness params read from `SequencerState` under a read lock.
pub struct TempoRandSnapshot {
    /// Probability (0–100) that the tempo randomness roll fires.
    pub tempo_rand: u8,
    /// When the tempo randomness roll fires.
    pub roll_point: TempoRollPoint,
    /// Maximum BPM variance applied to the base tempo.
    pub variance_max: u8,
    /// Shape of the tempo randomness curve.
    pub rand_type: TempoRandType,
}

// ── Clock-local RNG (Xorshift64) ────────────────────────────────────────────

/// Advance seed and return a pseudo-random u64 (Xorshift64).
///
/// Uses the same algorithm as `state::next_rand` but operates on the
/// clock-local seed, keeping tempo jitter independent of step/note randomness.
#[inline]
fn next_rand(seed: &mut u64) -> u64 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *seed = x;
    x
}

/// Returns true with probability `chance/100`. `chance` is clamped to 0–100.
#[inline]
fn prob_hit(seed: &mut u64, chance: u8) -> bool {
    if chance == 0 {
        return false;
    }
    if chance >= 100 {
        return true;
    }
    (next_rand(seed) % 100) < chance as u64
}

// ── Effective BPM computation ────────────────────────────────────────────────

/// Compute the effective BPM after applying tempo jitter.
///
/// `base_bpm` is the clean BPM from `SequencerState` (never mutated here).
/// `roll_state` is mutable clock-local phase/direction state.
/// `params` is a snapshot of randomness params copied from state under a read lock.
/// `rng` is the clock-local Xorshift64 seed (separate from `state.rng_seed`).
/// `step_count` is the total steps elapsed since clock start (for Beat/Seq roll points).
///
/// Returns the effective BPM clamped to 20–300.
pub fn compute_effective_bpm(
    base_bpm: u16,
    roll_state: &mut TempoRollState,
    params: &TempoRandSnapshot,
    rng: &mut u64,
    step_count: u64,
) -> u16 {
    // Off → jitter disabled; return base unchanged.
    if params.roll_point == TempoRollPoint::Off {
        return base_bpm;
    }

    // Determine whether a roll fires this step.
    let fires = match params.roll_point {
        TempoRollPoint::Off => false,
        TempoRollPoint::Step => true,
        TempoRollPoint::Beat => step_count.is_multiple_of(4),
        TempoRollPoint::Seq  => step_count.is_multiple_of(16),
    };

    if fires && prob_hit(rng, params.tempo_rand) {
        let vm = params.variance_max as i16;

        let new_offset = match params.rand_type {
            TempoRandType::Random => {
                let range = vm * 2 + 1;
                (next_rand(rng) % range as u64) as i16 - vm
            }
            TempoRandType::Up => {
                let next = roll_state.current_offset + 1;
                if next > vm { 0 } else { next }
            }
            TempoRandType::Down => {
                let next = roll_state.current_offset - 1;
                if next < -vm { 0 } else { next }
            }
            TempoRandType::Breathe => {
                // Triangle wave: rise from 0 to +vm, fall through -vm, repeat.
                // Phase counts steps; full cycle = 4 * vm steps.
                let cycle = (4 * vm as u64).max(1);
                let phase = roll_state.phase % cycle;
                let half = cycle / 2;
                if phase < half {
                    // Rising half: 0 → +vm → 0
                    let pos = if phase < half / 2 {
                        phase as i64 * vm as i64 / (half as i64 / 2).max(1)
                    } else {
                        let descend_phase = phase as i64 - half as i64 / 2;
                        let descend_len = (half as i64 - half as i64 / 2).max(1);
                        vm as i64 - descend_phase * vm as i64 / descend_len
                    };
                    pos as i16
                } else {
                    // Falling half: 0 → -vm → 0
                    let phase2 = phase - half;
                    let half2 = cycle - half;
                    if phase2 < half2 / 2 {
                        -(phase2 as i64 * vm as i64 / (half2 as i64 / 2).max(1)) as i16
                    } else {
                        let ascend_phase = phase2 as i64 - half2 as i64 / 2;
                        let ascend_len = (half2 as i64 - half2 as i64 / 2).max(1);
                        (-(vm as i64) + ascend_phase * vm as i64 / ascend_len) as i16
                    }
                }
            }
            TempoRandType::PingPong => {
                let next = roll_state.current_offset + roll_state.direction as i16;
                if next >= vm {
                    roll_state.direction = -1;
                    vm
                } else if next <= -vm {
                    roll_state.direction = 1;
                    -vm
                } else {
                    next
                }
            }
        };

        roll_state.current_offset = new_offset;
        roll_state.phase = roll_state.phase.wrapping_add(1);
    }

    // Apply the current (possibly just updated) offset to base BPM.
    let effective = base_bpm as i32 + roll_state.current_offset as i32;
    effective.clamp(BPM_MIN as i32, BPM_MAX as i32) as u16
}

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
/// Calls `libc::clock_gettime` which is safe to call with a valid timespec pointer.
/// The return value is not checked; on failure the timespec remains zero-initialized,
/// which the caller handles gracefully by sleeping until an already-past time.
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
/// Loops on EINTR so that signals do not cause premature wakeup and clock drift.
/// `TIMER_ABSTIME` guarantees each retry sleeps to the original absolute target,
/// not a relative remainder, so no drift accumulates across retries.
///
/// # Safety
/// Calls `libc::clock_nanosleep`; the timespec pointer is valid.
fn sleep_until(abs_time: &libc::timespec) {
    #[cfg(target_os = "linux")]
    unsafe {
        loop {
            let rc = libc::clock_nanosleep(
                libc::CLOCK_MONOTONIC,
                libc::TIMER_ABSTIME,
                abs_time as *const _,
                std::ptr::null_mut(),
            );
            if rc == 0 || rc != libc::EINTR {
                break;
            }
            // EINTR: retry — TIMER_ABSTIME guarantees we sleep to the original target time.
        }
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
    let total_ns: i64 = ts.tv_sec * 1_000_000_000 + ts.tv_nsec + nanos;
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
    // Tracks the last (channel, note) that received a NoteOn so consecutive
    // identical notes can be retriggered by sending a NoteOff first.
    let mut last_note: Option<(u8, u8)> = None;
    // Clock-local tempo jitter state (not stored in SequencerState).
    let mut roll_state = TempoRollState::default();
    // Clock-local RNG seed for tempo jitter (separate from state.rng_seed).
    let mut local_rng: u64 = CLOCK_RNG_INIT;

    loop {
        // --- read current parameters (read lock, released immediately) ---
        let (bpm, step_size, swing, playing, rand_snapshot) = {
            let s = state.read().expect("clock: state RwLock poisoned");
            let snap = TempoRandSnapshot {
                tempo_rand: s.tempo_rand,
                roll_point: s.tempo_roll_point,
                variance_max: s.tempo_variance_max,
                rand_type: s.tempo_rand_type,
            };
            (s.tempo_bpm, s.step_size, s.swing, s.playing, snap)
        };

        // Compute effective BPM (jitter applied clock-locally; base BPM never mutated).
        let effective_bpm = compute_effective_bpm(bpm, &mut roll_state, &rand_snapshot, &mut local_rng, step_count);

        let period = tick_nanos(effective_bpm, step_size);
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
                // Retrigger: if the same note is still held, send NoteOff first
                // so the MIDI device recognises the following NoteOn as a new note.
                if last_note == Some((channel, note))
                    && midi_tx.send(MidiEvent::NoteOff { channel, note }).is_err()
                {
                    // Receiver dropped — exit cleanly.
                    break;
                }
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
                last_note = Some((channel, note));
            }
        }

        // Advance next absolute time by one tick period.
        next_abs = add_nanos(next_abs, period);
        step_count = step_count.wrapping_add(1);
    }
}

