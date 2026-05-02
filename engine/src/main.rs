// Build requirement: midir requires alsa-lib-devel (Fedora) or libasound2-dev (Ubuntu).
// For development without ALSA: build without --features hw-io.
//
// Usage:
//   cargo run -p engine --features hw-io -- [--midi-port <name>] [--hid-vid <hex>] [--hid-pid <hex>]
//
// Thread wiring order (channel ownership):
//   1. run_midi_out  -- takes midi_rx, port_name        [hw-io]
//   2. run_clock     -- takes Arc<state>, midi_tx       [hw-io]
//   3. run_hid       -- takes cmd_tx clone, Arc<state> clone, ui_notify_tx clone, vid, pid [hw-io]
//   4. cmd-processor -- takes cmd_rx, Arc<state> clone, ui_notify_tx
//   5. run_ui        -- takes Arc<state>, ui_notify_rx, cmd_tx [hw-io]
//   main blocks on ui_thread.join() then joins cmd/clock/midi threads in order

use std::sync::{Arc, RwLock, mpsc};
use engine::state::{MidiEvent, SequencerState};
use engine::input::InputCommand;

/// Parse a hex string (with or without leading "0x") into a u16.
fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u16::from_str_radix(stripped, 16).map_err(|e| format!("invalid hex '{}': {}", s, e))
}

/// Minimal CLI argument parsing via std::env::args().
struct CliArgs {
    /// MIDI port name substring to match (None = first available).
    midi_port: Option<String>,
    /// HID Vendor ID override (None = use HID_VID constant).
    hid_vid: Option<u16>,
    /// HID Product ID override (None = use HID_PID constant).
    hid_pid: Option<u16>,
}

/// Parse CLI arguments from an arbitrary iterator (testable entry point).
///
/// The iterator must yield flag/value pairs as individual strings, exactly as
/// `std::env::args().skip(1)` would produce them.
fn parse_args_from_iter<I>(mut args: I) -> CliArgs
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

fn parse_args() -> CliArgs {
    parse_args_from_iter(std::env::args().skip(1))
}

