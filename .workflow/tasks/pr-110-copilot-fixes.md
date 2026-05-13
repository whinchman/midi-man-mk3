# Task: Address 6 Copilot review comments on PR #110

- **Type**: coder
- **Status**: done
- **Repo**: .
- **Parallel Group**: 1
- **Feature Branch**: feature/cli-commands
- **Branch**: feature/cli-commands/pr-110-copilot-fixes
- **Base Branch**: feature/cli-commands
- **Source Item**: PR #110 Copilot review (https://github.com/whinchman/midi-man-mk3/pull/110)
- **Dependencies**: none

## Description

Address all 6 Copilot inline review comments on PR #110. Five touch
`engine/src/ui.rs`, one touches `engine/src/midi_out.rs`. Bundle them in a
single sub-branch off `feature/cli-commands` and open a task PR back into
`feature/cli-commands`.

## Acceptance Criteria

### Fix 1 — `note set` step indexing must be 1–16 (user-facing)
- [ ] `engine/src/ui.rs` — `handle_cli_note_set` (around line 232): change step
  predicate from `Some(s) if s <= 15 => s` to accept **only 1–16** and store
  `s - 1` internally. Reject `0` and `> 16`.
- [ ] Error message updated from `"step must be 0–15"` to `"step must be 1–16"`.
- [ ] The success log message at line ~280 must display the **original
  user-facing 1–16 step** (so `note set 4 C4` logs as `note set 4 → C4 vel 127`,
  not `note set 3 → C4 vel 127`).
- [ ] `HELP_ENTRIES` entry at line 51 updated to read
  `("note set <1-16> <note> [vel]", "set a step's note and velocity")`.
- [ ] Update existing tests:
  - `note_set_valid_sends_note_set_cmd_and_logs_cmd` (line ~1197) — pass step
    1–16 inputs; assert the emitted `InputCommand::NoteSet { step: <user-1> }`.
  - `note_set_step_out_of_range_logs_error` (line ~1230) — assert `0` and `17`
    both error.
  - `note_set_step_15_is_valid_boundary` (line ~1282) — rename to
    `note_set_step_16_is_valid_boundary`, assert step 16 maps to internal 15.

### Fix 2 — reject trailing tokens in `note set`
- [ ] After parsing `velocity` (around line 273), check
  `parts.next().is_some()`. If trailing tokens remain, log
  `LogTag::Err` with message `"note set: unexpected trailing input"` and return
  without sending the command.
- [ ] Add unit test
  `note_set_rejects_trailing_tokens`: `note set 3 C4 64 extra` produces a single
  `LogTag::Err` log entry and no `InputCommand` is sent.

### Fix 3 — fix doc on `parse_ports_sentinel` (doc only, do NOT change impl)
- [ ] `engine/src/ui.rs` lines 286–296 — update the doc bullet:
  ```
  /// - `(false, "[ports-err] <msg>")` — single `LogTag::Err` with the full message.
  /// - Any other `(false, _)` falls through (`None`); caller handles it as a generic error.
  ```
- [ ] Do not modify the function body. The existing test
  `parse_ports_sentinel_non_sentinel_err_returns_none` (line 1189) must still
  pass unchanged.

### Fix 4 — update stale comment in `run_ui`
- [ ] `engine/src/ui.rs` line 573 — replace the comment line
  `(false, "[ports-err] ...")        → LogTag::Err (falls through to default)`
  with
  `(false, "[ports-err] ...")        → LogTag::Err via parse_ports_sentinel`.
- [ ] The two `(true, ...)` comment lines above are correct, leave them.

### Fix 5 — `ListPorts` must not emit blank port-name entries
- [ ] `engine/src/midi_out.rs` lines 249–254 — change the names mapping. When
  `output.port_name(p)` returns `Err`, fall back to `format!("port #{idx}")`
  using the port's index so the entry is never blank.
- [ ] Filter out any names that are still empty after mapping (defence in depth).
- [ ] If there is a second identical `ListPorts` handler at the testable path
  (search for `MidiCtrlMsg::ListPorts` in this file — there are two arms), apply
  the same change there.
- [ ] Add a unit test in `midi_out.rs` covering: when `port_name` fails for an
  entry, the sentinel payload contains `port #<idx>` not an empty slot.
  (If the real `port_name` cannot be made to fail in a unit test, instead add
  a test that asserts the payload, when joined, contains no `"\x1F\x1F"` and
  no leading/trailing `\x1F`.)

### Fix 6 — add `ok` alias to `HELP_ENTRIES`
- [ ] `engine/src/ui.rs` line 52 — change the `clear` entry to two entries
  or one combined entry, e.g.:
  ```
  ("clear",            "clear the CLI log"),
  ("ok",               "alias of clear"),
  ```
- [ ] Update the `help` test that asserts `ui.cli_log.len() == HELP_ENTRIES.len()`
  (line ~1144). It is keyed by the constant length, so it will pass automatically,
  but verify visually that the test still passes after the change.

### Global criteria
- [ ] `cargo test -p engine` passes (all 200+ existing tests plus the new ones).
- [ ] `cargo clippy -p engine -- -D warnings` passes.
- [ ] `cargo fmt --check` passes.
- [ ] No `unwrap()` introduced in non-test code.
- [ ] No heap allocations added on the MIDI/clock hot path. The only newly
  allocating path is `ListPorts`, which already allocates per call — fine.

## Context

PR #110: https://github.com/whinchman/midi-man-mk3/pull/110

Design decisions confirmed by the user before this task was written:
- Step indexing: **strict 1–16**, no leniency, no dual acceptance.
- `ok` alias: **keep it**, document it in `HELP_ENTRIES`.

Existing test sites you will be updating live in `engine/src/ui.rs` near:
- HELP_ENTRIES tests around line 1144
- `parse_ports_sentinel` tests at lines 1153–1192
- `handle_cli_note_set` tests at lines 1197–1290

## Notes

### Implementation summary (2026-05-13)

- Branch: `feature/cli-commands-pr-110-copilot-fixes` (note: had to drop the
  `/pr-110-copilot-fixes` segment from the planned name because git cannot
  create a sub-ref under an existing branch named `feature/cli-commands`).
- Commit: `cb668d5` — `fix(engine): address PR #110 Copilot review comments`.
- Files changed: `engine/src/ui.rs`, `engine/src/midi_out.rs`.
- All 6 Copilot fixes applied per the acceptance criteria above. The two new
  unit tests required by the spec are present:
  - `note_set_rejects_trailing_tokens`
  - `list_ports_payload_uses_indexed_fallback_when_port_name_fails` plus a
    companion `list_ports_payload_contains_no_blank_entries` test (the spec
    accepted either approach; both are included).
- Note on Fix 5 second arm: the testable `run_midi_out_with_open_fn`
  ListPorts arm just sends `(true, "[ports]")` and never iterates ports, so
  there is no blank-name path to fix there — only the real `run_midi_out`
  arm was updated.

### Test results

- `cargo test -p engine`: 635 passed, 0 failed.
- `cargo clippy -p engine -- -D warnings`: clean.
- `rustfmt --check --edition 2021 engine/src/{ui,midi_out}.rs`: clean.

### Last 5 lines of `cargo test -p engine`

```
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

   Doc-tests engine

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
