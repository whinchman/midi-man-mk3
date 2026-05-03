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

---

## BUG-019 — compute_effective_bpm: clippy::manual_is_multiple_of warnings (AC failure)

**Severity:** warning
**File:** `engine/src/clock.rs` — `compute_effective_bpm`, lines 114–115
**Branch:** `feature/randomness-layer/randomness-f-tempo-randomness-clock`

**Description:** Lines 114–115 use `step_count % 4 == 0` and `step_count % 16 == 0` instead of the idiomatic `.is_multiple_of(4)` / `.is_multiple_of(16)`. Clippy emits two `manual_is_multiple_of` warnings. The task AC requires `clippy` to pass with no new warnings; this is a hard AC failure blocking merge.

**Reproduction:** `cargo clippy -p engine` on the branch produces two `warning: manual implementation of .is_multiple_of()` diagnostics at lines 114 and 115.

**Fix:** Replace `step_count % 4 == 0` with `step_count.is_multiple_of(4)` and `step_count % 16 == 0` with `step_count.is_multiple_of(16)`.

---

## BUG-020 — compute_effective_bpm: clippy::let_and_return in Breathe branch (AC failure)

**Severity:** warning
**File:** `engine/src/clock.rs` — `compute_effective_bpm`, lines 154–161
**Branch:** `feature/randomness-layer/randomness-f-tempo-randomness-clock`

**Description:** The Breathe falling-half branch assigns the result to `let pos = …` then immediately returns `pos`. Clippy emits a `let_and_return` warning. The task AC requires `clippy` to pass with no new warnings; this is a hard AC failure blocking merge.

**Reproduction:** `cargo clippy -p engine` on the branch produces `warning: returning the result of a let binding from a block` at line 154.

**Fix:** Remove the `let pos =` binding and use the if-expression directly as the block value:
```rust
} else {
    // Falling half: 0 → -vm → 0
    let phase2 = phase - half;
    let half2 = cycle - half;
    if phase2 < half2 / 2 {
        -(phase2 as i64 * vm as i64 / (half2 as i64 / 2).max(1)) as i16
    } else {
        let ascend_phase = phase2 as i64 - half2 as i64 / 2;
        let ascend_len = (half2 as i64 - half2 as i64 / 2).max(1);
        (-(vm as i64) + ascend_phase * vm as i64 / ascend_len) as i16
    }
}
```
