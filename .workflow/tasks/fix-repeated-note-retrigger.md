# Task: fix-repeated-note-retrigger

**Type:** coder
**Status:** pending
**Feature Branch:** feature/fix-repeated-note-retrigger
**Branch:** feature/fix-repeated-note-retrigger/fix-repeated-note-retrigger
**Base Branch:** feature/fix-repeated-note-retrigger
**Parallel Group:** 1

## Goal

When two or more consecutive enabled steps share the same MIDI note, all of
them must play audibly. Currently only the first plays; subsequent identical
notes are silently dropped.

## Root Cause

`dispatch` in `midi_out.rs` sends `NoteOn` then spawns a thread to send
`NoteOff` after `duration_nanos`. When the next step fires the same pitch
before that delayed `NoteOff` arrives, the MIDI device ignores the second
`NoteOn` (the note is already held). The delayed `NoteOff` from step N then
silences everything.

## Fix

**File:** `engine/src/clock.rs` — `run_clock`

Add `last_note: Option<(u8, u8)>` (channel, note) local to the loop.
When a new `NoteOn` would repeat the same (channel, note) as `last_note`,
send `MidiEvent::NoteOff { channel, note }` on the MIDI channel immediately
before the `NoteOn`. Then send the `NoteOn` as normal and update `last_note`.

No changes to `SequencerState`, `dispatch`, or `midi_out.rs` are needed.

The relevant section of `run_clock` to modify (around line 170):

```rust
// Before:
if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = maybe_event {
    let event = MidiEvent::NoteOn { channel, note, velocity, duration_nanos: period };
    if midi_tx.send(event).is_err() { break; }
}

// After:
if let Some(MidiEvent::NoteOn { channel, note, velocity, .. }) = maybe_event {
    // Retrigger: if the same note is still held, send NoteOff first.
    if last_note == Some((channel, note)) {
        if midi_tx.send(MidiEvent::NoteOff { channel, note }).is_err() { break; }
    }
    let event = MidiEvent::NoteOn { channel, note, velocity, duration_nanos: period };
    if midi_tx.send(event).is_err() { break; }
    last_note = Some((channel, note));
}
```

Declare `last_note` before the loop:
```rust
let mut last_note: Option<(u8, u8)> = None;
```

## Acceptance Criteria

- [ ] Two consecutive steps with the same MIDI note both produce audible output
- [ ] Non-repeated notes are unaffected
- [ ] Disabled steps do not update `last_note` (no phantom NoteOff)
- [ ] All existing `cargo test -p engine` tests pass
- [ ] `clippy` passes with no new warnings

## Notes

