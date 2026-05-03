# Task: fix-cargo-config-tmp-paths

- **Status**: pending
- **Type**: coder
- **Feature Branch**: fix/known-bugs
- **Branch**: fix/known-bugs/fix-cargo-config-tmp-paths
- **Base Branch**: fix/known-bugs
- **Parallel Group**: 1
- **Bugs Fixed**: BUG-003

## Goal

Remove host-specific `/tmp` path workarounds from `.cargo/config.toml` and document the local-override pattern instead.

## Context

`.cargo/config.toml` unconditionally sets `PKG_CONFIG_PATH = "/tmp/alsa-pkg"` and `rustflags = ["-L", "/tmp/alsa-lib"]`. These are workarounds for a system missing `alsa-lib-devel` that were committed to source. They break CI and other developer machines.

**File:** `.cargo/config.toml`, lines 11 and 17.

## Acceptance Criteria

- `[env]` `PKG_CONFIG_PATH` entry and `[target.x86_64-unknown-linux-gnu]` `rustflags` entry removed from `.cargo/config.toml`.
- `.gitignore` gains an entry for `.cargo/config.local.toml`.
- A comment in `.cargo/config.toml` (or in a build note at the top of `engine/src/midi_out.rs`) documents the local-override pattern for developers who need the workaround:
  ```toml
  # .cargo/config.local.toml  (gitignored)
  [env]
  PKG_CONFIG_PATH = "/tmp/alsa-pkg"
  [target.x86_64-unknown-linux-gnu]
  rustflags = ["-L", "/tmp/alsa-lib"]
  ```
- `cargo test -p engine` passes on the current system (with whatever alsa setup is present).

## Notes

