# Plan: Step and Note Randomness in tick() (Stream B)

## Overview

Add `step_rand: u8` and `note_rand: u8` to `SequencerState`, wire `step_rand`
into `tick()` as a probabilistic mute gate after the playing/paused guard, and
mark the `note_rand` integration point with a TODO comment for Stream E.

## Files to modify

- `engine/src/state.rs` — add fields, update Default, update tick()

## Steps

### Step 1: Add fields to SequencerState and Default

- Add `pub step_rand: u8` and `pub note_rand: u8` to the struct
- Init both to `0` in `Default`

### Step 2: Wire step_rand gate into tick()

After the `!self.playing || self.paused` guard and before the playhead advance,
insert:

```rust
if self.step_rand > 0 && !prob_hit(&mut self.rng_seed, self.step_rand) {
    return None;
}
```

### Step 3: Mark note_rand integration point

Inside the `if step.enabled { … }` block, after note modifier would be applied
(Stream E), insert:

```rust
// TODO(stream-E): apply note_rand gate here — prob_hit(&mut self.rng_seed, self.note_rand)
// determines whether the note modifier is applied.
```

### Step 4: Tests

Write tests covering:
- `step_rand = 0` → all enabled steps always fire
- `step_rand = 100` → no enabled steps fire
- `step_rand = 50` → 40–60% of steps fire over 1000 ticks
- `note_rand` field exists and defaults to 0