fn main() {
    let args = parse_args();

    // Log any overrides.
    if let Some(ref port) = args.midi_port {
        println!("[main] MIDI port filter: {port}");
    }
    if let Some(vid) = args.hid_vid {
        println!("[main] HID VID override: {vid:#06x}");
    }
    if let Some(pid) = args.hid_pid {
        println!("[main] HID PID override: {pid:#06x}");
    }

    // --- Shared state ---
    let state: Arc<RwLock<SequencerState>> = Arc::new(RwLock::new(SequencerState::default()));

    // --- Channels ---
    // midi: clock -> midi_out  (bounded; clock never blocks waiting for midi_out)
    let (midi_tx, midi_rx) = mpsc::sync_channel::<MidiEvent>(64);
    // cmd:  hid/ui -> cmd-processor  (bounded; senders are non-blocking try_send from UI)
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<InputCommand>(64);
    // notify: cmd-processor -> ui  (bounded; try_send so clock never blocks)
    let (ui_notify_tx, ui_notify_rx) = mpsc::sync_channel::<()>(16);

    // --- Thread 1: MIDI output (hw-io only) ---
    #[cfg(feature = "hw-io")]
    let midi_thread = {
        let rx = midi_rx;
        let port_name = args.midi_port.clone();
        std::thread::Builder::new()
            .name("midi-out".to_owned())
            .spawn(move || engine::midi_out::run_midi_out(rx, port_name))
            .expect("failed to spawn midi-out thread")
    };
    // Without hw-io there is no clock or midi thread — drop midi_rx immediately.
    #[cfg(not(feature = "hw-io"))]
    let _ = midi_rx;

    // --- Thread 2: Clock (hw-io only) ---
    // In non-hw-io builds the clock is not spawned: with playing=false the only
    // exit condition (midi_tx.send() returning Err) is never reached, so the
    // clock would loop indefinitely.  The non-hw-io test path does not need a
    // real clock.
    #[cfg(feature = "hw-io")]
    let clock_thread = {
        let clock_state = Arc::clone(&state);
        let clock_midi_tx = midi_tx.clone();
        std::thread::Builder::new()
            .name("clock".to_owned())
            .spawn(move || engine::clock::run_clock(clock_state, clock_midi_tx))
            .expect("failed to spawn clock thread")
    };

    // --- Thread 3: HID host (hw-io only) ---
    #[cfg(feature = "hw-io")]
    let hid_thread = {
        let hid_cmd_tx = cmd_tx.clone();
        let hid_state = Arc::clone(&state);
        let hid_notify = ui_notify_tx.clone();
        let vid = args.hid_vid.unwrap_or(engine::hid::HID_VID);
        let pid = args.hid_pid.unwrap_or(engine::hid::HID_PID);
        std::thread::Builder::new()
            .name("hid".to_owned())
            .spawn(move || engine::hid::run_hid(hid_cmd_tx, hid_state, hid_notify, vid, pid))
            .expect("failed to spawn hid thread")
    };

    // --- Thread 4: Command processor ---
    // Consumes InputCommand values, acquires write lock, applies command, notifies UI.
    let cmd_state = Arc::clone(&state);
    let cmd_notify = ui_notify_tx.clone();
    let cmd_thread = std::thread::Builder::new()
        .name("cmd-processor".to_owned())
        .spawn(move || {
            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => {
                        {
                            let mut s = cmd_state.write()
                                .expect("cmd-processor: state RwLock poisoned");
                            s.apply_command(cmd);
                        }
                        // Best-effort notify; if UI is gone we continue until cmd_rx closes.
                        let _ = cmd_notify.try_send(());
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("failed to spawn cmd-processor thread");

    // Drop the original ui_notify_tx so the UI thread gets Disconnected when
    // cmd_notify (the only remaining sender) is dropped on cmd-processor exit.
    drop(ui_notify_tx);

    // --- Thread 5: UI (hw-io only) ---
    // run_ui blocks until Ctrl-C; main blocks here waiting for it.
    #[cfg(feature = "hw-io")]
    {
        let ui_state = Arc::clone(&state);
        let ui_cmd_tx = cmd_tx.clone();
        let ui_thread = std::thread::Builder::new()
            .name("ui".to_owned())
            .spawn(move || engine::ui::run_ui(ui_state, ui_notify_rx, ui_cmd_tx))
            .expect("failed to spawn ui thread");

        // Block until UI exits (user pressed Ctrl-C).
        let _ = ui_thread.join();
    }

    // Without hw-io there is no UI thread to join; drop the notify receiver so
    // the cmd-processor's try_send sees a disconnected channel and main exits.
    #[cfg(not(feature = "hw-io"))]
    {
        drop(ui_notify_rx);
    }

    // --- Cleanup ---
    // Send MIDI Stop before letting threads wind down.
    #[cfg(feature = "hw-io")]
    let _ = midi_tx.send(MidiEvent::Stop);

    // Drop all senders so threads detect disconnection and exit cleanly.
    // Drop cmd_tx first: cmd-processor exits when its receiver sees Disconnected.
    drop(cmd_tx);
    // Drop midi_tx: clock exits when its send() returns Err (receiver dropped).
    drop(midi_tx);

    // Join threads in dependency order so MidiEvent::Stop is consumed before exit.
    // cmd_thread first — it holds no dependency on clock/midi threads.
    let _ = cmd_thread.join();

    // hw-io threads: clock → midi (clock sends to midi; midi must outlive clock).
    #[cfg(feature = "hw-io")]
    {
        let _ = hid_thread.join();
        let _ = clock_thread.join();
        let _ = midi_thread.join();
    }
}

// ---------------------------------------------------------------------------
// Smoke tests — no real audio/MIDI devices required.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use engine::input::InputCommand;
    use engine::state::SequencerState;
    use std::sync::{Arc, RwLock, mpsc};
    use std::time::Duration;

    /// Verify that the command processor loop correctly applies commands to state
    /// and sends UI notifications without needing real MIDI/HID hardware.
    #[test]
    fn cmd_processor_applies_commands_and_notifies_ui() {
        let state: Arc<RwLock<SequencerState>> = Arc::new(RwLock::new(SequencerState::default()));

        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<InputCommand>(16);
        let (ui_notify_tx, ui_notify_rx) = mpsc::sync_channel::<()>(16);

        // Spawn the command processor.
        let proc_state = Arc::clone(&state);
        let proc_notify = ui_notify_tx.clone();
        let _proc = std::thread::spawn(move || {
            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => {
                        {
                            let mut s = proc_state.write().expect("poisoned");
                            s.apply_command(cmd);
                        }
                        let _ = proc_notify.try_send(());
                    }
                    Err(_) => break,
                }
            }
        });

        // Send a ToggleStep command for step 3.
        cmd_tx.send(InputCommand::StepSelect(3)).expect("send failed");
        cmd_tx.send(InputCommand::ToggleStep).expect("send failed");

        // Wait for both notifications (one per command).
        ui_notify_rx.recv_timeout(Duration::from_millis(500))
            .expect("no notify for StepSelect");
        ui_notify_rx.recv_timeout(Duration::from_millis(500))
            .expect("no notify for ToggleStep");

        // Verify state was mutated.
        let s = state.read().expect("poisoned");
        assert!(s.steps[3].enabled, "step 3 should be enabled after ToggleStep");

        // Drop sender to shut down the cmd-processor.
        drop(cmd_tx);
    }

    /// Verify SequencerState::default() wraps cleanly in Arc<RwLock<>>.
    #[test]
    fn state_arc_rwlock_default_is_sane() {
        let state: Arc<RwLock<SequencerState>> = Arc::new(RwLock::new(SequencerState::default()));
        let s = state.read().expect("poisoned");
        assert_eq!(s.tempo_bpm, 120);
        assert!(!s.playing);
    }

    /// Verify channel creation types match expected signatures.
    #[test]
    fn channel_types_are_correct() {
        let (_midi_tx, _midi_rx) = mpsc::sync_channel::<MidiEvent>(64);
        let (_cmd_tx, _cmd_rx) = mpsc::sync_channel::<InputCommand>(64);
        let (_notify_tx, _notify_rx) = mpsc::sync_channel::<()>(16);
        // If this compiles the types are correct.
    }

    /// Smoke test: run cmd-processor for multiple commands and verify ordering.
    #[test]
    fn cmd_processor_handles_multiple_commands_in_order() {
        let state: Arc<RwLock<SequencerState>> = Arc::new(RwLock::new(SequencerState::default()));
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<InputCommand>(64);
        let (ui_notify_tx, ui_notify_rx) = mpsc::sync_channel::<()>(64);

        let proc_state = Arc::clone(&state);
        let proc_notify = ui_notify_tx;
        let _proc = std::thread::spawn(move || {
            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => {
                        let mut s = proc_state.write().expect("poisoned");
                        s.apply_command(cmd);
                        drop(s);
                        let _ = proc_notify.try_send(());
                    }
                    Err(_) => break,
                }
            }
        });

        // Send 5 commands: select steps 0-4 and toggle each.
        for i in 0usize..5 {
            cmd_tx.send(InputCommand::StepSelect(i)).expect("send");
            cmd_tx.send(InputCommand::ToggleStep).expect("send");
        }

        // Drain 10 notifications.
        for _ in 0..10 {
            ui_notify_rx.recv_timeout(Duration::from_millis(500))
                .expect("timeout waiting for notify");
        }

        let s = state.read().expect("poisoned");
        for i in 0..5 {
            assert!(s.steps[i].enabled, "step {i} should be enabled");
        }
        for i in 5..16 {
            assert!(!s.steps[i].enabled, "step {i} should remain disabled");
        }

        drop(cmd_tx);
    }

    /// Verify parse_hex_u16 handles various formats.
    #[test]
    fn parse_hex_u16_variants() {
        assert_eq!(parse_hex_u16("0x2E8A").unwrap(), 0x2E8A);
        assert_eq!(parse_hex_u16("0X000A").unwrap(), 0x000A);
        assert_eq!(parse_hex_u16("FFFF").unwrap(), 0xFFFF);
        assert!(parse_hex_u16("GGGG").is_err());
    }

    // -----------------------------------------------------------------------
    // CLI argument parsing
    // -----------------------------------------------------------------------

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    /// No arguments: all fields are None (defaults are applied elsewhere).
    #[test]
    fn cli_defaults_when_no_args() {
        let a = parse_args_from_iter(args(&[]));
        assert!(a.midi_port.is_none(), "midi_port should default to None");
        assert!(a.hid_vid.is_none(), "hid_vid should default to None");
        assert!(a.hid_pid.is_none(), "hid_pid should default to None");
    }

    /// --midi-port sets midi_port to the provided string.
    #[test]
    fn cli_midi_port_is_set() {
        let a = parse_args_from_iter(args(&["--midi-port", "MyPort"]));
        assert_eq!(a.midi_port.as_deref(), Some("MyPort"));
    }

    /// --hid-vid with a valid hex value sets hid_vid.
    #[test]
    fn cli_hid_vid_is_set() {
        let a = parse_args_from_iter(args(&["--hid-vid", "0x1234"]));
        assert_eq!(a.hid_vid, Some(0x1234u16));
    }

    /// --hid-pid with a valid hex value sets hid_pid.
    #[test]
    fn cli_hid_pid_is_set() {
        let a = parse_args_from_iter(args(&["--hid-pid", "0xABCD"]));
        assert_eq!(a.hid_pid, Some(0xABCDu16));
    }

    /// Malformed hex for --hid-vid: should not panic; hid_vid remains None.
    #[test]
    fn cli_malformed_hid_vid_does_not_panic() {
        let a = parse_args_from_iter(args(&["--hid-vid", "notahex"]));
        assert!(a.hid_vid.is_none(), "malformed hex should leave hid_vid as None");
    }

    /// All three flags together.
    #[test]
    fn cli_all_flags_together() {
        let a = parse_args_from_iter(args(&[
            "--midi-port", "FluidSynth",
            "--hid-vid", "0x2E8A",
            "--hid-pid", "0x000A",
        ]));
        assert_eq!(a.midi_port.as_deref(), Some("FluidSynth"));
        assert_eq!(a.hid_vid, Some(0x2E8Au16));
        assert_eq!(a.hid_pid, Some(0x000Au16));
    }

    // -----------------------------------------------------------------------
    // Command processor: exits when cmd_tx is dropped
    // -----------------------------------------------------------------------

    /// Verify that the cmd-processor thread exits (joins without blocking) when
    /// the sender is dropped without sending any commands.
    #[test]
    fn cmd_processor_exits_when_sender_dropped() {
        let state: Arc<RwLock<SequencerState>> = Arc::new(RwLock::new(SequencerState::default()));
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<InputCommand>(16);
        let (ui_notify_tx, _ui_notify_rx) = mpsc::sync_channel::<()>(16);

        let proc_state = Arc::clone(&state);
        let proc_notify = ui_notify_tx;
        let proc = std::thread::spawn(move || {
            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => {
                        let mut s = proc_state.write().expect("poisoned");
                        s.apply_command(cmd);
                        drop(s);
                        let _ = proc_notify.try_send(());
                    }
                    Err(_) => break,
                }
            }
        });

        // Drop the sender — cmd-processor should exit its recv loop.
        drop(cmd_tx);

        // If the thread does not exit the join will block forever (test timeout
        // will catch it). A clean join confirms the processor detected Disconnected.
        proc.join().expect("cmd-processor thread panicked");
    }
}
