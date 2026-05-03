# Task: Engine main.rs Wiring

- **Type**: coder
- **Status**: done
- **Repo**: midi-man-mk3
- **Parallel Group**: 6
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/engine-main-wiring
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 9
- **Dependencies**: step4-clock-thread, step5-midi-output, step7-hid-host-reader-writer, step8-terminal-ui

## Description

Implement `engine/src/main.rs` to wire all engine threads together into a working binary. Parse CLI arguments, initialize shared state, create channels, spawn all threads in the correct order, and block on the UI thread. Handle clean shutdown: send MIDI Stop, close the HID device, and join threads.

When this step is complete, `cargo run -p engine` should produce a working 16-step sequencer playable from the keyboard alone (no Pico required).

## Acceptance Criteria

- [ ] CLI argument parsing implemented (use `std::env::args()` or a minimal arg parser — no heavy CLI crate needed for MVP): `--midi-port <name>`, `--hid-vid <hex>`, `--hid-pid <hex>`, all optional with defaults (first available MIDI port, `HID_VID`/`HID_PID` constants from `hid.rs`).
- [ ] `SequencerState::default()` initialized and wrapped in `Arc<RwLock<SequencerState>>`.
- [ ] Channels created: `SyncSender<MidiEvent>` / `Receiver<MidiEvent>` for clock→midi_out; `SyncSender<InputCommand>` / `Receiver<InputCommand>` for hid+keyboard→sequencer; `SyncSender<()>` / `Receiver<()>` for hid→ui notify.
- [ ] Threads spawned in order: `run_midi_out` → `run_clock` → `run_hid` → `run_ui`. Each receives the correct `Arc` clones and channel endpoints.
- [ ] A dedicated command-processor thread (or integrated into the clock thread, or the main thread) consumes `InputCommand` from the channel, acquires write lock on state, calls `state.apply_command(cmd)`, then releases lock and sends on `ui_notify`.
- [ ] `main` blocks on `ui_thread.join()` — when the UI thread exits (Ctrl-C), cleanup begins.
- [ ] On exit: send `MidiEvent::Stop` on the midi channel; drop all senders so threads detect disconnection and exit.
- [ ] Smoke test: start the engine, let it run for 100 ms without spawning any actual audio/MIDI devices (use a flag or environment variable to enable no-op stubs for midir and hidapi in test builds). Assert no panic.
- [ ] `cargo run -p engine` produces a working sequencer with keyboard controls.
- [ ] `cargo test -p engine` passes all tests from all modules.
- [ ] `cargo build -p engine --release` succeeds for `x86_64-unknown-linux-gnu`.

## Interface Contracts

Thread entry points wired in `main.rs`:

```rust
// engine/src/clock.rs
pub fn run_clock(state: Arc<RwLock<SequencerState>>, midi_tx: SyncSender<MidiEvent>);

// engine/src/midi_out.rs
pub fn run_midi_out(rx: Receiver<MidiEvent>);

// engine/src/hid.rs
pub fn run_hid(
    cmd_tx: SyncSender<InputCommand>,
    state: Arc<RwLock<SequencerState>>,
    ui_notify: SyncSender<()>,
);

// engine/src/ui.rs
pub fn run_ui(
    state: Arc<RwLock<SequencerState>>,
    notify: Receiver<()>,
    cmd_tx: SyncSender<InputCommand>,
);
```

`SequencerState::apply_command(cmd: InputCommand)` — from Step 6b, `engine/src/state.rs`.

Command processor pattern:
```rust
// Spawned as its own thread or run in the main thread before joining ui_thread:
loop {
    match cmd_rx.recv() {
        Ok(cmd) => {
            let mut state = state_arc.write().unwrap();
            state.apply_command(cmd);
            let _ = ui_notify_tx.try_send(());
        }
        Err(_) => break,
    }
}
```

## Context

From plan Section 8, Step 9:

The main thread serves as the process lifecycle owner:
1. Parse CLI args.
2. Initialize state.
3. Create channels.
4. Spawn threads.
5. Join on UI thread (blocks here).
6. Cleanup on return.

Thread spawn order matters for channel ownership: `run_midi_out` takes ownership of `midi_rx`; `run_clock` takes `midi_tx`; `run_hid` takes `cmd_tx` (clone) and `ui_notify_tx` (clone); `run_ui` takes `notify_rx` and `cmd_tx` (clone or move); the command processor takes `cmd_rx`.

