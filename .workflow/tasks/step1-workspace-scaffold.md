# Task: Workspace Scaffold and Build Proof

- **Type**: coder
- **Status**: pending
- **Repo**: midi-man-mk3
- **Parallel Group**: 1
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/workspace-scaffold
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 1
- **Dependencies**: none

## Description

Initialize the Cargo workspace with `engine` and `firmware` member crates. Add all required dependencies to each crate's `Cargo.toml`. Configure cross-compilation targets and the flip-link linker for the firmware crate. Add the RP2040 memory layout file and a `build.rs` for the firmware. Stub out `main.rs` in both crates (just enough to compile). Verify both crates build successfully.

This is a pure scaffolding task — no functional logic is written. The goal is a green `cargo build` for both crates.

## Acceptance Criteria

- [ ] `Cargo.toml` at workspace root defines `members = ["engine", "firmware"]` with `resolver = "2"`.
- [ ] `engine/Cargo.toml` declares dependencies: `midir`, `hidapi`, `ratatui`, `crossterm`, `libc`.
- [ ] `firmware/Cargo.toml` declares dependencies: `embassy-executor`, `embassy-rp`, `embassy-usb`, `embassy-time`, `flip-link`, `embedded-hal`, and either `mcp23017` community crate or an inline stub comment.
- [ ] `.cargo/config.toml` at workspace root configures host build defaults.
- [ ] `firmware/.cargo/config.toml` sets `target = "thumbv6m-none-eabi"`, linker to `flip-link`, and any required `rustflags` (e.g. for defmt if used).
- [ ] `firmware/memory.x` contains the RP2040 memory layout: 2 MB flash at `0x10000000`, 264 KB SRAM at `0x20000000`.
- [ ] `firmware/build.rs` copies `memory.x` to the linker search path (standard Embassy/flip-link pattern).
- [ ] `engine/src/main.rs` compiles with a trivial `fn main() {}` body.
- [ ] `firmware/src/main.rs` compiles with a minimal Embassy entry point (no peripherals initialized).
- [ ] `cargo build -p engine` succeeds for the host target.
- [ ] `cargo build -p firmware` succeeds targeting `thumbv6m-none-eabi`.

## Interface Contracts

None — this step produces the scaffold that all other steps build on. No public API is defined here.

## Context

Repository layout from the plan (`midi-man-mk3-mvp.md`, Section 7):

```
midi-man-mk3/
├── Cargo.toml
├── Cargo.lock
├── .cargo/
│   └── config.toml
├── engine/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── firmware/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── memory.x
│   ├── .cargo/
│   │   └── config.toml
│   └── src/
│       └── main.rs
```

Engine stack: Rust stable, edition 2021. MIDI via midir 0.10 (ALSA). UI via ratatui 0.29 + crossterm. HID via hidapi 2.x. Real-time timing via libc clock_nanosleep.

Firmware stack: Rust no_std, embassy-rp targeting RP2040, embassy-usb for HID vendor class, flip-link for stack-overflow detection.

The firmware crate must NOT be built by default when running `cargo build` at the workspace root without specifying `-p firmware` (the thumbv6m target requires explicit selection). Consider using a `[profile.dev]` or workspace `default-members` limited to `engine` to prevent accidental host-target firmware builds.

## Notes

