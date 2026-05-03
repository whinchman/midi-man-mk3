# Task: Tempo Randomness in clock.rs

- **Type**: coder
- **Status**: pending
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

