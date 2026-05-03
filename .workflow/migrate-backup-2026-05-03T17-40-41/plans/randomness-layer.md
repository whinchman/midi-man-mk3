# Randomness Layer — Architecture Plan

**Feature group:** Post-MVP Randomness Layer  
**Status:** Ready for implementation  
**Author:** Architect agent (2026-05-02)

---

## 0. Items in Scope

| # | Item | Backlog entry |
|---|------|--------------|
| R1 | Note Randomness (0–100) — per-step probability that note modifiers apply | Randomness Layer |
| R2 | Tempo Randomness (0–100) — roll point, variance max, type | Randomness Layer |
| R3 | Step Randomness (0–100) — per-step probability that step fires | Randomness Layer |
| S1 | Shift mode: Note Modifier (off / ±1–12 semitones / 1–8 oct) | Randomness Layer |
| S2 | Shift mode: Skip Modifier (off/on) | Randomness Layer |
| S3 | Shift mode: Velocity Modifier (off / 1–100 offset) | Randomness Layer |
| S4 | Shift mode: Generate Random Sequence | Randomness Layer |
| S5 | Shift mode: Scale Quantization toggle | Randomness Layer |
| S6 | Shift mode: Key Transposition (candidate — see §8) | Randomness Layer |

---

## 1. Architecture Overview

### 1.1 Threading model (unchanged)

```
UI thread  ──cmd_tx──►  state processor  ──apply_command──►  SequencerState
Clock thread  ──read lock──►  tick()  ──midi_tx──►  midi_out thread
```

The `SequencerState` is the single source of truth, wrapped in
`Arc<RwLock<SequencerState>>`. The clock thread holds only a read lock during
parameter sampling and a brief write lock during `tick()`. The state processor
(main loop, Step 9 wiring) holds the write lock during `apply_command`.

### 1.2 Where randomness is evaluated

All probability rolls happen **inside `tick()`** (state.rs), not in the clock
thread. Rationale:

- The clock thread must stay allocation-free and as short as possible inside
  the write lock. `tick()` already is that write-lock body.
- Seeding an RNG in the clock thread would require passing it in or storing it
  in the clock-local scope; `tick()` gets that scope naturally as a `&mut self`
  method.
- `tick()` already decides whether a `MidiEvent::NoteOn` is emitted — it is
  the natural place to apply probabilistic suppression or modification.

### 1.3 RNG storage and seeding

**Decision: store a `u64` LCG seed in `SequencerState` as `rng_seed: u64`.**

Rationale:

- No heap allocation. A Lehmer/LCG PRNG needs a single `u64` seed and produces
  a uniform `u64` per step using a few multiplications. This is correct for
  the no-alloc hot path.
- No external crate needed — a minimal LCG can be inlined in `state.rs` as a
  private helper. (If the project later wants to add `rand`, the seed field can
  be replaced with `rand::rngs::SmallRng`, but that is out of scope.)
- Thread-local would make the RNG invisible to tests. A seed in `SequencerState`
  is serialisable, reproducible, and reset-able with a command.
- The seed is initialized to a non-zero value in `Default` (e.g. `0x853C_49E6_748F_EA9B`
  — the splitmix64 golden ratio constant) and advanced on every `tick()` call
  regardless of whether randomness features are in use, so the sequence is
  deterministic from startup.

**LCG helper (private, in state.rs):**

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

### 1.4 Shift overlay param system

The Shift overlay currently renders "(shift mode — coming soon)" in
`render_overlay` (`ui_render.rs`). This plan builds out the Shift overlay as a
parallel to the Regular overlay: 8 named params, same Left/Right/Up/Down/Enter
navigation model, same `PendingEdit::Param { overlay: OverlayMode::Shift, .. }`
flow.

The existing `ParamValueDelta` / `committed_param_value` / `clamped_param_value`
/ `apply_param_value` dispatch in `state.rs` will be extended with an
overlay-aware dispatch: the `index` alone is not enough when both overlays have
params at index 0–7; the overlay field in `PendingEdit::Param` disambiguates.

**Key decision:** `committed_param_value` and friends must become
overlay-aware. The cleanest approach is to add a new private method trio:

```rust
fn shift_committed_param_value(&self, index: u8) -> i64 { ... }
fn shift_clamped_param_value(&self, index: u8, value: i64) -> i64 { ... }
fn shift_apply_param_value(&mut self, index: u8, value: i64) { ... }
```

