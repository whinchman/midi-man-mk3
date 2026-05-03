// Build requirement: midir requires alsa-lib-devel (or libasound2-dev on Debian).
// On Fedora: sudo dnf install alsa-lib-devel
// On Ubuntu: sudo apt install libasound2-dev
// For development without ALSA: build without the hw-io feature flag.

//! MIDI output thread — receives `MidiEvent` values and dispatches raw MIDI
//! bytes over ALSA via `midir`. Note-off scheduling is owned here.

use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

#[cfg(feature = "hw-io")]
use std::sync::{Arc, Mutex};

use crate::state::MidiEvent;

/// Runtime control messages for the MIDI output thread.
#[derive(Debug)]
pub enum MidiCtrlMsg {
    /// Switch to the port whose name contains this substring (case-insensitive).
    ChangePort(String),
    /// Set the MIDI channel (1-indexed; the actual channel byte update happens
    /// via InputCommand::ChannelSet in the state processor).
    ChangeChannel(u8),
}

/// Abstraction over a MIDI output connection.
///
/// `try_clone` returns a new handle that shares the same underlying connection
/// so that a spawned note-off thread can call `send_bytes` after a delay.
pub trait MidiSender: Send + 'static {
    /// Send raw MIDI bytes. Errors are logged but not propagated.
    fn send_bytes(&mut self, data: &[u8]);
    /// Return a cloneable handle to the same underlying connection.
    fn try_clone(&self) -> Box<dyn MidiSender>;
}

/// Production implementation wrapping `midir::MidiOutputConnection` behind an
/// `Arc<Mutex<…>>` so spawned note-off threads can share it.
#[cfg(feature = "hw-io")]
struct MidirSender {
    conn: Arc<Mutex<midir::MidiOutputConnection>>,
}

#[cfg(feature = "hw-io")]
impl MidiSender for MidirSender {
    fn send_bytes(&mut self, data: &[u8]) {
        let mut guard = self
            .conn
            .lock()
            .expect("MidiOutputConnection mutex poisoned");
        if let Err(e) = guard.send(data) {
            eprintln!("[midi_out] send error: {e}");
        }
    }

    fn try_clone(&self) -> Box<dyn MidiSender> {
        Box::new(MidirSender {
            conn: Arc::clone(&self.conn),
        })
    }
}

/// Choose which port index to open given a list of port names and an optional
/// filter string.
///
/// Returns `None` when `port_names` is empty (caller should disable MIDI).
/// When `filter` is `Some(f)`, returns the index of the first port whose name
/// contains `f` (case-insensitive substring match), or `Some(0)` when no port
/// matches (falling back to the first port with a logged warning).
/// When `filter` is `None`, returns `Some(0)`.
pub fn select_port_idx(port_names: &[&str], filter: Option<&str>) -> Option<usize> {
    if port_names.is_empty() {
        return None;
    }
    match filter {
        None => Some(0),
        Some(f) => {
            let f_lower = f.to_lowercase();
            let found = port_names
                .iter()
                .enumerate()
                .find(|(_, name)| name.to_lowercase().contains(&f_lower));
            match found {
                Some((idx, _)) => Some(idx),
                None => {
                    eprintln!(
                        "[midi_out] no port matching '{}' found — falling back to first port",
                        f
                    );
                    Some(0)
                }
            }
        }
    }
}

/// Open an ALSA MIDI output port by exact or substring name match.
///
/// Returns `None` if no ports are available or the port cannot be opened.
#[cfg(feature = "hw-io")]
fn open_port(port_name: Option<&str>) -> Option<Box<dyn MidiSender>> {
    let output = match midir::MidiOutput::new("midi-man-mk3") {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[midi_out] failed to create MidiOutput: {e}");
            return None;
        }
    };

    let ports = output.ports();
    if ports.is_empty() {
        eprintln!("[midi_out] no ALSA MIDI output ports available — MIDI output disabled");
        return None;
    }

    let port_name_strings: Vec<String> = ports
        .iter()
        .map(|p| output.port_name(p).unwrap_or_default())
        .collect();
    let port_name_refs: Vec<&str> = port_name_strings.iter().map(String::as_str).collect();

    let chosen_idx = select_port_idx(&port_name_refs, port_name).expect("ports is non-empty");

    let port = &ports[chosen_idx];
    let chosen_name = output
        .port_name(port)
        .unwrap_or_else(|_| "<unknown>".to_owned());

    match output.connect(port, "midi-man-mk3-out") {
        Ok(conn) => {
            println!("[midi_out] connected to: {chosen_name}");
            Some(Box::new(MidirSender {
                conn: Arc::new(Mutex::new(conn)),
            }))
        }
        Err(e) => {
            eprintln!("[midi_out] failed to connect to '{chosen_name}': {e}");
            None
        }
    }
}

