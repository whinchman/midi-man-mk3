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
// HID file logger — silent on failure.
// ---------------------------------------------------------------------------
//
// The HID thread MUST NOT write to stderr while the TUI owns the terminal —
// stderr text bleeds through the ratatui alternate screen and corrupts the
// rendered frame.  Instead, route HID error/info lines to a durable file
// sink.  Any I/O failure inside the logger is swallowed (never re-raised
// to stderr); losing a log line is preferable to garbling the TUI.

/// Default path (relative to the current working directory) for the HID log.
#[cfg(any(feature = "hw-io", test))]
const HID_LOG_DEFAULT_PATH: &str = ".workflow/logs/hid.log";

/// Override env var; if set and non-empty, takes precedence over the default
/// and tmp-fallback paths.
#[cfg(any(feature = "hw-io", test))]
const HID_LOG_ENV: &str = "MIDIMAN_HID_LOG";

/// Append `msg` (with a trailing newline + ISO-8601-ish UTC timestamp prefix)
/// to the HID log file.  Never writes to stderr; never panics.  Resolves the
/// target path in this order:
///   1. `$MIDIMAN_HID_LOG` if set and non-empty
///   2. `.workflow/logs/hid.log` (relative to cwd)
///   3. `<temp_dir>/midi-man-hid.log` as last-resort fallback
#[cfg(any(feature = "hw-io", test))]
fn hid_log(msg: &str) {
    if let Ok(p) = std::env::var(HID_LOG_ENV) {
        if !p.is_empty() && hid_log_to(std::path::Path::new(&p), msg).is_ok() {
            return;
        }
    }
    let default = std::path::Path::new(HID_LOG_DEFAULT_PATH);
    if hid_log_to(default, msg).is_ok() {
        return;
    }
    let mut fallback = std::env::temp_dir();
    fallback.push("midi-man-hid.log");
    let _ = hid_log_to(&fallback, msg);
}

/// Append `msg` to `path`, creating the file (and an existing-but-not-yet-
/// created log line) if needed.  Returns `Err` if the parent directory does
/// not exist or the file cannot be opened/written; the caller is expected
/// to swallow the error.
#[cfg(any(feature = "hw-io", test))]
fn hid_log_to(path: &std::path::Path, msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    // Best-effort UTC seconds-since-epoch prefix; format errors fall through.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    writeln!(f, "[{secs}] {msg}")
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
/// The thread exits cleanly when `shutdown` is set to `true` (checked at the
/// top of each iteration before the blocking HID read) or when `cmd_tx`
/// becomes disconnected.
#[cfg(feature = "hw-io")]
pub fn run_hid(
    cmd_tx: std::sync::mpsc::SyncSender<InputCommand>,
    state: Arc<RwLock<SequencerState>>,
    ui_notify: std::sync::mpsc::SyncSender<()>,
    vid: u16,
    pid: u16,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use hidapi::HidApi;

    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            hid_log(&format!(
                "[hid] hidapi init failed: {e}; HID thread exiting"
            ));
            return;
        }
    };

    let device = match api.open(vid, pid) {
        Ok(d) => d,
        Err(e) => {
            hid_log(&format!(
                "[hid] device {vid:#06x}:{pid:#06x} not found: {e}; HID thread exiting"
            ));
            return;
        }
    };

    let mut last_seq: Option<u8> = None;
    let mut buf = [0u8; 64];

    loop {
        // Check shutdown flag before blocking on HID read.
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // Zero the buffer before each read so stale bytes from a prior
        // short read cannot bleed into the current report's fields.
        buf = [0u8; 64];

        let n = match device.read_timeout(&mut buf, 5) {
            Ok(n) => n,
            Err(e) => {
                hid_log(&format!("[hid] read error: {e}; HID thread exiting"));
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
                hid_log(&format!(
                    "[hid] duplicate sequence number {}: possible stale report",
                    report.seq
                ));
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
                hid_log(&format!("[hid] write error: {e}"));
            }
        }

        // Wake the UI thread (non-blocking: if notify channel is full, skip).
        let _ = ui_notify.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Build a unique tempfile path for one test (no tempfile crate).
    fn unique_tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("midi-man-hid-{name}-{nanos}.log"));
        p
    }

    #[test]
    fn hid_log_to_creates_and_writes_file() {
        let p = unique_tmp("create");
        let _ = std::fs::remove_file(&p);
        hid_log_to(&p, "hello").expect("first write should succeed");
        let body = std::fs::read_to_string(&p).expect("read tempfile");
        assert!(body.contains("hello"), "body was: {body:?}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn hid_log_to_appends_existing_file() {
        let p = unique_tmp("append");
        let _ = std::fs::remove_file(&p);
        hid_log_to(&p, "first").expect("first write");
        hid_log_to(&p, "second").expect("second write");
        let body = std::fs::read_to_string(&p).expect("read tempfile");
        assert!(body.contains("first"));
        assert!(body.contains("second"));
        // Two newlines means at least two lines written.
        assert!(body.matches('\n').count() >= 2, "body was: {body:?}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn hid_log_to_returns_err_for_missing_directory() {
        let mut p = std::env::temp_dir();
        p.push("midi-man-hid-nonexistent-dir-xyzzy");
        p.push("hid.log");
        // Parent dir does not exist; OpenOptions(append, create) must fail.
        let res = hid_log_to(&p, "should not write");
        assert!(res.is_err(), "expected Err, got Ok writing to {p:?}");
    }

    #[test]
    fn hid_log_does_not_panic_when_default_path_missing() {
        // Default path `.workflow/logs/hid.log` may not exist relative to the
        // test binary's cwd; the function must still not panic.  We do not
        // assert on the file system state — only that the call completes.
        // Use the env-override to a path whose parent does not exist so the
        // first branch fails and we fall through to the tmp fallback.
        // SAFETY: tests do not run concurrently for the same env var, but to
        // avoid races with other tests we use a separate process-unique key
        // by setting it to a definitely-bad path then unsetting it.
        let bad = std::env::temp_dir().join("definitely-not-a-dir-xyzzy/hid.log");
        // Keep this scoped — tests in the same process share env, so we
        // restore afterwards.
        let prev = std::env::var(HID_LOG_ENV).ok();
        // SAFETY: env mutation is guarded; this test is single-threaded.
        unsafe {
            std::env::set_var(HID_LOG_ENV, &bad);
        }
        hid_log("smoke");
        // Restore.
        // SAFETY: see above.
        unsafe {
            match prev {
                Some(v) => std::env::set_var(HID_LOG_ENV, v),
                None => std::env::remove_var(HID_LOG_ENV),
            }
        }
    }
}
