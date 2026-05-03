# Backlog

Raw, unprocessed features and ideas. Items here have not been researched,
planned, or estimated. This is the intake queue.

Add items as `[ ]` checkbox entries under the appropriate section. The
Coordinator runs the Refinement stage (Architect → Designer → Manager) to
research, plan, and decompose items into tasks, then moves them to TODO.md.

---

## Features

New functionality that does not currently exist.

## Post-MVP / Randomness Layer

- [ ] Note Randomness (0-100) — chance each step note modifiers apply
- [ ] Tempo Randomness (0-100) — roll point (off/step/beat/seq), variance max (1-99), type (random/up/down/breathe/pingpong)
- [ ] Step Randomness (0-100) — chance each step modifier applies
- [ ] Shift mode: Note Modifier (off / ±1-12 semitones / 1-8 oct)
- [ ] Shift mode: Skip Modifier (off/on)
- [ ] Shift mode: Velocity Modifier (off / 1-100 offset)
- [ ] Shift mode: Generate Random Sequence
- [ ] Shift mode: Scale Quantization toggle

- [x] Physical component research — BOM complete at .workflow/plans/midi-man-mk3-bom.md. All 18 knobs are PEC11R encoders (detent feel preferred). ~$97 Digikey order. MCP23017s ordered x6, encoders x20.

## Changes

Updates or improvements to existing functionality.

## Issues

Possible bugs, regressions, or things that feel broken or degraded.
- [ ] changing notes with up/down on keyboard only allows for changing 1 note up or down from current note. (pressing enter properly sets new note though, but we should be able to cycle all the way up or down)
- [ ] in Regular Overlay mode - all selections show a number instead of proper selection type (ie [key:C->1] not [Key:C->D]).
- [ ] in regular overlay mode - none of the options actually work.