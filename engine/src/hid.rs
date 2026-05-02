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
//
// Translation note: overlay-aware routing (NoteDelta vs ParamValueDelta for
// encoders when an overlay is open) is a future improvement. The HID thread
// reads `state.active_overlay` to determine context; sending NoteDelta
// unconditionally here is the stub behaviour documented in the task spec.

use crate::input::{InputCommand, OverlayMode};
#[cfg(feature = "hw-io")]
use crate::state::SequencerState;
#[cfg(feature = "hw-io")]
use std::sync::{Arc, RwLock};

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

// ---------------------------------------------------------------------------
// Pure translation helpers — no hw-io dependency; fully unit-testable.
// ---------------------------------------------------------------------------

/// Translate one `InReport` into a sequence of `InputCommand` values.
///
/// Pure function: no I/O, no locks, no side effects.  The `active_overlay`
/// argument is the current overlay state (read from shared state by the caller).
///
/// Param-button mapping (0-indexed from LSB of the two `param_buttons` bytes):
/// - Bit 0  → OpenOverlay(Regular) + ParamSelect(0) (Key)
/// - Bit 1  → ParamSelect(1) (Mode)
/// - Bit 2  → ParamSelect(2) (Swing)
/// - Bit 3  → ParamSelect(3) (Step Size)
/// - Bits 4–7 → reserved (ignored)
/// - Bit 8  → loop cycle: active_overlay determines in/out/clear cycling
/// - Bit 9  → reserved (ignored)
/// - Bit 10 → pause toggle (ParamValueDelta(1) stub — full state write in run_hid)
/// - Bit 11 → stop/start toggle (handled via direct state write in run_hid)
///
/// Tempo delta and stop/start/pause are handled by `run_hid` with a write
/// lock; they do NOT appear in the returned Vec.
pub fn translate_in_report(
    report: &InReport,
    active_overlay: Option<OverlayMode>,
) -> Vec<InputCommand> {
    let mut cmds: Vec<InputCommand> = Vec::new();

    // --- Encoder deltas (steps 0–15): note delta per step.
    // Overlay-aware routing is a future improvement; always send NoteDelta.
    for (i, &delta) in report.encoder_deltas.iter().enumerate() {
        if delta != 0 {
            // When an overlay is active the encoder controls param value;
            // without overlay it controls the note for that step.
            // Future: route to ParamValueDelta when overlay is open.
            let _ = active_overlay; // suppress unused-variable lint
            cmds.push(InputCommand::StepSelect(i));
            cmds.push(InputCommand::NoteDelta(delta));
        }
    }

    // --- Step buttons (bits 0–15 across two bytes).
    let step_word = (report.step_buttons[0] as u16) | ((report.step_buttons[1] as u16) << 8);
    for bit in 0..16u16 {
        if step_word & (1 << bit) != 0 {
            cmds.push(InputCommand::StepSelect(bit as usize));
            cmds.push(InputCommand::ToggleStep);
        }
    }

    // --- Param buttons (bits 0–11 across two bytes).
    let param_word = (report.param_buttons[0] as u16) | ((report.param_buttons[1] as u16) << 8);

    // Bits 0–3: overlay + param select.
    // Bit 0: Key (index 0) — also opens overlay.
    if param_word & (1 << 0) != 0 {
        cmds.push(InputCommand::OpenOverlay(OverlayMode::Regular));
        cmds.push(InputCommand::ParamSelect(0));
    }
    // Bits 1–3: Mode, Swing, Step Size (indices 1–3).
    for idx in 1u8..=3 {
        if param_word & (1 << idx) != 0 {
            cmds.push(InputCommand::ParamSelect(idx));
        }
    }

    // Bit 8: loop in/out/clear cycling.
    // Loop cycle is a 3-state machine: set loop_in → set loop_out → clear.
    // The HID thread advances the cycle by emitting ParamSelect(4) (loop param)
    // + ParamValueDelta(1). Full loop-state machine lives in state.rs (future).
    if param_word & (1 << 8) != 0 {
        cmds.push(InputCommand::ParamSelect(4));
        cmds.push(InputCommand::ParamValueDelta(1));
    }

    // Bit 9: reserved — ignored.

    // Bit 10: pause toggle — emit ParamValueDelta on param index 5 (pause).
    // Direct state mutation (paused flag) is performed by run_hid under write lock.
    if param_word & (1 << 10) != 0 {
        cmds.push(InputCommand::ParamSelect(5));
        cmds.push(InputCommand::ParamValueDelta(1));
    }

    // Bit 11: stop/start — direct state mutation in run_hid; no InputCommand emitted.
    // (The playing flag toggle is a direct write, not expressible as InputCommand yet.)

    // --- Param knob delta.
    if report.param_knob_delta != 0 {
        cmds.push(InputCommand::ParamValueDelta(report.param_knob_delta));
    }

    cmds
}

