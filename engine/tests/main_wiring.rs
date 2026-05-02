use engine::cli::{parse_args_from_iter, parse_hex_u16};
use engine::input::InputCommand;
use engine::state::{MidiEvent, SequencerState};
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
