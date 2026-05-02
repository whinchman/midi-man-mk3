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
