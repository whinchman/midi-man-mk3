// See midi-man-mk3-mvp.md Section 4 — HID report byte layout.
// Must match firmware/src/report.rs byte-for-byte.
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

/// USB Vendor ID — Raspberry Pi
pub const HID_VID: u16 = 0x2E8A;

/// USB Product ID — HID test device (per plan assumptions)
pub const HID_PID: u16 = 0x000A;

/// IN report: Pico → Engine, 64 bytes.
///
/// `repr(C)` ensures a deterministic, padding-free layout matching the wire
/// format.  No heap allocation occurs in the encode/decode path.
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

impl InReport {
    /// Decode a raw 64-byte HID IN report buffer into an `InReport`.
    ///
    /// No heap allocation.  All fields are copied from `buf` by value.
    pub fn from_bytes(buf: &[u8; 64]) -> InReport {
        // Reinterpret encoder_deltas bytes as [i8; 16].
        let mut encoder_deltas = [0i8; 16];
        for (i, b) in buf[9..25].iter().enumerate() {
            encoder_deltas[i] = *b as i8;
        }

        let mut reserved = [0u8; 37];
        reserved.copy_from_slice(&buf[27..64]);

        InReport {
            report_id: buf[0],
            seq: buf[1],
            flags: buf[2],
            step_buttons: [buf[3], buf[4]],
            step_enable_state: [buf[5], buf[6]],
            param_buttons: [buf[7], buf[8]],
            encoder_deltas,
            tempo_delta: buf[25] as i8,
            param_knob_delta: buf[26] as i8,
            reserved,
        }
    }
}

/// OUT report: Engine → Pico, 64 bytes.
///
/// `repr(C)` ensures a deterministic, padding-free layout matching the wire
/// format.  No heap allocation occurs in the encode/decode path.
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

impl OutReport {
    /// Encode the `OutReport` into a 64-byte HID OUT report buffer.
    ///
    /// No heap allocation.  The returned array is stack-allocated.
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        buf[0] = self.report_id;
        buf[1] = self.seq_echo;
        buf[2] = self.led_state[0];
        buf[3] = self.led_state[1];
        buf[4..64].copy_from_slice(&self.reserved);
        buf
    }
}