/// Dispatch a single `MidiEvent` through `sender`.
///
/// All MIDI messages are assembled as fixed-size stack arrays — no heap
/// allocation on the send path. For `NoteOn`, a note-off is scheduled by
/// spawning a short-lived thread that sleeps `duration_nanos` then sends the
/// note-off bytes via a cloned sender handle.
pub fn dispatch(sender: &mut Box<dyn MidiSender>, event: MidiEvent) {
    match event {
        MidiEvent::NoteOn {
            channel,
            note,
            velocity,
            duration_nanos,
        } => {
            // Send note-on immediately — stack array, no heap.
            let on_msg: [u8; 3] = [0x90 | channel, note, velocity];
            sender.send_bytes(&on_msg);

            // Spawn a short-lived thread that sleeps then sends note-off.
            let mut off_sender = sender.try_clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_nanos(duration_nanos));
                let off_msg: [u8; 3] = [0x80 | channel, note, 0];
                off_sender.send_bytes(&off_msg);
            });
        }

        MidiEvent::NoteOff { channel, note } => {
            let msg: [u8; 3] = [0x80 | channel, note, 0];
            sender.send_bytes(&msg);
        }

        MidiEvent::Start => {
            let msg: [u8; 1] = [0xFA];
            sender.send_bytes(&msg);
        }

        MidiEvent::Stop => {
            let msg: [u8; 1] = [0xFC];
            sender.send_bytes(&msg);
        }

        MidiEvent::Continue => {
            let msg: [u8; 1] = [0xFB];
            sender.send_bytes(&msg);
        }
    }
}

