# Task: Workspace Scaffold and Build Proof

- **Type**: coder
- **Status**: done
- **Review Status**: approved (1 warning, 2 info)
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

---

## Code Review — ws/workspace-scaffold

**Reviewer:** code-reviewer agent
**Date:** 2026-05-02
**Verdict:** APPROVE (1 warning, 2 info; no critical findings)

### Acceptance Criteria Verification

All 11 acceptance criteria are met:
- [x] Workspace root `Cargo.toml`: `members = ["engine", "firmware"]`, `default-members = ["engine"]`, `resolver = "2"`.
- [x] `engine/Cargo.toml`: midir 0.11, hidapi 2.6, ratatui 0.30, crossterm 0.29, libc 0.2.
- [x] `firmware/Cargo.toml`: embassy-executor 0.10, embassy-rp 0.10, embassy-usb 0.6, embassy-time 0.5, embedded-hal 1.0, defmt 0.3, defmt-rtt 0.4, panic-probe 0.3, cortex-m 0.7, cortex-m-rt 0.7. mcp23017 stub comment present.
- [x] `.cargo/config.toml` at workspace root: host build defaults present.
- [x] `firmware/.cargo/config.toml`: `target = "thumbv6m-none-eabi"`, linker = `flip-link`, rustflags include `--nmagic`, `-Tlink.x`, `-Tdefmt.x`.
- [x] `firmware/memory.x`: BOOT2 at 0x10000000 (256 B), FLASH at 0x10000100 (2 MB − 256 B), RAM at 0x20000000 (264 KB). Layout arithmetic verified correct.
- [x] `firmware/build.rs`: reads `memory.x`, writes to `OUT_DIR`, emits `cargo:rustc-link-search` and `cargo:rerun-if-changed`.
- [x] `engine/src/main.rs`: trivial `fn main() {}`.
- [x] `firmware/src/main.rs`: `#![no_std] #![no_main]`, Embassy entry macro, `bind_interrupts!` for USBCTRL_IRQ, `embassy_rp::init(Default::default())`, empty loop.
- [x] Both crates reported to build successfully (alsa-lib-devel host gap is documented and excluded per reviewer brief).

### Findings

#### [WARNING] Cargo.toml:13-16 — `debug = 2` in `[profile.release]` applies to firmware binary

The workspace-root `[profile.release]` sets `debug = 2` (full debug info). Because workspace profiles apply globally, `cargo build -p firmware --release` will embed full DWARF debug symbols in the firmware ELF. For an RP2040 with 2 MB flash this is unlikely to overflow the flash region in a scaffold, but it is contrary to standard embedded release practice and will become a real problem as the firmware grows. The `debug = 2` in release is appropriate for the engine (to support profiling) but not for firmware. Suggested fix: override with a `[profile.release.package.firmware]` table setting `debug = 0` or `debug = false`, or document that firmware is always built with a custom `cargo build --profile firmware-release` profile defined later.

#### [INFO] engine/Cargo.toml:12 — `linux-static-hidraw` feature bundles hidraw statically

`hidapi = { version = "2.6", features = ["linux-static-hidraw"] }` statically links the hidraw backend. This means `libhidapi` is compiled into the binary — no shared-library runtime dependency, which is good for distribution. However, it excludes the `libusb` backend (for non-HID-class USB devices). This is intentional for the MK3 MIDI controller use case but worth noting for future developers.

#### [INFO] firmware/src/main.rs — peripherals initialized but never used in scaffold

`let _p = embassy_rp::init(Default::default())` initializes all RP2040 peripherals. The leading underscore suppresses the unused-variable warning from rustc. This is standard Embassy scaffold practice and is not a bug; just noting it for future steps that will claim peripherals from `_p`.

### Summary

The scaffold is correct and complete. All required files are present with the right structure, dependency versions resolve cleanly in `Cargo.lock`, the RP2040 memory layout is arithmetically verified, and the build.rs follows the standard Embassy pattern. The one warning (debug symbols in release firmware) is low-risk at scaffold stage but should be addressed before the first real firmware release build.

---

## PR Feedback

PR: https://github.com/whinchman/midi-man-mk3/pull/2

### Local Test Gate: PASSED

- `PKG_CONFIG_PATH=/tmp/alsa-pkg cargo build -p engine` — exit 0
- `cargo build -p firmware --target thumbv6m-none-eabi` — exit 0
- `PKG_CONFIG_PATH=/tmp/alsa-pkg cargo test -p engine` — exit 0 (0 tests; scaffold stage)

### Comments Requiring Action

(none)

### CI Failures

(none — no CI pipeline configured for this repository)

### Questions / Acknowledged

(none)
