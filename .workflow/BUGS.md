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

## BUG-008 — [WARNING] CLI args `--midi-port`, `--hid-vid`, `--hid-pid` are parsed but never forwarded to thread functions

- **File:** `engine/src/main.rs`, lines 75–83, 102–127
- **Branch:** `engine-main-wiring`
- **Discovered:** 2026-05-02 by code-reviewer agent (step9-engine-main-wiring review)
- **Severity:** warning

### Description

`parse_args()` returns a `CliArgs` struct with `midi_port`, `hid_vid`, and `hid_pid`. In `main()`, the values are logged (lines 75–83) but never passed to the thread entry points. `run_midi_out` (hw-io) calls the private `open_first_port()` which always selects `ports[0]` regardless of the `--midi-port` filter. `run_hid` opens the device using the hardcoded `HID_VID`/`HID_PID` constants regardless of `--hid-vid`/`--hid-pid` overrides.

The acceptance criteria explicitly states: "`--midi-port <name>`, `--hid-vid <hex>`, `--hid-pid <hex>`, all optional with defaults (first available MIDI port, `HID_VID`/`HID_PID` constants from `hid.rs`)." These args must be respected — they are not respected today.

### Reproduction

```
cargo run -p engine --features hw-io -- --midi-port "some-port" --hid-vid 0x1234 --hid-pid 0x5678
```

The log shows the overrides are parsed, but the MIDI output thread still opens the first available port and the HID thread attempts to open device 0x2E8A:0x000A.

### Suggested Fix

Add a `port_filter: Option<String>` parameter to `run_midi_out` (or provide a separate `open_port_by_name` helper) and pass `args.midi_port`. Add `vid: u16, pid: u16` parameters to `run_hid`, defaulting to `HID_VID`/`HID_PID` when `None`:

```rust
// midi_out.rs
pub fn run_midi_out(rx: Receiver<MidiEvent>, port_filter: Option<&str>) { ... }

// hid.rs
pub fn run_hid(cmd_tx: ..., state: ..., ui_notify: ..., vid: u16, pid: u16) { ... }

// main.rs — spawn with args:
engine::midi_out::run_midi_out(rx, args.midi_port.as_deref())
engine::hid::run_hid(hid_cmd_tx, hid_state, hid_notify,
    args.hid_vid.unwrap_or(engine::hid::HID_VID),
    args.hid_pid.unwrap_or(engine::hid::HID_PID))
```

---

## BUG-009 — [WARNING] Clock thread never exits in non-hw-io builds; not joined on shutdown

- **File:** `engine/src/main.rs`, lines 111–116, 180–186
- **Branch:** `engine-main-wiring`
- **Discovered:** 2026-05-02 by code-reviewer agent (step9-engine-main-wiring review)
- **Severity:** warning

### Description

In non-hw-io builds, `midi_rx` is immediately dropped (line 108). The clock thread is spawned unconditionally (lines 111–116). The clock loop only breaks when `midi_tx.send(event).is_err()`, which requires `playing == true` and a step to be enabled. Since `SequencerState::default()` initialises `playing = false`, the send path is never reached, so the clock thread runs forever in its sleep loop.

Main then drops `midi_tx` (line 185) and returns. The process exits, killing the clock thread via OS teardown. While not a hang (the process terminates), the MidiEvent::Stop sent on line 182 is dispatched onto `midi_tx` after `midi_rx` is already dropped — the send returns `Err` silently (the `let _ =` suppresses it), meaning the Stop event is never processed even if midi_out were running.

In hw-io builds the clock thread is also never joined: `_clock_thread` is dropped at end of `main` without an explicit `join()`. If `midi_out` processes the Stop event and exits before the clock thread unblocks from its sleep, the clock may send another NoteOn to a closed channel after main has returned.

### Reproduction

1. Build without hw-io: the clock thread leaks as an unjoined background thread.
2. Add `s.playing = true` to `SequencerState::default()` — now the clock exits immediately when midi_rx is dropped.

### Suggested Fix

Add a shutdown channel (a `SyncSender<()>` / `Receiver<()>`) to the clock so main can signal it to stop independently of the midi_tx state. Alternatively, for the non-hw-io path, do not spawn the clock thread at all. At minimum, join `_clock_thread`, `_cmd_thread`, `_midi_thread` explicitly after dropping the senders so that the Stop event is guaranteed to be flushed before the process exits:

```rust
let _ = midi_tx.send(MidiEvent::Stop);
drop(midi_tx);
drop(cmd_tx);
// Now join in reverse dependency order so threads drain their queues.
let _ = _cmd_thread.join();
let _ = _clock_thread.join();
let _ = _midi_thread.join();
```

---

## BUG-010 — [WARNING] `NoteDelta` accumulates only ±1 from committed note; repeated Up/Down is a no-op

- **File:** `engine/src/state.rs` (NoteDelta arm), `engine/src/music_theory.rs` (`next_note`)
- **Branch:** main
- **Discovered:** 2026-05-02 by user report
- **Severity:** warning

