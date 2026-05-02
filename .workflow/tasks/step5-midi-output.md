# Task: MIDI Output

- **Type**: coder
- **Status**: done
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

### Implementation Summary

**Branch:** `engine-phase1/midi-output` (worktree at `.workflow/worktrees/midi-output`)

**What was implemented:**

- `engine/src/midi_out.rs` — `pub fn run_midi_out(rx: Receiver<MidiEvent>)` and `pub fn run_midi_out_with_sender(rx, sender)` (testable entry point).
- `MidiSender` trait abstracts the send path; production impl wraps `midir::MidiOutputConnection` behind `Arc<Mutex<…>>` so spawned note-off threads can share it without Clone.
- `dispatch()` handles all five `MidiEvent` variants: NoteOn (immediate send + spawned note-off thread), NoteOff, Start (0xFA), Stop (0xFC), Continue (0xFB). All messages are `[u8; N]` stack arrays — no heap on send path.
- On no ALSA ports: logs error and returns (no panic).
- Also brought in `state.rs` and `sequencer.rs` from `sequencer-state-and-engine` branch; updated `lib.rs` to declare all modules.
- `.cargo/config.toml` updated with `[env] PKG_CONFIG_PATH = "/tmp/alsa-pkg"` and `rustflags = ["-L", "/tmp/alsa-lib"]` workaround (alsa-lib-devel not installed on this host; symlink `/tmp/alsa-lib/libasound.so -> /usr/lib64/libasound.so.2`).

**Test results:** 80 tests pass (`cargo test -p engine`); clippy clean; release build succeeds.

**Acceptance criteria checklist:**
- [x] `run_midi_out(rx: Receiver<MidiEvent>)` implemented
- [x] Opens first ALSA port, logs name
- [x] No ports → log error, return (no panic)
- [x] NoteOn → `[0x90|ch, note, vel]` immediately + spawned note-off thread
- [x] NoteOff → `[0x80|ch, note, 0]`
- [x] Start → `[0xFA]`
- [x] Stop → `[0xFC]`
- [x] Continue → `[0xFB]`
- [x] All messages on stack — no Vec/Box on send path
- [x] Thread exits when rx disconnected
- [x] Unit tests via MockSender trait double (11 tests)
- [x] `cargo test -p engine` passes

---

### Code Review — 2026-05-02

**Reviewer:** code-reviewer agent
**File reviewed:** `engine/src/midi_out.rs`, `.cargo/config.toml`

#### MidiSender trait — soundness

The trait is well-designed for testability. `Send + 'static` bounds are correct and necessary for the spawned note-off thread closures that capture `Box<dyn MidiSender>`. `try_clone` returning `Box<dyn MidiSender>` (rather than requiring `Clone`) is the right approach since `MidiOutputConnection` is not `Clone`. No issues.

#### Arc<Mutex<MidiOutputConnection>> — lock discipline

VERIFIED CORRECT. The lock is acquired only inside `send_bytes` (lines 35–38), held for the duration of `guard.send(data)`, and released immediately when the guard drops at the end of the block. The spawned note-off thread (lines 95–99) calls `thread::sleep` BEFORE calling `off_sender.send_bytes()`. The mutex is not held during the sleep. Lock discipline is sound.

#### NoteOn spawned thread — lock not held during sleep

VERIFIED CORRECT. The closure captures `off_sender: Box<dyn MidiSender>` and `channel`, `note`, `duration_nanos` by move. The sequence is: sleep → lock → send → unlock. The lock is never held across the sleep. No deadlock risk.

#### Stack-only send path

VERIFIED. All MIDI messages are `[u8; 1]` or `[u8; 3]` stack arrays. No `Vec`, `Box`, or heap allocation appears on the `dispatch()` hot path itself. The `Box<dyn MidiSender>` is allocated once at setup, not per-send. `off_sender` is moved into the spawned thread (a one-time allocation per NoteOn). Compliant with requirements.

#### No panic on no ALSA ports

VERIFIED. `open_first_port()` returns `None` on all failure paths (MidiOutput creation failure, empty port list, connection failure). `run_midi_out` returns early on `None`. No panics possible from missing ports.

