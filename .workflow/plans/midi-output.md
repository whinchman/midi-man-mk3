# Plan: MIDI Output (Step 5)

## Overview

Implement `engine/src/midi_out.rs` — a thread function that receives `MidiEvent`
values from a channel and dispatches them as raw MIDI bytes via `midir` over ALSA.
Note-off scheduling is owned here: on NoteOn, send immediately, then spawn a
short-lived thread that sleeps `duration_nanos` and sends NoteOff.

All MIDI messages are fixed-size stack arrays. No heap on the send path.

## Steps

### Step 1: Update lib.rs to declare state, sequencer, and midi_out modules

Files: `engine/src/lib.rs`

- Add `pub mod state;`, `pub mod sequencer;`, `pub mod midi_out;`

### Step 2: Implement midi_out.rs

Files: `engine/src/midi_out.rs`

- `pub fn run_midi_out(rx: Receiver<MidiEvent>)`
- Open first available ALSA MIDI output port via midir
- Log port name on success; log error and return if no ports
- Loop on `rx.recv()`:
  - `NoteOn`: send `[0x90|ch, note, vel]` immediately; clone connection, spawn thread sleeping `duration_nanos`, send `[0x80|ch, note, 0]`
  - `NoteOff`: send `[0x80|ch, note, 0]`
  - `Start`: send `[0xFA]`
  - `Stop`: send `[0xFC]`
  - `Continue`: send `[0xFB]`
  - `Err(_)` from recv: break (sender dropped)

### Step 3: Unit tests (trait-based mock)

Files: `engine/src/midi_out.rs` (test module) or separate test file

- Define `MidiSender` trait with `send(&mut self, data: &[u8])` and `try_clone(&self)`
- Implement with a `VecSender` that collects bytes for assertions
- Test all five event types produce correct byte sequences
- Test NoteOff thread is spawned correctly (via sleep + mock)

### Step 4: Cargo.toml update

- Ensure `midir = "0.11"` is in dependencies (already present)
- Add env var workaround for ALSA pkg-config path in `.cargo/config.toml`

### Step 5: Verify build and tests pass

- `PKG_CONFIG_PATH=/tmp/alsa-pkg cargo test -p engine`
- `PKG_CONFIG_PATH=/tmp/alsa-pkg cargo build -p engine --release`