### Description

`InputCommand::NoteDelta(d)` always reads `self.steps[step].midi_note` (the last *committed* value) as the base for `next_note`. The result is stored in `PendingEdit::Note`, but the pending value is never fed back as the base for the *next* delta. So the second Up/Down keypress overwrites the first pending edit with the same result — the note effectively sticks at ±1 from the committed value until Enter is pressed.

### Reproduction

1. Select a step. Press Up five times without pressing Enter.
2. The note preview jumps by one degree on the first press and stays there on presses 2–5.
3. Pressing Enter commits only ±1 from the original note.

### Suggested Fix

In the `NoteDelta` arm, use the current pending note value (if it exists for the selected step) as the base instead of the committed value:

```rust
InputCommand::NoteDelta(d) => {
    let step = self.selected_step;
    let base_note = match self.pending_edit {
        PendingEdit::Note { step: ps, midi_note } if ps == step => midi_note,
        _ => self.steps[step].midi_note,
    };
    let new_note = crate::music_theory::next_note(base_note, self.key, self.mode, d);
    self.pending_edit = PendingEdit::Note { step, midi_note: new_note };
}
```

---

## BUG-011 — [WARNING] Regular Overlay shows raw numeric delta instead of human-readable value label

- **File:** `engine/src/ui_render.rs` (`render_overlay`, `param_value_string`), `engine/src/state.rs` (`ParamValueDelta` arm)
- **Branch:** main
- **Discovered:** 2026-05-02 by user report
- **Severity:** warning

### Description

When a param is selected in the Regular Overlay and Up/Down is pressed, `ParamValueDelta` stores `PendingEdit::Param { index, value: current_value + d, .. }` where `current_value` starts at 0 (not the current state value). The render code then displays `format!(" {}[{}→{}] ", name, value_str, pv)` where `pv` is that raw integer (`0`, `1`, `-1`, etc.) — showing e.g. `[key:C->1]` instead of `[key:C->D]`.

### Suggested Fix

`ParamValueDelta` should seed `current_value` from the actual committed state value (converted to the same integer space used by `PendingEdit::Param`), not from 0. The pending value stored in `PendingEdit::Param` should represent the fully-resolved new value (same units as the committed field), and `param_value_string` should format it the same way it formats the committed value.

---

## BUG-012 — [WARNING] Regular Overlay `Confirm` discards param edit; state fields are never updated

- **File:** `engine/src/state.rs` (Confirm arm for `PendingEdit::Param`)
- **Branch:** main
- **Discovered:** 2026-05-02 by user report
- **Severity:** warning

### Description

The `Confirm` handler for `PendingEdit::Param` contains only:
```rust
PendingEdit::Param { .. } => {
    // Param commits are handled by Step 7 (param overlay logic).
    self.pending_edit = PendingEdit::None;
}
```
The comment references a "Step 7" that was never implemented. Pressing Enter in the overlay clears the pending edit without writing to any state field (`self.key`, `self.swing`, `self.step_size`, etc.), so every overlay edit is silently discarded.

### Suggested Fix

Implement the `PendingEdit::Param` commit arm to dispatch to the correct field based on `index`. The exact form depends on whether enum helpers (`Key::from_index`, etc.) exist — add them if not. At minimum: index 0 → `self.key`, 1 → `self.mode`, 2 → `self.swing`, 3 → `self.step_size`, with appropriate clamping/wrapping.

---

## BUG-013 — [WARNING] `.cargo/config.toml` comment references non-existent `CARGO_CONFIG_TOML` env var

- **File:** `.cargo/config.toml` (lines 7–8 on branch `fix/known-bugs`, commit 75b7cdd)
- **Branch:** `fix/known-bugs`
- **Discovered:** 2026-05-02 by code-reviewer agent (fix-cargo-config-tmp-paths review)
- **Severity:** warning

### Description

The comment added in commit 75b7cdd tells developers they can use the `.cargo/config.local.toml` override file "via `CARGO_CONFIG_TOML`". No such environment variable exists in Cargo (verified against Cargo 1.93.1). A developer following the instructions will be unable to find any documentation or support for this env var, and may conclude the override mechanism is broken or unavailable.

### Reproduction

1. Read the comment in `.cargo/config.toml` on `fix/known-bugs` (line 7–8).
2. Search Cargo documentation for `CARGO_CONFIG_TOML` — not found.
3. Try `CARGO_CONFIG_TOML=.cargo/config.local.toml cargo build` — env var is silently ignored.

### Suggested Fix

Replace the `CARGO_CONFIG_TOML` reference with the actual supported mechanism — the `--config` flag:

```
# To activate without editing this file, pass --config on the command line:
#   cargo build --config .cargo/config.local.toml
# Or export the overrides directly in your shell before building:
#   export PKG_CONFIG_PATH=/tmp/alsa-pkg
#   export RUSTFLAGS="-L /tmp/alsa-lib"
```

---
