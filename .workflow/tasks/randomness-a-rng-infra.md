# Task: RNG Infrastructure

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 1
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-a-rng-infra
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream A
- **Dependencies**: none

## Description

Add a deterministic, heap-free pseudo-random number generator to `SequencerState`
in `engine/src/state.rs`. This is the foundational RNG that all other randomness
streams depend on.

Three concrete changes:

1. Add `rng_seed: u64` field to `SequencerState`, initialised to
   `0x853C_49E6_748F_EA9B` in `Default`.
2. Add two private helper functions (Xorshift64 + probability gate):

```rust
/// Advance seed and return a pseudo-random u64 (Xorshift64).
fn next_rand(seed: &mut u64) -> u64 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *seed = x;
    x
}

/// Returns true with probability `chance/100`. `chance` is clamped to 0–100.
fn prob_hit(seed: &mut u64, chance: u8) -> bool {
    if chance == 0 { return false; }
    if chance >= 100 { return true; }
    (next_rand(seed) % 100) < chance as u64
}
```

3. At the very top of `tick()` (before any playback guard), advance `rng_seed`
   unconditionally on every call:

```rust
next_rand(&mut self.rng_seed);
```

Advancing on every tick — even when not playing — ensures the RNG stream is not
biased by start/stop timing and the sequence is deterministic from startup.

## Acceptance Criteria

- [ ] `SequencerState` has a `rng_seed: u64` field
- [ ] `Default` initialises `rng_seed` to `0x853C_49E6_748F_EA9B`
- [ ] `next_rand` and `prob_hit` exist as private helpers in `state.rs`
- [ ] `tick()` advances `rng_seed` unconditionally on every call (first statement, before the playing/paused guard)
- [ ] `prob_hit(seed, 0)` always returns `false`
- [ ] `prob_hit(seed, 100)` always returns `true`
- [ ] Over 10 000 calls `prob_hit(seed, 50)` returns true between 45% and 55% of the time
- [ ] No heap allocation introduced
- [ ] `SequencerState` remains `Clone`
- [ ] `cargo test -p engine` passes
- [ ] `clippy` passes with no new warnings
- [ ] All new public items have a doc comment

## Interface Contracts

These helpers are private but their signatures are shared across streams B, C, E:

```rust
// engine/src/state.rs  (private module-level fns)
fn next_rand(seed: &mut u64) -> u64;
fn prob_hit(seed: &mut u64, chance: u8) -> bool;
```

`SequencerState` new field (used by streams B, C, E, F):

```rust
pub rng_seed: u64,   // Default: 0x853C_49E6_748F_EA9B
```

## Context

- File: `engine/src/state.rs`
- `SequencerState` is defined starting at line ~112; `Default` at line ~149; `tick()` at line ~199.
- The existing `tick()` body starts with `if !self.playing || self.paused { return None; }`.
  The `next_rand` call must precede this guard.
- No external crates: implement Xorshift64 inline.
- Code standard: no `unsafe`, no heap allocation on hot path.

## Notes

