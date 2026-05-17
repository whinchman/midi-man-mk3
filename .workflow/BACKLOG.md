# BACKLOG
- plan for "song mode".
  1. pattern saving/loading. 
  2. songs = pattern chaining.
  3. song mode - like a 8bit tracker?
  4. songs, save/load. FileFormat?
  5. switch between pattern/song mode? f9/f10?


## Test hygiene: add `parse_note_name` test for `s` sharp suffix (BUG-019)
- **File:** `engine/src/music_theory.rs` test module.
- The `parse_note_name` implementation matches `b'#' | b's'` for sharps but
  the test suite only exercises `#` (e.g. `F#3`). The `s` variant (e.g. `Fs3`)
  has zero coverage — removing `| b's'` from the match arm passes tests.
- **Fix:** add `assert_eq!(parse_note_name("Fs3"), Some(54));` to the existing
  `#[cfg(test)] mod tests` block.

## Test hygiene: fix pre-existing `clippy --all-targets` errors in engine/tests
- 17 errors across `engine/tests/{clock,hid,main_wiring,ui}.rs`.
- None block the lib build (`cargo clippy -p engine -- -D warnings` is clean);
  surfaced by `cargo clippy -p engine --all-targets -- -D warnings`.
- Goal: get `--all-targets` clean so CI (once added) can run the strict variant.
- Discovered during PR #110 Copilot-fix coder run, 2026-05-13.
