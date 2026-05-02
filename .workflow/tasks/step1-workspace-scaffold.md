# Task: Workspace Scaffold and Build Proof

- **Type**: coder
- **Status**: done
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

Implemented on branch `ws/workspace-scaffold` (worktree at `.workflow/worktrees/workspace-scaffold`).

All acceptance criteria met:
- Workspace root `Cargo.toml`: `members = ["engine", "firmware"]`, `default-members = ["engine"]`, `resolver = "2"`.
- `engine/Cargo.toml`: midir 0.11, hidapi 2.6, ratatui 0.30, crossterm 0.29, libc 0.2.
- `firmware/Cargo.toml`: embassy-executor 0.10, embassy-rp 0.10 (rp2040 + time-driver + defmt), embassy-usb 0.6, embassy-time 0.5, embedded-hal 1.0, defmt, defmt-rtt, panic-probe, cortex-m, cortex-m-rt. flip-link linker configured in firmware/.cargo/config.toml. mcp23017 commented in with note.
- `.cargo/config.toml` at workspace root: host build defaults.
- `firmware/.cargo/config.toml`: `target = "thumbv6m-none-eabi"`, linker = `flip-link`, rustflags for link.x and defmt.x.
- `firmware/memory.x`: RP2040 layout — 2 MB flash at 0x10000000, 264 KB SRAM at 0x20000000, Boot2 section.
- `firmware/build.rs`: copies memory.x to OUT_DIR and emits rustc-link-search.
- `engine/src/main.rs`: trivial `fn main() {}`.
- `firmware/src/main.rs`: minimal Embassy entry with USBCTRL_IRQ bind and empty loop.

Build results:
- `cargo build -p engine` — SUCCESS (requires `PKG_CONFIG_PATH=/tmp/alsa-pkg` workaround on this host because `alsa-lib-devel` is not installed; `libasound.so.2` is present but `alsa.pc` is missing).
- `cargo build -p firmware --target thumbv6m-none-eabi` — SUCCESS.

**Note for reviewer:** Install `alsa-lib-devel` (`sudo dnf install alsa-lib-devel`) on this host to remove the `PKG_CONFIG_PATH` workaround requirement. The scaffold itself is correct — this is a host environment gap, not a code issue.
