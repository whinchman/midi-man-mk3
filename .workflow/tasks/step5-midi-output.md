# Task: MIDI Output

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 3
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/midi-output
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 5
- **Dependencies**: step3-sequencer-state-and-engine

## Description

Implement `engine/src/midi_out.rs`. This thread receives `MidiEvent` values from the clock thread via a `Receiver<MidiEvent>` channel and dispatches them as MIDI messages over ALSA using the `midir` crate. All MIDI messages are assembled on the stack as `[u8; 3]` arrays — no heap allocation on the send path. The thread exits cleanly when the sender drops.

## Acceptance Criteria

- [ ] `pub fn run_midi_out(rx: Receiver<MidiEvent>)` implemented in `engine/src/midi_out.rs`.
- [ ] Opens the first available ALSA MIDI output port via `midir::MidiOutput::new()` and `connect()`; logs the port name to stdout at startup.
- [ ] If no MIDI ports are available, logs an error and returns (does not panic).
- [ ] `MidiEvent::NoteOn { channel, note, velocity }` encoded as `[0x90 | channel, note, velocity]` and sent via `connection.send()`.
- [ ] `MidiEvent::NoteOn` handling: send note-on bytes immediately, then spawn a short-lived `std::thread` that sleeps `duration_nanos` (via `std::thread::sleep(Duration::from_nanos(duration_nanos))`) then sends `[0x80 | channel, note, 0]` (NoteOff) on a clone of the `MidiOutputConnection`. The spawned thread is fire-and-forget; no join handle needed.
- [ ] `MidiEvent::NoteOff { channel, note }` encoded as `[0x80 | channel, note, 0]` and sent (kept for direct use if ever needed).
- [ ] `MidiEvent::Start` encoded as `[0xFA]` and sent (single-byte message).
- [ ] `MidiEvent::Stop` encoded as `[0xFC]` and sent.
- [ ] `MidiEvent::Continue` encoded as `[0xFB]` and sent.
- [ ] All messages assembled on the stack — no `Vec` or `Box` on the send path.
- [ ] Thread exits when channel `rx` is disconnected (i.e. `recv()` returns `Err`).
- [ ] Unit test (or integration test with a virtual ALSA port via `aconnect`/`alsa` crate): verify that `NoteOn`, `NoteOff`, `Start`, `Stop`, and `Continue` produce the correct byte sequences. A mock or trait-based test is acceptable if a virtual port is not available in CI.
- [ ] `cargo test -p engine` passes.

## Interface Contracts

```rust
// engine/src/midi_out.rs

use std::sync::mpsc::Receiver;
use crate::state::MidiEvent;

pub fn run_midi_out(rx: Receiver<MidiEvent>);
```

`MidiEvent` enum (from Step 3, `engine/src/state.rs`):
```rust
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8, duration_nanos: u64 },
    NoteOff { channel: u8, note: u8 },
    Start,
    Stop,
    Continue,
}
```

MIDI message byte encodings (from plan Section 5):
| Event     | Bytes              |
|-----------|--------------------|
| Note On   | `0x90|ch, note, vel` |
| Note Off  | `0x80|ch, note, 0`  |
| Clock     | `0xF8` (not in MVP) |
| Start     | `0xFA`              |
| Stop      | `0xFC`              |
| Continue  | `0xFB`              |

Channel is fixed to 0 for MVP (from plan Section 5).

## Context

From plan Section 5: `midir` 0.10 with ALSA backend. `MidiOutputConnection::send(&[u8])` copies into a kernel buffer — the slice lives on the stack, no dynamic allocation.

The MIDI output thread is decoupled from the clock thread via a `SyncSender<MidiEvent>` / `Receiver<MidiEvent>` pair. The clock thread produces events; this thread consumes them. Thread join ordering in `main.rs` is handled in Step 9.

Note-off ownership: `midi_out.rs` owns note-off scheduling. On `NoteOn`, it sends the note-on immediately, then spawns a short-lived thread that sleeps `duration_nanos` and sends NoteOff. `midir::MidiOutputConnection` is `Send` — clone it for the spawned thread. This design supports future per-note duration (e.g. gate length) without changes to the clock or sequencer.

Build-time note: `midir` with the ALSA backend requires `libasound2-dev` installed on the build host. This is a known risk from the plan (Section 10). Document this requirement in a comment at the top of `midi_out.rs`.

## Notes

