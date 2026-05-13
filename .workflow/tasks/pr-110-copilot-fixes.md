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

### Code review (2026-05-13)

Reviewer: code-reviewer agent. Diff size: +336 / -66 across engine/src/ui.rs
and engine/src/midi_out.rs. Verdict: **approve**.

#### Fix-by-fix verification

- **Fix 1 — 1–16 step indexing.**
  - `handle_cli_note_set` (engine/src/ui.rs:246-263): predicate is
    `Some(s) if (1..=16).contains(&s) => s`, rejecting 0 and >16. Error
    message reads `"step must be 1–16"`. Internal `step = user_step - 1` is
    correctly 0-indexed. Success log uses `format!("note set {user_step} →
    {} vel {velocity}", …)`, preserving the user-facing 1–16 value (e.g.
    `note set 4` logs as `note set 4`, not `note set 3`).
  - `HELP_ENTRIES` updated to `"note set <1-16> <note> [vel]"`.
  - `note_set_step_16_is_valid_boundary` (line 1425) renamed correctly,
    asserts `InputCommand::NoteSet { step: 15, .. }` for input `note set 16
    A4`.
  - `note_set_step_out_of_range_logs_error` (line 1361) asserts both 0 and
    17 are rejected.
  - `note_set_valid_sends_note_set_cmd_and_logs_cmd` (line 1306) updated:
    input `note set 4 C4`, asserts emitted `step: 3` and log text contains
    `"note set 4"`.

- **Fix 2 — trailing-token rejection.**
  - `handle_cli_note_set` (engine/src/ui.rs:315-323): after velocity parse,
    `if parts.next().is_some()` logs `LogTag::Err` with `"note set:
    unexpected trailing input"` and returns without sending. Order is
    correct (after the velocity Some-branch, before the `cmd_tx.send`).
  - Test `note_set_rejects_trailing_tokens` (line 1439) covers input
    `note set 3 C4 64 extra`, asserts `cmd_rx.try_recv().is_err()` (zero
    `InputCommand` sent), single `LogTag::Err` log entry containing
    `"unexpected trailing input"`.

- **Fix 3 — doc-only on `parse_ports_sentinel`.**
  - Doc updated (engine/src/ui.rs:354-355) to the two-bullet form
    specified.
  - Function body is semantically unchanged. The single-line
    `Some(payload.split('\x1f').map(...).collect())` was reformatted to
    multi-line by rustfmt — same parser, same output. Test
    `parse_ports_sentinel_non_sentinel_err_returns_none` is unchanged and
    passes.

- **Fix 4 — stale comment in `run_ui`.**
  - engine/src/ui.rs:637 now reads `(false, "[ports-err] ...")        →
    LogTag::Err via parse_ports_sentinel`. Match is exact.

- **Fix 5 — `ListPorts` blank-entry guard.**
  - engine/src/midi_out.rs:268-280: names mapping uses
    `.enumerate().map(|(idx, p)| match output.port_name(p) { Ok(name) if
    !name.is_empty() => name, _ => format!("port #{idx}") })`, followed by
    a defensive `.filter(|name| !name.is_empty())`. The fallback covers
    both the `Err` arm AND the `Ok(empty)` arm, which is stricter than the
    spec required and an improvement. `idx` is the iteration position in
    `ports()`, which is the user-visible slot number — correct.
  - Spec's note about the second `ListPorts` arm
    (`run_midi_out_with_open_fn` at line 321) is correct: it only sends
    `(true, "[ports]")` with no port iteration, so no fix is needed there.
    The task note already documents this.
  - Two new tests added: `list_ports_payload_uses_indexed_fallback_when_
    port_name_fails` (asserts exact payload `"[ports]Port Zero\x1Fport
    #1\x1Fport #2"` covering Err and empty-Ok cases) and
    `list_ports_payload_contains_no_blank_entries` (asserts no `\x1F\x1F`,
    no leading/trailing `\x1F`). Both go through a `build_ports_payload`
    helper that mirrors the production mapping logic.

- **Fix 6 — `ok` alias documented.**
  - engine/src/ui.rs:55-56: separate entries `("clear", "clear the CLI
    log")` and `("ok", "alias of clear")`. The implementation at line 218
    (`line == "clear" || line == "ok"`) was pre-existing — this fix is
    docs-only as specified. Help test (`cli_submit_help_pushes_one_info_
    per_entry`, line 1247) is keyed off `HELP_ENTRIES.len()` so it
    auto-tracks.

#### Other observations

