# Test Plan: fix-ratatui-crossterm-gate

## Goal

Validate that the feature-gate fix for BUG-007 is correctly tested: `crossterm` must not
appear in the dependency tree when `hw-io` is absent, and all UI rendering must be
exercisable through `ratatui::TestBackend` without requiring the `hw-io` feature.

## What already exists

The `engine/tests/ui.rs` suite uses `ratatui::TestBackend` for all rendering tests —
12 tests covering top bar, step rows, overlays, pending edits, and loop bounds.
These tests compile and run without `hw-io`, which already implicitly validates the gate.

## Gap

There is no test that:
1. Explicitly asserts the `ui` module is absent (not compiled) when `hw-io` is missing.
   This is a compile-time property, not directly testable at runtime.
2. Asserts that `ratatui` itself (without crossterm backend) can fully construct a
   `TestBackend` terminal and render a full frame — exercising the `#[cfg(not(feature
   = "hw-io"))]` path end-to-end in a single, self-documenting test.
3. Confirms the `ratatui/crossterm` feature path is absent: verified at
   crate-metadata level via `cargo metadata` (a build-time assertion, not runtime).

## Test strategy

### Unit tests to add (in `engine/tests/ui.rs`)

All tests compile without `hw-io`. They exercise the `ui_render` module through
`TestBackend` — the exact path that must work when crossterm is absent.

| Test | Scenario | Expected |
|---|---|---|
| `test_backend_renders_without_hw_io` | Render a default state with TestBackend | Frame draws without panic; buffer is non-empty |
| `test_backend_terminal_clear_and_redraw` | Clear terminal then redraw | Second draw produces same content as first |
| `render_all_sixteen_steps_renders_full_row` | State with all 16 steps enabled | Note row contains note names for all enabled steps |
| `top_bar_shows_channel_number` | State with midi_channel = 3 | Top bar contains "Ch: 3" |

### What cannot be unit-tested

- The absence of the `crossterm` crate in the binary when `hw-io` is off — this is a
  cargo/linker property, verified by `cargo tree` and acceptance criteria, not by
  runtime assertions.
- `run_ui` function — it requires a real tty and the `hw-io` feature; it is already
  gated behind `#[cfg(feature = "hw-io")]` in lib.rs.

## Dependencies / fixtures

- `SequencerState::default()` — sufficient for the new tests.
- `ratatui::backend::TestBackend`, `ratatui::Terminal` — already used by existing tests.
- `engine::ui_render::render_frame` — already imported.
