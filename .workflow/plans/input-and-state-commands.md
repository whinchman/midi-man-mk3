# Plan: input-and-state-commands

## Overview

Add three new `InputCommand` variants and their `apply_command` arms + helper methods.

## Steps

### Step 1: Add variants to `input.rs`

Add to `InputCommand` enum:
- `RandAll`
- `RandVelocities`
- `NoteSet { step: usize, midi_note: u8, velocity: u8 }`

### Step 2: Add `randomise_velocities` and `randomise_all` to `state.rs`

Private helpers:
- `randomise_velocities`: iterates all 16 steps, uses `next_rand(&mut self.rng_seed)`, sets velocity to `(raw % 88) as u8 + 40`
- `randomise_all`: calls `generate_random_sequence()` then `randomise_velocities()`

### Step 3: Add `apply_command` arms in `state.rs`

- `RandAll` → `self.randomise_all()`
- `RandVelocities` → `self.randomise_velocities()`
- `NoteSet { step, midi_note, velocity }` → if step < 16, set both fields; else no-op

### Step 4: Write unit tests in `state.rs`

Tests as required by the acceptance criteria:
- `rand_all_sets_notes_in_range_and_velocities_in_range`
- `rand_velocities_changes_velocities_only`
- `note_set_step_3_sets_correct_fields`
- `note_set_out_of_range_is_noop`
