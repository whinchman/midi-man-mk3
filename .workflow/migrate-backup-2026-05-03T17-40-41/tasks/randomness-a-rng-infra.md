# Task: RNG Infrastructure

- **Type**: coder
- **Status**: done
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

Implemented on branch `randomness-a-rng-infra` (worktree at `.workflow/worktrees/randomness-a-rng-infra`), based off `feature/randomness-layer`.

Changes made to `engine/src/state.rs`:
- Added `pub rng_seed: u64` field to `SequencerState`, defaulting to `0x853C_49E6_748F_EA9B`
- Added private `next_rand(seed: &mut u64) -> u64` (Xorshift64 algorithm)
- Added private `prob_hit(seed: &mut u64, chance: u8) -> bool` with `#[allow(dead_code)]` since it is infrastructure for dependent streams B/C/E/F
- Added `next_rand(&mut self.rng_seed)` as first statement in `tick()`, before the playing/paused guard

Test results: `cargo test -p engine` — 28 unit tests + 278 integration tests all pass. `cargo clippy -p engine` — no warnings. `cargo build -p engine --release` — success.

---

## Code Review (code-reviewer agent, 2026-05-02)

**Verdict: APPROVE — 0 critical, 0 warning, 2 info**

### Findings

#### [INFO] `engine/src/state.rs:177-184` — Xorshift64 correctness
The shift triple (13, 7, 17) is a known-valid Xorshift64 triple from Marsaglia 2003 with a full period of 2^64-1 over non-zero seeds. The implementation correctly maps `*seed → x`, applies all three XOR/shift ops, writes back, and returns `x`. Verified by Python simulation over 1000 steps — no zero produced from the default seed.

#### [INFO] `engine/src/state.rs:186-196` — `prob_hit` edge cases
`chance == 0` returns `false` (no RNG call consumed). `chance >= 100` returns `true` (no RNG call consumed). Interior path uses `next_rand(seed) % 100 < chance as u64` — correct. The `#[allow(dead_code)]` attribute is appropriate: the function is infrastructure for downstream streams B/C/E/F and is exercised by the in-module tests.

### Checklist
- [x] `rng_seed` field present and public on `SequencerState` (line 148)
- [x] Default seed `0x853C_49E6_748F_EA9B` is non-zero
- [x] `next_rand` is correct Xorshift64 (13, 7, 17 triple; seed never becomes 0 from non-zero input)
- [x] `prob_hit(seed, 0)` always false — early-return without RNG call
- [x] `prob_hit(seed, 100)` always true — early-return without RNG call
- [x] `rng_seed` advanced as the very first statement of `tick()`, before the playing/paused guard (line 225)
- [x] `#[allow(dead_code)]` on `prob_hit` is appropriate (downstream streams not yet merged)
- [x] No heap allocation introduced; all operations on stack locals and `u64` fields
- [x] `SequencerState` remains `Clone` (derives it; `u64` is `Copy`)
- [x] All new public items documented; private helpers have doc comments
- [x] `cargo test -p engine` passes (30 tests as of reviewer run)
- [x] `cargo clippy -p engine` passes with no warnings
- [x] 6 new unit tests cover all acceptance criteria including the 10 000-call statistical check

---

## QA Review (qa agent, 2026-05-02)

Three coverage gaps found and filled (committed to `randomness-a-rng-infra` branch):
- `test_rng_seed_advances_every_tick_when_paused` — seed advances when playing=true, paused=true
- `test_rng_seed_advances_every_tick_when_playing` — seed advances on the normal playing path
- `test_next_rand_produces_distinct_values` — confirms next_rand is not an identity function

Total RNG tests: 9. Full suite: 342 tests, 0 failures.

---

## PR Feedback

PR: https://github.com/whinchman/midi-man-mk3/pull/26

### Comments Requiring Action

_(none)_

### CI Failures

_(none — no CI checks configured on this repository)_

### Questions / Acknowledged

- Branch naming: `feature/randomness-layer/randomness-a-rng-infra` could not be pushed to the remote because `feature/randomness-layer` already exists as a ref. Git cannot create a sub-path ref under an existing ref. Branch was pushed as `randomness-a-rng-infra` (flat name) and the PR was opened from that name against `feature/randomness-layer`.
  Action: acknowledged
