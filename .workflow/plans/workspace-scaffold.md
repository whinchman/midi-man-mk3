# Plan: Workspace Scaffold and Build Proof

**Task:** step1-workspace-scaffold
**Branch:** ws/workspace-scaffold
**Date:** 2026-05-02

## Overview

Initialize the Cargo workspace with `engine` and `firmware` member crates, add
all required dependencies, configure cross-compilation for the firmware crate,
and verify both crates build.

## Steps

### Step 1 — Workspace root Cargo.toml
- `[workspace]` with `members = ["engine", "firmware"]`, `default-members = ["engine"]`, `resolver = "2"`.
- Workspace-level `[profile.dev]` and `[profile.release]` (firmware profiles must live at root).

### Step 2 — Root `.cargo/config.toml`
- Host build defaults; no special linker for `x86_64-unknown-linux-gnu`.

### Step 3 — `engine/Cargo.toml`
- Dependencies: `midir`, `hidapi`, `ratatui`, `crossterm`, `libc`.

### Step 4 — `engine/src/main.rs`
- Trivial `fn main() {}` stub.

### Step 5 — `firmware/Cargo.toml`
- Dependencies: `embassy-executor`, `embassy-rp`, `embassy-usb`, `embassy-time`,
  `embedded-hal`, `defmt`, `defmt-rtt`, `panic-probe`, `cortex-m`, `cortex-m-rt`.
- `flip-link` is the linker binary; configured in `firmware/.cargo/config.toml`.
- `mcp23017` commented out (added in Step 10+).

### Step 6 — `firmware/.cargo/config.toml`
- `target = "thumbv6m-none-eabi"`, linker = `flip-link`.
- `rustflags` for `link.x`, `defmt.x`, `--nmagic`.

### Step 7 — `firmware/memory.x`
- RP2040 memory layout: 2 MB flash at `0x10000000`, 264 KB SRAM at `0x20000000`.
- Boot2 section at start of flash.

### Step 8 — `firmware/build.rs`
- Copies `memory.x` to `OUT_DIR` so the linker can find it.
- Emits `cargo:rustc-link-search` for `OUT_DIR`.

### Step 9 — `firmware/src/main.rs`
- Minimal Embassy entry point (`#[embassy_executor::main]`).
- Binds `USBCTRL_IRQ`; initialises peripherals with `embassy_rp::init(Default::default())`.
- Empty loop (no tasks in this scaffold).

## Build Notes

On this Fedora host the `alsa-lib-devel` package is not installed.
`libasound.so.2` is present at `/usr/lib64/` but `alsa.pc` is missing.
Work-around: create `/tmp/alsa-pkg/alsa.pc` pointing at the installed library
and pass `PKG_CONFIG_PATH=/tmp/alsa-pkg` to cargo.

Command to build engine:
```
PKG_CONFIG_PATH=/tmp/alsa-pkg cargo build -p engine
```

Command to build firmware:
```
cargo build -p firmware --target thumbv6m-none-eabi
```

When `alsa-lib-devel` is installed (`sudo dnf install alsa-lib-devel`),
the `PKG_CONFIG_PATH` prefix is not needed.
