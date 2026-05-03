# Task: Step and Note Randomness in tick()

- **Type**: coder
- **Status**: done
- **Review Status**: approved
- **Repo**: midi-man-mk3
- **Parallel Group**: 2
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-b-step-note-rand-tick
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream B
- **Dependencies**: randomness-a-rng-infra

## Description

Add `step_rand` and `note_rand` fields to `SequencerState` and wire probabilistic
muting / note-modifier gating into `tick()` in `engine/src/state.rs`.

This stream depends on Stream A completing first (fields `rng_seed`, `next_rand`,
`prob_hit`, and the unconditional seed advance in `tick()` must all exist).

### Fields to add

```rust
/// Step Randomness (0–100): per-step probability that an enabled step fires.
/// 0 = always fires (existing behaviour). 100 = never fires.
pub step_rand: u8,

/// Note Randomness (0–100): per-step probability that the note modifier is
/// applied. Only relevant when `note_modifier != 0`.
/// 0 = modifier never applied. 100 = modifier always applied.
pub note_rand: u8,
```

Both initialise to `0` in `Default`.

### tick() changes

After the `next_rand(&mut self.rng_seed)` call added by Stream A, and after the
`!self.playing || self.paused` guard:

**Step Randomness** (applied immediately when step would fire):

```rust
// Step Randomness: probabilistic mute of the whole step.
if self.step_rand > 0 && !prob_hit(&mut self.rng_seed, self.step_rand) {
    return None;
}
```

**Note Randomness** (applied after `note_modifier` is computed in the emit path —
see Stream E for note_modifier; in this stream just gate a no-op stub or leave a
comment marking the integration point if Stream E is not yet merged):

The probability gate for `note_rand` controls whether the note modifier is
applied. Stream E owns the modifier application; Stream B's job is to ensure
the `note_rand` field exists and is correctly consumed when Stream E merges.

If Stream E has not yet landed on the base branch, leave a clearly-marked
`// TODO(stream-E): apply note_rand gate here` comment at the correct location
in `tick()`.

## Acceptance Criteria

- [ ] `SequencerState` has `step_rand: u8` and `note_rand: u8` fields
- [ ] Both default to `0`
- [ ] `step_rand = 0` → all enabled steps always fire (existing behaviour preserved)
- [ ] `step_rand = 100` → no enabled steps fire (all probabilistically muted)
- [ ] `step_rand = 50` → over 1 000 ticks, between 40% and 60% of enabled steps fire (probabilistic tolerance test)
- [ ] `note_rand` field exists and is accessible; its use in `tick()` is either wired or has a TODO comment at the correct insertion point
- [ ] No heap allocation introduced
- [ ] `cargo test -p engine` passes with new tests covering all three criteria above
- [ ] `clippy` passes with no new warnings
- [ ] All new public items have a doc comment

## Interface Contracts

Fields produced by this task, consumed by Stream E:

```rust
// engine/src/state.rs — SequencerState
pub step_rand: u8,   // Default: 0
pub note_rand: u8,   // Default: 0
```

## Context

- File: `engine/src/state.rs`
- `tick()` starts at line ~199 after Stream A lands.
- Step Randomness must be applied **after** the `!self.playing || self.paused`
  guard and **before** the `let step = &self.steps[self.playhead as usize]` read.
- Note Randomness gate sits **inside** the `if step.enabled { … }` block, after
  the note modifier is applied (Stream E). If Stream E is not yet merged, mark
  with a TODO comment so the integration merge is obvious.
- Stream A must be merged to `feature/randomness-layer` before this branch is cut.

## Notes

Implemented on branch `randomness-b-step-note-rand-tick` (worktree at
`.workflow/worktrees/randomness-b-step-note-rand-tick`), based off
`feature/randomness-layer` which already contains Stream A.

### Changes

- `engine/src/state.rs`: added `step_rand: u8` and `note_rand: u8` to
  `SequencerState`; both default to `0`.
- `tick()`: added probabilistic mute gate using `prob_hit(&mut self.rng_seed,
  self.step_rand)` after the playing/paused guard and after the playhead
  advance, before reading step data. Added `// TODO(stream-E)` comment at the
  note_rand integration point inside the `if step.enabled` block.