And route in `apply_command`'s `ParamValueDelta` and `Confirm` arms based on
`self.active_overlay`.

### 1.5 Shift param index map

| Index | Param | Type | Range |
|-------|-------|------|-------|
| 0 | Note Randomness | u8 | 0–100 |
| 1 | Tempo Randomness | u8 | 0–100 |
| 2 | Tempo Roll Point | enum | off/step/beat/seq |
| 3 | Tempo Variance Max | u8 | 1–99 |
| 4 | Tempo Type | enum | random/up/down/breathe/pingpong |
| 5 | Step Randomness | u8 | 0–100 |
| 6 | Scale Quantization | bool | off/on |
| 7 | (reserved / Key Transposition — see §8) | — | — |

The Shift modifiers (Note, Skip, Velocity) and Generate Random Sequence are
**action-style commands** not continuous param values: they do not fit the
Up/Down value-dial model. They are triggered by dedicated `InputCommand`
variants, accessible as buttons in the Shift overlay UI rather than param slots.

---

## 2. Data Model Changes

### 2.1 `SequencerState` additions

```rust
// --- Randomness ---
/// Xorshift64 seed; advanced on every tick().
pub rng_seed: u64,
/// Note Randomness (0–100): per-step probability that note modifiers apply.
pub note_rand: u8,
/// Step Randomness (0–100): per-step probability that step fires.
pub step_rand: u8,
/// Tempo Randomness (0–100): probability that a tempo variance roll fires.
pub tempo_rand: u8,
/// When tempo randomness fires: how often (off/step/beat/seq).
pub tempo_roll_point: TempoRollPoint,
/// Maximum BPM variance (1–99) for tempo randomness.
pub tempo_variance_max: u8,
/// Tempo randomness curve type.
pub tempo_rand_type: TempoRandType,
/// Whether scale quantization is active (snaps output note to key/mode).
pub scale_quant: bool,

// --- Shift modifiers (applied transiently at play time, not written to StepData) ---
/// Semitone offset applied to every NoteOn. 0 = off. ±1–12 = semitone steps;
/// ParamValueDelta switches to 12-semitone (octave) increments beyond ±12. Max ±96.
pub note_modifier: i8,
/// When true, step is skipped at play time (note is held/muted).
pub skip_modifier: bool,
/// Velocity offset applied to every NoteOn (0 = off; -127..=127, clamped to 0–127).
pub velocity_modifier: i8,
```

**New enums:**

```rust
/// When the tempo randomness roll fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TempoRollPoint {
    Off,
    Step,   // every step
    Beat,   // every beat (4 steps at 1/16 resolution)
    Seq,    // every sequence loop
}

/// Shape of the tempo randomness curve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TempoRandType {
    Random,    // uniform random within ±variance_max
    Up,        // ramps BPM upward then resets
    Down,      // ramps BPM downward then resets
    Breathe,   // sine-like: up then down
    PingPong,  // bounces between extremes
}
```

Both enums get `COUNT`, `from_index`, `to_index` following the `Key`/`Mode`
pattern already in `music_theory.rs`. They live in `state.rs` alongside
`StepSize`.

### 2.2 `StepData` — no changes

Shift modifiers (Note, Skip, Velocity) are **transient at play time**. They are
stored on `SequencerState` as single global values, not per-step. The decision
to not write them into `StepData` is deliberate:

- Per-step storage would require a UI to set each step's modifier individually —
  that is a separate future feature.
- Global modifiers are the simplest useful implementation and match hardware
  sequencer conventions (knob that shifts all notes).
- Scale Quantization is a global toggle — no per-step variant needed.

### 2.3 `Default` initialisation additions

```rust
rng_seed: 0x853C_49E6_748F_EA9B,
note_rand: 0,
step_rand: 0,
tempo_rand: 0,
tempo_roll_point: TempoRollPoint::Off,
tempo_variance_max: 10,
tempo_rand_type: TempoRandType::Random,
scale_quant: false,
note_modifier: 0,
skip_modifier: false,
velocity_modifier: 0,
```

---

## 3. `tick()` Changes

The updated `tick()` signature stays `pub fn tick(&mut self) -> Option<MidiEvent>`.

New logic, in order:

