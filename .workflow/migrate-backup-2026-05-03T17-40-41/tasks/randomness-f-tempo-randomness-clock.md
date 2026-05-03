# Task: Tempo Randomness in clock.rs

- **Type**: coder
- **Status**: done
- **Repo**: midi-man-mk3
- **Parallel Group**: 3
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-f-tempo-randomness-clock
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream F
- **Dependencies**: randomness-c-shift-param-routing

## Description

Add clock-local tempo jitter to `engine/src/clock.rs`. The state's `tempo_bpm`
field must **never** be mutated by this feature — jitter is computed entirely
in the clock thread from a clock-local RNG seed and applied only to the
`tick_nanos` calculation.

Stream C must be merged before this branch is cut (it adds `TempoRollPoint`,
`TempoRandType`, `tempo_rand`, `tempo_roll_point`, `tempo_variance_max`,
`tempo_rand_type` to `SequencerState`).

### New types (add to clock.rs)

```rust
/// Clock-local tempo jitter state. Not stored in SequencerState — no lock needed.
struct TempoRollState {
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

/// Snapshot of tempo randomness params read from SequencerState under a read lock.
pub struct TempoRandSnapshot {
    pub tempo_rand: u8,
    pub roll_point: TempoRollPoint,
    pub variance_max: u8,
    pub rand_type: TempoRandType,
}
```

### Core function

```rust
/// Compute the effective BPM after applying tempo jitter.
///
/// `base_bpm` — the clean BPM from SequencerState (never mutated).
/// `roll_state` — mutable clock-local phase/direction state.
/// `params` — snapshot of randomness params.
/// `rng` — clock-local Xorshift64 seed (separate from state's rng_seed).
/// `step_count` — total steps elapsed since clock start (for Seq roll point).
///
/// Returns the effective BPM clamped to 20–300.
pub fn compute_effective_bpm(
    base_bpm: u16,
    roll_state: &mut TempoRollState,
    params: &TempoRandSnapshot,
    rng: &mut u64,
    step_count: u64,
) -> u16;
```

Roll point logic:
- `Off` → always returns `base_bpm` (no jitter)
- `Step` → rolls on every call
- `Beat` → rolls every 4 steps (`step_count % 4 == 0`)
- `Seq` → rolls every 16 steps (`step_count % 16 == 0`)

When a roll fires AND `prob_hit(rng, params.tempo_rand)`:

- `Random` → `offset = (next_rand(rng) % (variance_max as u64 * 2 + 1)) as i16 - variance_max as i16`
  (uniform random in `[-variance_max, +variance_max]`)
- `Up` → `current_offset` ramps from 0 to `+variance_max` then resets to 0
- `Down` → `current_offset` ramps from 0 to `-variance_max` then resets to 0
- `Breathe` → triangle wave: up to `+variance_max`, then down to `-variance_max`, repeat
- `PingPong` → bounces between `+variance_max` and `-variance_max`,
  reversing `direction` at each extreme

The `next_rand` inline Xorshift64 used here is the **same algorithm** as in
`state.rs` but it operates on the clock-local `rng` seed, not `rng_seed` in
state. Copy the 3-line implementation into `clock.rs` or extract it to a small
shared helper if the project structure allows (prefer duplication to avoid a
module dependency from clock to state internals).

Clock-local rng seed initialisation: use a compile-time constant (e.g.
`0xA24B_AED4_963D_37C5`) or read from `std::time::SystemTime` if available —
both are acceptable.

### Integration into run_clock

In the clock thread loop, after taking the read lock on state to get `tempo_bpm`
and `step_size`:

1. Also read `TempoRandSnapshot` from state under the same read lock.
2. Call `compute_effective_bpm(base_bpm, &mut roll_state, &snapshot, &mut local_rng, step_count)`.
3. Pass the result to `tick_nanos(effective_bpm, step_size)` instead of `base_bpm`.
4. Increment `step_count` after each tick.