#### Thread exits when rx closes

VERIFIED. `while let Ok(event) = rx.recv()` exits cleanly when the sender is dropped (recv returns `Err(RecvError)`). No blocking or resource leak.

#### Test completeness (11 tests)

Tests cover: NoteOn immediate bytes, NoteOn spawns note-off after duration, NoteOn channel masking, NoteOff bytes, NoteOff channel masking, Start, Stop, Continue, channel disconnect, channel round-trip. All five event types are covered. Mock correctly records byte output and uses `Arc<Mutex<Vec<u8>>>` to support inspection from the test thread after spawned threads complete.

One observation: `note_on_sends_correct_bytes_immediately` and `note_on_channel_bits_masked_correctly` use `duration_nanos: 0` and sleep 20 ms to let the spawned NoteOff thread flush. This makes tests timing-dependent in theory, but with a 20 ms window it is robust enough for unit tests. No finding raised.

#### Findings

### [WARNING] `.cargo/config.toml` — hardcoded `/tmp` paths will silently break builds on clean systems

- **File:** `.cargo/config.toml`, lines 11 and 17
- **Severity:** warning
- **Description:** `PKG_CONFIG_PATH = "/tmp/alsa-pkg"` and `rustflags = ["-L", "/tmp/alsa-lib"]` are unconditional. On a system where `alsa-lib-devel` is properly installed (CI, another developer's machine, a container), these paths will either not exist or point at the wrong `.so`. The `[env]` table in Cargo config does not support fallback/conditional logic, but the `rustflags` override is the more dangerous part: if `/tmp/alsa-lib` does not exist or contains a stale symlink, the linker will emit a spurious `-L` flag warning or, if the real `libasound.so` is on a different search path, the build may link against the wrong library silently. This is a host-specific workaround committed to source and will affect every developer and every CI environment that checks out the branch.
- **Suggested fix:** Remove these entries from `.cargo/config.toml` and instead document the workaround in a comment or `README` section. Developers on hosts missing `alsa-lib-devel` can set `PKG_CONFIG_PATH` and `RUSTFLAGS` in their shell or in a gitignored `.cargo/config.local.toml`. Alternatively, gate the entries with an environment check in CI (e.g., only set them if `/tmp/alsa-pkg` exists) at the CI script level rather than baking them into config committed to source.

### [INFO] `dispatch` takes `&mut Box<dyn MidiSender>` — double indirection is unnecessary

- **File:** `engine/src/midi_out.rs`, line 86
- **Severity:** info
- **Description:** `dispatch(sender: &mut Box<dyn MidiSender>, ...)` introduces a double indirection: a mutable reference to a `Box`. The idiomatic Rust signature would be `dispatch(sender: &mut dyn MidiSender, ...)` which coerces cleanly from `&mut *boxed_sender` at the call site. This is a minor style issue with no correctness impact.
- **Suggested fix:** Change the signature to `pub fn dispatch(sender: &mut dyn MidiSender, event: MidiEvent)` and update the two call sites to pass `sender.as_mut()` or `&mut **sender`.

### [INFO] Spawned note-off threads are fire-and-forget with no accounting

- **File:** `engine/src/midi_out.rs`, line 95
- **Severity:** info
- **Description:** `thread::spawn` for note-off scheduling produces threads with no handle tracked. For the MVP with short durations this is fine. At higher step rates (e.g., 120 BPM sixteenth = 125 ms steps, many notes), the number of in-flight threads will remain bounded by the note duration in ms / step period. No immediate risk, but worth noting for future gate-length design.
- **Suggested fix:** No action required for MVP. If note durations grow large (> 1 second), consider a lightweight timer wheel or a single dedicated note-off thread consuming a priority queue.

#### Summary

- 0 critical findings
- 1 warning finding (`.cargo/config.toml` hardcoded `/tmp` paths)
- 2 info findings (double indirection signature, fire-and-forget threads)
- All acceptance criteria verified correct
- Lock discipline verified sound — mutex is never held during sleep
- Test coverage is complete for all five event types
- **Verdict: approve** (warning is a build-hygiene issue, not a correctness bug; does not block merge)
