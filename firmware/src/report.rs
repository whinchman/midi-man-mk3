// See midi-man-mk3-mvp.md Section 4 — must match engine/src/hid.rs byte-for-byte.
//
// IN report (Pico → Engine), 64 bytes:
//   Byte  0   : report_id = 0x01
//   Byte  1   : sequence number (u8, wraps)
//   Byte  2   : flags (bit0=encoder_tap, bit1=param_tap, bit2=tempo_tap, bit3=reserved)
//   Bytes 3-4  : step_buttons[15:0] — press edges
//   Bytes 5-6  : step_enable_state[15:0] — LED mirror
//   Byte  7   : param_buttons low byte
//   Byte  8   : param_buttons high nibble
//   Bytes 9-24 : encoder_deltas[16] — signed i8
//   Byte 25   : tempo_delta — signed i8
//   Byte 26   : param_knob_delta — signed i8
//   Bytes 27-63: reserved (zero-filled)
//
// OUT report (Engine → Pico), 64 bytes:
//   Byte  0   : report_id = 0x02
//   Byte  1   : sequence number echo
//   Bytes 2-3  : led_state[15:0]
//   Bytes 4-63 : reserved

/// IN report: Pico → Engine, 64 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InReport {
    /// Always 0x01.
    pub report_id: u8,
    /// Wrapping sequence number.
    pub seq: u8,
    /// Flags: bit0 = encoder_tap pending, bit1 = param_tap, bit2 = tempo_tap.
    pub flags: u8,
    /// Bytes 3–4: step button press edges, one bit per step (16 steps).
    pub step_buttons: [u8; 2],
    /// Bytes 5–6: LED mirror — current step-enable state.
    pub step_enable_state: [u8; 2],
    /// Bytes 7–8: 12 param buttons packed in low 12 bits across two bytes.
    pub param_buttons: [u8; 2],
    /// Bytes 9–24: signed delta per encoder (16 encoders).
    pub encoder_deltas: [i8; 16],
    /// Byte 25: tempo encoder signed delta.
    pub tempo_delta: i8,
    /// Byte 26: param knob signed delta.
    pub param_knob_delta: i8,
    /// Bytes 27–63: reserved, zero-filled.
    pub reserved: [u8; 37],
}

/// OUT report: Engine → Pico, 64 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutReport {
    /// Always 0x02.
    pub report_id: u8,
    /// Echo of the last received `InReport.seq`.
    pub seq_echo: u8,
    /// Bytes 2–3: 16 step LEDs, one bit per step.
    pub led_state: [u8; 2],
    /// Bytes 4–63: reserved, zero-filled.
    pub reserved: [u8; 60],
}
