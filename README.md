# midi-man-mk3

A 16-step MIDI sequencer built around a custom RP2040 HID controller. A host engine (Rust, runs on PC or Pi Zero 2W) reads pad/encoder input from the controller over USB HID, drives a real-time clock, and outputs MIDI notes via ALSA. Firmware runs on the RP2040 Pico and exposes the controller as a USB HID vendor device.

---

## Prerequisites

### Engine (host binary)

| Requirement | Notes |
|---|---|
| Rust stable ≥ 1.75 | `rustup update stable` |
| ALSA development headers | Fedora: `sudo dnf install alsa-lib-devel` · Ubuntu/Debian: `sudo apt install libasound2-dev` |
| hidraw kernel module | Usually loaded by default on Linux; `modprobe hidraw` if not |
| libhidapi (hidraw backend) | Fedora: `sudo dnf install hidapi-devel` · Ubuntu: `sudo apt install libhidapi-dev` |

The engine can be built and tested without ALSA or hidraw by omitting the `hw-io` feature (see Testing below).

### Firmware (RP2040)

| Requirement | Notes |
|---|---|
| Rust nightly | `rustup install nightly` |
| thumbv6m-none-eabi target | `rustup target add thumbv6m-none-eabi --toolchain nightly` |
| flip-link | `cargo install flip-link` — stack-overflow-safe linker |
| probe-rs | `cargo install probe-rs-tools` — for flashing via SWD |

---

## Build

### Engine (default — no hardware required)

```
cargo build -p engine
```

### Engine with hardware I/O (ALSA + HID)

```
cargo build -p engine --features hw-io --release
```

### Firmware

```
cargo build -p firmware --target thumbv6m-none-eabi --release
```

---

## Testing

Tests are in `engine/tests/` and run entirely without hardware (no ALSA, no HID device, no real terminal).

```
cargo test -p engine
```

To verify the full build compiles with hardware features enabled (requires ALSA headers):

```
cargo build -p engine --features hw-io
```

---

## Run

### With a connected MIDI interface and HID controller

```
cargo run -p engine --features hw-io --release
```

### Specifying a MIDI port or custom HID VID/PID

```
cargo run -p engine --features hw-io --release -- --midi-port "UM-ONE" --hid-vid 0xCAFE --hid-pid 0x4004
```

- `--midi-port <name>` — substring match against available ALSA MIDI ports; defaults to the first port found
- `--hid-vid <hex>` — USB vendor ID of the controller (default `0xCAFE`)
- `--hid-pid <hex>` — USB product ID of the controller (default `0x4004`)

Press **Ctrl-C** to stop cleanly (sends MIDI Stop, closes all notes).

### Keyboard controls (terminal UI)

| Key | Action |
|---|---|
| Space | Toggle selected step on/off |
| ← / → | Move step selection |
| ↑ / ↓ | Shift note up/down by scale degree |
| Shift + ↑ / ↓ | Velocity up/down |
| Enter | Confirm pending edit |
| F1 | Open param overlay (tempo, swing, key, mode, step size, loop) |
| Esc | Close overlay / cancel edit |

### Flash firmware to RP2040

```
cargo run -p firmware --target thumbv6m-none-eabi --release
```

Requires a debug probe (e.g. Raspberry Pi Debug Probe) connected via SWD. `probe-rs` is configured as the runner in `firmware/.cargo/config.toml`.

---

## Project structure

```
midi-man-mk3/
├── engine/          # Host sequencer engine (PC / Pi Zero 2W)
│   ├── src/
│   │   ├── clock.rs        # Real-time clock thread (clock_nanosleep, swing)
│   │   ├── hid.rs          # USB HID host reader/writer
│   │   ├── input.rs        # Input command abstraction + key translation
│   │   ├── midi_out.rs     # ALSA MIDI output + NoteOff scheduling
│   │   ├── music_theory.rs # Scale tables, note naming, scale navigation
│   │   ├── state.rs        # Shared sequencer state (Arc<RwLock<_>>)
│   │   ├── ui.rs           # Terminal UI event loop (crossterm, hw-io)
│   │   └── ui_render.rs    # Pure ratatui rendering (no crossterm dep)
│   └── tests/              # Integration tests (no hardware required)
└── firmware/        # RP2040 Embassy firmware scaffold
    └── src/
        ├── main.rs         # Embassy async entry point
        └── report.rs       # HID InReport/OutReport wire structs
```

---

## Attributions

All direct dependencies are used under permissive open-source licenses. Notable ones:

| Crate | License | Use |
|---|---|---|
| [midir](https://github.com/Boddlnagg/midir) | MIT | ALSA MIDI I/O |
| [hidapi](https://github.com/ruabmbua/hidapi-rs) | MIT | USB HID host via hidraw |
| [ratatui](https://github.com/ratatui/ratatui) | MIT | Terminal UI rendering |
| [crossterm](https://github.com/crossterm-rs/crossterm) | MIT | Cross-platform terminal control |
| [libc](https://github.com/rust-lang/libc) | MIT OR Apache-2.0 | `clock_nanosleep`, `SCHED_FIFO` |
| [embassy-rp](https://github.com/embassy-rs/embassy) | MIT OR Apache-2.0 | RP2040 async HAL |
| [embassy-usb](https://github.com/embassy-rs/embassy) | MIT OR Apache-2.0 | USB HID device stack |
| [defmt](https://github.com/knurling-rs/defmt) | MIT OR Apache-2.0 | Embedded structured logging |
| [cortex-m-rt](https://github.com/rust-embedded/cortex-m) | MIT OR Apache-2.0 | Cortex-M runtime |
| [rp2040-boot2](https://github.com/rp-rs/rp2040-boot2) | BSD-3-Clause | RP2040 second-stage bootloader |
| [rp-pac](https://github.com/rp-rs/rp-pac) | BSD-3-Clause | RP2040 peripheral access crate |
| [usbd-hid](https://github.com/twitchyliquid64/usbd-hid) | MIT OR Apache-2.0 | USB HID descriptor macros |

Full license texts are available in each crate's source repository. The BSD-3-Clause crates (`rp2040-boot2`, `rp-pac`) require retention of copyright notices in binary distributions — these are embedded in the firmware image and satisfied by the crates themselves.

This project does not have a license file yet; all original source is copyright Will Hinchman.
