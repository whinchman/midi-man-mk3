# Test Plan: fix-cargo-config-tmp-paths

## What is being tested

BUG-003: Hardcoded `/tmp` ALSA paths removed from `.cargo/config.toml`.

The change has three observable, testable properties that can be verified by
reading files at test time:

1. `.cargo/config.toml` does NOT contain the string `/tmp/alsa-pkg` or
   `/tmp/alsa-lib` as live configuration values (i.e., not inside a comment).
2. `.gitignore` contains a line `.cargo/config.local.toml`.
3. `.cargo/config.toml` contains a comment documenting the local-override
   pattern (the gitignored `.cargo/config.local.toml` workaround).

These are file-content assertions, not logic assertions. They belong in the
`engine/tests/` directory under a new `cargo_config.rs` integration test file.

## Test cases

| ID  | Name | Scenario | Expected |
|-----|------|----------|----------|
| T1  | `cargo_config_no_live_tmp_alsa_pkg` | Read `.cargo/config.toml`, scan non-comment lines | No non-comment line contains `/tmp/alsa-pkg` |
| T2  | `cargo_config_no_live_tmp_alsa_lib` | Read `.cargo/config.toml`, scan non-comment lines | No non-comment line contains `/tmp/alsa-lib` |
| T3  | `gitignore_contains_config_local_toml` | Read `.gitignore` | File contains `.cargo/config.local.toml` |
| T4  | `cargo_config_documents_local_override_pattern` | Read `.cargo/config.toml` | File comment mentions `.cargo/config.local.toml` |

## Edge cases / boundary conditions

- Lines beginning with `#` are comment lines; they may contain `/tmp` strings
  as documentation. Only non-comment lines must be free of `/tmp` paths.
- Blank lines and `[section]` headers must not trigger false positives.

## Test data

No fixtures needed. Tests read the real project files via the `CARGO_MANIFEST_DIR`
environment variable, which Cargo sets to the workspace/crate root at test time.

## Notes

All tests are pure file-reads. No mocking needed — no external connections
are touched. Tests are deterministic and order-independent.
