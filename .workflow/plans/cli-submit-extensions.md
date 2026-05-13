# Plan: cli-submit-extensions

## Overview

Extend `handle_cli_submit` in `engine/src/ui.rs` with all new CLI command branches, add `HELP_ENTRIES` constant and `handle_cli_note_set` helper, and update `run_ui` to handle the `[ports]` sentinel from the MIDI thread.

## Steps

### Step 1: Add HELP_ENTRIES const

Add `HELP_ENTRIES: &[(&str, &str)]` near top of `ui.rs` listing all 10 commands with short descriptions.

### Step 2: Add new branches in handle_cli_submit

Before the final `else` (unknown command) branch, add:
- `"rand all"` → `InputCommand::RandAll`, `LogTag::Cmd`
- `"rand velo"` → `InputCommand::RandVelocities`, `LogTag::Cmd`
- `"rand notes"` → `InputCommand::GenerateRandomSequence`, `LogTag::Cmd`
- `"note set ..."` → call `handle_cli_note_set`
- `"port list"` → `MidiCtrlMsg::ListPorts`, `LogTag::Cmd` "port list (querying...)"
- `"clear"` | `"ok"` → `ui.cli_log.clear()` (no log after)
- `"help"` → iterate `HELP_ENTRIES`, push `LogTag::Info` per entry

### Step 3: Add handle_cli_note_set free function

Signature: `fn handle_cli_note_set(ui: &mut UiState, cmd_tx: &SyncSender<InputCommand>, ts: u64, rest: &str)`
- Parse step (0–15), note name via `parse_note_name`, optional velocity (0–127, default 127)
- `LogTag::Err` on any parse failure
- `LogTag::Cmd` with resolved note name on success

### Step 4: Update run_ui midi_log_rx drain for [ports] sentinel

In the `while let Ok((ok, msg)) = midi_log_rx.try_recv()` block, detect `[ports]` prefix before falling through to existing handling:
- `(true, "[ports]name1\x1fname2")` → push one `LogTag::Info` per name
- `(true, "[ports]")` → push "no MIDI ports available" `LogTag::Info`
- `(false, "[ports-err] ...")` → push `LogTag::Err`
- Other messages: existing handling

### Step 5: Write unit tests and verify

Write tests for each new branch and run `cargo test -p engine`.