1. **Advance rng_seed** (always, every tick, regardless of playing state —
   ensures the sequence is not biased by start/stop).
2. **Step Randomness:** if `step_rand > 0`, roll `prob_hit(&mut self.rng_seed, self.step_rand)`;
   if false, treat the step as if disabled (return `None` even when `step.enabled`).
3. **Skip Modifier:** if `self.skip_modifier`, return `None` (mute the step).
4. When `step.enabled` and not muted:
   - Compute `note = step.midi_note`.
   - **Note Modifier:** if `self.note_modifier != 0`, apply note modifier
     (add semitones, clamp to 0–127).
   - **Note Randomness:** if `note_rand > 0`, roll; if miss, revert to
     `step.midi_note` (modifier not applied).
   - **Scale Quantization:** if `self.scale_quant`, snap note to key/mode using
     existing `music_theory::snap_to_key`.
   - Compute `velocity = step.velocity`.
   - **Velocity Modifier:** if `self.velocity_modifier != 0`, add offset, clamp
     0–127.
   - Emit `MidiEvent::NoteOn { note, velocity, .. }`.

The tick body never allocates and the RNG call is 3 XOR/shift operations —
well within the budget for the write-lock window.

### Tempo randomness — handled in the clock thread

Tempo randomness requires **mutating `tempo_bpm` before the next sleep period
is computed**. The clock thread reads `tempo_bpm` each iteration. The options
are:

1. **Mutate `tempo_bpm` inside `tick()` (write lock).** The clock thread reads
   the new BPM on the next iteration. Simple, but `tempo_bpm` is now dirty
   state — the UI will show a jittered BPM.
2. **Maintain a `bpm_jitter: i16` field** that the clock thread adds to
   `tempo_bpm` when computing `tick_nanos`. `tempo_bpm` stays clean (UI shows
   the base tempo). The clock thread writes `bpm_jitter` under the write lock
   at the roll-point.
3. **Apply jitter in the clock thread directly**, reading roll parameters from
   the state under a read lock, computing jitter locally, and passing the
   effective BPM to `tick_nanos` without touching state.

**Decision: Option 3 — clock-local jitter, no state mutation.**

Rationale:
- No write-lock needed for BPM jitter; the clock thread holds a read lock only.
- `tempo_bpm` in state always reflects the user's intent (UI shows clean BPM).
- Tempo roll state (phase counter for Up/Down/Breathe/PingPong) lives in a
  clock-local struct, not in `SequencerState` — no lock required.
- Keeps `tick()` free of time-domain logic.

The clock-local roll state:

```rust
struct TempoRollState {
    phase: u64,   // step counter since last roll
    direction: i8, // +1 or -1 for PingPong
    current_offset: i16, // last computed jitter
}
```

The function `compute_bpm_with_jitter(base_bpm, roll_state, rand_params, step_count) -> u16`
is a pure function added to `clock.rs`. It reads `tempo_rand`, `tempo_roll_point`,
`tempo_variance_max`, `tempo_rand_type` from a snapshot. The clock thread's
`rng_seed` for tempo jitter is a **separate** clock-local seed (not the state's
`rng_seed`) so tempo randomness does not consume the state's RNG budget.

---

## 4. New `InputCommand` Variants

```rust
// Shift overlay — modifier params (handled via existing ParamValueDelta flow)
// No new commands needed for R1/R2/R3/S5/S6 — they are regular shift params.

// Shift overlay — action triggers
/// Apply a semitone offset to all steps' notes (Shift: Note Modifier).
/// `semitones` = 0 clears the modifier. Range -12..=12 (semitones) or
/// -96..=-13 / 13..=96 for octave-based jumps (±1–8 oct = ±12..±96 semitones).
NoteModifierSet(i8),

/// Toggle per-step skip modifier on/off (Shift: Skip Modifier).
SkipModifierToggle,

/// Set velocity offset modifier (0 = off). Range 0..=100.
VelocityModifierSet(i8),

/// Randomise all 16 step notes within the current key/mode (Shift: Generate Random Sequence).
GenerateRandomSequence,
```

`NoteModifierSet`, `VelocityModifierSet` are set via the shift overlay param
dials (they have numeric ranges). `SkipModifierToggle` and
`GenerateRandomSequence` are triggered by buttons — they can be mapped to F3/F4
keys or a dedicated "action" key in the shift overlay.

