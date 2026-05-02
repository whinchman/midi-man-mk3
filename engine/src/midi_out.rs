// Build note: midir requires alsa-lib-devel (libasound2-dev) installed on the
// build host. On this host the pkg-config metadata is available via:
//   PKG_CONFIG_PATH=/tmp/alsa-pkg cargo build -p engine
// The .cargo/config.toml [env] section sets this automatically.

//! MIDI output thread — receives `MidiEvent` values and dispatches raw MIDI
//! bytes over ALSA via `midir`. Note-off scheduling is owned here.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::state::MidiEvent;

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
struct MidirSender {
    conn: Arc<Mutex<midir::MidiOutputConnection>>,
}

impl MidiSender for MidirSender {
    fn send_bytes(&mut self, data: &[u8]) {
        let mut guard = self.conn.lock().expect("MidiOutputConnection mutex poisoned");
        if let Err(e) = guard.send(data) {
            eprintln!("[midi_out] send error: {e}");
        }
    }

    fn try_clone(&self) -> Box<dyn MidiSender> {
        Box::new(MidirSender { conn: Arc::clone(&self.conn) })
    }
}

/// Open the first available ALSA MIDI output port and return a boxed sender.
///
/// Returns `None` if no ports are available or if opening the port fails.
fn open_first_port() -> Option<Box<dyn MidiSender>> {
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

    let port = &ports[0];
    let port_name = output.port_name(port).unwrap_or_else(|_| "<unknown>".to_owned());
    println!("[midi_out] opening port: {port_name}");

    match output.connect(port, "midi-man-mk3-out") {
        Ok(conn) => {
            println!("[midi_out] connected to port: {port_name}");
            Some(Box::new(MidirSender { conn: Arc::new(Mutex::new(conn)) }))
        }
        Err(e) => {
            eprintln!("[midi_out] failed to connect to port '{port_name}': {e}");
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
        MidiEvent::NoteOn { channel, note, velocity, duration_nanos } => {
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
/// Opens the first available ALSA MIDI output port via `midir`, then loops
/// dispatching `MidiEvent` values received on `rx`. If no ports are available,
/// logs an error and returns without panicking. Exits when `rx` is
/// disconnected (sender dropped).
pub fn run_midi_out(rx: Receiver<MidiEvent>) {
    let mut sender = match open_first_port() {
        Some(s) => s,
        None => return,
    };
    run_midi_out_with_sender(rx, &mut sender);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Shared byte log accessible from both the main thread and spawned note-off threads.
    type Log = Arc<Mutex<Vec<u8>>>;

    /// Test double: records all bytes passed to `send_bytes`.
    struct MockSender {
        log: Log,
    }

    impl MockSender {
        fn new() -> (Self, Log) {
            let log: Log = Arc::new(Mutex::new(Vec::new()));
            (Self { log: Arc::clone(&log) }, log)
        }
    }

    impl MidiSender for MockSender {
        fn send_bytes(&mut self, data: &[u8]) {
            self.log.lock().expect("mock lock").extend_from_slice(data);
        }

        fn try_clone(&self) -> Box<dyn MidiSender> {
            Box::new(MockSender { log: Arc::clone(&self.log) })
        }
    }

    fn boxed(sender: MockSender) -> Box<dyn MidiSender> {
        Box::new(sender)
    }

    // --- NoteOn ---

    #[test]
    fn note_on_sends_correct_bytes_immediately() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(
            &mut sender,
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100, duration_nanos: 0 },
        );
        // Give the spawned note-off thread (duration_nanos=0) a moment to flush.
        thread::sleep(Duration::from_millis(20));
        let bytes = log.lock().expect("lock").clone();
        // First three bytes must be the NoteOn message.
        assert_eq!(&bytes[..3], &[0x90, 60, 100], "NoteOn bytes incorrect");
    }

    #[test]
    fn note_on_spawns_note_off_after_duration() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(
            &mut sender,
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100, duration_nanos: 1_000_000 }, // 1 ms
        );
        // Before duration elapses: only NoteOn (3 bytes).
        {
            let bytes = log.lock().expect("lock").clone();
            assert_eq!(bytes.len(), 3, "only NoteOn should be present before duration elapses");
        }
        // After duration: NoteOff bytes appended by spawned thread.
        thread::sleep(Duration::from_millis(50));
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(bytes.len(), 6, "NoteOff should be appended after duration elapses");
        assert_eq!(&bytes[3..6], &[0x80, 60, 0], "NoteOff bytes incorrect");
    }

    #[test]
    fn note_on_channel_bits_masked_correctly() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(
            &mut sender,
            MidiEvent::NoteOn { channel: 3, note: 72, velocity: 80, duration_nanos: 0 },
        );
        thread::sleep(Duration::from_millis(20));
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(bytes[0], 0x93, "NoteOn status byte for channel 3 should be 0x93");
        assert_eq!(bytes[3], 0x83, "NoteOff status byte for channel 3 should be 0x83");
    }

    // --- NoteOff ---

    #[test]
    fn note_off_sends_correct_bytes() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(&mut sender, MidiEvent::NoteOff { channel: 0, note: 60 });
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(&bytes[..], &[0x80, 60, 0], "NoteOff bytes incorrect");
    }

    #[test]
    fn note_off_channel_bits_masked_correctly() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(&mut sender, MidiEvent::NoteOff { channel: 9, note: 36 });
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(bytes[0], 0x89, "NoteOff channel 9 status byte should be 0x89");
    }

    // --- Start ---

    #[test]
    fn start_sends_0xfa() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(&mut sender, MidiEvent::Start);
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(&bytes[..], &[0xFA], "Start byte should be 0xFA");
    }

    // --- Stop ---

    #[test]
    fn stop_sends_0xfc() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(&mut sender, MidiEvent::Stop);
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(&bytes[..], &[0xFC], "Stop byte should be 0xFC");
    }

    // --- Continue ---

    #[test]
    fn continue_sends_0xfb() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        dispatch(&mut sender, MidiEvent::Continue);
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(&bytes[..], &[0xFB], "Continue byte should be 0xFB");
    }

    // --- Channel disconnect ---

    #[test]
    fn loop_exits_when_channel_closes() {
        let (mock, _log) = MockSender::new();
        let mut sender = boxed(mock);
        let (tx, rx) = std::sync::mpsc::channel::<MidiEvent>();
        // Drop the sender immediately so the channel closes.
        drop(tx);
        // run_midi_out_with_sender should return without blocking.
        run_midi_out_with_sender(rx, &mut sender);
    }

    // --- Round-trip through channel ---

    #[test]
    fn channel_delivers_events_to_loop() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        let (tx, rx) = std::sync::mpsc::channel::<MidiEvent>();

        tx.send(MidiEvent::Start).expect("send Start");
        tx.send(MidiEvent::Stop).expect("send Stop");
        drop(tx);

        run_midi_out_with_sender(rx, &mut sender);

        let bytes = log.lock().expect("lock").clone();
        assert_eq!(&bytes[..], &[0xFA, 0xFC], "Start then Stop through channel");
    }
}