The `TempoRollState` and `local_rng: u64` are declared in `run_clock`'s local
scope before the loop.

## Acceptance Criteria

- [ ] `TempoRollState` struct exists in `clock.rs` (clock-local, no Arc)
- [ ] `TempoRandSnapshot` struct exists in `clock.rs`
- [ ] `compute_effective_bpm` is a pure function with no side effects other than updating `roll_state` and advancing `rng`
- [ ] `tempo_rand = 0` → effective BPM always equals `base_bpm` (no jitter)
- [ ] `roll_point = Off` → effective BPM always equals `base_bpm` regardless of other params
- [ ] `tempo_rand = 100`, `roll_point = Step`, `type = Random`, `variance_max = 20` → effective BPM stays within `base_bpm ± 20` over 1 000 calls
- [ ] `type = PingPong` → BPM bounces monotonically between `base_bpm - variance_max` and `base_bpm + variance_max`
- [ ] `type = Breathe` → BPM forms a smooth triangle-wave curve within variance bounds
- [ ] `tempo_bpm` field in `SequencerState` is never written by the clock thread (verified by reading it after 1 000 ticks and confirming it equals the initial value)
- [ ] Effective BPM is always clamped to 20–300
- [ ] No heap allocation in `run_clock` inner loop
- [ ] `cargo test -p engine` passes with unit tests for `compute_effective_bpm` covering the criteria above
- [ ] `clippy` passes with no new warnings
- [ ] All new public items have a doc comment

## Interface Contracts

Consumed from Stream C (`engine/src/state.rs`):

```rust
pub enum TempoRollPoint { Off, Step, Beat, Seq }
impl TempoRollPoint { pub fn from_index … pub fn to_index … }

pub enum TempoRandType { Random, Up, Down, Breathe, PingPong }
impl TempoRandType { pub fn from_index … pub fn to_index … }

// SequencerState fields read under read lock:
pub tempo_rand: u8,
pub tempo_roll_point: TempoRollPoint,
pub tempo_variance_max: u8,
pub tempo_rand_type: TempoRandType,
pub tempo_bpm: u16,   // read-only; never written
```

Existing clock.rs API (unchanged):

```rust
pub fn tick_nanos(bpm: u16, step_size: StepSize) -> u64;
```

## Context

- File: `engine/src/clock.rs`
- `tick_nanos` function at line ~33; `run_clock` function contains the main loop.
- The clock thread already holds a read lock to read `tempo_bpm`, `step_size`,
  `swing`. The same read lock window can be extended to also copy `TempoRandSnapshot`.
- The clock-local `rng` seed is separate from `SequencerState::rng_seed` so
  that tempo jitter does not consume the state's RNG budget for step/note randomness.
- For `Breathe` / `PingPong`, a triangle-wave approximation is acceptable and
  preferred over sine (no-alloc, no lookup table needed for triangle).
- Code standard: no `unsafe` except the existing `clock_nanosleep` FFI wrapper,
  no heap allocation in the loop, `clippy` clean.

## Notes

### Implementation Summary

**Branch**: `randomness-f-tempo-randomness-clock` (based off `feature/randomness-layer`)
**File modified**: `engine/src/clock.rs`

**What was implemented:**

1. `TempoRollState` (pub(crate) struct) — clock-local phase/direction/current_offset state; no Arc needed.
2. `TempoRandSnapshot` (pub(crate) struct) — cheap copy of tempo randomness params from state under read lock.
3. Clock-local `next_rand`/`prob_hit` Xorshift64 helpers — same algorithm as state.rs, operate on `local_rng` (u64, separate from `state.rng_seed`).
4. `compute_effective_bpm` (pub(crate)) — pure function applying Roll point (Off/Step/Beat/Seq), prob gate, and jitter type (Random/Up/Down/Breathe/PingPong). BPM clamped to 20–300. `SequencerState::tempo_bpm` is never written.
5. `run_clock` integration — `TempoRollState` and `local_rng` declared before loop; `TempoRandSnapshot` copied under the existing read lock alongside bpm/step_size/swing; `compute_effective_bpm` called before `tick_nanos`.