The `apply_command` arms for these:

```rust
InputCommand::NoteModifierSet(s) => { self.note_modifier = s; }
InputCommand::SkipModifierToggle => { self.skip_modifier = !self.skip_modifier; }
InputCommand::VelocityModifierSet(v) => { self.velocity_modifier = v; }
InputCommand::GenerateRandomSequence => { self.generate_random_sequence(); }
```

`generate_random_sequence` is a new `SequencerState` method that randomises all
16 steps' notes within `self.key`/`self.mode` using `next_rand(&mut self.rng_seed)`.

---

## 5. Overlay UI Changes

### 5.1 `SHIFT_PARAMS` constant (new in `ui_render.rs`)

```rust
pub const SHIFT_PARAMS: [&str; 8] = [
    "Note Rnd",    // 0 — note_rand (0–100)
    "Tempo Rnd",   // 1 — tempo_rand (0–100)
    "Roll Point",  // 2 — tempo_roll_point enum
    "Var Max",     // 3 — tempo_variance_max (1–99)
    "Tempo Type",  // 4 — tempo_rand_type enum
    "Step Rnd",    // 5 — step_rand (0–100)
    "Scale Quant", // 6 — scale_quant bool
    "(reserved)",  // 7 — Key Transposition if accepted; empty if deferred
];
```

### 5.2 `render_overlay` for `OverlayMode::Shift`

Replace the "(coming soon)" placeholder with the same span-building loop used
for `OverlayMode::Regular`, but keyed off `SHIFT_PARAMS` and the new
`shift_param_value_string` / `shift_pending_param_value_string` helpers.

Action buttons (Skip Modifier, Generate Random Sequence) rendered below the
param row as `[S]kip  [G]en` labels when shift overlay is open.

### 5.3 `param_count` for modulo wrap

The current `ParamSelectDelta` handler wraps at 8 (`rem_euclid(8)`). The Shift
overlay also has 8 params — no change needed to the wrap logic.

---

## 6. Implementation Work Streams

These streams can be executed in parallel by independent coder agents, subject
to the dependency edges in §7.

### Stream A — RNG Infrastructure (state.rs)
**Files:** `engine/src/state.rs`

A1. Add `rng_seed: u64` to `SequencerState` + `Default`.  
A2. Add private `next_rand` and `prob_hit` helpers.  
A3. Advance `rng_seed` at the top of `tick()` unconditionally.  

No other streams depend on A being complete before starting, but A3 must land
before B and C.

---

### Stream B — Step & Note Randomness in tick() (state.rs)
**Depends on:** A  
**Files:** `engine/src/state.rs`

B1. Add `step_rand: u8`, `note_rand: u8` to `SequencerState`.  
B2. Apply Step Randomness roll in `tick()` (probabilistic mute).  
B3. Apply Note Randomness roll in `tick()` (probabilistic note modifier miss).  
B4. Tests: `step_rand=0` always fires; `step_rand=100` never fires; `note_rand`
    probability distribution is correct over N samples.

---

### Stream C — Shift Overlay Params: Routing Infrastructure (state.rs)
**Depends on:** A (for rng_seed; can start before A3)  
**Files:** `engine/src/state.rs`

C1. Add overlay-aware dispatch: `active_overlay` is already stored; add
    `shift_committed_param_value`, `shift_clamped_param_value`,
    `shift_apply_param_value` methods.  
C2. Route `ParamValueDelta` and `Confirm` arms to correct overlay-specific
    methods based on `self.active_overlay`.  
C3. Add remaining state fields: `scale_quant`, `note_modifier`, `skip_modifier`,
    `velocity_modifier`, `tempo_rand`, `tempo_roll_point`, `tempo_variance_max`,
    `tempo_rand_type` + enums `TempoRollPoint`, `TempoRandType`.  
C4. Implement `shift_committed_param_value` / `shift_clamped_param_value` /
    `shift_apply_param_value` for all 8 shift indices.  
C5. Tests: each shift param round-trips (set via `ParamValueDelta` + `Confirm`,
    read back from state).

---

### Stream D — Shift Action Commands (state.rs + input.rs)
**Depends on:** C (for note_modifier/velocity_modifier/skip_modifier fields)  
**Files:** `engine/src/state.rs`, `engine/src/input.rs`

