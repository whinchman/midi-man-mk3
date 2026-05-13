# BACKLOG

## Test hygiene: fix pre-existing `clippy --all-targets` errors in engine/tests
- 17 errors across `engine/tests/{clock,hid,main_wiring,ui}.rs`.
- None block the lib build (`cargo clippy -p engine -- -D warnings` is clean);
  surfaced by `cargo clippy -p engine --all-targets -- -D warnings`.
- Goal: get `--all-targets` clean so CI (once added) can run the strict variant.
- Discovered during PR #110 Copilot-fix coder run, 2026-05-13.
