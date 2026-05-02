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
use engine::cli::{CliArgs, parse_args_from_iter};

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
