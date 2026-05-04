// Build requirement: midir requires alsa-lib-devel (Fedora) or libasound2-dev (Ubuntu).
// For development without ALSA: build without --features hw-io.
//
// Usage:
//   cargo run -p engine --features hw-io -- [--midi-port <name>] [--hid-vid <hex>] [--hid-pid <hex>]
//
// Thread wiring order (channel ownership):
//   1. run_midi_out  -- takes midi_rx, port_name        [hw-io]
//   2. run_clock     -- takes Arc<state>, midi_tx       [hw-io]
//   3. run_hid       -- takes cmd_tx clone, Arc<state> clone, ui_notify_tx clone, vid, pid, shutdown [hw-io]
//   4. cmd-processor -- takes cmd_rx, Arc<state> clone, ui_notify_tx
//   5. run_ui        -- takes Arc<state>, ui_notify_rx, cmd_tx [hw-io]
//   Shutdown order: set hid_shutdown flag → join hid_thread → drop cmd_tx → join cmd_thread
//   → join clock_thread → join midi_thread

use engine::cli::{parse_args_from_iter, CliArgs};
use engine::input::InputCommand;
#[cfg(feature = "hw-io")]
use engine::midi_out::MidiCtrlMsg;
use engine::state::{MidiEvent, SequencerState};
#[cfg(feature = "hw-io")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, RwLock};

fn parse_args() -> CliArgs {
    parse_args_from_iter(std::env::args().skip(1))
}

