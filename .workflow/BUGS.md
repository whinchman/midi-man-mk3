# Bugs

Known bugs discovered by QA and Code Reviewer agents. Each bug should have
enough detail for a Coder agent to reproduce and fix it.

Bugs here follow the same approval flow as features — the stakeholder moves
approved fixes to TODO.md (removing them from this file).

---

<!-- BUG-001 through BUG-017 fixed and merged in PR #22 (fix/known-bugs → main, 2026-05-03) -->

## BUG-018 — repeated consecutive notes: only first plays

**Severity:** warning
**File:** `engine/src/clock.rs` — `run_clock`

**Description:** When two or more consecutive enabled steps share the same MIDI note, only the first plays audibly. Subsequent identical notes are silently dropped.

**Root cause:** `dispatch` in `midi_out.rs` sends `NoteOn` then spawns a thread to send `NoteOff` after `duration_nanos`. When the next step fires the same pitch before that `NoteOff` arrives, most MIDI devices ignore the second `NoteOn` (note is already held). The delayed `NoteOff` from step N then silences both.

**Fix:** In `run_clock`, track `last_note: Option<(u8, u8)>` (channel, note). When a new `NoteOn` matches `last_note`, send `MidiEvent::NoteOff` on the channel before the `NoteOn` to retrigger the note. No changes to `SequencerState` needed.