/// Non-fatal HID device opener.
///
/// Returns `Some(device)` when the device is found and opened, or `None` when
/// it is unavailable.  Errors are logged to stderr so the engine can continue
/// with keyboard-only input.
#[cfg(feature = "hw-io")]
pub fn open_device() -> Option<hidapi::HidDevice> {
    let api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("warn: hidapi init failed ({e}) — running without HID device");
            return None;
        }
    };
    match api.open(HID_VID, HID_PID) {
        Ok(dev) => Some(dev),
        Err(e) => {
            eprintln!(
                "warn: could not open HID device {:04X}:{:04X} ({e}) — running without HID device",
                HID_VID, HID_PID
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a raw 64-byte buffer with distinct non-zero values in every field
    /// so that a round-trip test can verify no field is silently aliased or
    /// swapped.
    fn synthetic_in_buf() -> [u8; 64] {
        let mut buf = [0u8; 64];
        buf[0] = 0x01; // report_id
        buf[1] = 0xAB; // seq
        buf[2] = 0x07; // flags: bits 0-2 set
        buf[3] = 0b1010_1010; // step_buttons low
        buf[4] = 0b0101_0101; // step_buttons high
        buf[5] = 0b1111_0000; // step_enable_state low
        buf[6] = 0b0000_1111; // step_enable_state high
        buf[7] = 0xCC; // param_buttons low
        buf[8] = 0x0D; // param_buttons high nibble
        // encoder_deltas: bytes 9-24, use distinct signed values
        for i in 0..16usize {
            buf[9 + i] = (i as i8).wrapping_add(1).wrapping_mul(-1) as u8;
        }
        buf[25] = 0xF2_u8; // tempo_delta = -14 as i8
        buf[26] = 0x05; // param_knob_delta = +5
        // reserved: bytes 27-63, fill with a pattern
        for i in 0..37usize {
            buf[27 + i] = (i as u8).wrapping_add(0x10);
        }
        buf
    }

    #[test]
    fn in_report_round_trip_nonzero() {
        let buf = synthetic_in_buf();
        let report = InReport::from_bytes(&buf);

        assert_eq!(report.report_id, 0x01);
        assert_eq!(report.seq, 0xAB);
        assert_eq!(report.flags, 0x07);
        assert_eq!(report.step_buttons, [0b1010_1010, 0b0101_0101]);
        assert_eq!(report.step_enable_state, [0b1111_0000, 0b0000_1111]);
        assert_eq!(report.param_buttons, [0xCC, 0x0D]);

        // Verify all encoder deltas were decoded correctly.
        for i in 0..16usize {
            let expected = (i as i8).wrapping_add(1).wrapping_mul(-1);
            assert_eq!(report.encoder_deltas[i], expected, "encoder_deltas[{i}] mismatch");
        }

        assert_eq!(report.tempo_delta, 0xF2_u8 as i8); // -14
        assert_eq!(report.param_knob_delta, 5);

        // Verify reserved bytes survived intact.
        for i in 0..37usize {
            assert_eq!(report.reserved[i], (i as u8).wrapping_add(0x10), "reserved[{i}] mismatch");
        }
    }

    #[test]
    fn in_report_all_zeros_produces_zero_struct() {
        let buf = [0u8; 64];
        let report = InReport::from_bytes(&buf);

        assert_eq!(report.report_id, 0);
        assert_eq!(report.seq, 0);
        assert_eq!(report.flags, 0);
        assert_eq!(report.step_buttons, [0, 0]);
        assert_eq!(report.step_enable_state, [0, 0]);
        assert_eq!(report.param_buttons, [0, 0]);
        assert_eq!(report.encoder_deltas, [0i8; 16]);
        assert_eq!(report.tempo_delta, 0);
        assert_eq!(report.param_knob_delta, 0);
        assert_eq!(report.reserved, [0u8; 37]);
    }

    #[test]
    fn out_report_to_bytes_round_trip() {
        let report = OutReport {
            report_id: 0x02,
            seq_echo: 0xAB,
            led_state: [0b1111_0000, 0b0000_1111],
            reserved: [0u8; 60],
        };
        let buf = report.to_bytes();

        assert_eq!(buf[0], 0x02);
        assert_eq!(buf[1], 0xAB);
        assert_eq!(buf[2], 0b1111_0000);
        assert_eq!(buf[3], 0b0000_1111);
        // Remaining bytes must be zero.
        assert!(buf[4..].iter().all(|&b| b == 0));
    }

    #[test]
    fn out_report_to_bytes_is_64_bytes() {
        let report = OutReport {
            report_id: 0x02,
            seq_echo: 0,
            led_state: [0, 0],
            reserved: [0u8; 60],
        };
        assert_eq!(report.to_bytes().len(), 64);
    }

    #[test]
    fn in_report_from_bytes_is_64_bytes_in() {
        let buf = [0u8; 64];
        let _ = InReport::from_bytes(&buf);
        // Compile-time guarantee: function signature requires &[u8; 64].
    }

    #[test]
    fn hid_vid_pid_constants() {
        assert_eq!(HID_VID, 0x2E8A);
        assert_eq!(HID_PID, 0x000A);
    }

    // -----------------------------------------------------------------------
    // Struct size assertions — both structs must be exactly 64 bytes.
    // -----------------------------------------------------------------------

    #[test]
    fn in_report_size_is_64_bytes() {
        assert_eq!(std::mem::size_of::<InReport>(), 64,
            "InReport must be exactly 64 bytes to match the HID wire format");
    }

    #[test]
    fn out_report_size_is_64_bytes() {
        assert_eq!(std::mem::size_of::<OutReport>(), 64,
            "OutReport must be exactly 64 bytes to match the HID wire format");
    }

    // -----------------------------------------------------------------------
    // Field offset assertions — must match Section 4 of the plan byte-for-byte.
    // -----------------------------------------------------------------------

    #[test]
    fn in_report_field_offsets_match_wire_spec() {
        // Verify field offsets using std::mem::offset_of! (stable since Rust 1.77).
        //
        // Spec:
        //   Byte  0   : report_id
        //   Byte  1   : seq
        //   Byte  2   : flags
        //   Bytes 3-4 : step_buttons[2]
        //   Bytes 5-6 : step_enable_state[2]
        //   Bytes 7-8 : param_buttons[2]
        //   Bytes 9-24: encoder_deltas[16]
        //   Byte 25   : tempo_delta
        //   Byte 26   : param_knob_delta
        //   Bytes 27-63: reserved[37]

        assert_eq!(std::mem::offset_of!(InReport, report_id), 0, "report_id must be at byte 0");
        assert_eq!(std::mem::offset_of!(InReport, seq), 1, "seq must be at byte 1");
        assert_eq!(std::mem::offset_of!(InReport, flags), 2, "flags must be at byte 2");
        assert_eq!(std::mem::offset_of!(InReport, step_buttons), 3, "step_buttons must start at byte 3");
        assert_eq!(std::mem::offset_of!(InReport, step_enable_state), 5, "step_enable_state must start at byte 5");
        assert_eq!(std::mem::offset_of!(InReport, param_buttons), 7, "param_buttons must start at byte 7");
        assert_eq!(std::mem::offset_of!(InReport, encoder_deltas), 9, "encoder_deltas must start at byte 9");
        assert_eq!(std::mem::offset_of!(InReport, tempo_delta), 25, "tempo_delta must be at byte 25");
        assert_eq!(std::mem::offset_of!(InReport, param_knob_delta), 26, "param_knob_delta must be at byte 26");
        assert_eq!(std::mem::offset_of!(InReport, reserved), 27, "reserved must start at byte 27");
    }

    #[test]
    fn out_report_field_offsets_match_wire_spec() {
        // Spec:
        //   Byte  0   : report_id
        //   Byte  1   : seq_echo
        //   Bytes 2-3 : led_state[2]
        //   Bytes 4-63: reserved[60]
        let report = OutReport {
            report_id: 0x02,
            seq_echo: 0xAA,
            led_state: [0xBB, 0xCC],
            reserved: [0u8; 60],
        };

        let buf = report.to_bytes();
        assert_eq!(buf[0], 0x02, "report_id must be at byte 0");
        assert_eq!(buf[1], 0xAA, "seq_echo must be at byte 1");
        assert_eq!(buf[2], 0xBB, "led_state[0] must be at byte 2");
        assert_eq!(buf[3], 0xCC, "led_state[1] must be at byte 3");
        for i in 4..64usize {
            assert_eq!(buf[i], 0x00, "reserved byte {i} must be zero");
        }
    }

    // -----------------------------------------------------------------------
    // Boundary value tests — from_bytes with 0, i8::MIN (0x80), i8::MAX (0x7F),
    // and u8::MAX (0xFF) in every relevant field position.
    // -----------------------------------------------------------------------

    #[test]
    fn from_bytes_boundary_u8_max_in_all_u8_fields() {
        let mut buf = [0u8; 64];
        buf[0] = u8::MAX;   // report_id
        buf[1] = u8::MAX;   // seq
        buf[2] = u8::MAX;   // flags
        buf[3] = u8::MAX;   // step_buttons[0]
        buf[4] = u8::MAX;   // step_buttons[1]
        buf[5] = u8::MAX;   // step_enable_state[0]
        buf[6] = u8::MAX;   // step_enable_state[1]
        buf[7] = u8::MAX;   // param_buttons[0]
        buf[8] = u8::MAX;   // param_buttons[1]
        // leave encoder_deltas as 0
        // leave tempo_delta/param_knob_delta as 0
        for i in 27..64usize {
            buf[i] = u8::MAX; // reserved
        }

        let r = InReport::from_bytes(&buf);
        assert_eq!(r.report_id, 0xFF);
        assert_eq!(r.seq, 0xFF);
        assert_eq!(r.flags, 0xFF);
        assert_eq!(r.step_buttons, [0xFF, 0xFF]);
        assert_eq!(r.step_enable_state, [0xFF, 0xFF]);
        assert_eq!(r.param_buttons, [0xFF, 0xFF]);
        assert_eq!(r.encoder_deltas, [0i8; 16]);
        assert_eq!(r.tempo_delta, 0i8);
        assert_eq!(r.param_knob_delta, 0i8);
        assert_eq!(r.reserved, [0xFF; 37]);
    }

    #[test]
    fn from_bytes_i8_min_sign_extends_correctly() {
        // i8::MIN = -128 = 0x80 as u8. All signed fields set to 0x80.
        let mut buf = [0u8; 64];
        for i in 9..25usize {
            buf[i] = 0x80; // encoder_deltas: all i8::MIN
        }
        buf[25] = 0x80; // tempo_delta: i8::MIN
        buf[26] = 0x80; // param_knob_delta: i8::MIN

        let r = InReport::from_bytes(&buf);
        for i in 0..16usize {
            assert_eq!(r.encoder_deltas[i], i8::MIN,
                "encoder_deltas[{i}] should be i8::MIN (-128) when byte is 0x80");
        }
        assert_eq!(r.tempo_delta, i8::MIN,
            "tempo_delta should be i8::MIN (-128) when byte is 0x80");
        assert_eq!(r.param_knob_delta, i8::MIN,
            "param_knob_delta should be i8::MIN (-128) when byte is 0x80");
    }

    #[test]
    fn from_bytes_i8_max_sign_extends_correctly() {
        // i8::MAX = 127 = 0x7F as u8. All signed fields set to 0x7F.
        let mut buf = [0u8; 64];
        for i in 9..25usize {
            buf[i] = 0x7F; // encoder_deltas: all i8::MAX
        }
        buf[25] = 0x7F; // tempo_delta: i8::MAX
        buf[26] = 0x7F; // param_knob_delta: i8::MAX

        let r = InReport::from_bytes(&buf);
        for i in 0..16usize {
            assert_eq!(r.encoder_deltas[i], i8::MAX,
                "encoder_deltas[{i}] should be i8::MAX (127) when byte is 0x7F");
        }
        assert_eq!(r.tempo_delta, i8::MAX,
            "tempo_delta should be i8::MAX (127) when byte is 0x7F");
        assert_eq!(r.param_knob_delta, i8::MAX,
            "param_knob_delta should be i8::MAX (127) when byte is 0x7F");
    }

    #[test]
    fn from_bytes_zero_signed_fields_are_zero() {
        // 0x00 as i8 = 0. Redundant with all_zeros test but explicit for signed fields.
        let buf = [0u8; 64];
        let r = InReport::from_bytes(&buf);
        assert_eq!(r.encoder_deltas, [0i8; 16]);
        assert_eq!(r.tempo_delta, 0i8);
        assert_eq!(r.param_knob_delta, 0i8);
    }

    #[test]
    fn from_bytes_each_encoder_delta_index_independently() {
        // Verify each encoder_deltas slot maps to exactly the right byte offset.
        for idx in 0..16usize {
            let mut buf = [0u8; 64];
            buf[9 + idx] = 0x80; // i8::MIN into exactly one slot
            let r = InReport::from_bytes(&buf);
            for j in 0..16usize {
                if j == idx {
                    assert_eq!(r.encoder_deltas[j], i8::MIN,
                        "encoder_deltas[{j}] should be i8::MIN when buf[{}]=0x80", 9 + idx);
                } else {
                    assert_eq!(r.encoder_deltas[j], 0i8,
                        "encoder_deltas[{j}] should be 0 when only buf[{}] is set", 9 + idx);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // OutReport LED bit patterns — all bits set vs none set.
    // -----------------------------------------------------------------------

    #[test]
    fn out_report_to_bytes_all_led_bits_set() {
        let report = OutReport {
            report_id: 0x02,
            seq_echo: 0x00,
            led_state: [0xFF, 0xFF], // all 16 LEDs on
            reserved: [0u8; 60],
        };
        let buf = report.to_bytes();
        assert_eq!(buf[2], 0xFF, "led_state[0] all-ones must encode to 0xFF at byte 2");
        assert_eq!(buf[3], 0xFF, "led_state[1] all-ones must encode to 0xFF at byte 3");
        // Verify reserved tail is untouched.
        for i in 4..64usize {
            assert_eq!(buf[i], 0x00, "reserved byte {i} must be 0 when led bits are all set");
        }
    }

    #[test]
    fn out_report_to_bytes_no_led_bits_set() {
        let report = OutReport {
            report_id: 0x02,
            seq_echo: 0x00,
            led_state: [0x00, 0x00], // all 16 LEDs off
            reserved: [0u8; 60],
        };
        let buf = report.to_bytes();
        assert_eq!(buf[2], 0x00, "led_state[0] all-zeros must encode to 0x00 at byte 2");
        assert_eq!(buf[3], 0x00, "led_state[1] all-zeros must encode to 0x00 at byte 3");
    }

    #[test]
    fn out_report_to_bytes_alternating_led_bits() {
        // Verify individual bit positions are not swapped.
        let report = OutReport {
            report_id: 0x02,
            seq_echo: 0x00,
            led_state: [0b1010_1010, 0b0101_0101],
            reserved: [0u8; 60],
        };
        let buf = report.to_bytes();
        assert_eq!(buf[2], 0b1010_1010, "led_state[0] alternating pattern must be preserved");
        assert_eq!(buf[3], 0b0101_0101, "led_state[1] alternating pattern must be preserved");
    }
}
