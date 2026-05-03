# Test Plan: fix-cargo-firmware-debug

**Task:** fix-cargo-firmware-debug
**Branch:** task/fix-cargo-firmware-debug

## What was changed

`Cargo.toml` (workspace root) gained a new table:

```toml
[profile.release.package.firmware]
debug = false
```

This strips DWARF symbols from firmware release builds while engine release
builds retain `debug = 2`.

## Acceptance criteria to verify

1. `[profile.release.package.firmware]` with `debug = false` is present in the
   workspace `Cargo.toml`.
2. No `[profile.release.package.engine]` section exists — engine inherits
   `debug = 2` from the workspace release profile.
3. `cargo test -p engine` passes.

## Test strategy

Since the fix is a pure Cargo.toml manifest change there are no new Rust
symbols to exercise. The most direct unit-testable verification is to parse the
workspace `Cargo.toml` file at test time and assert on the expected keys.

### Test cases

#### cargo_profile.rs (new integration test file)

| # | Test name | Scenario |
|---|-----------|----------|
| 1 | `workspace_cargo_toml_exists` | The workspace `Cargo.toml` file can be found relative to `CARGO_MANIFEST_DIR`. |
| 2 | `firmware_release_profile_has_debug_false` | The file contains the literal line `debug = false` inside a `[profile.release.package.firmware]` section. |
| 3 | `engine_has_no_release_package_override` | No `[profile.release.package.engine]` table exists; engine inherits workspace defaults. |
| 4 | `workspace_release_profile_debug_is_2` | The workspace-level `[profile.release]` still has `debug = 2` (engine profiling preserved). |
| 5 | `firmware_override_does_not_affect_engine_profile` | Confirm that `[profile.release.package.engine]` is absent so engine retains `debug = 2`. |

### Edge cases

- File read failures produce clear assertion messages (not panic on unwrap).
- TOML is line-based; tests use string search rather than a full TOML parser to
  avoid adding a dev-dependency.