D1. Add `NoteModifierSet(i8)`, `SkipModifierToggle`, `VelocityModifierSet(i8)`,
    `GenerateRandomSequence` to `InputCommand`.  
D2. Implement `apply_command` arms for all four.  
D3. Implement `generate_random_sequence` helper on `SequencerState`:
    iterate all 16 steps, assign `snap_to_key(random_midi_note, self.key, self.mode)`
    where `random_midi_note` is drawn from `next_rand` in the MIDI range 48–84
    (sensible default range — 3 octaves around C4).  
D4. Tests: `GenerateRandomSequence` produces only in-key notes; `NoteModifierSet(0)`
    clears; `SkipModifierToggle` flips.

---

### Stream E — Note Modifier + Skip + Velocity applied in tick() (state.rs)
**Depends on:** C (for modifier fields), A (for rng_seed)  
**Files:** `engine/src/state.rs`

E1. Apply `note_modifier` in `tick()` (semitone shift, clamped 0–127).  
E2. Apply `skip_modifier` in `tick()` (early return None).  
E3. Apply `velocity_modifier` in `tick()` (velocity offset, clamped 0–127).  
E4. Apply `scale_quant` in `tick()` (snap output note via `snap_to_key`).  
E5. Note Randomness interaction: note modifier applied only when `prob_hit`
    returns true.  
E6. Tests: modifier=0 produces original note/velocity; modifier nonzero with
    rand=100 always applies; modifier nonzero with rand=0 never applies.

---

### Stream F — Tempo Randomness in clock.rs
**Depends on:** C (state fields readable; no write needed from clock)  
**Files:** `engine/src/clock.rs`

F1. Add `TempoRollState` struct (clock-local, no Arc needed).  
F2. Add clock-local LCG seed (`u64`, initialized from wall clock or a fixed constant).  
F3. Implement `compute_effective_bpm(base: u16, roll_state: &mut TempoRollState, params: TempoRandSnapshot, rng: &mut u64, step_count: u64) -> u16`.  
F4. `TempoRandSnapshot` is a small copy of the relevant state fields taken under
    the read lock: `{ tempo_rand: u8, roll_point: TempoRollPoint, variance_max: u8, rand_type: TempoRandType }`.  
F5. Integrate into `run_clock`: sample snapshot under read lock, call
    `compute_effective_bpm`, pass result to `tick_nanos`.  
F6. Tests: `tempo_rand=0` always returns base BPM; `Random` type stays within
    ±variance_max; PingPong bounces.

---

### Stream G — Shift Overlay UI (ui_render.rs)
**Depends on:** C (state fields must exist for value rendering)  
**Files:** `engine/src/ui_render.rs`

G1. Add `SHIFT_PARAMS: [&str; 8]` constant.  
G2. Add `shift_param_value_string(state, index) -> String`.  
G3. Add `shift_pending_param_value_string(index, v: i64) -> String`.  
G4. Replace "(coming soon)" in `render_overlay` with real span-building loop,
    identical structure to `OverlayMode::Regular`.  
G5. Add action label row below the param row when shift overlay is active.  
G6. Tests: `render_frame` with `overlay = Some(OverlayMode::Shift)` does not
    panic; selected param is highlighted.

---

### Stream H — Keyboard Wiring for Shift Actions (input.rs + ui.rs)
**Depends on:** D  
**Files:** `engine/src/input.rs`, `engine/src/ui.rs`

H1. Map keyboard shortcuts for Shift actions when shift overlay is open:  
    - `s` → `SkipModifierToggle`  
    - `g` → `GenerateRandomSequence`  
H2. Update `overlay_key_to_command` (or add a shift-specific variant) for the
    new action keys.  
H3. Update `translate_key` in `ui.rs` to emit the new commands when in shift
    overlay mode.

---

## 7. Dependency Graph

```
A (RNG infra)
├─► B (Step/Note Randomness in tick)
│
├─► C (Shift param routing infra)
│   ├─► D (Shift action commands)
│   │   └─► H (Keyboard wiring)
│   ├─► E (Modifiers in tick)
│   ├─► F (Tempo randomness — clock.rs)
│   └─► G (Shift overlay UI)
```

Streams B, C can start in parallel (C does not need A3 to finish, only A1/A2).
Streams D, E, F, G can all start once C is complete.
H depends only on D.

