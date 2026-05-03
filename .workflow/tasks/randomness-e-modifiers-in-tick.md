# Task: Modifiers in tick() — Note, Skip, Velocity, Scale Quant

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 3
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-e-modifiers-in-tick
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream E
- **Dependencies**: randomness-c-shift-param-routing, randomness-a-rng-infra

## Description

Wire the four Shift modifiers (`note_modifier`, `skip_modifier`,
`velocity_modifier`, `scale_quant`) and the `note_rand` probability gate into
`tick()` inside `engine/src/state.rs`.

Streams A and C must both be merged before this branch is cut. Stream B may or
may not be merged; if `note_rand` already exists (from B), use it; if not, add
it here.

### Updated tick() emit logic

After the existing Step Randomness check (from Stream B) and the `let step =
&self.steps[self.playhead as usize]` read, replace the simple emit block with:

```rust
if step.enabled {
    // 1. Skip modifier: mute the step entirely.
    if self.skip_modifier {
        return None;
    }

    // 2. Compute base note.
    let mut note = step.midi_note;

    // 3. Note modifier + Note Randomness gate.
    //    Apply note_modifier first; then gate on note_rand probability.
    //    If the prob roll misses, revert to the original note.
    if self.note_modifier != 0 {
        let modified = (note as i16 + self.note_modifier as i16).clamp(0, 127) as u8;
        if prob_hit(&mut self.rng_seed, self.note_rand) {
            note = modified;
        }
        // If note_rand == 0, prob_hit returns false → modifier never applied.
        // If note_rand == 100, prob_hit returns true → modifier always applied.
    }

    // 4. Scale Quantization: snap to key after note_modifier is applied.
    //    Apply note_modifier first, then snap_to_key. If the modifier pushes
    //    the note out of key, quantization corrects it.
    if self.scale_quant {
        note = crate::music_theory::snap_to_key(note, self.key, self.mode);
    }

    // 5. Velocity modifier.
    let velocity =
        (step.velocity as i16 + self.velocity_modifier as i16).clamp(0, 127) as u8;

    Some(MidiEvent::NoteOn {
        channel: self.midi_channel,
        note,
        velocity,
        duration_nanos: 0,
    })
} else {
    None
}
```

Key design decisions already resolved (from plan §11):
- `note_modifier != 0`: apply modifier first, then gate with `note_rand`.
- `scale_quant`: applied **after** `note_modifier` (so quantization corrects
  any out-of-key offset). This order is documented in the code comment above.
- `velocity_modifier`: clamped to 0–127 at emit; stored as i8 (-127..=127).
- `skip_modifier`: returns `None` before any note computation.

## Acceptance Criteria

- [ ] `skip_modifier = false` → steps fire normally
- [ ] `skip_modifier = true` → tick() returns None for all enabled steps
- [ ] `note_modifier = 0` → emitted note equals stored note
- [ ] `note_modifier = 7` → emitted note is stored note + 7, clamped to 127
- [ ] `note_modifier = -12` → emitted note is stored note - 12, clamped to 0
- [ ] `note_modifier != 0` with `note_rand = 100` → modifier always applied (100% of ticks)
- [ ] `note_modifier != 0` with `note_rand = 0` → modifier never applied (stored note emitted)
- [ ] `scale_quant = false` → note emitted as stored (no snapping)
- [ ] `scale_quant = true` → emitted note passes `snap_to_key` identity check
- [ ] `scale_quant` applied **after** `note_modifier` (order documented in code comment)
- [ ] `velocity_modifier = 0` → velocity unchanged
- [ ] `velocity_modifier = 20` → velocity += 20, clamped to 127
- [ ] `velocity_modifier = -20` → velocity -= 20, clamped to 0
- [ ] No heap allocation in tick()
- [ ] `cargo test -p engine` passes with tests covering all criteria
- [ ] `clippy` passes with no new warnings
- [ ] All new code has doc comments where appropriate

## Interface Contracts

Consumed from Stream A (`engine/src/state.rs`):

```rust
fn next_rand(seed: &mut u64) -> u64;
fn prob_hit(seed: &mut u64, chance: u8) -> bool;
pub rng_seed: u64,
```

Consumed from Stream C (`engine/src/state.rs`):

```rust
pub note_modifier: i8,
pub skip_modifier: bool,
pub velocity_modifier: i8,
pub scale_quant: bool,
```

Consumed from Stream B (or added here if B not yet merged):

```rust
pub note_rand: u8,
```

Consumed from `music_theory.rs` (already exists, no change needed):

```rust
pub fn snap_to_key(note: u8, key: Key, mode: Mode) -> u8;
```

## Context

- File: `engine/src/state.rs`
- `tick()` currently at line ~199; simple `if step.enabled { Some(NoteOn {…}) }`
  block at line ~218.
- `snap_to_key` is already imported via `crate::music_theory::snap_to_key` (used
  in `snap_all_steps_to_key`).
- `prob_hit` and `next_rand` are private module-level fns added by Stream A.
- The ordering of checks inside `tick()` after all streams merge:
  1. `next_rand` unconditional advance (Stream A)
  2. `!playing || paused` guard
  3. Playhead advance
  4. Step Randomness roll (Stream B)
  5. `let step = &self.steps[…]`
  6. `if step.enabled { skip_modifier / note computation / scale_quant / velocity (this stream) }`

## Notes

