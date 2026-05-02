# Task: HID Report Structs (Shared Definitions)

- **Type**: coder
- **Status**: done
- **Repo**: midi-man-mk3
- **Parallel Group**: 1
- **Feature Branch**: feature/engine-phase1
- **Branch**: feature/engine-phase1/hid-report-structs
- **Base Branch**: feature/engine-phase1
- **Source Item**: midi-man-mk3-mvp.md — Step 6
- **Dependencies**: none

## Description

Define the USB HID report byte layout as Rust structs on both the engine and firmware sides. The structs must be byte-for-byte identical between the two crates (no shared crate to avoid cross-compile complexity at MVP). Each side gets its own copy with a comment pointing to the spec in `midi-man-mk3-mvp.md` Section 4.

Files to create or modify:
- `engine/src/hid.rs` — `InReport`, `OutReport`, `from_bytes`, `to_bytes`, VID/PID constants
- `firmware/src/report.rs` — identical struct definitions for the firmware side

## Acceptance Criteria

- [ ] `InReport` defined as a `repr(C)` struct in `engine/src/hid.rs` matching the 64-byte IN report layout from the plan (Section 4): `report_id`, `seq`, `flags`, `step_buttons: [u8; 2]`, `step_enable_state: [u8; 2]`, `param_buttons: [u8; 2]`, `encoder_deltas: [i8; 16]`, `tempo_delta: i8`, `param_knob_delta: i8`, `reserved: [u8; 37]`.
- [ ] `fn InReport::from_bytes(buf: &[u8; 64]) -> InReport` implemented and correct.
- [ ] `OutReport` defined as a `repr(C)` struct in `engine/src/hid.rs` matching the 64-byte OUT report layout: `report_id`, `seq_echo`, `led_state: [u8; 2]`, `reserved: [u8; 60]`.
- [ ] `fn OutReport::to_bytes(&self) -> [u8; 64]` implemented.
- [ ] VID/PID constants defined in `engine/src/hid.rs`: `const HID_VID: u16 = 0x2E8A;` `const HID_PID: u16 = 0x000A;` (Raspberry Pi HID test device PID per plan assumptions).
- [ ] `firmware/src/report.rs` contains byte-identical struct definitions for `InReport` and `OutReport` with a comment: `// See midi-man-mk3-mvp.md Section 4 — must match engine/src/hid.rs byte-for-byte`.
- [ ] Unit tests in `engine/src/hid.rs`: round-trip encode/decode of a synthetic `InReport` with non-zero values in every field; verify all fields survive the round trip. Verify `from_bytes` of an all-zero buffer produces expected zero-value struct.
- [ ] `cargo test -p engine` passes.
- [ ] No heap allocations in encode/decode paths.

## Interface Contracts

These structs are consumed by Step 7 (`run_hid`) and Step 6b (`hid.rs` translate section):

```rust
// engine/src/hid.rs

pub const HID_VID: u16 = 0x2E8A;
pub const HID_PID: u16 = 0x000A;

#[repr(C)]
pub struct InReport {
    pub report_id: u8,           // always 0x01
    pub seq: u8,                 // wrapping sequence number
    pub flags: u8,               // bit0=encoder_tap pending, bit1=param_tap, bit2=tempo_tap
    pub step_buttons: [u8; 2],   // bytes 3-4: step button edges, one bit per step
    pub step_enable_state: [u8; 2], // bytes 5-6: LED mirror
    pub param_buttons: [u8; 2],  // bytes 7-8: 12 param buttons in low 12 bits
    pub encoder_deltas: [i8; 16],// bytes 9-24: signed delta per encoder
    pub tempo_delta: i8,         // byte 25
    pub param_knob_delta: i8,    // byte 26
    pub reserved: [u8; 37],      // bytes 27-63
}

impl InReport {
    pub fn from_bytes(buf: &[u8; 64]) -> InReport;
}

#[repr(C)]
pub struct OutReport {
    pub report_id: u8,           // always 0x02
    pub seq_echo: u8,            // echo of last InReport.seq
    pub led_state: [u8; 2],      // bytes 2-3: 16 LEDs, bit per step
    pub reserved: [u8; 60],      // bytes 4-63
}

impl OutReport {
    pub fn to_bytes(&self) -> [u8; 64];
}
```

HID report spec (from plan Section 4):
```
IN report (Pico → Engine), 64 bytes:
  Byte  0   : report_id = 0x01
  Byte  1   : sequence number (u8, wraps)
  Byte  2   : flags (bit0=encoder_tap, bit1=param_tap, bit2=tempo_tap, bit3=reserved)
  Bytes 3-4  : step_buttons[15:0] — press edges
  Bytes 5-6  : step_enable_state[15:0] — LED mirror
  Byte  7   : param_buttons low byte
  Byte  8   : param_buttons high nibble
  Bytes 9-24 : encoder_deltas[16] — signed i8
  Byte 25   : tempo_delta — signed i8
  Byte 26   : param_knob_delta — signed i8
  Bytes 27-63: reserved (zero-filled)

OUT report (Engine → Pico), 64 bytes:
  Byte  0   : report_id = 0x02
  Byte  1   : sequence number echo
  Bytes 2-3  : led_state[15:0]
  Bytes 4-63 : reserved
```

## Context

From plan Section 4: the HID protocol is intentionally flat and versioned. The Pico is a dumb peripheral — all musical logic lives in the engine. Report size is 64 bytes in both directions (max HID interrupt packet; no fragmentation). Poll interval is 1 ms.

No shared crate is used to avoid cross-compile complexity. The comment in `firmware/src/report.rs` pointing to the spec is the maintenance contract.

Step 6b will add the `InputCommand` translation layer on top of `InReport` — this task only defines the raw byte structs.

## Notes

Implemented on branch `hid-report-structs` (worktree at `.workflow/worktrees/hid-report-structs`), commit `bf44ab5`.

### What was implemented

- **`engine/src/hid.rs`**: `HID_VID`/`HID_PID` constants, `InReport` and `OutReport` as `repr(C)` 64-byte structs, `InReport::from_bytes(&[u8; 64]) -> InReport`, `OutReport::to_bytes(&self) -> [u8; 64]`. No heap allocation in encode/decode paths.
- **`firmware/src/report.rs`**: byte-identical struct definitions for `InReport` and `OutReport` with comment pointing to spec: `// See midi-man-mk3-mvp.md Section 4 — must match engine/src/hid.rs byte-for-byte`.
- **Cargo workspace stub**: `Cargo.toml`, `engine/Cargo.toml`, `firmware/Cargo.toml` created to allow `cargo test -p engine` to run independently of the parallel workspace-scaffold step.

### Test results

`cargo test -p engine`: **6/6 tests passed**

- `in_report_round_trip_nonzero` — all fields with distinct non-zero values survive encode/decode
- `in_report_all_zeros_produces_zero_struct` — zero buffer produces zero-value struct
- `out_report_to_bytes_round_trip` — all fields correctly packed into 64-byte buffer
- `out_report_to_bytes_is_64_bytes` — output is exactly 64 bytes
- `in_report_from_bytes_is_64_bytes_in` — compile-time signature check
- `hid_vid_pid_constants` — VID=0x2E8A, PID=0x000A

`cargo check -p firmware`: clean compile.
