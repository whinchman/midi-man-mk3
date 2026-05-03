# Plan: silence HID thread stderr while TUI is active (issue #75)

## Overview
The HID thread emits `eprintln!` messages (e.g. "[hid] device 0x2e8a:0x000a not
found: hidapi error: ...; HID thread exit") that bleed through the ratatui
alternate screen and corrupt rendered frames. Replace the HID thread's
stderr writes with a small file-sink logger that appends to a log file under
`.workflow/logs/`. No in-TUI status indicator (reserved for upcoming refactor).

## Steps

### Step 1 — Add `hid_log` helper in `engine::hid`
- New private function `hid_log(msg: &str)` that opens
  `.workflow/logs/hid.log` in append mode and writes a timestamped line. On
  any I/O failure it silently no-ops (must NEVER write to stderr while the
  TUI owns the terminal).
- Use only `std::fs`, `std::io::Write`, `std::time::SystemTime` — no new
  crates.
- Resolve the log path via env var `MIDIMAN_HID_LOG` if set, otherwise
  `.workflow/logs/hid.log` relative to current working directory; fall back
  to `/tmp/midi-man-hid.log` if `.workflow/logs` does not exist or is not
  writable.
- Unit-testable: factor out `hid_log_to(path, msg)` so tests can use a
  tempfile.

### Step 2 — Replace `eprintln!` calls in `run_hid` and the init paths
- Swap each `eprintln!("[hid] ...")` for `hid_log(&format!(...))`.
- Preserve early-return / exit semantics.

### Step 3 — Tests
- `hid_log_to` writes the given message to the given path, appending if the
  file exists.
- `hid_log_to` returns Ok and creates the file when its directory exists.
- `hid_log_to` returns Err and does not panic when the directory does not
  exist (caller swallows the error).

### Step 4 — Verify
- `cargo test -p engine` passes.
- `cargo clippy -p engine` clean.
- `cargo build -p engine --release` succeeds (with hw-io feature).

## Out of scope
- midi_out and clock eprintln calls (different threads, separate task).
- An in-TUI error indicator — reserved for upcoming UI refactor.
