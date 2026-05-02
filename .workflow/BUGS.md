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