HID is optional: `run_hid` exits immediately if the device is not found. This is already handled in Step 7 — `main.rs` spawns the HID thread unconditionally; it will self-terminate if no device is present.

The full Phase 1 acceptance criteria from the plan (Section 9, Engine block) apply to the output of this step, since this is the final integration step.

Build dependency note: `libasound2-dev` must be installed for `midir` to link. Document in a comment at the top of `main.rs` or in a `README` if one exists.

## Notes

**Branch**: `engine-main-wiring` (based on `feature/engine-phase1`)

**Test results**: 219 passed (214 pre-existing + 5 new smoke tests in main.rs), 0 failed.

**Build result**: `cargo build -p engine --release` succeeded on x86_64-unknown-linux-gnu.

**Implementation summary**:
- `engine/src/main.rs` wires 5 threads in order: midi-out (hw-io), clock, hid (hw-io), cmd-processor, ui (hw-io).
- CLI args `--midi-port`, `--hid-vid`, `--hid-pid` parsed via `std::env::args()` with a `parse_hex_u16` helper.
- `SequencerState::default()` wrapped in `Arc<RwLock<>>` and shared via clones.
- Three `mpsc::sync_channel` channels: `MidiEvent` (clock→midi_out), `InputCommand` (hid/ui→cmd-proc), `()` (cmd-proc→ui notify).
- Command processor thread: `recv()` loop → write lock → `apply_command` → `try_send(())` on notify.
- On hw-io builds: `main` blocks on `ui_thread.join()`; cleanup sends `MidiEvent::Stop` then drops senders.
- On non-hw-io builds: midi_rx and ui_notify_rx are dropped; `main` returns immediately (useful for test builds).
- Build comment at top of `main.rs` documents ALSA dependency.

**Notable decisions**:
- Used `#[cfg(not(feature = "hw-io"))]` to drop `midi_rx` immediately so the clock thread's `send()` fails fast if the MIDI receiver is gone in no-hw-io builds.
- `ui_notify_tx` is dropped after spawning the cmd-processor so `run_ui` gets `Disconnected` when the cmd-processor exits.
- Smoke tests in `#[cfg(test)] mod tests` in `main.rs` exercise the cmd-processor pattern directly without real hardware.

---

## Code Review

**Verdict**: REQUEST-CHANGES

**Findings**: 0 critical, 2 warning, 2 info

**Test run**: 219 passed (214 pre-existing + 5 new), 0 failed — confirmed.
**Release build**: `cargo build -p engine --release` — succeeded, confirmed.

### [WARNING] engine/src/main.rs lines 75–83, 102–127 — CLI args parsed but never forwarded

`--midi-port`, `--hid-vid`, and `--hid-pid` are parsed and logged but never passed to `run_midi_out` or `run_hid`. Those functions use the first available port and hardcoded `HID_VID`/`HID_PID` constants. This violates the acceptance criteria which requires the args to be respected at runtime. Filed as **BUG-008** in `.workflow/BUGS.md`.

### [WARNING] engine/src/main.rs lines 111–116, 180–186 — clock thread never exits in non-hw-io build; no thread joins on shutdown

In non-hw-io builds, the clock's only exit condition (`midi_tx.send().is_err()`) can never trigger because `playing` defaults to false. The clock loops indefinitely until the process exits. Additionally, no thread handle is explicitly joined after cleanup: `_clock_thread`, `_cmd_thread`, and `_midi_thread` are all dropped without joining. This means the `MidiEvent::Stop` sent on line 182 may not be processed by `midi_out` before the process terminates. Filed as **BUG-009** in `.workflow/BUGS.md`.

### [INFO] engine/src/hid.rs line 397 — HID thread uses blocking `send()` for ui_notify

`run_hid` calls `ui_notify.send(())` (blocking `SyncSender::send`) rather than `try_send()`. The main.rs comment says "try_send so clock never blocks." If the notify channel fills (capacity 16) the HID thread stalls waiting for the UI to drain. Not a deadlock (UI drains within 50 ms), but inconsistent with the stated design intent.

### [INFO] engine/src/main.rs lines 111–116 — clock always spawned in non-hw-io builds

The comment says "Without hw-io, drop midi_rx so the clock thread's send() fails fast." But with `playing = false` the fast-fail path is never reached. The clock thread is unnecessary in non-hw-io test builds. Consider gating the clock spawn behind `#[cfg(feature = "hw-io")]` or adding a non-hw-io stub that returns immediately.
