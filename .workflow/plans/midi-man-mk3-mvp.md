# Midi-Man Mk3 — MVP Architecture Plan

**Status:** revised  
**Author:** Architect agent + Coordinator  
**Date:** 2026-05-02  
**Scope:** MVP only — 16-step sequencer, clock, key/mode, swing, step-size, loop, pause/stop-start, MIDI out, PC UI, keyboard input, HID control surface protocol. Shift-mode randomness layer is explicitly out of scope.

### Revision Notes (2026-05-02)

- **Keyboard input added:** Engine UI handles keyboard controls so the MVP is fully testable before physical hardware arrives. HID connection is now optional (graceful degradation if no Pico is connected).
- **Phasing:** Implementation is split into two phases. Phase 1 = all engine steps (Steps 1–9 + keyboard). Phase 2 = firmware steps (Steps 10–15), to begin once MCP23017 I/O expanders are in hand.
- **Shared InputCommand abstraction added** (new Step 6b): both keyboard and HID produce the same `InputCommand` enum; sequencer state mutation is handled in one place.
- **Confirm contract:** keyboard mode matches physical surface — parameter changes via up/down are "pending" until Enter confirms them. Applies in both Root and overlay modes.

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Stack Evaluation Matrix](#2-stack-evaluation-matrix)
3. [Recommended Stack with Justification](#3-recommended-stack-with-justification)
4. [Module Boundary and Communication Protocol](#4-module-boundary-and-communication-protocol)
5. [MIDI Output Approach](#5-midi-output-approach)
6. [UI Approach](#6-ui-approach)
7. [Repository Layout](#7-repository-layout)
8. [Step-by-Step Implementation Breakdown](#8-step-by-step-implementation-breakdown)
9. [Acceptance Criteria](#9-acceptance-criteria)
10. [Risks and Assumptions](#10-risks-and-assumptions)

---

## 1. System Overview

```
┌─────────────────────────────────────────────────────────┐
│  PC / Raspberry Pi Zero 2W  (Engine)                    │
│                                                         │
│  ┌──────────────┐    ┌───────────────┐    ┌──────────┐  │
│  │  HID Reader  │───▶│   Sequencer   │───▶│ MIDI Out │  │
│  │  (USB host)  │    │   Engine      │    │  (ALSA)  │  │
│  └──────────────┘    └──────┬────────┘    └──────────┘  │
│                             │                           │
│                      ┌──────▼────────┐                  │
│                      │  TUI (ratatui)│                  │
│                      └───────────────┘                  │
└─────────────────────────────────────────────────────────┘
          ▲  USB HID (vendor-defined, interrupt IN/OUT)
          │
┌─────────┴──────────────────────────┐
│  Raspberry Pi Pico (RP2040)        │
│  Control Surface Firmware          │
│                                    │
│  16 rotary encoders → MCP23017 ×2  │
│  16 step buttons   → MCP23017 ×2   │
│  16 LEDs           → shift reg ×2  │
│  2 extra knobs     → direct GPIO   │
│  12 param buttons  → MCP23017 ×1   │
│                                    │
│  USB HID (vendor class, full-speed)│
└────────────────────────────────────┘
```

Two modules, one repository, zero shared runtime code:

- **engine/** — Rust workspace crate. Runs on the host (PC or Pi Zero 2W). Owns the sequencer state machine, clock, MIDI output, and the terminal UI.
- **firmware/** — Rust `no_std` crate with Embassy. Runs on the RP2040 Pico. Reads all physical controls via I2C I/O expanders and shift registers, reports state over USB HID, receives LED state from the engine.

---

## 2. Stack Evaluation Matrix

### 2A. Engine (PC / Pi Zero 2W)

| Criterion | **Rust** | C | C++ | Go | Python |
|---|---|---|---|---|---|
| Compile-ahead-of-time | Yes | Yes | Yes | Yes (GC pauses) | No |
| Memory safety (no alloc hot path) | Ownership guarantees at compile time | Manual; UB risk | Manual; UB risk | GC; pauses | GC |
| Real-time timer precision | clock_nanosleep via libc; SCHED_FIFO accessible | Same | Same | GC pauses hurt | Unsuitable |
| ARM aarch64 cross-compile | cargo + cross; first-class | gcc-aarch64; mature | Same | go build GOARCH=arm64 | N/A |
| MIDI library maturity | midir 0.10 (ALSA backend); active | ALSA directly; verbose | RtMidi; mature | portmidi bindings; less active | mido; Python-only |
| UI on Pi Zero (512 MB RAM) | ratatui: <5 MB binary, no GPU required | ncurses; workable | FTXUI; workable | bubbletea; fine | curses; fine |
| Async / threading model | tokio or std threads; zero-cost abstractions | pthreads | std::thread | goroutines (GC) | asyncio (GIL) |
| Binary size (stripped) | ~2–4 MB typical | ~200–500 KB | ~2–5 MB | ~8–15 MB | N/A |
| Hot path dynamic allocation | Avoidable with pre-allocated ring buffers | Avoidable | Avoidable | GC allocates | Always |
| Ecosystem for this domain | Growing: midir, midly, ratatui, libc | Mature but verbose | Mature | Thin | Rich but slow |
| Pi Zero 2W viability | Yes — 512 MB RAM, 4× A53 @1 GHz | Yes | Yes | Marginal (binary size + GC) | No |
| **Overall score** | **Best fit** | Viable | Viable | Marginal | No |

**Rust wins** for the engine: memory safety, no GC, smallest binary that still has a rich ecosystem for MIDI and TUI, and idiomatic no-alloc patterns on the hot path.

**C** is a credible fallback if Rust compile times or ecosystem gaps become blocking. The ALSA sequencer API and ncurses are both well understood and run comfortably on a Pi Zero.

**C++** offers RtMidi and FTXUI but adds no benefit over C for this project and brings C++'s manual memory discipline without Rust's static guarantees.

**Go** is ruled out: GC pauses are incompatible with sub-millisecond tick jitter targets, and the binary is large for a 512 MB device.

**Python** is ruled out: interpreted, GIL, unsuitable for real-time clock.

---

### 2B. Control Surface Firmware (Raspberry Pi Pico / RP2040)

| Criterion | **Rust + Embassy** | C + Pico SDK + TinyUSB | C++ + Pico SDK | MicroPython | CircuitPython |
|---|---|---|---|---|---|
| Language safety | Compile-time; no UB | Manual | Manual | GC; safe | GC; safe |
| `no_std` / bare-metal | Yes — embassy-rp targets RP2040 | Yes | Yes | No (needs runtime) | No |
| USB HID vendor class | embassy-usb; descriptor builder in Rust | TinyUSB; battle-tested; most docs/examples | Same as C SDK | Limited; no custom descriptors easily | Limited |
| Async I2C (I/O expanders) | Embassy async I2C; non-blocking | Blocking SDK I2C or DMA | Same | Blocking | Blocking |
| Binary size (RP2040 has 2 MB flash) | ~13 KB blink; full HID project ~50–100 KB | ~30–60 KB typical | ~30–80 KB | ~600 KB+ runtime | ~700 KB+ runtime |
| RAM footprint (RP2040 has 264 KB SRAM) | Very low; stack-allocated buffers | Low | Low | High (GC heap) | High |
| Toolchain complexity | cargo + flip-link; slightly more setup | CMake; straightforward | CMake | Drag-and-drop .uf2 | Drag-and-drop .uf2 |
| Community/examples for HID | Growing fast (embassy-usb HID examples exist) | Largest; most forum answers | Large | Adequate for simple | Adequate for simple |
| I/O expander (MCP23017) driver | community crate mcp23017 exists | Write by hand; ~100 lines | Same | adafruit lib | adafruit lib |
| **Overall score** | **Best fit** | Strong second | Viable | No | No |

**Rust + Embassy** wins for firmware: same language as the engine (shared mental model, one toolchain, one Cargo workspace), async I2C avoids blocking while polling 5× I2C expanders, embassy-usb has a working vendor HID example, and the binary fits in 2 MB flash with room to spare.

**C + Pico SDK + TinyUSB** is the best fallback. TinyUSB is battle-tested, has more HID examples in the wild, and is what most Pico HID projects use. If embassy-usb hits a blocking issue (e.g., an RP2040 USB driver bug), the protocol design is the same and porting is feasible in a weekend.

**MicroPython / CircuitPython** are ruled out: 600–700 KB runtime consumes most flash, GC heap is incompatible with deterministic polling, and custom vendor HID descriptors are not cleanly supported.

---

## 3. Recommended Stack with Justification

### Engine

| Component | Choice | Version / Notes |
|---|---|---|
| Language | **Rust** | stable, edition 2021 |
| MIDI output | **midir 0.10** (ALSA backend) | zero-copy message dispatch; no allocations on send path |
| Clock / tick loop | **`std::thread` + `libc::clock_nanosleep`** | CLOCK_MONOTONIC; optionally set SCHED_FIFO via `libc` |
| UI | **ratatui 0.29** | immediate-mode TUI; < 5 MB binary on ARM; 60+ FPS at full state redraw |
| HID host reader | **hidapi crate 2.x** (wraps libhidapi) | reads interrupt IN reports from Pico |
| HID LED writer | same hidapi connection | writes interrupt OUT reports back to Pico |
| Async runtime | **none on hot path** — two `std::thread`s (clock + UI) | avoids tokio overhead; simpler to reason about timing |
| Music theory | inline lookup tables (`const` arrays) | no heap; scale intervals embedded at compile time |

### Firmware

| Component | Choice | Version / Notes |
|---|---|---|
| Language | **Rust** (`no_std`) | stable, edition 2021 |
| HAL / async runtime | **Embassy (embassy-rp, embassy-executor)** | targets RP2040; async task per peripheral group |
| USB stack | **embassy-usb** | vendor-defined HID class; 64-byte IN + 64-byte OUT reports |
| I2C I/O expanders | **MCP23017** × 5 (via I2C0 and I2C1) | `mcp23017` community crate or inline driver |
| LED shift registers | 74HC595 × 2 via SPI0 | inline SPI driver; 16 bits = 16 LEDs |
| Rotary encoder decode | interrupt-on-change from MCP23017 INT pin | debounce in firmware state machine |
| Linker | **flip-link** | stack overflow detection |

---

## 4. Module Boundary and Communication Protocol

### Design Principle

The Pico is a **dumb peripheral**: it reports raw physical events and receives LED state. All musical logic (scale lookup, step enable/disable, playhead position) lives in the engine. The HID protocol is intentionally flat and versioned so the firmware can be reflashed independently.

### USB HID Configuration

- Device class: HID (vendor-defined usage page `0xFF01`)
- Interface: single HID interface, interrupt IN + interrupt OUT
- Report size: **64 bytes** both directions (max HID interrupt packet, no fragmentation needed)
- Poll interval: **1 ms** (USB full-speed interrupt maximum rate)

### Report Format — Pico → Engine (IN report, 64 bytes)

```
Byte  0      : report_id = 0x01
Byte  1      : sequence number (u8, wraps)
Byte  2      : flags
               bit 0 = encoder_tap[0..15] pending (spread across bytes 6–7)
               bit 1 = param_tap pending
               bit 2 = tempo_tap pending
               bit 3 = reserved
Bytes 3–4    : step_buttons[15:0]  — one bit per step button, 1 = pressed this poll
Bytes 5–6    : step_enable_state[15:0] — LED mirror (engine echoes back, Pico tracks)
Byte  7      : param_buttons — 12 bits in low bits of 2 bytes
Byte  8      : param_buttons high nibble
Bytes 9–24   : encoder_deltas[16] — signed i8 per encoder, -127..+127 since last report
Byte 25      : tempo_delta — signed i8
Byte 26      : param_knob_delta — signed i8
Bytes 27–63  : reserved (zero-filled)
```

### Report Format — Engine → Pico (OUT report, 64 bytes)

```
Byte  0      : report_id = 0x02
Byte  1      : sequence number echo (from last IN report)
Bytes 2–3    : led_state[15:0] — 1 = LED on, matches step enable state
Bytes 4–63   : reserved
```

### State Machine Contract

- Encoder deltas accumulate in the Pico between polls (no events dropped if host is briefly busy).
- Step button bytes report **edges** (press events since last report), not levels, to avoid missed taps.
- The engine is the authority on LED state; it writes OUT reports after every sequencer state change.
- If the engine receives two consecutive IN reports with the same sequence number it logs a warning (indicates USB enumeration issue).

---

## 5. MIDI Output Approach

### Library: `midir` (ALSA backend on Linux)

- `MidiOutput::new()` + `MidiOutputPort` opened once at startup; handle kept alive for the process lifetime.
- MIDI messages are assembled into fixed-size arrays on the stack (`[u8; 3]` for note on/off).
- **No heap allocation** on the send path: `MidiOutputConnection::send(&[u8])` copies into a kernel buffer; the slice lives on the stack.

### MIDI Events Generated by the Engine

| Event | MIDI Message | Notes |
|---|---|---|
| Step note on | `0x90 ch note vel` | sent at tick boundary; channel fixed to 0 for MVP |
| Step note off | `0x80 ch note 0` | sent exactly one step-duration later |
| Clock pulse | `0xF8` | 24 PPQN; optional, enabled by config flag |
| Start | `0xFA` | on play from stopped |
| Stop | `0xFC` | on stop |
| Continue | `0xFB` | on resume from pause |

### Timing

The clock thread uses `libc::clock_nanosleep(CLOCK_MONOTONIC, ...)` to wake at each tick boundary. At 120 BPM with 1/16 note steps: tick period = 60 000 ms / (120 × 16) ≈ 31.25 ms. The thread computes the next absolute wake time before sleeping to prevent drift accumulation.

SCHED_FIFO priority 50 is requested at startup via `libc::sched_setscheduler`. If it fails (non-root), the engine logs a warning and continues — latency will be slightly worse but acceptable for personal use.

```
Swing implementation:
  Even steps: play at tick_time
  Odd steps:  play at tick_time + (swing_factor × tick_period / 100)
  swing_factor range: -50 to +50 (maps to parameter button 3 knob value)
```

---

## 6. UI Approach

### Library: `ratatui` (terminal, crossterm backend)

The UI is a read-only view of the sequencer state. It renders on every state change event received from the engine's internal channel, plus a forced redraw every 50 ms (20 FPS cap) to catch playhead animation.

**No user interaction via the UI** — all input comes from the physical control surface. The terminal window is purely informational.

### Layout (80×24 terminal minimum, scales up)

```
┌─ Midi-Man Mk3 ──────────────── BPM: 120 ── Key: C ── Mode: Dorian ── Step: 1/16 ─┐
│                                                                                    │
│  Steps:  1    2    3    4    5    6    7    8    9   10   11   12   13   14   15   16│
│  Note:   C4   E4   G4   A4   C4   ──   G4   ──   C4   E4   G4   A4   C4   ──   G4   ──│
│  On/Off: ●    ●    ●    ●    ●    ○    ●    ○    ●    ●    ●    ●    ●    ○    ●    ○ │
│          ▲                                                                          │
│          playhead                                                                   │
│                                                                                    │
│  Swing: +15%    Loop: 3–10    Status: PLAYING                                      │
└────────────────────────────────────────────────────────────────────────────────────┘
```

- `●` = step enabled, `○` = step disabled
- `▲` = playhead indicator under the active step column
- Note shows pitch name (e.g. `C4`, `F#3`) computed from MIDI note number
- Playhead column highlighted with a distinct color via ratatui style
- All state comes from a single `Arc<RwLock<SequencerState>>` shared with the sequencer thread; UI thread only reads

### Performance on Pi Zero 2W

ratatui renders only the diff between frames (intermediate buffer comparison). A full 16-step redraw touches < 200 bytes of terminal output. At 20 FPS this is negligible on a 1 GHz A53.

---

## 7. Repository Layout

```
midi-man-mk3/
├── Cargo.toml                  # workspace root
├── Cargo.lock
├── .cargo/
│   └── config.toml             # cross-compile targets, flip-link linker
├── engine/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # startup: open MIDI, open HID, spawn threads
│       ├── sequencer.rs        # SequencerState, step engine, playhead
│       ├── clock.rs            # real-time tick loop, clock_nanosleep
│       ├── hid.rs              # USB HID reader/writer (hidapi)
│       ├── midi_out.rs         # MIDI message dispatch (midir)
│       ├── music_theory.rs     # scale tables, note name lookup
│       ├── ui.rs               # ratatui render loop
│       └── state.rs            # SequencerState struct (shared between threads)
├── firmware/
│   ├── Cargo.toml
│   ├── build.rs                # link script selection
│   ├── memory.x                # RP2040 memory layout
│   └── src/
│       ├── main.rs             # Embassy executor entry, task spawn
│       ├── usb_hid.rs          # embassy-usb HID class, IN/OUT report handlers
│       ├── report.rs           # InReport / OutReport structs, pack/unpack
│       ├── encoders.rs         # MCP23017 interrupt handler, delta accumulation
│       ├── buttons.rs          # step buttons + param buttons, edge detection
│       ├── leds.rs             # 74HC595 SPI shift-out
│       └── i2c_expander.rs     # MCP23017 register-level driver
├── .workflow/
│   ├── BACKLOG.md
│   ├── TODO.md
│   ├── DONE.md
│   ├── BUGS.md
│   ├── plans/
│   │   └── midi-man-mk3-mvp.md  (this file)
│   └── tasks/
├── agent.yaml
├── requirements.md
└── .gitignore
```

### Cargo Workspace (`Cargo.toml`)

```toml
[workspace]
members = ["engine", "firmware"]
resolver = "2"
```

The firmware crate uses a separate `.cargo/config.toml` inside `firmware/` to set `target = "thumbv6m-none-eabi"` and the flip-link linker, so `cargo build` in the workspace root builds the engine for the host and the firmware for the Pico independently.

---

## 8. Step-by-Step Implementation Breakdown

Each step is sized for a single coder-agent session. Steps are ordered by dependency.

---

### Step 1 — Workspace Scaffold and Build Proof

**Agent:** coder  
**Files:** `Cargo.toml`, `engine/Cargo.toml`, `firmware/Cargo.toml`, `.cargo/config.toml`, `firmware/.cargo/config.toml`, `firmware/memory.x`, `firmware/build.rs`

Tasks:
1. Initialize Cargo workspace with `engine` and `firmware` members.
2. `engine/Cargo.toml`: add `midir`, `hidapi`, `ratatui`, `crossterm`, `libc`.
3. `firmware/Cargo.toml`: add `embassy-executor`, `embassy-rp`, `embassy-usb`, `embassy-time`, `flip-link`, `embedded-hal`, `mcp23017` (or inline stub).
4. `firmware/.cargo/config.toml`: set target `thumbv6m-none-eabi`, linker `flip-link`, rustflags for defmt if used.
5. `firmware/memory.x`: RP2040 memory layout (2 MB flash, 264 KB SRAM).
6. Stub `main.rs` in both crates — just compiles without error.
7. Verify: `cargo build -p engine` succeeds on host; `cargo build -p firmware` succeeds targeting RP2040.

**Expected outcome:** Both crates compile to empty binaries. No functional code yet.

---

### Step 2 — Music Theory Tables

**Agent:** coder  
**Files:** `engine/src/music_theory.rs`

Tasks:
1. Define `Key` enum: `C, Cs, D, Ds, E, F, Fs, G, Gs, A, As, B` (12 values).
2. Define `Mode` enum: `Major, NaturalMinor, Dorian, Phrygian, Lydian, Mixolydian, Locrian` (7 values for MVP).
3. Define `const SCALE_INTERVALS: [[u8; 7]; 7]` — semitone intervals for each mode (e.g. Major = [2,2,1,2,2,2,1]).
4. Implement `fn notes_in_key(key: Key, mode: Mode) -> [u8; 7]` — returns MIDI note numbers for one octave starting at key root (C4 = 60).
5. Implement `fn note_name(midi_note: u8) -> &'static str` — returns e.g. `"C4"`, `"F#3"`.
6. Implement `fn next_note(current: u8, key: Key, mode: Mode, direction: i8) -> u8` — wraps within the 7-note set across octaves (clamped to MIDI 0–127).
7. Unit tests for each function covering edge cases (octave wrap, root, flat/sharp naming).

**Expected outcome:** Music theory module passes all tests; no heap allocation.

---

### Step 3 — Sequencer State and Engine

**Agent:** coder  
**Files:** `engine/src/state.rs`, `engine/src/sequencer.rs`

Tasks:
1. Define `SequencerState` struct:
   ```rust
   pub struct SequencerState {
       pub steps: [StepData; 16],
       pub key: Key,
       pub mode: Mode,
       pub tempo_bpm: u16,          // 20–300
       pub swing: i8,               // -50 to +50
       pub step_size: StepSize,     // Quarter, Eighth, Sixteenth
       pub loop_in: u8,             // 0–15
       pub loop_out: u8,            // 0–15
       pub loop_active: bool,
       pub playhead: u8,            // 0–15
       pub playing: bool,
       pub paused: bool,
   }
   pub struct StepData {
       pub enabled: bool,
       pub midi_note: u8,
   }
   pub enum StepSize { Quarter, Eighth, Sixteenth }
   ```
2. All fields `pub`; `SequencerState` is `Clone` and `Default` (all steps disabled, C Major, 120 BPM).
3. Implement `SequencerState::apply_encoder_delta(step: usize, delta: i8)` — calls `next_note`.
4. Implement `SequencerState::toggle_step(step: usize)`.
5. Implement `SequencerState::tick(&mut self) -> Option<MidiEvent>` — advances playhead respecting loop bounds, returns note-on event if step enabled.
6. Unit tests: tick advances playhead, loop wraps, disabled steps return None.

**Expected outcome:** Sequencer logic passes tests; state struct has no heap fields.

---

### Step 4 — Clock Thread

**Agent:** coder  
**Files:** `engine/src/clock.rs`

Tasks:
1. Implement `fn run_clock(state: Arc<RwLock<SequencerState>>, midi_tx: SyncSender<MidiEvent>)` — runs in a dedicated `std::thread`.
2. Use `libc::clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, ...)` for tick timing.
3. Compute `tick_nanos` from `state.tempo_bpm` and `state.step_size`; re-read after each tick so tempo changes take effect on the next step.
4. Apply swing: even steps fire at `next_abs`; odd steps fire at `next_abs + swing_offset_nanos`.
5. On each tick: acquire write lock, call `state.tick()`, send `MidiEvent` on channel if `Some`.
6. Attempt `SCHED_FIFO` priority 50 via `libc::sched_setscheduler` at thread start; log warning if denied.
7. Tests: mock state advancing 32 ticks, verify playhead at expected position; verify swing offset math.

**Expected outcome:** Clock thread compiles; timing math unit-tested without needing actual sleep.

---

### Step 5 — MIDI Output

**Agent:** coder  
**Files:** `engine/src/midi_out.rs`

Tasks:
1. Implement `fn run_midi_out(rx: Receiver<MidiEvent>)` — runs in a dedicated `std::thread`.
2. Open first available ALSA MIDI output port via `midir::MidiOutput`.
3. Receive `MidiEvent` from channel; encode to `[u8; 3]` on stack; call `connection.send(&msg)`.
4. Handle `MidiEvent::NoteOn { channel, note, velocity }` and `MidiEvent::NoteOff { channel, note }`.
5. Send MIDI Start (`0xFA`) on first play, Stop (`0xFC`) on stop, Continue (`0xFB`) on resume from pause.
6. Log port name at startup; exit thread cleanly on channel disconnect.
7. Test: mock `MidiOutput` trait (or integration test with a virtual ALSA port via `aconnect`).

**Expected outcome:** MIDI output thread wires up; MIDI messages sent correctly.

---

### Step 6b — InputCommand Abstraction and Keyboard Input

**Agent:** coder  
**Files:** `engine/src/input.rs`, `engine/src/ui.rs` (keyboard event loop added)

#### InputCommand Enum

Both the keyboard handler and the HID reader produce `InputCommand` values on a shared `SyncSender<InputCommand>`. The sequencer/HID thread consumes them and applies state mutations. This keeps all state mutation logic in one place.

```rust
pub enum InputCommand {
    // Step navigation
    StepSelect(usize),          // absolute step index 0–15
    StepSelectDelta(i8),        // +1 / -1 relative

    // Note editing (Root mode)
    NoteDelta(i8),              // +1 / -1; pending until Confirm
    Confirm,                    // apply pending note change for selected step
    ToggleStep,                 // enable/disable selected step

    // Velocity editing (Root mode, Shift held)
    VelocityDelta(i8),          // +1 / -1; pending until Confirm

    // Parameter overlay (F1 = regular, F2 = shift)
    OpenOverlay(OverlayMode),   // RegularSettings | ShiftSettings
    CloseOverlay,               // Esc — discard pending param change

    ParamSelect(u8),            // highlight param by index (0-based)
    ParamSelectDelta(i8),       // left/right in overlay
    ParamValueDelta(i8),        // up/down in overlay; pending until Confirm
    // Confirm re-used for param confirm
}

pub enum OverlayMode { Regular, Shift }
```

**Pending state:** `engine/src/state.rs` gains a `PendingEdit` field:
```rust
pub enum PendingEdit {
    None,
    Note { step: usize, midi_note: u8 },
    Velocity { step: usize, velocity: u8 },
    Param { overlay: OverlayMode, index: u8, value: i64 },
}
```
`Confirm` commits the pending edit to the live state. `CloseOverlay` / `StepSelectDelta` / `StepSelect` discard a pending note/velocity edit.

#### Keyboard Mapping

**Root mode (no overlay):**

| Key | InputCommand |
|---|---|
| Left arrow | `StepSelectDelta(-1)` |
| Right arrow | `StepSelectDelta(+1)` |
| Up arrow | `NoteDelta(+1)` |
| Down arrow | `NoteDelta(-1)` |
| Shift + Up | `VelocityDelta(+1)` |
| Shift + Down | `VelocityDelta(-1)` |
| Space | `ToggleStep` |
| Enter | `Confirm` |
| F1 | `OpenOverlay(Regular)` |
| F2 | `OpenOverlay(Shift)` |

**Regular settings overlay (F1):**

| Key | InputCommand |
|---|---|
| Left arrow | `ParamSelectDelta(-1)` |
| Right arrow | `ParamSelectDelta(+1)` |
| Up arrow | `ParamValueDelta(+1)` |
| Down arrow | `ParamValueDelta(-1)` |
| Enter | `Confirm` |
| Esc | `CloseOverlay` |

**Shift settings overlay (F2):** same key map as F1.

**Regular overlay parameters (left→right):**
1. Key (musical key)
2. Mode (scale/mode)
3. Swing (−50 to +50)
4. Step Size (1/4, 1/8, 1/16)
5. Loop (in/out/clear — three sequential Enter presses cycles through)
6. Pause (toggle)
7. Stop / Start (toggle)

**Shift overlay parameters:** placeholder row for now (MVP); renders as "(shift mode — coming soon)" to reserve the overlay structure.

#### HID Becomes Optional

`engine/src/hid.rs`: if `hidapi::HidApi::new()` or `open()` fails (device not connected), the HID thread logs a warning and exits immediately. The engine continues running with keyboard-only input. The `SyncSender<InputCommand>` is still the sole path into state mutation — HID and keyboard are peers on the same channel.

#### UI Changes (Step 8 impact)

The UI thread is no longer read-only. It runs a crossterm event loop alongside the render loop:
- On `Event::Key`: translate to `InputCommand`, send on channel.
- On `Event::Resize` or 50 ms timeout: redraw.
- Overlay state (`Option<OverlayMode>`, `selected_param: u8`) lives in the UI thread only — it is presentation state, not sequencer state.
- Pending edit value is read from `SequencerState.pending_edit` for display.

**Tasks:**
1. Define `InputCommand` and `OverlayMode` enums in `engine/src/input.rs`.
2. Add `PendingEdit` to `SequencerState`; implement `apply_command(cmd: InputCommand)` on `SequencerState`.
3. Implement keyboard event loop in `ui.rs`; translate crossterm `KeyEvent` → `InputCommand`.
4. Update overlay render: F1/F2 overlays with highlighted param, current value, pending value.
5. Update `hid.rs` to translate `InReport` fields into `InputCommand` values (matching same semantics as keyboard).
6. Make HID thread non-fatal on device-not-found.
7. Unit tests: `apply_command` for each command variant; keyboard translation for each mapped key.

**Expected outcome:** Full engine is playable from keyboard alone, no Pico required.

---

### Step 6 — HID Report Structs (shared definitions)

**Agent:** coder  
**Files:** `engine/src/hid.rs` (engine side), `firmware/src/report.rs` (firmware side)

Tasks:
1. Define `InReport` (Pico → Engine) as a `repr(C)` struct matching the byte layout in Section 4. Implement `fn from_bytes(buf: &[u8; 64]) -> InReport`.
2. Define `OutReport` (Engine → Pico) as a `repr(C)` struct. Implement `fn to_bytes(&self) -> [u8; 64]`.
3. Duplicate the structs in `firmware/src/report.rs` (no shared crate to avoid cross-compile complexity at MVP; they must match byte-for-byte — add a comment pointing to the spec in this document).
4. Unit tests in engine: round-trip encode/decode of a sample InReport; verify all fields.

**Expected outcome:** Both sides have matching report structs; tested at engine level.

---

### Step 7 — HID Host Reader/Writer (Engine)

**Agent:** coder  
**Files:** `engine/src/hid.rs`

Tasks:
1. Implement `fn run_hid(state: Arc<RwLock<SequencerState>>, ui_notify: SyncSender<()>)`.
2. Open HID device by vendor ID / product ID (VID/PID constants defined as `const` in `hid.rs`; placeholder values during development).
3. Poll `device.read_timeout(&mut buf, 5)` in a loop (5 ms timeout, non-blocking equivalent).
4. Parse `InReport` from buffer; acquire write lock on state; apply:
   - encoder deltas → `apply_encoder_delta`
   - step button edges → `toggle_step`
   - param button presses → match to state fields (key, mode, swing, step size, loop, pause, stop/start)
5. After applying, compute `OutReport` (LED state from `step.enabled`), write via `device.write()`.
6. Send on `ui_notify` channel to wake the UI thread.
7. Integration test: feed synthetic InReport bytes; verify state changes; verify OutReport LED bytes.

**Expected outcome:** Engine reads control surface events and drives sequencer state.

---

### Step 8 — Terminal UI

**Agent:** coder  
**Files:** `engine/src/ui.rs`

Tasks:
1. Implement `fn run_ui(state: Arc<RwLock<SequencerState>>, notify: Receiver<()>)`.
2. Set up crossterm raw mode and alternate screen.
3. On each notify (or 50 ms timeout), acquire read lock, clone state, release lock immediately.
4. Render with ratatui:
   - Top bar: BPM, Key, Mode, Step size, status (PLAYING / PAUSED / STOPPED)
   - Step row: 16 columns, each showing note name + enabled indicator (`●`/`○`)
   - Playhead: highlight the active column
   - Second row: Swing value, Loop in/out if active
5. Restore terminal on exit (implement `Drop` guard or catch Ctrl-C).
6. No user keyboard input handled (Ctrl-C only for exit).
7. Test: render to a `TestBackend` and assert expected cell contents for a known state.

**Expected outcome:** UI renders correctly; playhead moves visually as sequencer runs.

---

### Step 9 — Engine `main.rs` Wiring

**Agent:** coder  
**Files:** `engine/src/main.rs`

Tasks:
1. Parse CLI args: `--midi-port <name>`, `--hid-vid <hex>`, `--hid-pid <hex>` (all optional with defaults).
2. Initialize `SequencerState` wrapped in `Arc<RwLock<_>>`.
3. Spawn threads in order: midi_out, clock, hid, ui. Pass clones of Arc and channels.
4. Join on UI thread (main blocks until UI exits).
5. On exit: send stop MIDI message, close HID device.
6. Test: smoke test that all threads start and the main loop runs for 100 ms without panic.

**Expected outcome:** `cargo run -p engine` produces a working sequencer.

---

### Step 10 — Firmware: I2C Expander Driver

**Agent:** coder  
**Files:** `firmware/src/i2c_expander.rs`

Tasks:
1. Implement `Mcp23017` driver struct wrapping `embassy_rp::i2c::I2c`.
2. Methods: `read_gpio_ab(&mut self) -> (u8, u8)`, `write_gpio_a(&mut self, val: u8)`, `configure_as_input_pullup(&mut self, port: Port)`.
3. Configure interrupt-on-change (IOCON, GPINTEN, DEFVAL, INTCON registers).
4. Test with a simulated I2C transaction (mock embedded-hal trait).

---

### Step 11 — Firmware: Encoders and Buttons

**Agent:** coder  
**Files:** `firmware/src/encoders.rs`, `firmware/src/buttons.rs`

Tasks:
1. `encoders.rs`: Embassy `#[embassy_executor::task]` that wakes on MCP23017 INT pin, reads all 5 expanders, decodes quadrature for 16 encoders + 2 extra knobs, accumulates signed delta per encoder in a `Mutex<CriticalSectionRawMutex, [i8; 18]>`.
2. `buttons.rs`: same task or separate — reads step button states and param button states from expanders; detects falling edges (press events) using XOR with previous state; accumulates edge mask in `Mutex`.
3. Both tasks run at 1 ms intervals (embassy-time timer) in addition to interrupt wake.

---

### Step 12 — Firmware: LED Driver

**Agent:** coder  
**Files:** `firmware/src/leds.rs`

Tasks:
1. `run_leds` task: holds current 16-bit LED state; on change writes two bytes to 74HC595 via SPI0 with RCLK pulse.
2. Expose `set_led_state(bits: u16)` via shared `Mutex`.
3. Test: verify SPI byte order (MSB first, bit 0 = step 1 LED).

---

### Step 13 — Firmware: USB HID Task

**Agent:** coder  
**Files:** `firmware/src/usb_hid.rs`, `firmware/src/report.rs`

Tasks:
1. Define HID report descriptor bytes for vendor usage page `0xFF01`, 64-byte IN report ID 0x01, 64-byte OUT report ID 0x02.
2. Implement `run_usb_hid` Embassy task using `embassy_usb::class::hid::HidWriter` and `HidReader`.
3. Every 1 ms: read from encoder/button Mutexes, pack into `InReport`, call `writer.write()`.
4. Poll `reader.read()` for `OutReport`; unpack LED bits; call `set_led_state`.
5. Clear deltas and edge masks after each successful write.

---

### Step 14 — Firmware: `main.rs` Entry Point

**Agent:** coder  
**Files:** `firmware/src/main.rs`

Tasks:
1. Embassy executor entry (`#[embassy_executor::main]`).
2. Initialize peripherals: I2C0 (expanders for encoders 1–8 + step buttons 1–8), I2C1 (expanders for encoders 9–16 + step buttons 9–16 + param buttons), SPI0 (LEDs), USB.
3. Spawn tasks: `run_usb_hid`, `run_encoders`, `run_buttons`, `run_leds`.
4. Verify: `.uf2` builds and device enumerates on host as HID vendor device.

---

---

## Phase Boundary

**Phase 1 (engine — start now):** Steps 1, 2, 3, 4, 5, 6, 6b, 7, 8, 9
**Phase 2 (firmware — begin once MCP23017s arrive):** Steps 10, 11, 12, 13, 14, 15

Phase 1 is fully self-contained. The engine runs with keyboard input only; HID connection is optional. No firmware or hardware is required to develop, test, or demo Phase 1.

---

### Step 15 — Integration Test and VID/PID Finalization

**Agent:** coder / qa  
**Files:** `engine/src/hid.rs` (update VID/PID constants), integration test script

Tasks:
1. Flash firmware to Pico; confirm USB enumeration (`lsusb` shows VID/PID).
2. Run engine; confirm HID connection opens.
3. Turn encoder 1 → engine step 1 note changes; LED 1 reflects enable state.
4. Press play (param button 12) → MIDI start message appears on virtual ALSA port; playhead animates in UI.
5. Loop in/out (param button 9) → playhead bounces between set steps.
6. Pause (param button 11) → MIDI continue on resume.
7. Tempo knob → BPM changes visible in UI and clock tick interval changes.

---

## 9. Acceptance Criteria

### Engine

- [ ] `cargo build -p engine --release` succeeds for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` targets.
- [ ] Engine opens the first available ALSA MIDI output port on startup; logs port name.
- [ ] Engine opens HID device by configured VID/PID; logs success.
- [ ] Sequencer advances playhead at the correct interval for tempo 60–240 BPM (measured with system clock, error < 2 ms per step at 120 BPM 1/16 note).
- [ ] Swing offset is applied: odd steps play later by `swing_factor × step_period / 100` (verified by logging timestamps).
- [ ] Note-on and note-off MIDI events are emitted for every enabled step; disabled steps produce no MIDI output.
- [ ] MIDI Start sent when playback begins from stopped; MIDI Continue sent when resuming from pause; MIDI Stop sent on stop.
- [ ] Turning an encoder while stopped changes the note for that step immediately; the note sounds via MIDI.
- [ ] Turning an encoder while playing queues the change; it is applied on the next pass through step 0.
- [ ] Key selection (param button 1 + param knob) changes the key; all step notes are re-mapped to the new key's note set, preserving scale degree position.
- [ ] Mode selection (param button 2 + param knob) changes the mode; same re-mapping applies.
- [ ] Swing (param button 3 + param knob) changes swing value in range -50 to +50.
- [ ] Step size (param button 4 + param knob) switches between 1/4, 1/8, 1/16; clock tick period updates on next step.
- [ ] Loop in/out (param button 9): first press sets loop-in to current playhead position; second press sets loop-out; third press clears loop.
- [ ] Loop: playhead wraps at loop-out back to loop-in when loop is active.
- [ ] Pause (param button 11): playhead freezes; MIDI note off sent for any held note; resume sends MIDI Continue and continues from same step.
- [ ] Stop/Start (param button 12): starts from step 0 (or loop-in if loop active); if pressed while paused, resets playhead but does not start.
- [ ] UI displays: all 16 steps with note names and on/off indicators, active key and mode, BPM, step size, swing value, loop bounds if active, playhead position, and play/pause/stop status.
- [ ] UI playhead indicator updates within 50 ms of a step change (20 FPS cap).
- [ ] All unit tests pass: `cargo test -p engine`.

### Firmware

- [ ] `cargo build -p firmware --release` produces a `.uf2` binary < 512 KB.
- [ ] Device enumerates on Linux host as USB HID vendor device with configured VID/PID.
- [ ] All 16 step button presses are reported as edge events (not held state) in the IN report within 1 ms of press.
- [ ] All 12 parameter button presses are reported in the IN report within 1 ms.
- [ ] Encoder deltas accumulate correctly between polls; turning an encoder 4 detents while the host is unresponsive (simulated) results in accumulated delta of ±4 on the next polled report.
- [ ] Tempo knob and parameter knob deltas are reported correctly.
- [ ] LED state from OUT report is reflected on the physical LEDs within 2 ms of receiving the report.
- [ ] Tap (press + release on encoder shaft button) is detected and reported in the encoder tap bitmask.
- [ ] Firmware does not crash or hang after 1 hour of continuous operation with no host connected.

### Integration

- [ ] End-to-end: physical encoder turn → note change visible in UI within 50 ms.
- [ ] End-to-end: step button press → LED changes state within 50 ms.
- [ ] End-to-end: play button press → MIDI Start + first note-on within 50 ms.
- [ ] Sequencer runs at 120 BPM 1/16 note for 5 minutes with no drift > 10 ms cumulative (verified by MIDI timestamp capture).

---

## 10. Risks and Assumptions

### Assumptions

1. The host runs a standard Linux with ALSA (Raspbian or Fedora). No JACK dependency in MVP.
2. A Raspberry Pi Pico (RP2040) is used, not Pico 2 (RP2350). The plan is compatible with either; embassy-rp supports both.
3. VID/PID: during development, use the Raspberry Pi foundation's PID for HID test devices (`0x2E8A:0x000A`) or register a free PID from OpenMoko. Finalize before distribution.
4. The hardware multiplexing scheme (MCP23017 × 5 via I2C) is assumed but not designed in this plan. The firmware driver is written to that assumption; if the actual hardware uses a different expander, Step 10 is the only change required.
5. Encoder tap detection: encoder shafts with push-button capability are assumed. If the chosen encoders lack this, encoder tap events are dropped and the confirm mechanic falls back to "change applies on next loop."
6. Pi Zero 2W is the deployment target for the engine. If the original Pi Zero (single-core, 512 MB) is used instead, Rust's single-threaded async executor (smol or tokio current_thread) should replace std::thread to avoid scheduling overhead. Flag for stakeholder decision.

### Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| embassy-usb HID vendor class has RP2040-specific bug | Low | Fallback: port firmware to C + TinyUSB; protocol unchanged |
| SCHED_FIFO denied on Pi Zero (non-root) | Medium | Accept ~1–2 ms extra jitter; use `nice -n -20` as partial mitigation |
| MCP23017 interrupt latency too high for encoder at fast spin rate | Medium | Accumulate deltas; no event is lost, only temporal resolution is coarser |
| hidapi on Raspbian requires udev rule for non-root access | High (certain) | Ship a udev rule file in `contrib/udev/`; document in README |
| midir ALSA backend needs `libasound2-dev` at build time | High (certain) | Document build dependency; provide install command |
| Encoder quadrature decode incorrect (missed pulses) | Medium | Test with known-good 24 PPR encoder; add configurable debounce constant |

---

*Plan complete. Next step: stakeholder review of stack choices and HID protocol byte layout, then hand off to Manager agent to decompose Steps 1–15 into individual task files.*
