//! Minimal CLI argument parsing for the engine binary.
//!
//! Exposed as `pub(crate)` so that integration tests in `engine/tests/` can
//! exercise the pure parsing logic without needing to spawn the binary.

/// Parsed CLI arguments.
pub struct CliArgs {
    /// MIDI port name substring to match (None = first available).
    pub midi_port: Option<String>,
    /// HID Vendor ID override (None = use HID_VID constant).
    pub hid_vid: Option<u16>,
    /// HID Product ID override (None = use HID_PID constant).
    pub hid_pid: Option<u16>,
}

/// Parse a hex string (with or without leading "0x") into a u16.
pub fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u16::from_str_radix(stripped, 16).map_err(|e| format!("invalid hex '{}': {}", s, e))
}

/// Parse CLI arguments from an arbitrary iterator (testable entry point).
///
/// The iterator must yield flag/value pairs as individual strings, exactly as
/// `std::env::args().skip(1)` would produce them.
pub fn parse_args_from_iter<I>(mut args: I) -> CliArgs
where
    I: Iterator<Item = String>,
{
    let mut midi_port = None;
    let mut hid_vid = None;
    let mut hid_pid = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--midi-port" => {
                midi_port = args.next();
            }
            "--hid-vid" => {
                if let Some(val) = args.next() {
                    match parse_hex_u16(&val) {
                        Ok(v) => hid_vid = Some(v),
                        Err(e) => eprintln!("[main] --hid-vid: {e}"),
                    }
                }
            }
            "--hid-pid" => {
                if let Some(val) = args.next() {
                    match parse_hex_u16(&val) {
                        Ok(v) => hid_pid = Some(v),
                        Err(e) => eprintln!("[main] --hid-pid: {e}"),
                    }
                }
            }
            other => {
                eprintln!("[main] unknown argument: {other}");
            }
        }
    }

    CliArgs { midi_port, hid_vid, hid_pid }
}
