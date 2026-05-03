# Task: Step and Note Randomness in tick()

- **Type**: coder
- **Status**: pending
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