fn main() {
    let args = parse_args();

    if let Some(vid) = args.hid_vid {
        println!("[main] HID VID override: {vid:#06x}");
    }
    if let Some(pid) = args.hid_pid {
        println!("[main] HID PID override: {pid:#06x}");
    }

    // --- MIDI port from CLI arg (no pre-TUI prompt — port can be set via F4 CLI) ---
    #[cfg(feature = "hw-io")]
    let selected_midi_port: Option<String> = args.midi_port.clone();

    // --- Shared state ---
    let state: Arc<RwLock<SequencerState>> = Arc::new(RwLock::new(SequencerState::default()));

    // --- Channels ---
    // midi: clock -> midi_out  (bounded; clock never blocks waiting for midi_out)
    let (midi_tx, midi_rx) = mpsc::sync_channel::<MidiEvent>(64);
    // cmd:  hid/ui -> cmd-processor  (bounded; senders are non-blocking try_send from UI)
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<InputCommand>(64);
    // notify: cmd-processor -> ui  (bounded; try_send so clock never blocks)
    let (ui_notify_tx, ui_notify_rx) = mpsc::sync_channel::<()>(16);
    // midi_ctrl: ui -> midi_out  (runtime port/channel changes from F4 CLI)
    #[cfg(feature = "hw-io")]
    let (midi_ctrl_tx, midi_ctrl_rx) = mpsc::sync_channel::<MidiCtrlMsg>(16);
    // midi_log: midi_out -> ui  (MIDI thread log messages routed into CLI panel)
    #[cfg(feature = "hw-io")]
    let (midi_log_tx, midi_log_rx) = mpsc::sync_channel::<(bool, String)>(64);

    // --- Thread 1: MIDI output (hw-io only) ---
    #[cfg(feature = "hw-io")]
    let midi_thread = {
        let rx = midi_rx;
        let ctrl_rx = midi_ctrl_rx;
        let port_name = selected_midi_port;
        let log_tx = midi_log_tx;
        std::thread::Builder::new()
            .name("midi-out".to_owned())
            .spawn(move || engine::midi_out::run_midi_out(rx, ctrl_rx, port_name, log_tx))
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

    // --- Shutdown flag (shared with HID thread to allow clean exit) ---
    #[cfg(feature = "hw-io")]
    let hid_shutdown = Arc::new(AtomicBool::new(false));

    // --- Thread 3: HID host (hw-io only) ---
    #[cfg(feature = "hw-io")]
    let hid_thread = {
        let hid_cmd_tx = cmd_tx.clone();
        let hid_state = Arc::clone(&state);
        let hid_notify = ui_notify_tx.clone();
        let vid = args.hid_vid.unwrap_or(engine::hid::HID_VID);
        let pid = args.hid_pid.unwrap_or(engine::hid::HID_PID);
        let hid_shutdown_flag = Arc::clone(&hid_shutdown);
        std::thread::Builder::new()
            .name("hid".to_owned())
            .spawn(move || {
                engine::hid::run_hid(
                    hid_cmd_tx,
                    hid_state,
                    hid_notify,
                    vid,
                    pid,
                    hid_shutdown_flag,
                )
            })
            .expect("failed to spawn hid thread")
    };

    // --- Thread 4: Command processor ---
    // Consumes InputCommand values, acquires write lock, applies command, notifies UI.
    let cmd_state = Arc::clone(&state);
    let cmd_notify = ui_notify_tx.clone();
    let cmd_thread = std::thread::Builder::new()
        .name("cmd-processor".to_owned())
        .spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                {
                    let mut s = cmd_state
                        .write()
                        .expect("cmd-processor: state RwLock poisoned");
                    s.apply_command(cmd);
                }
                // Best-effort notify; if UI is gone we continue until cmd_rx closes.
                let _ = cmd_notify.try_send(());
            }
        })
        .expect("failed to spawn cmd-processor thread");

    // Drop the original ui_notify_tx so the UI thread gets Disconnected when
    // cmd_notify (the only remaining sender) is dropped on cmd-processor exit.
    drop(ui_notify_tx);

    // --- Thread 5: UI (hw-io only) ---
    // run_ui blocks until Ctrl-C; main blocks here waiting for it.
    //
    // BUG-032 fix: clone midi_ctrl_tx before moving it into the UI thread so that
    // the original sender remains alive in main. Without the clone, when run_ui
    // returns, ctrl_rx disconnects and run_midi_out exits its loop — before main
    // gets a chance to send MidiEvent::Stop. Holding the original here ensures the
    // MIDI thread stays alive until we explicitly drop it after sending Stop.
    #[cfg(feature = "hw-io")]
    {
        let ui_state = Arc::clone(&state);
        let ui_cmd_tx = cmd_tx.clone();
        let ui_ctrl_tx = midi_ctrl_tx.clone(); // pass clone; original stays alive in main
        let ui_thread = std::thread::Builder::new()
            .name("ui".to_owned())
            .spawn(move || engine::ui::run_ui(ui_state, ui_cmd_tx, ui_notify_rx, ui_ctrl_tx, midi_log_rx))
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
    // BUG-032: midi_ctrl_tx (original) is still alive here — run_midi_out is
    // still running because ctrl_rx is not yet disconnected. The Stop event
    // is therefore guaranteed to reach the MIDI thread.
    #[cfg(feature = "hw-io")]
    let _ = midi_tx.send(MidiEvent::Stop);

    // Drop midi_ctrl_tx now — all MIDI control changes have been sent. The
    // MIDI thread will see ctrl_rx disconnect and can wind down after Stop.
    #[cfg(feature = "hw-io")]
    drop(midi_ctrl_tx);

    // Drop midi_tx: clock exits when its send() returns Err (receiver dropped).
    drop(midi_tx);

    // hw-io threads: signal HID to stop, then join in dependency order.
    // Join order matters for the cmd_tx deadlock:
    //   1. Signal HID to stop (set shutdown flag)
    //   2. Join hid_thread  — guarantees hid_cmd_tx (clone of cmd_tx) is dropped
    //   3. Drop original cmd_tx  — now ALL cmd_tx senders are gone
    //   4. Join cmd_thread  — receiver sees Disconnected and exits cleanly
    //   5. Join clock_thread, then midi_thread
    #[cfg(feature = "hw-io")]
    {
        hid_shutdown.store(true, Ordering::Relaxed);
        let _ = hid_thread.join();
    }

    // Now all cmd_tx clones are dropped; cmd-processor exits when its receiver sees Disconnected.
    drop(cmd_tx);
    let _ = cmd_thread.join();

    #[cfg(feature = "hw-io")]
    {
        let _ = clock_thread.join();
        let _ = midi_thread.join();
    }
}

// ---------------------------------------------------------------------------
// Smoke tests — no real audio/MIDI devices required.