/// Run the MIDI output thread using a real ALSA port.
///
/// Accepts a `ctrl_rx` channel for runtime control messages (port/channel
/// changes) in addition to the `rx` channel for MIDI events.
///
/// When `port_name` is `Some(filter)`, searches for a port whose name contains
/// `filter` (case-insensitive substring match); falls back to the first port if
/// no match is found.  When `None`, opens the first available port.
///
/// Polls `ctrl_rx` non-blockingly before each MIDI recv (50 ms timeout). Exits
/// when `ctrl_rx` is disconnected or `rx` is disconnected.
///
/// Requires the `hw-io` feature (ALSA/midir).
#[cfg(feature = "hw-io")]
pub fn run_midi_out(
    rx: Receiver<MidiEvent>,
    ctrl_rx: Receiver<MidiCtrlMsg>,
    port_name: Option<String>,
) {
    use std::sync::mpsc::RecvTimeoutError;
    use std::sync::mpsc::TryRecvError;

    let mut sender = open_port(port_name.as_deref());

    loop {
        // Non-blocking ctrl check first.
        match ctrl_rx.try_recv() {
            Ok(MidiCtrlMsg::ChangePort(name)) => {
                sender = open_port(Some(&name));
            }
            Ok(MidiCtrlMsg::ChangeChannel(_)) => {
                // Channel is applied at the MidiEvent level via state.midi_channel.
                // This message is informational only for the midi_out thread.
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
        // Blocking recv with 50ms timeout on MIDI events.
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                if let Some(ref mut s) = sender {
                    dispatch(s, event);
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Run the MIDI output loop with an injected sender (testable entry point).
///
/// Loops on `rx` until the channel closes, dispatching each event via
/// `dispatch()`.
pub fn run_midi_out_with_sender(rx: Receiver<MidiEvent>, sender: &mut Box<dyn MidiSender>) {
    while let Ok(event) = rx.recv() {
        dispatch(sender, event);
    }
    // Loop exits when channel closes (sender dropped).
}

/// Run the dual-channel polling loop with an injected port-open function.
///
/// This is a testable entry point that mirrors `run_midi_out` without requiring
/// the `hw-io` feature. The `open_port_fn` closure is called when a
/// `ChangePort` message arrives; returning `None` disables MIDI output.
///
/// Exits when `ctrl_rx` is disconnected or `midi_rx` is disconnected.
pub fn run_midi_out_with_open_fn<F>(
    midi_rx: std::sync::mpsc::Receiver<MidiEvent>,
    ctrl_rx: std::sync::mpsc::Receiver<MidiCtrlMsg>,
    initial_sender: Option<Box<dyn MidiSender>>,
    mut open_port_fn: F,
) where
    F: FnMut(&str) -> Option<Box<dyn MidiSender>>,
{
    use std::sync::mpsc::RecvTimeoutError;
    use std::sync::mpsc::TryRecvError;

    let mut sender = initial_sender;

    loop {
        match ctrl_rx.try_recv() {
            Ok(MidiCtrlMsg::ChangePort(name)) => {
                sender = open_port_fn(&name);
            }
            Ok(MidiCtrlMsg::ChangeChannel(_)) => {
                // Channel is applied at the MidiEvent level — no-op here.
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
        match midi_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                if let Some(ref mut s) = sender {
                    dispatch(s, event);
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    /// A mock `MidiSender` that records all bytes sent.
    struct MockSender {
        sent: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl MockSender {
        fn new() -> (Self, Arc<Mutex<Vec<Vec<u8>>>>) {
            let sent = Arc::new(Mutex::new(Vec::new()));
            (
                MockSender {
                    sent: Arc::clone(&sent),
                },
                sent,
            )
        }
    }

    impl MidiSender for MockSender {
        fn send_bytes(&mut self, data: &[u8]) {
            self.sent
                .lock()
                .expect("MockSender mutex poisoned")
                .push(data.to_vec());
        }

        fn try_clone(&self) -> Box<dyn MidiSender> {
            Box::new(MockSender {
                sent: Arc::clone(&self.sent),
            })
        }
    }

    // ── select_port_idx ──────────────────────────────────────────────────────

    #[test]
    fn select_port_idx_returns_none_for_empty() {
        assert_eq!(select_port_idx(&[], None), None);
        assert_eq!(select_port_idx(&[], Some("foo")), None);
    }

    #[test]
    fn select_port_idx_returns_zero_with_no_filter() {
        let ports = ["Port A", "Port B"];
        assert_eq!(select_port_idx(&ports, None), Some(0));
    }

    #[test]
    fn select_port_idx_matches_case_insensitively() {
        let ports = ["ALSA Port 0", "USB Synth"];
        assert_eq!(select_port_idx(&ports, Some("synth")), Some(1));
    }

    #[test]
    fn select_port_idx_falls_back_to_zero_on_no_match() {
        let ports = ["Port A", "Port B"];
        assert_eq!(select_port_idx(&ports, Some("zzz")), Some(0));
    }

    // ── dispatch ─────────────────────────────────────────────────────────────

    #[test]
    fn dispatch_note_on_sends_on_bytes() {
        let (sender, sent) = MockSender::new();
        let mut boxed: Box<dyn MidiSender> = Box::new(sender);
        dispatch(
            &mut boxed,
            MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
                duration_nanos: 1,
            },
        );
        let guard = sent.lock().unwrap();
        assert_eq!(guard[0], vec![0x90, 60, 100]);
    }

    #[test]
    fn dispatch_start_sends_fa() {
        let (sender, sent) = MockSender::new();
        let mut boxed: Box<dyn MidiSender> = Box::new(sender);
        dispatch(&mut boxed, MidiEvent::Start);
        assert_eq!(sent.lock().unwrap()[0], vec![0xFA]);
    }

    // ── run_midi_out_with_sender ──────────────────────────────────────────────

    #[test]
    fn run_with_sender_dispatches_events_and_exits_on_disconnect() {
        let (tx, rx) = mpsc::channel::<MidiEvent>();
        let (sender, sent) = MockSender::new();
        let mut boxed: Box<dyn MidiSender> = Box::new(sender);

        tx.send(MidiEvent::Start).unwrap();
        drop(tx); // disconnect

        run_midi_out_with_sender(rx, &mut boxed);

        assert_eq!(sent.lock().unwrap()[0], vec![0xFA]);
    }

    // ── MidiCtrlMsg tests ─────────────────────────────────────────────────────

    /// Verify that when ctrl_rx is disconnected the run loop exits cleanly.
    #[test]
    fn ctrl_rx_disconnect_exits_loop() {
        let (midi_tx, midi_rx) = mpsc::channel::<MidiEvent>();
        let (ctrl_tx, ctrl_rx) = mpsc::channel::<MidiCtrlMsg>();

        // Drop ctrl_tx immediately — ctrl_rx will be Disconnected on first try_recv.
        drop(ctrl_tx);
        drop(midi_tx);

        // The thread should finish without hanging or panicking.
        let handle = std::thread::spawn(move || {
            run_midi_out_with_open_fn(midi_rx, ctrl_rx, None, |_| None);
        });

        handle
            .join()
            .expect("loop thread panicked instead of exiting cleanly");
    }

    /// Verify that a ChangePort message is processed and open_port_fn is called.
    #[test]
    fn ctrl_rx_port_change_swaps_sender() {
        let (midi_tx, midi_rx) = mpsc::channel::<MidiEvent>();
        let (ctrl_tx, ctrl_rx) = mpsc::channel::<MidiCtrlMsg>();

        // Send a ChangePort message then drop ctrl_tx to trigger Disconnected.
        ctrl_tx
            .send(MidiCtrlMsg::ChangePort("test-port".to_owned()))
            .unwrap();
        drop(ctrl_tx);
        drop(midi_tx);

        let port_change_received = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&port_change_received);

        let handle = std::thread::spawn(move || {
            run_midi_out_with_open_fn(midi_rx, ctrl_rx, None, |_name| {
                *flag.lock().unwrap() = true;
                None // no real ALSA hardware in tests
            });
        });

        handle
            .join()
            .expect("loop thread panicked instead of exiting cleanly");
        assert!(
            *port_change_received.lock().unwrap(),
            "ChangePort message was not processed by open_port_fn"
        );
    }
}
