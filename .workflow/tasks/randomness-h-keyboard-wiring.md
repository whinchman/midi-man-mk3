# Task: Keyboard Wiring for Shift Actions

- **Type**: coder
- **Status**: done
- **Repo**: midi-man-mk3
- **Parallel Group**: 4
- **Feature Branch**: feature/randomness-layer
- **Branch**: feature/randomness-layer/randomness-h-keyboard-wiring
- **Base Branch**: feature/randomness-layer
- **Source Item**: Randomness Layer — Stream H
- **Dependencies**: randomness-d-shift-action-commands

## Description

Wire the four Shift action commands to keyboard shortcuts in
`engine/src/input.rs` and `engine/src/ui.rs` so they are reachable when the
Shift overlay is open.

Stream D must be merged before this branch is cut (it adds `NoteModifierSet`,
`SkipModifierToggle`, `VelocityModifierSet`, `GenerateRandomSequence` to
`InputCommand`).

### Keyboard mappings when Shift overlay is active

| Key | Command |
|-----|---------|
| `s` | `SkipModifierToggle` |
| `g` | `GenerateRandomSequence` |

These are overlay-specific shortcuts — they only fire when
`active_overlay == Some(OverlayMode::Shift)`.

### Changes to engine/src/input.rs

Add a new pure translation function for Shift overlay action keys:

```rust
/// Translate a key event to a Shift overlay action command.
///
/// Called only when the Shift overlay is active. Returns `None` for keys that
/// are not Shift actions (caller falls through to `overlay_key_to_command`).
pub fn shift_action_key_to_command(key_code: KeyCodeSimple) -> Option<InputCommand> {
    match key_code {
        KeyCodeSimple::Char('s') | KeyCodeSimple::Char('S') => {
            Some(InputCommand::SkipModifierToggle)
        }
        KeyCodeSimple::Char('g') | KeyCodeSimple::Char('G') => {
            Some(InputCommand::GenerateRandomSequence)
        }
        _ => None,
    }
}
```

### Changes to engine/src/ui.rs

In the keyboard event loop (the section that translates key events to
`InputCommand` values), add a dispatch step for the Shift overlay:

When the active overlay is `Some(OverlayMode::Shift)`:
1. Try `shift_action_key_to_command(key)` first.
2. If it returns `Some(cmd)`, send that command.
3. Otherwise fall through to `overlay_key_to_command(key)` (for Left/Right/Up/Down/Enter/Esc).

This preserves the existing param-navigation behaviour for arrow keys while
adding the action shortcuts for `s` and `g`.

## Acceptance Criteria

- [ ] `shift_action_key_to_command` exists in `input.rs` as a pure function
- [ ] `Char('s')` / `Char('S')` → `SkipModifierToggle` when Shift overlay is active
- [ ] `Char('g')` / `Char('G')` → `GenerateRandomSequence` when Shift overlay is active
- [ ] Arrow keys and Enter/Esc still work for param navigation in the Shift overlay (not overridden)
- [ ] Neither `s` nor `g` triggers Shift actions when Regular overlay or no overlay is active
- [ ] `shift_action_key_to_command` is covered by unit tests (pure function, no terminal needed)
- [ ] `cargo test -p engine` passes
- [ ] `clippy` passes with no new warnings
- [ ] All new public items have a doc comment

## Interface Contracts

Consumed from Stream D (`engine/src/input.rs`):

```rust
pub enum InputCommand {
    // … existing variants …
    SkipModifierToggle,
    GenerateRandomSequence,
}
```

Existing key translation functions (unchanged, called as fallthrough):

```rust
// engine/src/input.rs
pub fn overlay_key_to_command(key_code: KeyCodeSimple) -> Option<InputCommand>;
pub fn root_key_to_command(key_code: KeyCodeSimple, shift: bool) -> Option<InputCommand>;
```

## Context

- File: `engine/src/input.rs` — `overlay_key_to_command` at line ~79;
  `root_key_to_command` at line ~56; `KeyCodeSimple` enum at line ~94.
- File: `engine/src/ui.rs` — keyboard event loop reads crossterm events,
  converts to `KeyCodeSimple`, calls translation functions, sends on `cmd_tx`.
- The Shift overlay is opened by `InputCommand::OpenOverlay(OverlayMode::Shift)`;
  `state.active_overlay` tracks the current overlay. In `ui.rs`, the UI thread
  can track the active overlay locally (it already does this for rendering
  decisions) or read it from state.
- Code standard: no `unsafe`, `clippy` clean.

## Notes

### Implementation summary

- **Branch**: `randomness-h-keyboard-wiring` (worktree based off `feature/randomness-layer` + Stream D merged in)
- **engine/src/input.rs**: Added `shift_action_key_to_command(key_code: KeyCodeSimple) -> Option<InputCommand>` — pure function mapping `'s'`/`'S'` → `SkipModifierToggle` and `'g'`/`'G'` → `GenerateRandomSequence`; all other keys return `None`. Added 8 unit tests in an inline `#[cfg(test)]` module covering all four key variants, arrow/enter/esc returning `None`, other chars returning `None`, and a sanity check that `overlay_key_to_command` fallthrough still works.
- **engine/src/ui.rs**: Updated `translate_key` to import and call `shift_action_key_to_command` when `ui.overlay == Some(OverlayMode::Shift)`. The Shift arm tries the action function first; if it returns `None` it falls through to `overlay_key_to_command` so arrow/enter/esc param-navigation is preserved. Regular overlay and no-overlay paths are unchanged.
- **Test results**: `cargo test -p engine` — 92 unit + all integration tests passed (0 failures). `cargo build -p engine --release` clean. `cargo clippy -p engine` clean (0 warnings).
