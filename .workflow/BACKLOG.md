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
- [ ] NEW - Shift Mode: Key Transposition?

- [x] Physical component research — BOM complete at .workflow/plans/midi-man-mk3-bom.md. All 18 knobs are PEC11R encoders (detent feel preferred). ~$97 Digikey order. MCP23017s ordered x6, encoders x20.

## Changes

Updates or improvements to existing functionality.
- [x] When Key or Mode changes, we should shift the current notes to the nearest note within the new key/mode. Note for selecting a note we should go always "round" to the lower note. → refined, see .workflow/plans/key-mode-note-shifting.md

## Issues

Possible bugs, regressions, or things that feel broken or degraded.

<!-- All three issues (note cycling, overlay display, overlay functionality) fixed in PR #22 (BUG-010, BUG-011, BUG-012, BUG-014, BUG-017) -->
- [ ] there's a mis-match on the regular overlay menu - every time you wrap the menu, the high-lighted item and the item that actually changes gets wider and wider - see .workflow/bug-img/bug-img.jpg .workflow/bug-img/bug-img2.jpg for a clearer view.