- No `unwrap()` introduced in non-test code. The only new `.expect()` is
  in the test helper `list_ports_payload_contains_no_blank_entries`
  (engine/src/midi_out.rs:682) — acceptable.
- No new heap allocations on the MIDI/clock hot path. `ListPorts` is a
  one-shot CLI query; its allocations are bounded by port count.
- No TODO/FIXME/XXX/HACK markers introduced.
- The vast majority of `+` lines in the diff (≈two-thirds) are pure
  rustfmt line-wraps of pre-existing one-line `send`/`assert_eq!`/etc.
  calls. These are noise but not regressions; they make the diff larger
  than the spec strictly required, but every semantic change matches the
  spec.
- Pre-existing minor inconsistency (not introduced by this PR): the
  producer joins with `\x1F` (uppercase hex) and the parser splits on
  `\x1f` (lowercase hex). These are byte-equivalent (both `0x1F`) so it
  is harmless, just stylistically inconsistent. Not worth a follow-up.
- `cargo test -p engine` confirmed locally: all suites pass (the run
  shown in the task's prior notes — 635 passed, 0 failed — was
  reproduced).

#### Findings

No findings — approve.

#### Summary

- 0 critical, 0 warning, 0 info findings
- Branch: feature/cli-commands-pr-110-copilot-fixes
- Diff size: +336 -66 (≈two-thirds is incidental rustfmt reformatting)

### QA pass (2026-05-13)

Reviewer: qa agent. Reviewed coder commit `cb668d5` against all six
acceptance criteria. All existing tests green. Filled five small coverage
gaps where the existing tests were too loose to catch plausible
regressions; no new bugs found in production code.

**Verdict: qa-pass.**

#### Coverage gaps found (and filled)

- **Fix 1 (lower boundary)**: step=1 was exercised only incidentally by
  `note_set_with_velocity_uses_provided_velocity`. Added explicit named
  test `note_set_step_1_is_valid_boundary` that pins both the
  user→internal mapping and the user-facing log text.
- **Fix 1 (upper-boundary log text)**: `note_set_step_16_is_valid_boundary`
  only checked the emitted `InputCommand::NoteSet { step: 15, .. }` and
  said nothing about the log text. A regression where the success log
  leaked the internal 0-indexed step at the upper edge would slip
  through. Added `note_set_step_16_log_displays_user_facing_step` to
  assert the log says "note set 16", not "note set 15".
- **Fix 2 (exact message wording)**: `note_set_rejects_trailing_tokens`
  only matched `.contains("unexpected trailing input")`. The spec
  mandates the full message "note set: unexpected trailing input". Added
  `note_set_trailing_tokens_uses_exact_spec_message` with an `assert_eq!`
  for the full text.
- **Fix 6 (ok alias presence)**: `cli_submit_help_pushes_one_info_per_
  entry` keys off `HELP_ENTRIES.len()`, which would pass even if the
  `ok` entry were swapped for an unrelated one. Added
  `help_entries_includes_ok_alias` (constant-level) and
  `cli_submit_help_output_includes_ok_alias_line` (renders the help
  output and looks for the actual `ok` line) to lock in the alias.

Fix 3, Fix 4, and Fix 5 were verified to have adequate test coverage
already; no additions needed. (Fix 5's `build_ports_payload` helper is
not strictly a mutation-test of the production code path, but the spec
explicitly accepts the "no blank entries" approach, and the
implementation is short enough that the helper-mirror is the best
practical option without spinning up a fake `midir::MidiOutput`.)

#### Tests added on this branch (qa commit)

- `engine/src/ui.rs::tests::note_set_step_1_is_valid_boundary`
- `engine/src/ui.rs::tests::note_set_step_16_log_displays_user_facing_step`
- `engine/src/ui.rs::tests::note_set_trailing_tokens_uses_exact_spec_message`
- `engine/src/ui.rs::tests::help_entries_includes_ok_alias`
- `engine/src/ui.rs::tests::cli_submit_help_output_includes_ok_alias_line`

QA commit: `38a69fc` — `test(engine): add QA coverage for PR #110
Copilot fixes`. Only `engine/src/ui.rs` was modified (test module only).
No application code touched.

#### Final test counts

- Before QA: 635 passed, 0 failed.
- After QA: **640 passed, 0 failed** (+5 new tests).
- `cargo clippy -p engine -- -D warnings`: clean.
- `rustfmt --check --edition 2021 engine/src/{ui,midi_out}.rs`: clean.

#### Bugs found

None.