/// Compute the two LED state bytes from the current step-enable bitmap.
///
/// `steps_enabled[i]` is `true` if step `i` is enabled.  Bit `i` of the
/// returned `[u8; 2]` reflects `steps_enabled[i]`.
pub fn compute_led_bytes(steps_enabled: &[bool; 16]) -> [u8; 2] {
    let mut lo: u8 = 0;
    let mut hi: u8 = 0;
    for i in 0..8 {
        if steps_enabled[i] {
            lo |= 1 << i;
        }
        if steps_enabled[i + 8] {
            hi |= 1 << (i);
        }
    }
    [lo, hi]
}

// ---------------------------------------------------------------------------
// Hardware I/O — only compiled with the `hw-io` feature.
// ---------------------------------------------------------------------------

/// Run the HID host read/translate/write loop.
///
/// Opens the Pico HID device using the provided `vid` and `pid`.  Pass
/// `HID_VID`/`HID_PID` for the defaults.  If the device is not found, logs a
/// warning and returns immediately without panicking.  Otherwise, polls for
/// `InReport` data in a 5 ms timeout loop, translates each report into
/// `InputCommand` values sent on `cmd_tx`, writes back an `OutReport` with
/// the current LED state, and sends on `ui_notify` to wake the UI thread.
///
/// The thread exits cleanly when `cmd_tx` becomes disconnected.
#[cfg(feature = "hw-io")]
pub fn run_hid(
    cmd_tx: std::sync::mpsc::SyncSender<InputCommand>,
    state: Arc<RwLock<SequencerState>>,
    ui_notify: std::sync::mpsc::SyncSender<()>,
    vid: u16,
    pid: u16,
) {
    use hidapi::HidApi;

    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[hid] hidapi init failed: {e}; HID thread exiting");
            return;
        }
    };

    let device = match api.open(vid, pid) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[hid] device {vid:#06x}:{pid:#06x} not found: {e}; HID thread exiting");
            return;
        }
    };

    let mut last_seq: Option<u8> = None;
    let mut buf = [0u8; 64];

    loop {
        // Zero the buffer before each read so stale bytes from a prior
        // short read cannot bleed into the current report's fields.
        buf = [0u8; 64];

        let n = match device.read_timeout(&mut buf, 5) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("[hid] read error: {e}; HID thread exiting");
                return;
            }
        };

        if n == 0 {
            // Timeout — no data this cycle.
            continue;
        }

        let report = InReport::from_bytes(&buf);

        // Sequence-number duplicate check.
        if let Some(prev) = last_seq {
            if prev == report.seq {
                eprintln!("[hid] duplicate sequence number {}: possible stale report", report.seq);
            }
        }
        last_seq = Some(report.seq);

        // --- Direct state writes (tempo, pause, stop/start). ---
        let param_word = (report.param_buttons[0] as u16) | ((report.param_buttons[1] as u16) << 8);
        {
            let mut st = state.write().expect("hid: state write lock poisoned");

            // Tempo delta — direct write, clamped to 20–300 BPM.
            if report.tempo_delta != 0 {
                let new_bpm = (st.tempo_bpm as i32 + report.tempo_delta as i32)
                    .clamp(20, 300) as u16;
                st.tempo_bpm = new_bpm;
            }

            // Bit 10: pause toggle.
            if param_word & (1 << 10) != 0 {
                st.paused = !st.paused;
            }

            // Bit 11: stop/start toggle.
            if param_word & (1 << 11) != 0 {
                st.playing = !st.playing;
                if !st.playing {
                    st.paused = false;
                    st.playhead = 0;
                }
            }
        }

        // Read active_overlay for translation context.
        let active_overlay = state.read().expect("hid: state read lock poisoned").active_overlay;

        // --- Translate to InputCommand and send. ---
        let cmds = translate_in_report(&report, active_overlay);
        for cmd in cmds {
            if cmd_tx.send(cmd).is_err() {
                // Receiver dropped — engine is shutting down.
                return;
            }
        }

        // --- Build and write OutReport (LED state). ---
        {
            let st = state.read().expect("hid: state read lock poisoned");
            let mut enabled = [false; 16];
            for (i, step) in st.steps.iter().enumerate() {
                enabled[i] = step.enabled;
            }
            let led_bytes = compute_led_bytes(&enabled);
            let out = OutReport {
                report_id: 0x02,
                seq_echo: report.seq,
                led_state: led_bytes,
                reserved: [0u8; 60],
            };
            let out_buf = out.to_bytes();
            if let Err(e) = device.write(&out_buf) {
                eprintln!("[hid] write error: {e}");
            }
        }

        // Wake the UI thread (non-blocking: if notify channel is full, skip).
        let _ = ui_notify.try_send(());
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

    // -----------------------------------------------------------------------
    // translate_in_report — pure translation logic tests (no hw-io needed).
    // -----------------------------------------------------------------------

    /// Build a zeroed InReport for easy field-level mutation in tests.
    fn zero_report() -> InReport {
        InReport::from_bytes(&[0u8; 64])
    }

    #[test]
    fn translate_encoder_delta_step0_emits_step_select_and_note_delta() {
        let mut buf = [0u8; 64];
        buf[9] = 3i8 as u8; // encoder_deltas[0] = +3
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        // Should see StepSelect(0) then NoteDelta(3).
        let mut iter = cmds.iter();
        assert!(matches!(iter.next(), Some(InputCommand::StepSelect(0))));
        assert!(matches!(iter.next(), Some(InputCommand::NoteDelta(3))));
    }

    #[test]
    fn translate_encoder_delta_negative_emits_correct_delta() {
        let mut buf = [0u8; 64];
        buf[9 + 5] = (-2i8) as u8; // encoder_deltas[5] = -2
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert!(matches!(cmds[0], InputCommand::StepSelect(5)));
        assert!(matches!(cmds[1], InputCommand::NoteDelta(-2)));
    }

    #[test]
    fn translate_zero_encoder_deltas_produces_no_encoder_commands() {
        let report = zero_report();
        let cmds = translate_in_report(&report, None);
        // With all-zero input, no commands should be produced.
        assert!(cmds.is_empty(), "expected no commands for zeroed report, got {cmds:?}");
    }

    #[test]
    fn translate_step_button_bit3_emits_step_select_and_toggle() {
        let mut buf = [0u8; 64];
        buf[3] = 0b0000_1000; // step_buttons low: bit 3 set → step 3
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 2, "expected StepSelect + ToggleStep");
        assert!(matches!(cmds[0], InputCommand::StepSelect(3)));
        assert!(matches!(cmds[1], InputCommand::ToggleStep));
    }

    #[test]
    fn translate_step_button_high_byte_bit_emits_correct_step_index() {
        // Step 9 = bit 1 of the high byte (buf[4]).
        let mut buf = [0u8; 64];
        buf[4] = 0b0000_0010; // bit 9 overall
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], InputCommand::StepSelect(9)));
        assert!(matches!(cmds[1], InputCommand::ToggleStep));
    }

    #[test]
    fn translate_param_button_bit0_opens_overlay_and_selects_param0() {
        let mut buf = [0u8; 64];
        buf[7] = 0b0000_0001; // param_buttons[0] bit 0 = Key
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], InputCommand::OpenOverlay(OverlayMode::Regular)));
        assert!(matches!(cmds[1], InputCommand::ParamSelect(0)));
    }

    #[test]
    fn translate_param_button_bit1_selects_param1() {
        let mut buf = [0u8; 64];
        buf[7] = 0b0000_0010; // bit 1 = Mode
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], InputCommand::ParamSelect(1)));
    }

    #[test]
    fn translate_param_button_bit8_emits_loop_param_cycle() {
        // Bit 8 = byte index 1 (buf[8]), bit 0 of that byte.
        let mut buf = [0u8; 64];
        buf[8] = 0b0000_0001; // param_buttons high byte bit 0 = overall bit 8
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], InputCommand::ParamSelect(4)));
        assert!(matches!(cmds[1], InputCommand::ParamValueDelta(1)));
    }

    #[test]
    fn translate_param_button_bit10_emits_pause_param_cycle() {
        // Bit 10 = buf[8] bit 2.
        let mut buf = [0u8; 64];
        buf[8] = 0b0000_0100;
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], InputCommand::ParamSelect(5)));
        assert!(matches!(cmds[1], InputCommand::ParamValueDelta(1)));
    }

    #[test]
    fn translate_param_button_bit11_emits_no_input_command() {
        // Bit 11 = buf[8] bit 3. Stop/start handled by direct state write in run_hid.
        let mut buf = [0u8; 64];
        buf[8] = 0b0000_1000;
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);
        // bit11 alone should produce no InputCommand entries.
        assert!(cmds.is_empty(), "stop/start should not emit InputCommand, got {cmds:?}");
    }

    #[test]
    fn translate_param_knob_delta_emits_param_value_delta() {
        let mut buf = [0u8; 64];
        buf[26] = 7i8 as u8; // param_knob_delta = +7
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], InputCommand::ParamValueDelta(7)));
    }

    #[test]
    fn translate_synthetic_full_report_encoder_step_param() {
        // Encoder delta on step 0, step button 3 press, param button 1 press.
        let mut buf = [0u8; 64];
        buf[9] = 1i8 as u8;    // encoder_deltas[0] = +1
        buf[3] = 0b0000_1000;  // step_buttons bit 3 = step 3
        buf[7] = 0b0000_0010;  // param_buttons bit 1 = Mode
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        // Expected: StepSelect(0), NoteDelta(1), StepSelect(3), ToggleStep, ParamSelect(1).
        assert_eq!(cmds.len(), 5, "expected 5 commands, got {cmds:?}");
        assert!(matches!(cmds[0], InputCommand::StepSelect(0)));
        assert!(matches!(cmds[1], InputCommand::NoteDelta(1)));
        assert!(matches!(cmds[2], InputCommand::StepSelect(3)));
        assert!(matches!(cmds[3], InputCommand::ToggleStep));
        assert!(matches!(cmds[4], InputCommand::ParamSelect(1)));
    }

    #[test]
    fn translate_multiple_simultaneous_encoder_deltas_all_produce_commands() {
        // Encoders 0, 7, and 15 all have non-zero deltas simultaneously.
        // Each must produce a StepSelect + NoteDelta pair in index order.
        let mut buf = [0u8; 64];
        buf[9 + 0]  = 5i8 as u8;   // encoder_deltas[0]  = +5
        buf[9 + 7]  = (-3i8) as u8; // encoder_deltas[7]  = -3
        buf[9 + 15] = 1i8 as u8;   // encoder_deltas[15] = +1
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        // Expect exactly 6 commands: (StepSelect(0), NoteDelta(5)),
        // (StepSelect(7), NoteDelta(-3)), (StepSelect(15), NoteDelta(1)).
        assert_eq!(cmds.len(), 6, "expected 6 commands for 3 encoder deltas, got {cmds:?}");
        assert!(matches!(cmds[0], InputCommand::StepSelect(0)),  "cmds[0] should be StepSelect(0)");
        assert!(matches!(cmds[1], InputCommand::NoteDelta(5)),   "cmds[1] should be NoteDelta(5)");
        assert!(matches!(cmds[2], InputCommand::StepSelect(7)),  "cmds[2] should be StepSelect(7)");
        assert!(matches!(cmds[3], InputCommand::NoteDelta(-3)),  "cmds[3] should be NoteDelta(-3)");
        assert!(matches!(cmds[4], InputCommand::StepSelect(15)), "cmds[4] should be StepSelect(15)");
        assert!(matches!(cmds[5], InputCommand::NoteDelta(1)),   "cmds[5] should be NoteDelta(1)");
    }

    // -----------------------------------------------------------------------
    // compute_led_bytes — LED bit packing tests.
    // -----------------------------------------------------------------------

    #[test]
    fn compute_led_bytes_all_enabled_returns_ff_ff() {
        let enabled = [true; 16];
        assert_eq!(compute_led_bytes(&enabled), [0xFF, 0xFF]);
    }

    #[test]
    fn compute_led_bytes_all_disabled_returns_00_00() {
        let enabled = [false; 16];
        assert_eq!(compute_led_bytes(&enabled), [0x00, 0x00]);
    }

    #[test]
    fn compute_led_bytes_only_step0_sets_lo_bit0() {
        let mut enabled = [false; 16];
        enabled[0] = true;
        let [lo, hi] = compute_led_bytes(&enabled);
        assert_eq!(lo, 0b0000_0001, "step 0 should set bit 0 of lo byte");
        assert_eq!(hi, 0x00);
    }

    #[test]
    fn compute_led_bytes_only_step8_sets_hi_bit0() {
        let mut enabled = [false; 16];
        enabled[8] = true;
        let [lo, hi] = compute_led_bytes(&enabled);
        assert_eq!(lo, 0x00);
        assert_eq!(hi, 0b0000_0001, "step 8 should set bit 0 of hi byte");
    }

    #[test]
    fn compute_led_bytes_only_step15_sets_hi_bit7() {
        let mut enabled = [false; 16];
        enabled[15] = true;
        let [lo, hi] = compute_led_bytes(&enabled);
        assert_eq!(lo, 0x00);
        assert_eq!(hi, 0b1000_0000, "step 15 should set bit 7 of hi byte");
    }

    #[test]
    fn compute_led_bytes_alternating_steps_match_expected_pattern() {
        let mut enabled = [false; 16];
        // Enable even steps: 0, 2, 4, 6, 8, 10, 12, 14.
        for i in (0..16).step_by(2) {
            enabled[i] = true;
        }
        let [lo, hi] = compute_led_bytes(&enabled);
        // Steps 0, 2, 4, 6 → bits 0, 2, 4, 6 of lo.
        assert_eq!(lo, 0b0101_0101);
        // Steps 8, 10, 12, 14 → bits 0, 2, 4, 6 of hi.
        assert_eq!(hi, 0b0101_0101);
    }

    #[test]
    fn compute_led_bytes_only_step7_sets_lo_bit7() {
        // Step 7 is the MSB of the low byte; byte 0 must be 0x80, byte 1 = 0x00.
        let mut enabled = [false; 16];
        enabled[7] = true;
        let [lo, hi] = compute_led_bytes(&enabled);
        assert_eq!(lo, 0x80, "step 7 should set bit 7 (MSB) of lo byte → 0x80");
        assert_eq!(hi, 0x00, "hi byte must be 0x00 when only step 7 is enabled");
    }

    // -----------------------------------------------------------------------
    // translate_in_report — additional edge cases required by QA task.
    // -----------------------------------------------------------------------

    #[test]
    fn translate_all_zero_report_emits_no_commands() {
        // An InReport where every field is zero must produce an empty command list.
        // This covers: zero encoder deltas, no step buttons pressed, no param buttons,
        // zero tempo_delta, zero param_knob_delta.
        let report = InReport::from_bytes(&[0u8; 64]);
        let cmds = translate_in_report(&report, None);
        assert!(cmds.is_empty(), "all-zero InReport must emit no InputCommands, got {cmds:?}");
    }

    #[test]
    fn translate_all_16_step_buttons_set_emits_32_commands() {
        // All 16 step_buttons bits set → 16 × (StepSelect(i) + ToggleStep) pairs,
        // in ascending index order 0..=15.
        let mut buf = [0u8; 64];
        buf[3] = 0xFF; // step_buttons low byte: steps 0–7
        buf[4] = 0xFF; // step_buttons high byte: steps 8–15
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        assert_eq!(cmds.len(), 32, "expected 32 commands (16 × StepSelect+ToggleStep), got {cmds:?}");
        for i in 0..16usize {
            assert!(
                matches!(cmds[i * 2], InputCommand::StepSelect(s) if s == i),
                "cmds[{}] should be StepSelect({i}), got {:?}", i * 2, cmds[i * 2]
            );
            assert!(
                matches!(cmds[i * 2 + 1], InputCommand::ToggleStep),
                "cmds[{}] should be ToggleStep, got {:?}", i * 2 + 1, cmds[i * 2 + 1]
            );
        }
    }

    #[test]
    fn translate_param_buttons_bits_0_1_2_3_all_set_emits_correct_commands() {
        // param_buttons bits 0–3 all set simultaneously.
        // Bit 0 → OpenOverlay(Regular) + ParamSelect(0)
        // Bit 1 → ParamSelect(1)
        // Bit 2 → ParamSelect(2)
        // Bit 3 → ParamSelect(3)
        let mut buf = [0u8; 64];
        buf[7] = 0b0000_1111; // bits 0, 1, 2, 3 set
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, None);

        // Expected order: OpenOverlay(Regular), ParamSelect(0), ParamSelect(1),
        // ParamSelect(2), ParamSelect(3) — 5 total.
        assert_eq!(cmds.len(), 5, "expected 5 commands for bits 0–3, got {cmds:?}");
        assert!(matches!(cmds[0], InputCommand::OpenOverlay(OverlayMode::Regular)),
            "cmds[0] should be OpenOverlay(Regular)");
        assert!(matches!(cmds[1], InputCommand::ParamSelect(0)),
            "cmds[1] should be ParamSelect(0)");
        assert!(matches!(cmds[2], InputCommand::ParamSelect(1)),
            "cmds[2] should be ParamSelect(1)");
        assert!(matches!(cmds[3], InputCommand::ParamSelect(2)),
            "cmds[3] should be ParamSelect(2)");
        assert!(matches!(cmds[4], InputCommand::ParamSelect(3)),
            "cmds[4] should be ParamSelect(3)");
    }

    #[test]
    fn translate_encoder_delta_with_active_overlay_emits_note_delta_not_param_value_delta() {
        // Documents current behaviour: overlay-aware routing is deferred.
        // Even with active_overlay = Some(Regular), translate_in_report always
        // emits StepSelect + NoteDelta, never ParamValueDelta, for encoder inputs.
        // This test records the current (stub) behaviour so a future change is visible.
        let mut buf = [0u8; 64];
        buf[9 + 3] = 2i8 as u8; // encoder_deltas[3] = +2
        let report = InReport::from_bytes(&buf);
        let cmds = translate_in_report(&report, Some(OverlayMode::Regular));

        assert_eq!(cmds.len(), 2, "expected exactly 2 commands, got {cmds:?}");
        assert!(matches!(cmds[0], InputCommand::StepSelect(3)),
            "cmds[0] should be StepSelect(3) regardless of overlay; got {:?}", cmds[0]);
        assert!(matches!(cmds[1], InputCommand::NoteDelta(2)),
            "cmds[1] should be NoteDelta(2) — overlay-aware routing is deferred; got {:?}", cmds[1]);
    }
}
