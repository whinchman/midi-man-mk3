Name: cargo-deps
Type: coder
Status: pending
Repo: /home/whinchman/experiments/midi-man-mk3
Parallel Group: 1
Feature Branch: feature/song-mode
Branch: feature/song-mode/cargo-deps
Base Branch: feature/song-mode
Goal: Add serde (with derive feature) and toml 0.8 to engine/Cargo.toml so all subsequent song-mode tasks can compile.

Context:
  File to modify: engine/Cargo.toml

  Current [dependencies] block (lines 11-17):
    midir = { version = "0.11", optional = true }
    hidapi = { version = "2.6", features = ["linux-static-hidraw"], optional = true }
    ratatui = { version = "0.30", default-features = false, features = ["macros"] }
    crossterm = { version = "0.29", optional = true }
    libc = "0.2"

  Add these two lines to the [dependencies] section:
    serde = { version = "1", features = ["derive"] }
    toml = "0.8"

  Note: serde 1.x is already transitively present via ratatui but does not
  have the `derive` feature enabled; it must be added as a direct dep. toml 0.8
  depends on serde 1 — no version conflict exists. Do NOT touch [features].

  After editing, verify the lockfile resolves cleanly:
    cargo build -p engine

Acceptance Criteria:
  - [ ] engine/Cargo.toml [dependencies] contains `serde = { version = "1", features = ["derive"] }`
  - [ ] engine/Cargo.toml [dependencies] contains `toml = "0.8"`
  - [ ] `cargo build -p engine` succeeds with no errors
  - [ ] `cargo test -p engine` continues to pass (no regressions)

Dependencies: (none — this is the root task)
