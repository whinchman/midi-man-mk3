# Bugs

Known bugs discovered by QA and Code Reviewer agents. Each bug should have
enough detail for a Coder agent to reproduce and fix it.

Bugs here follow the same approval flow as features — the stakeholder moves
approved fixes to TODO.md (removing them from this file).

---

## BUG-001 — [WARNING] Workspace release profile embeds full debug info in firmware binary

- **File:** `Cargo.toml` (workspace root), lines 13–16
- **Branch:** `ws/workspace-scaffold`
- **Discovered:** 2026-05-02 by code-reviewer agent (step1-workspace-scaffold review)
- **Severity:** warning

### Description

The workspace-root `[profile.release]` sets `debug = 2` (full DWARF debug symbols). Because Cargo workspace profiles apply to all member crates, building `cargo build -p firmware --release` will embed full debug info into the firmware ELF, significantly increasing binary size. For the RP2040's 2 MB flash this is tolerable at scaffold stage but will become a flash-overflow risk as the firmware grows. Debug symbols have no business being in a production firmware image.

### Reproduction

1. Checkout branch `ws/workspace-scaffold`.
2. Run `cargo build -p firmware --target thumbv6m-none-eabi --release`.
3. Inspect the ELF: `arm-none-eabi-size target/thumbv6m-none-eabi/release/firmware` — the `.debug_*` sections will be present and large.

### Suggested Fix

Add a package-level profile override in the workspace `Cargo.toml` to strip debug info from firmware release builds:

```toml
[profile.release.package.firmware]
debug = false
```

Or define a dedicated `firmware-release` profile later and document that firmware release builds use `--profile firmware-release`. Either approach keeps the engine's `debug = 2` (useful for profiling) while producing a lean firmware image.

---

## BUG-002 — [WARNING] `add_nanos_signed` drops `tv_sec` borrow on negative swing overflow

- **File:** `engine/src/clock.rs`, lines 114–124
- **Branch:** `clock-thread`
- **Discovered:** 2026-05-02 by code-reviewer agent (step4-clock-thread review)
- **Severity:** warning

### Description

`add_nanos_signed` adds the signed offset only to `tv_nsec` and then clamps the result to zero when it goes negative. The clamp discards the needed borrow from `tv_sec`. When a negative swing offset is larger than the current `tv_nsec` value (i.e. the swing crosses a whole-second boundary), the resulting `tv_sec` is left unchanged while `tv_nsec` is clamped to 0. This means the absolute wake time becomes `ts.tv_sec + 0.0s` instead of the correct `(ts.tv_sec - 1) + (1.0 - |delta|)s`.

In practice: at 120 BPM sixteenth steps (tick = 125 ms) with swing = -50, the offset is -62.5 ms. If `tv_nsec` at the start of a second is below 62,500,000 ns the wake time gets pinned to the start of the current second rather than 62.5 ms before the beat — causing up to a ~62 ms timing error. `clock_nanosleep(TIMER_ABSTIME)` with a time in the past returns immediately, so the odd step fires too early rather than hanging.

The existing test (`add_nanos_signed_clamps_to_zero`) only asserts `tv_nsec >= 0` and does not catch the incorrect `tv_sec`.

### Reproduction

```rust
let ts = libc::timespec { tv_sec: 1, tv_nsec: 100_000_000 }; // 1.1 s
let result = add_nanos_signed(ts, -200_000_000);              // offset -0.2 s
// Expected: tv_sec=0, tv_nsec=900_000_000 (= 0.9 s)
// Actual:   tv_sec=1, tv_nsec=0           (= 1.0 s) — 100 ms wrong
assert_eq!(result.tv_sec, 0);            // FAILS
assert_eq!(result.tv_nsec, 900_000_000); // FAILS
```

### Suggested Fix

Perform the arithmetic in full nanoseconds spanning both fields, then re-normalise:

```rust
fn add_nanos_signed(ts: libc::timespec, nanos: i64) -> libc::timespec {
    let total_ns: i64 = ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec + nanos;
    let total_ns = total_ns.max(0);
    libc::timespec {
        tv_sec: (total_ns / 1_000_000_000) as libc::time_t,
        tv_nsec: (total_ns % 1_000_000_000) as libc::c_long,
    }
}
```

Also update the existing test to assert the corrected `tv_sec` value alongside `tv_nsec`.

---

## BUG-003 — [WARNING] `.cargo/config.toml` hardcodes `/tmp` paths that break builds on clean systems