**Test results**: `cargo test -p engine` — 78 unit tests + 248 integration tests: all passed. `cargo build -p engine --release` clean.

---

### Code Review — 2026-05-02

**Reviewer verdict: request-changes**

**QA run**: `cargo test -p engine` — 78 tests passed, 0 failed.

**Acceptance criteria status**:

- [x] `TempoRollState` struct exists in `clock.rs` (clock-local, no Arc)
- [x] `TempoRandSnapshot` struct exists in `clock.rs`
- [x] `compute_effective_bpm` is a pure function (only updates `roll_state` and advances `rng`)
- [x] `tempo_rand = 0` → effective BPM always equals `base_bpm` — test passes
- [x] `roll_point = Off` → effective BPM always equals `base_bpm` — early-return at line 106–108, test passes
- [x] `tempo_rand=100, roll_point=Step, type=Random, variance_max=20` → BPM stays within base±20 over 1 000 calls — test passes
- [x] `type = PingPong` → bounces within bounds — test passes
- [x] `type = Breathe` → stays within variance bounds — test passes (note: waveform shape is double-tent, not simple triangle — see INFO below)
- [x] `tempo_bpm` in `SequencerState` never written by clock thread — verified by test and by code inspection
- [x] Effective BPM clamped to 20–300 — line 184 uses `.clamp(BPM_MIN as i32, BPM_MAX as i32)`
- [x] No heap allocation in `run_clock` inner loop — confirmed by inspection (loop is stack-only)
- [x] `cargo test -p engine` passes with unit tests — 78 passed
- [ ] **clippy passes with no new warnings — FAILS: 3 warnings (see WARNING below)**
- [x] All new public items have a doc comment

**Findings**:

#### [WARNING] engine/src/clock.rs:114–115 — clippy::manual_is_multiple_of

Lines 114–115 use `step_count % 4 == 0` and `step_count % 16 == 0` instead of `.is_multiple_of(4)` / `.is_multiple_of(16)`. Clippy flags these as `manual_is_multiple_of` warnings. The AC requires clippy to pass with no new warnings; this is a hard AC failure.

Fix: Replace `step_count % 4 == 0` with `step_count.is_multiple_of(4)` and `step_count % 16 == 0` with `step_count.is_multiple_of(16)`.

#### [WARNING] engine/src/clock.rs:154–161 — clippy::let_and_return

The `let pos = …; pos` pattern in the Breathe falling-half branch (lines 154–161) triggers a `let_and_return` clippy warning. The AC requires clippy to pass with no new warnings.

Fix: Remove the `let pos =` binding and return the expression directly, as suggested by `cargo clippy --fix`.

#### [INFO] No test for Seq roll point (fires every 16 steps)

The Beat roll point has a test (`test_compute_effective_bpm_beat_fires_every_4_steps`) that verifies changes only happen at multiples of 4. There is no equivalent test for `Seq` (changes only at multiples of 16). The AC mentions Seq roll behaviour as a criterion.

Fix: Add a test analogous to the Beat test but using `TempoRollPoint::Seq` and checking that changes only occur at multiples of 16 within 80 steps.

#### [INFO] Breathe waveform is a double-tent, not a simple triangle

The spec says "triangle wave: up to +variance_max, then down to -variance_max, repeat." The implementation produces a double-tent: +vm→0→-vm→0 (each half-cycle is itself a triangle that peaks and returns to zero), rather than a simple 0→+vm→-vm→0 triangle. The Breathe bounds test passes because both shapes stay within ±vm, but the waveform does not match the spec description literally. This is noted as info because the task says "triangle-wave approximation is acceptable."

Fix: No change required unless the spec intent is a strict 0→+vm→-vm→0 triangle. Flag for stakeholder awareness.

**Summary**: 2 warning, 2 info findings. The two clippy warnings are AC failures that must be fixed before merge.