**Minimum critical path:** A → C → E → (tick fully wired with all modifiers)

---

## 8. Key Transposition — Recommendation

**Recommendation: defer to a follow-up plan.**

Rationale:

1. **Scope creep risk.** Key Transposition ("transpose the whole sequence up/down
   by N semitones") either (a) mutates all step MIDI notes (same as a Key change
   but in semitone space, not scale-degree space) or (b) adds a transposition
   offset applied at play time in `tick()`. Path (a) is destructive and
   hard to undo. Path (b) requires a new `transpose_semitones: i8` field and
   interaction with Scale Quantization.

2. **Design question not yet answered.** Does Key Transposition interact with
   `scale_quant`? Does it transpose before or after quantisation? Does it update
   the displayed `key` in the top bar? These require a design decision by the
   stakeholder before implementation.

3. **Shift param slot 7 is reserved.** The plan leaves index 7 as `"(reserved)"`
   in `SHIFT_PARAMS`. If Key Transposition is accepted, slot 7 maps to
   `transpose_semitones: i8`, range -12..=12, with `committed_param_value`
   / `clamped_param_value` / `apply_param_value` arms following the same pattern.

**If accepted later:** the implementation is straightforward — add field,
extend shift param dispatch, apply in `tick()` after scale_quant. Estimated 0.5
day of coder work.

---

## 9. Scale Quantization vs. Existing `snap_to_key`

`snap_to_key` already exists in `music_theory.rs` and is used by
`snap_all_steps_to_key` (called when Key/Mode changes). Stream E (§6) reuses
it directly: when `scale_quant == true`, the note emitted by `tick()` is
passed through `snap_to_key(note, self.key, self.mode)`. No new music-theory
code is needed — this is a one-line addition in `tick()`.

---

## 10. Acceptance Criteria

### R1 — Note Randomness
- [ ] `note_rand = 0` → note modifier is never applied (existing behaviour preserved)
- [ ] `note_rand = 100` → note modifier is always applied when set
- [ ] `note_rand = 50` → over 1000 ticks, between 40% and 60% of steps apply the modifier (probabilistic tolerance test)
- [ ] Note modifier value is accessible and editable via Shift overlay param 0

### R2 — Tempo Randomness
- [ ] `tempo_rand = 0` → effective BPM always equals `tempo_bpm` (no jitter)
- [ ] `tempo_rand = 100`, `roll_point = Step`, `type = Random`, `variance_max = 20` → effective BPM stays within `tempo_bpm ± 20`
- [ ] `type = PingPong` → BPM bounces monotonically between `tempo_bpm - variance_max` and `tempo_bpm + variance_max`
- [ ] `type = Breathe` → BPM forms a smooth sine-approximated curve within variance bounds
- [ ] `roll_point = Off` → no jitter regardless of other params
- [ ] `tempo_bpm` field in `SequencerState` is never mutated by tempo randomness (clock-local only)
- [ ] All tempo randomness params accessible via Shift overlay params 1–4

### R3 — Step Randomness
- [ ] `step_rand = 0` → all enabled steps fire (existing behaviour preserved)
- [ ] `step_rand = 100` → no enabled steps fire (all probabilistically muted)
- [ ] `step_rand = 50` → over 1000 ticks, between 40% and 60% of enabled steps fire (probabilistic tolerance test)
- [ ] Step randomness param accessible via Shift overlay param 5

### S1 — Note Modifier
- [ ] `note_modifier = 0` → no note pitch change
- [ ] `note_modifier = 7` → each emitted note is 7 semitones above stored note, clamped to 0–127
- [ ] `note_modifier = -12` → each emitted note is 1 octave below stored note, clamped to 0
- [ ] Note modifier is set via `NoteModifierSet` command and displayed in Shift overlay

### S2 — Skip Modifier
- [ ] `skip_modifier = false` → steps fire normally
- [ ] `skip_modifier = true` → no steps fire (all muted)
- [ ] Toggled via `SkipModifierToggle` command

### S3 — Velocity Modifier
- [ ] `velocity_modifier = 0` → velocity unchanged
- [ ] `velocity_modifier = 20` → velocity += 20, clamped to 127
- [ ] `velocity_modifier = -20` → velocity -= 20, clamped to 0
- [ ] Set via `VelocityModifierSet` command

