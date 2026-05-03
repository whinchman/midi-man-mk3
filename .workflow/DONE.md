# Done

Completed work. Features moved here after implementation, review, and merge
to the default branch.

---

## fix/known-bugs — Bug batch BUG-001 through BUG-017 — merged 2026-05-03 (PR #22)

Fixed all 17 known warnings in a single feature branch. Key changes:
- BUG-001: firmware release profile strips debug symbols (`Cargo.toml`)
- BUG-002: `add_nanos_signed` handles second-boundary borrows correctly (`clock.rs`)
- BUG-003: `.cargo/config.toml` /tmp paths removed; local-override pattern documented
- BUG-004: `tick()` uses `step.velocity` instead of hardcoded 100 (`state.rs`)
- BUG-005: `unsafe transmute` replaced with `offset_of!` in hid tests
- BUG-006: `run_hid` zeroes buffer each iteration to prevent stale bytes
- BUG-007: `ratatui` crossterm backend gated behind `hw-io` feature
- BUG-008: `--midi-port`/`--hid-vid`/`--hid-pid` CLI args forwarded to thread functions
- BUG-009: all threads joined on shutdown; clock skipped in non-hw-io builds
- BUG-010: `NoteDelta` accumulates across keypresses using pending note as base
- BUG-011: overlay param display shows human-readable labels, not raw integers
- BUG-012: overlay `Confirm` writes param changes to state fields
- BUG-013: `.cargo/config.toml` comment corrected to `--config` flag
- BUG-014: `loop_out` has its own edit slot in Regular Overlay (8 params total)
- BUG-015: dead `open_device()` removed from `hid.rs`
- BUG-016: `choose_midi_port` warns when filter matches no ports
- BUG-017: overlay `Confirm` `playing=true` clears `paused`
