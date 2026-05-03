# Test Plan: fix-hid-and-main-followup

## Scope

QA tests for three acceptance criteria from task fix-hid-and-main-followup.

## What is Being Tested

### AC1 — BUG-015: `open_device()` removed from `engine/src/hid.rs`
- Verify the function is absent at compile time (the test file must not import it)
- Verify `HID_VID` and `HID_PID` constants are still exported (they are used by callers)
- No dead code: the file must compile cleanly without referencing a removed symbol

### AC2 — BUG-016: `choose_midi_port` emits warning on filter miss
- `choose_midi_port` is a hardware path (`#[cfg(feature = "hw-io")]`) that drives
  real ALSA ports. It cannot be unit-tested directly.
- The **testable proxy** is `select_port_idx`, which already has unit coverage.
- New tests verify the `select_port_idx` fallback-to-zero path covers every edge
  case (empty filter string, whitespace-only filter, filter with many ports, etc.)
  to document and lock down the behaviour that `choose_midi_port` mirrors.
- Add a compile-time / grep test (documentation test) that confirms the warning
  is present in the source.

### AC3 — BUG-013: `.cargo/config.toml` comment references correct invocation
- A test that reads `.cargo/config.toml` as a string and asserts:
  - It does NOT contain `CARGO_CONFIG_TOML`
  - It DOES contain `--config .cargo/config.local.toml`

## Test Cases

### hid.rs additions
1. `open_device_symbol_does_not_exist` — compile-time proof: importing
   `engine::hid::open_device` must not compile. Cannot be a `#[test]` (would
   need `compile_fail`). Document with a comment in the test file instead.
2. `hid_vid_pid_constants_remain_accessible` — already exists; no change needed.

### midi_out.rs additions
3. `select_port_idx_empty_filter_string_matches_all` — filter `""` matches
   every port (substring of everything); first matching port (index 0) returned.
4. `select_port_idx_filter_matches_multiple_returns_first` — when filter matches
   ports 1 and 2, returns index 1 (first match).
5. `select_port_idx_no_ports_with_filter_is_none` — already tested (empty list).
6. `select_port_idx_filter_is_case_insensitive_all_uppercase_port` — port name
   is all-uppercase; lowercase filter must still match.
7. `select_port_idx_fallback_with_many_ports` — 5 ports, no match, still falls
   back to index 0.

### New integration test file: cargo_config.rs
8. `cargo_config_toml_does_not_reference_env_var` — reads `.cargo/config.toml`
   and asserts `CARGO_CONFIG_TOML` is absent.
9. `cargo_config_toml_references_config_flag` — reads `.cargo/config.toml` and
   asserts `--config .cargo/config.local.toml` is present.

## Dependencies / Fixtures
- All tests are pure (no I/O beyond reading a static file).
- `select_port_idx` tests use `&str` slices — no mocks needed.
- `cargo_config.rs` reads the project file using `include_str!` or `std::fs::read_to_string`.