### S4 — Generate Random Sequence
- [ ] All 16 steps' notes are updated to random values
- [ ] Every generated note is in the current key and mode (passes `snap_to_key` identity check)
- [ ] Generated notes fall within MIDI range 48–84 (C3–C6) or a configurable range
- [ ] Triggered via `GenerateRandomSequence` command

### S5 — Scale Quantization
- [ ] `scale_quant = false` → notes emitted as stored (existing behaviour)
- [ ] `scale_quant = true` → every emitted note is snapped to the current key/mode
- [ ] Out-of-key stored notes are snapped on emission, not written back to `StepData`
- [ ] Toggle accessible via Shift overlay param 6

### Cross-cutting
- [ ] `cargo test -p engine` passes with all new tests included
- [ ] `clippy` passes with no new warnings
- [ ] No heap allocation in `tick()` or `run_clock` inner loop
- [ ] `SequencerState` is `Clone` (verify after field additions)
- [ ] All new public items have a doc comment

---

## 11. Risks and Open Questions

| # | Risk / Question | Severity | Mitigation |
|---|-----------------|----------|-----------|
| Q1 | What MIDI note range should `GenerateRandomSequence` use? Hardcoded 48–84 or a configurable param? | Low | Hardcode 48–84 for MVP; add param in a follow-up. |
| Q2 | `Note Modifier` range: ±12 semitones AND ±1–8 octaves are both listed. How is this encoded in a single `i8`? | ~~Medium~~ **RESOLVED** | Single `i8` field storing actual semitone offset. `ParamValueDelta` increments by 1 semitone while abs(value) ≤ 12, then switches to 12-semitone (1 octave) increments beyond ±12. Max range ±96 (8 oct). Display shows semitones for ≤12, octaves for >12. |
| Q3 | `Velocity Modifier` range is listed as 1–100 but MIDI velocity is 0–127. Should it be 1–127? | ~~Low~~ **RESOLVED** | Use 0–127; 0 = off. `velocity_modifier: i8`, field range -127..=127, 0 = no effect. |
| Q4 | Tempo Randomness Breathe/PingPong require a phase counter that persists across ticks. Where is the authoritative state for PingPong direction? | Low | Clock-local `TempoRollState` (see Stream F). Not in `SequencerState` — no serialisation needed for now. |
| Q5 | Does `scale_quant` snap before or after `note_modifier`? | ~~Medium~~ **RESOLVED** | Apply note_modifier first, then scale_quant snap. If modifier pushes note out of key, quantization corrects it. Document in code comment. |
| Q6 | Does `GenerateRandomSequence` also set `enabled = true` for all steps or leave enabled flags alone? | Low | Leave enabled flags alone — user controls which steps fire. |
| Q7 | Key Transposition interaction with Scale Quantization (see §8). | High (deferred) | Defer the item; resolve before implementing. |
| Q8 | `TempoRandType::Breathe` — is a sine approximation (e.g. triangle wave) acceptable? | Low | Triangle wave is simpler and no-alloc. Sine would require a lookup table (256 bytes, stack-allocatable). Clarify before F3 implementation. |

---

## 12. Files Changed Summary

| File | Stream(s) | Nature of change |
|------|-----------|-----------------|
| `engine/src/state.rs` | A, B, C, D, E | New fields on `SequencerState`, new enums, new `InputCommand` arms, updated `tick()`, new helper methods |
| `engine/src/input.rs` | D, H | New `InputCommand` variants, updated key translation |
| `engine/src/clock.rs` | F | `TempoRollState`, `compute_effective_bpm`, updated `run_clock` parameter sampling |
| `engine/src/ui_render.rs` | G | `SHIFT_PARAMS`, shift param value strings, real Shift overlay render |
| `engine/src/ui.rs` | H | Shift action key mappings |

No changes to: `music_theory.rs` (all needed functions already exist), `midi_out.rs`, `sequencer.rs`, `hid.rs`, `cli.rs`, `main.rs`, `lib.rs`.

---

## 13. Recommended Agent Assignments

| Stream | Type | Parallelisable |
|--------|------|----------------|
| A | coder | Start immediately |
| B | coder | After A |
| C | coder | After A1/A2 |
| D | coder | After C |
| E | coder | After C, A |
| F | coder | After C |
| G | coder | After C |
| H | coder | After D |

Maximum parallel agents once C is complete: D, E, F, G (4 agents).