- **File:** `.cargo/config.toml`, lines 11 and 17
- **Branch:** `engine-phase1/midi-output`
- **Discovered:** 2026-05-02 by code-reviewer agent (step5-midi-output review)
- **Severity:** warning

### Description

`PKG_CONFIG_PATH = "/tmp/alsa-pkg"` and `rustflags = ["-L", "/tmp/alsa-lib"]` are unconditional entries in the workspace `.cargo/config.toml`. These are host-specific workarounds for a system missing `alsa-lib-devel` that were committed to source. On any other system (CI, another developer's machine, a container with `alsa-lib-devel` properly installed):

- `/tmp/alsa-pkg` will not exist — `pkg-config` will use an empty extra search path (harmless but noisy).
- `/tmp/alsa-lib` will not exist — the linker receives a spurious `-L /tmp/alsa-lib` flag. If the directory does not exist the linker ignores it; if it exists and contains a stale symlink the build may silently link the wrong `libasound.so`.
- Any CI system that installs `alsa-lib-devel` normally will have `alsa.pc` in its default `PKG_CONFIG_PATH` already; the `/tmp/alsa-pkg` override is benign only if the override path is missing, but it creates confusion.

The real risk is a developer on a system where `/tmp/alsa-lib` happens to contain something gets a build that links against an unexpected library version.

### Reproduction

1. Checkout `engine-phase1/midi-output` on a system with `alsa-lib-devel` installed.
2. Run `cargo build -p engine --verbose`.
3. Observe `-L /tmp/alsa-lib` in the linker invocation regardless of whether that path is meaningful on the current host.

### Suggested Fix

Remove the `[env]` `PKG_CONFIG_PATH` and `[target.x86_64-unknown-linux-gnu]` `rustflags` entries from `.cargo/config.toml`. Document the workaround in a comment in `engine/src/midi_out.rs` or in build notes. Developers needing the workaround can set variables in their shell or in a gitignored local override file:

```toml
# .cargo/config.local.toml  (gitignored)
[env]
PKG_CONFIG_PATH = "/tmp/alsa-pkg"

[target.x86_64-unknown-linux-gnu]
rustflags = ["-L", "/tmp/alsa-lib"]
```

Add `.cargo/config.local.toml` to `.gitignore` and document this pattern in the build notes.

---

## BUG-004 — [WARNING] `tick()` ignores `StepData.velocity`; hardcodes 100 for every NoteOn

- **File:** `engine/src/state.rs`, line 185
- **Branch:** `engine-phase1/input-command-abstraction`
- **Discovered:** 2026-05-02 by code-reviewer agent (step6b-input-command-abstraction review)
- **Severity:** warning

### Description

This step added `velocity: u8` to `StepData` and wired up the full `VelocityDelta` → `Confirm` → `StepData.velocity` commit pipeline. However, `SequencerState::tick()` (line 185) still uses a hardcoded `velocity: 100` in the `MidiEvent::NoteOn` it produces instead of reading `step.velocity`. As a result, velocity edits committed by `Confirm` are silently discarded — every note plays at velocity 100 regardless of what the user set.

The existing test `tick_note_on_has_correct_fields` also asserts `velocity: 100` so the bug is invisible to the test suite.

### Reproduction

```rust
let mut s = SequencerState::default();
s.playing = true;
s.steps[0].enabled = true;
s.steps[0].velocity = 64;  // set explicitly
s.playhead = 15;            // so next tick lands on step 0
let evt = s.tick();
// Expected: velocity: 64
// Actual:   velocity: 100  -- bug
assert!(matches!(evt, Some(MidiEvent::NoteOn { velocity: 64, .. })));
```

### Suggested Fix

Change line 185 in `engine/src/state.rs`:

```rust
// Before:
velocity: 100,
// After:
velocity: step.velocity,
```

Also update `tick_note_on_has_correct_fields` to set a non-default `step.velocity` value (e.g. 64) and assert it is reflected in the `NoteOn` event.

---

## BUG-005 — [WARNING] `unsafe { std::mem::transmute(report) }` in test violates Safe-Rust standard

- **File:** `engine/src/hid.rs`, line 317
- **Branch:** `engine-phase1/input-command-abstraction`
- **Discovered:** 2026-05-02 by code-reviewer agent (step6b-input-command-abstraction review)
- **Severity:** warning

### Description

`in_report_field_offsets_match_wire_spec` uses `std::mem::transmute::<InReport, [u8; 64]>` to read the raw byte layout of a `repr(C)` struct. The project code standard states "Safe Rust only — no unsafe without a comment explaining why." The comment claims safety based on `repr(C)` and "no padding", but `repr(C)` only guarantees field order — it does not guarantee zero inter-field padding if field alignments differ. While the current field types (`u8`, `[u8; N]`, `[i8; N]`) all have alignment 1 (so no padding is inserted in practice), the transmute is technically unsound if the struct is later modified to include an aligned field. The test can be rewritten without `unsafe` using `std::mem::offset_of!` (stable since Rust 1.77).

### Suggested Fix

Replace the `unsafe` transmute block with stable `offset_of!` assertions:

```rust
use std::mem::offset_of;
assert_eq!(offset_of!(InReport, report_id), 0);
assert_eq!(offset_of!(InReport, seq), 1);
assert_eq!(offset_of!(InReport, flags), 2);
assert_eq!(offset_of!(InReport, step_buttons), 3);
assert_eq!(offset_of!(InReport, step_enable_state), 5);
assert_eq!(offset_of!(InReport, param_buttons), 7);
assert_eq!(offset_of!(InReport, encoder_deltas), 9);
assert_eq!(offset_of!(InReport, tempo_delta), 25);
assert_eq!(offset_of!(InReport, param_knob_delta), 26);
assert_eq!(offset_of!(InReport, reserved), 27);
```

---

## BUG-006 — [WARNING] `run_hid` reuses `buf` across loop iterations; partial reads leave stale bytes

- **File:** `engine/src/hid.rs`, lines 307–323
- **Branch:** `hid-host-reader-writer`
- **Discovered:** 2026-05-02 by code-reviewer agent (step7-hid-host-reader-writer review)
- **Severity:** warning

### Description

`buf` is declared once before the loop (`let mut buf = [0u8; 64];`) and passed to `device.read_timeout` each iteration. `hidapi`'s `read_timeout` only writes `n` bytes into the buffer; the remaining `64 - n` bytes retain their previous values. The code guards only `n == 0` (timeout) and proceeds to `InReport::from_bytes(&buf)` for any `n > 0`. If the device sends a short report (n > 0 but n < 64), fields beyond byte `n` are parsed from the previous iteration's data, silently producing a corrupt `InReport` with fields drawn from two different reports.

In practice the RP2040 firmware always sends exactly 64-byte reports, but defensive code should zero the buffer each cycle to avoid latent bugs if the firmware changes or if a different host OS's HID layer pads differently.

### Reproduction

Simulate a short read: fill `buf` with `0xFF` before a report, call `read_timeout` with a mock returning `n = 1` (only the report_id byte written); `from_bytes(&buf)` will see `seq`, `encoder_deltas`, etc. from the `0xFF` fill rather than valid data.

### Suggested Fix

Zero `buf` at the start of each loop iteration before calling `read_timeout`:

```rust
loop {
    buf = [0u8; 64];  // clear stale data from previous iteration
    let n = match device.read_timeout(&mut buf, 5) { ... };
    ...
}
```

Or add a short-read guard after the `n == 0` check:

```rust
if n < 64 {
    eprintln!("[hid] short read ({n} bytes); skipping report");
    continue;
}
```

---

## BUG-007 — [WARNING] `ratatui` default features pull in `crossterm` unconditionally despite stated intent

- **File:** `engine/Cargo.toml`
- **Branch:** `feat/terminal-ui`
- **Discovered:** 2026-05-02 by code-reviewer agent (step8-terminal-ui review)
- **Severity:** warning

### Description

The Cargo.toml comment reads "Only crossterm (the real terminal backend) is gated behind hw-io" but `ratatui = "0.30"` uses ratatui's default feature set, which includes the `crossterm` feature. This causes `ratatui-crossterm v0.1.0` and `crossterm v0.29.0` to appear in the dependency tree even without the `hw-io` feature enabled. The stated goal — keeping crossterm gated — is not achieved.

### Reproduction

```
cd .workflow/worktrees/terminal-ui
cargo tree -p engine | grep crossterm
# Outputs: ratatui-crossterm v0.1.0 and crossterm v0.29.0 even without hw-io
```

### Suggested Fix

Declare ratatui without default features and activate the crossterm feature only via `hw-io`:

```toml
ratatui = { version = "0.30", default-features = false, features = ["all-widgets", "macros", "layout-cache", "underline-color"] }
crossterm = { version = "0.29", optional = true }

[features]
hw-io = ["midir", "hidapi", "crossterm", "ratatui/crossterm"]
```

Verify with `cargo tree -p engine` (no hw-io) that crossterm no longer appears, and `cargo test -p engine` still passes (TestBackend does not need crossterm).

---
