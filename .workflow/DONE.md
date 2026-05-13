# DONE

## cli-commands (issue #99) — merged PR #110 — 2026-05-13
8 new CLI commands (`rand all/velo/notes`, `note set`, `port list`, `clear`/`ok`,
`help`) wired through ui → state → midi-out. `parse_note_name` added as the
inverse of `note_name`. MIDI `ListPorts` enumeration via control channel with
sentinel-based UI rendering. Six Copilot review nits resolved in PR #111
(strict 1–16 step indexing, trailing-token rejection, ports-sentinel doc
cleanup, indexed-fallback port names, `ok` alias in HELP_ENTRIES). 640
engine tests passing.

## ui-refactor (issue #78) — merged PR #93 — 2026-05-03
4-panel cyberpunk TUI with focus model, CLI panel, runtime MIDI port/channel
switching, HID compatibility, and clock NoteOff race fix (PR #93 + hotfixes).
