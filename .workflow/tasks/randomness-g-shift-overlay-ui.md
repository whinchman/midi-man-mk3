# Task: Shift Overlay UI

- **Type**: coder
- **Status**: done
- **Repo**: midi-man-mk3
- **Parallel Group**: 3
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-g-shift-overlay-ui
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream G
- **Dependencies**: randomness-c-shift-param-routing

## Description

Replace the "(shift mode — coming soon)" placeholder in `engine/src/ui_render.rs`
with a real Shift overlay that renders the 8 named shift parameters using the
same span-building pattern as the Regular overlay.

Stream C must be merged before this branch is cut (it adds all new
`SequencerState` fields needed for value rendering).

### Changes to engine/src/ui_render.rs

**1. Add `SHIFT_PARAMS` constant:**

```rust
/// Shift overlay parameter names (index 0–7).
pub const SHIFT_PARAMS: [&str; 8] = [
    "Note Rnd",    // 0 — note_rand (0–100)
    "Tempo Rnd",   // 1 — tempo_rand (0–100)
    "Roll Point",  // 2 — tempo_roll_point enum
    "Var Max",     // 3 — tempo_variance_max (1–99)
    "Tempo Type",  // 4 — tempo_rand_type enum
    "Step Rnd",    // 5 — step_rand (0–100)
    "Scale Quant", // 6 — scale_quant bool
    "(reserved)",  // 7 — Key Transposition if accepted; empty for now
];
```

**2. Add `shift_param_value_string` helper:**

Returns the current committed value for shift param `index` as a display string.

```rust
/// Return the display string for shift param `index` given the current state.
pub fn shift_param_value_string(state: &SequencerState, index: u8) -> String;
```

Display format per index:
- 0 (`note_rand`): `"0"` – `"100"`
- 1 (`tempo_rand`): `"0"` – `"100"`
- 2 (`tempo_roll_point`): `"Off"` / `"Step"` / `"Beat"` / `"Seq"`
- 3 (`tempo_variance_max`): `"1"` – `"99"`
- 4 (`tempo_rand_type`): `"Random"` / `"Up"` / `"Down"` / `"Breathe"` / `"PingPong"`
- 5 (`step_rand`): `"0"` – `"100"`
- 6 (`scale_quant`): `"Off"` / `"On"`
- 7 (reserved): `"—"`

**3. Add `shift_pending_param_value_string` helper:**

Returns the pending (not-yet-confirmed) value for shift param `index` as a
display string. `v` is the raw `i64` from `PendingEdit::Param { value, .. }`.

```rust
/// Return the display string for a pending shift param edit.
pub fn shift_pending_param_value_string(index: u8, v: i64) -> String;
```

Same display format as above, but derived from `v` (the uncommitted value),
not from state fields directly.

**4. Update `render_overlay` for `OverlayMode::Shift`:**

Replace the "(coming soon)" placeholder with the same span-building loop used
for `OverlayMode::Regular`, keyed off `SHIFT_PARAMS`:

- For each of the 8 shift params:
  - If `state.pending_edit == PendingEdit::Param { overlay: OverlayMode::Shift, index: i, value: v }`:
    display `shift_pending_param_value_string(i, v)` highlighted.
  - Otherwise: display `shift_param_value_string(state, i)`.
  - If `state.selected_param == i` and the shift overlay is active: highlight
    the param name.

**5. Add action label row:**

Below the param row, when the Shift overlay is active, render a single-line
label row showing available action buttons:

```
[S]kip  [G]en
```

Use a dimmed style to distinguish from param values.

## Acceptance Criteria

- [ ] `SHIFT_PARAMS` constant exists with 8 entries matching the index map
- [ ] `shift_param_value_string` returns correct display strings for all 8 indices
- [ ] `shift_pending_param_value_string` returns correct display strings for all 8 indices from raw i64 values
- [ ] `render_frame` with `overlay = Some(OverlayMode::Shift)` does not panic
- [ ] The selected shift param (matching `state.selected_param`) is highlighted
- [ ] A pending edit value is shown highlighted, distinct from the committed value
- [ ] The `[S]kip  [G]en` action label row is visible when the Shift overlay is open
- [ ] The "(coming soon)" placeholder is fully removed
- [ ] `cargo test -p engine` passes including a test that calls `render_frame` with Shift overlay open
- [ ] `clippy` passes with no new warnings
- [ ] All new public items have a doc comment

## Interface Contracts

Consumed from Stream C (`engine/src/state.rs`):

```rust
// SequencerState fields read for display:
pub note_rand: u8,
pub tempo_rand: u8,
pub tempo_roll_point: TempoRollPoint,
pub tempo_variance_max: u8,
pub tempo_rand_type: TempoRandType,
pub step_rand: u8,
pub scale_quant: bool,
```

Consumed from Stream C enums:

```rust
pub enum TempoRollPoint { Off, Step, Beat, Seq }
pub enum TempoRandType  { Random, Up, Down, Breathe, PingPong }
```

Existing render infrastructure (unchanged):

```rust
// engine/src/ui_render.rs
pub const REGULAR_PARAMS: [&str; 8];
pub fn render_frame(f: &mut Frame, state: &SequencerState, overlay: Option<OverlayMode>);
// PendingEdit::Param { overlay: OverlayMode, index: u8, value: i64 }
```

## Context

- File: `engine/src/ui_render.rs`
- `REGULAR_PARAMS` constant already defined; `SHIFT_PARAMS` follows the same pattern.
- `render_overlay` function renders both overlay modes; it contains the
  "(coming soon)" placeholder for `OverlayMode::Shift`.
- The Regular overlay render loop uses `PendingEdit::Param` to check for
  pending edits and highlight them — replicate this for the Shift overlay.
- Use `ratatui` span/style primitives already imported; no new crate dependencies.
- Code standard: no `unsafe`, `clippy` clean.

## Notes

### Implementation Summary (Stream G)

**Branch**: `randomness-g-shift-overlay-ui` (worktree at `.workflow/worktrees/randomness-g-shift-overlay-ui`)

**Changes to `engine/src/ui_render.rs`:**
- Added `SHIFT_PARAMS: [&str; 8]` constant with all 8 shift param names
- Added `shift_param_value_string(state, index)` — returns committed value display string for all 8 indices using `SequencerState` fields from Stream C
- Added `shift_pending_param_value_string(index, v)` — returns pending value display string from raw `i64`
- Added `tempo_roll_point_name` and `tempo_rand_type_name` private helpers
- Replaced `(shift mode — coming soon)` placeholder in `render_overlay` with a full span-building loop matching the Regular overlay pattern
- Added `[S]kip  [G]en` action label row (dimmed style) below the param row
- Bumped Shift overlay height from 3 to 4 to accommodate the extra action label row
- Added `TempoRollPoint` and `TempoRandType` imports

**Changes to `engine/tests/ui.rs`:**
- Updated `overlay_shift_shows_coming_soon` and `overlay_shift_shows_shift_mode_text` to assert on new content
- Added 15 new tests: `shift_param_value_string_*` (8), `shift_pending_param_value_string_*` (5), `shift_overlay_pending_edit_shown_in_render`, `shift_overlay_render_frame_does_not_panic`

**Test results**: 134 tests total (89 unit + 45 integration), all pass. Clippy clean. Release build succeeded.

