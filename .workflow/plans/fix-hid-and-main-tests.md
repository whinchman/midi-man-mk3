# Test Plan: fix-hid-and-main

## Goal

Write comprehensive unit tests verifying the four bug fixes from `fix-hid-and-main`:
BUG-005 (unsafe transmute), BUG-006 (stale buffer bytes), BUG-008 (CLI args not
forwarded), and BUG-009 (clock thread not joined).

## Coverage Areas

### 1. offset_of! field assertions (BUG-005)

Already implemented in `engine/tests/hid.rs`:
- `in_report_field_offsets_match_wire_spec` — all 10 InReport fields
- `out_report_field_offsets_match_wire_spec` — OutReport via to_bytes round-trip

New: add `out_report_offset_of_fields_match_wire_spec` using `std::mem::offset_of!`
directly on OutReport fields to match the same pattern used for InReport.

### 2. Buffer zeroing behavior (BUG-006)

`run_hid` zeroes `buf = [0u8; 64]` at the top of each loop iteration.
The fix cannot be directly tested via unit tests (requires `hw-io` feature and
real device). Instead, test the consequence:
- A second call to `InReport::from_bytes` with a clean buffer produces zeroed fields
  even when the previous buffer had non-zero values — i.e., the decode function is
  stateless and produces correct output from a freshly-zeroed buffer.
- `in_report_from_zeroed_buf_after_nonzero_produces_zero_struct` — re-zeroes buffer
  between two decode calls, confirms no cross-contamination.

Document the direct code path (`buf = [0u8; 64]` at line 323 of hid.rs) with a
source-inspection test that verifies `from_bytes` on a zero buffer always returns
the zero struct (confirming the contract the fix relies on).

### 3. CLI arg forwarding (BUG-008)

Already implemented in `engine/tests/main_wiring.rs`:
- `cli_defaults_when_no_args`
- `cli_midi_port_is_set`
- `cli_hid_vid_is_set`
- `cli_hid_pid_is_set`
- `cli_malformed_hid_vid_does_not_panic`
- `cli_all_flags_together`

New tests:
- `cli_hid_vid_defaults_to_none_when_absent` — verify absent flag leaves None
- `cli_hid_pid_defaults_to_none_when_absent` — verify absent flag leaves None
- `cli_hid_vid_decimal_value_rejected` — plain decimal (no 0x prefix) for known-bad string
- `cli_hid_pid_uppercase_0X_prefix_accepted` — 0X prefix variant
- `cli_unknown_args_ignored` — unknown flags do not panic; known fields remain None
- `cli_hid_vid_value_zero_accepted` — 0x0000 is valid
- `cli_hid_pid_max_value_accepted` — 0xFFFF is valid u16 boundary

### 4. Thread join ordering (BUG-009)

Already covered by:
- `cmd_processor_exits_when_sender_dropped`

New tests (testing the cmd-processor and channel wiring, not the OS thread scheduler):
- `cmd_processor_thread_joins_cleanly_after_n_commands` — send N commands, drop sender,
  join returns without blocking; verifies all N commands were applied
- `cmd_tx_drop_before_join_allows_clean_exit` — explicit coverage: drop cmd_tx then join
- `midi_channel_drop_exits_run_midi_out_with_sender` — drop midi_tx, verify loop exits
  (tests the midi_thread join pre-condition)
- `multiple_cmd_tx_clones_all_must_drop_before_thread_exits` — clone cmd_tx twice, drop
  original and one clone, verify thread still running; drop last clone, verify join succeeds
  (covers the hid_cmd_tx + original cmd_tx pattern from BUG-009 fix)

## Test Data

All tests use in-process channels and mock senders. No real hardware, no filesystem,
no network. Tests are deterministic.

## Branch

`fix/known-bugs/fix-hid-and-main` off `task/fix-hid-and-main`
