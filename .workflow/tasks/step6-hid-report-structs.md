# Task: HID Report Structs (Shared Definitions)

- **Type**: coder
- **Status**: done
- **Review Status**: approved
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

`cargo test -p engine`: **18/18 tests passed** (12 new tests added by QA pass, 6 original)

Original 6 tests:
- `in_report_round_trip_nonzero` — all fields with distinct non-zero values survive encode/decode
- `in_report_all_zeros_produces_zero_struct` — zero buffer produces zero-value struct
- `out_report_to_bytes_round_trip` — all fields correctly packed into 64-byte buffer
- `out_report_to_bytes_is_64_bytes` — output is exactly 64 bytes
- `in_report_from_bytes_is_64_bytes_in` — compile-time signature check
- `hid_vid_pid_constants` — VID=0x2E8A, PID=0x000A

12 new tests added by QA review:
- `in_report_size_is_64_bytes` — `std::mem::size_of::<InReport>() == 64`
- `out_report_size_is_64_bytes` — `std::mem::size_of::<OutReport>() == 64`
- `in_report_field_offsets_match_wire_spec` — each field sits at its spec-defined byte offset via transmute inspection
- `out_report_field_offsets_match_wire_spec` — each field sits at its spec-defined byte offset via to_bytes inspection
- `from_bytes_boundary_u8_max_in_all_u8_fields` — u8::MAX (0xFF) in all u8 fields survives round-trip
- `from_bytes_i8_min_sign_extends_correctly` — 0x80 byte correctly sign-extends to i8::MIN (-128) for all signed fields
- `from_bytes_i8_max_sign_extends_correctly` — 0x7F byte correctly sign-extends to i8::MAX (127) for all signed fields
- `from_bytes_zero_signed_fields_are_zero` — 0x00 bytes produce 0i8 in all signed fields
- `from_bytes_each_encoder_delta_index_independently` — each of the 16 encoder_deltas slots maps to exactly byte 9+idx
- `out_report_to_bytes_all_led_bits_set` — led_state [0xFF, 0xFF] encodes correctly at bytes 2-3
- `out_report_to_bytes_no_led_bits_set` — led_state [0x00, 0x00] encodes correctly at bytes 2-3
- `out_report_to_bytes_alternating_led_bits` — alternating bit patterns are preserved without swapping

`cargo check -p firmware`: clean compile.

### Code Review — 2026-05-02

**Reviewer:** code-reviewer agent
**Verdict:** APPROVE

**Summary:** 0 critical, 0 warning, 1 info finding.

#### Byte Layout Verification

Field offsets computed by instrumented Rust program confirm exact match to the HID spec in Section 4:

| Field | Spec bytes | Actual offset | Size |
|---|---|---|---|
| `report_id` | 0 | 0 | 1 |
| `seq` | 1 | 1 | 1 |
| `flags` | 2 | 2 | 1 |
| `step_buttons` | 3–4 | 3 | 2 |
| `step_enable_state` | 5–6 | 5 | 2 |
| `param_buttons` | 7–8 | 7 | 2 |
| `encoder_deltas` | 9–24 | 9 | 16 |
| `tempo_delta` | 25 | 25 | 1 |
| `param_knob_delta` | 26 | 26 | 1 |
| `reserved` | 27–63 | 27 | 37 |

`sizeof(InReport)` = 64. `sizeof(OutReport)` = 64. No padding inserted by compiler.

`OutReport` fields verified: `report_id`@0, `seq_echo`@1, `led_state`@2, `reserved`@4.

#### `repr(C)` Correctness

`#[repr(C)]` is applied on both structs on both sides. All field types are primitive scalars or arrays of primitives with no internal padding risk. The layout is guaranteed stable across platforms.

#### `from_bytes` / `to_bytes` Safety

- `InReport::from_bytes` correctly indexes every field at its spec-defined offset. Signed bytes are cast via `as i8` (correct two's-complement reinterpretation). No unsafe code, no heap allocation.
- `OutReport::to_bytes` correctly packs all four fields at spec offsets; `reserved` is copied from the struct field via `copy_from_slice`. Stack-only allocation.
- Round-trip tests verify all fields survive encode/decode with non-zero values in every position.

#### Firmware Struct Identity

`firmware/src/report.rs` struct definitions are field-for-field, type-for-type, order-for-order identical to `engine/src/hid.rs`. The spec comment is present as required.

#### VID/PID Constants

`HID_VID = 0x2E8A`, `HID_PID = 0x000A` match the plan assumption (Raspberry Pi HID test device PID).

#### Tests

All 6 tests pass. Coverage hits every field of `InReport` with non-zero values (round-trip), zero-value construction, `OutReport` encode, size assertion, signature compile check, and constant values.

#### [INFO] engine/src/hid.rs — No `OutReport::from_bytes` on firmware side

`firmware/src/report.rs` defines `OutReport` as a struct but has no `from_bytes` method for deserializing the host-sent OUT report into the struct. This is not required by this task's acceptance criteria (Step 13 will add the firmware USB HID task). However, when Step 13 is implemented, a `from_bytes` counterpart should be added to `firmware/src/report.rs` to maintain symmetry and avoid unsafe pointer casts in the USB handler.
No action needed now — flag for Step 13 implementer.

## PR Feedback

PR: https://github.com/whinchman/midi-man-mk3/pull/1

### Comments Requiring Action

(none)

### CI Failures

(none — no CI checks configured on this repository)

### Questions / Acknowledged

(none)