- The task description's gate used `!prob_hit`, which is semantically
  inverted for a mute-probability field. Implemented without the negation so
  that `step_rand=100` correctly mutes all steps.

### Test results

`cargo test -p engine`: 36 unit tests pass (4 new: test_step_rand_default_zero,
test_note_rand_default_zero, test_step_rand_zero_always_fires,
test_step_rand_hundred_never_fires, test_step_rand_fifty_statistical).
Full suite (integration + doc tests): all pass. `cargo clippy`: clean.
`cargo build -p engine --release`: success.

---

## Code Review

**Reviewer:** code-reviewer agent
**Date:** 2026-05-02
**Verdict:** APPROVE

### Acceptance Criteria Checklist

- [x] `SequencerState` has `step_rand: u8` and `note_rand: u8` fields — confirmed at lines 151–155
- [x] Both default to `0` — confirmed in `Default` impl at lines 179–180
- [x] `step_rand = 0` → all enabled steps always fire — `prob_hit(0)` short-circuits to `false`; test `test_step_rand_zero_always_fires` verifies 1000/1000 fires
- [x] `step_rand = 100` → no enabled steps fire — `prob_hit(100)` short-circuits to `true`; test `test_step_rand_hundred_never_fires` verifies 0/1000 fires
- [x] `step_rand = 50` → 40–60% fire — `test_step_rand_fifty_statistical` verifies over 1000 ticks
- [x] `note_rand` field exists and has a TODO comment at the correct Stream-E insertion point (inside `if step.enabled` block at line 261–262)
- [x] No heap allocation introduced — all changes use stack values and existing fields
- [x] `cargo test -p engine` passes — 36 unit tests + full integration suite all pass (verified)
- [x] `clippy` passes — no warnings (verified)
- [x] All new public items have doc comments — both fields have single-line doc comments

### Semantic Correctness — step_rand Inversion

The coder correctly identified that the task spec pseudocode (`!prob_hit(...)`) had inverted semantics. The spec simultaneously defined `step_rand` as "fire probability" (in the sample code) and "0 = always fires, 100 = never fires" (in the field doc). These two are contradictory: if `step_rand=100` means "never fires," then it is a *mute* probability, not a fire probability, and the `!` must be dropped.

The implementation drops the `!` and treats `step_rand` as mute probability. The observable behavior is correct: `step_rand=0` → all steps fire, `step_rand=100` → all steps muted. Field-level doc comments, inline comments, and tests are all consistent with this semantics. **The coder's correction is right.**

### Asymmetric Semantics: step_rand vs. note_rand

`step_rand` is a mute probability (0 = never mute). `note_rand` is an apply probability (0 = modifier never applied). These are intentionally opposite, matching the task spec's description for each field. Stream E must call `prob_hit(&mut self.rng_seed, self.note_rand)` to gate modifier application (shown in the TODO comment). This asymmetry is by design and correctly documented.

### Minor Observations (info — no action required)

1. **`step_rand > 0` guard removed (info):** The task spec included `if self.step_rand > 0 && !prob_hit(...)` to short-circuit when randomness is off. The implementation relies on `prob_hit`'s own `chance == 0` early return instead. Functionally equivalent and cleaner.

2. **RNG consumed for disabled steps when step_rand > 0 (info):** The `prob_hit` call at line 255 fires before the `step.enabled` check at line 260. This means an extra RNG value is consumed even for disabled steps when `step_rand > 0`. This is acceptable — it preserves RNG determinism regardless of step enable state and is consistent with Stream A's unconditional seed advance on every tick.

3. **`#[allow(dead_code)]` on `prob_hit` (info):** The `allow(dead_code)` attribute on `prob_hit` was present before this stream. After Stream B it is now used in `tick()`, making the attribute redundant (though harmless). Stream E or a cleanup pass can remove it.

### Findings Summary

- Critical: 0
- Warning: 0
- Info: 3 (no action required)

**Overall verdict: APPROVE — implementation is correct, complete, and test-covered. No bugs to file.**
