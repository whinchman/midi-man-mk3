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
        let mut guard = self.conn.lock().expect("MidiOutputConnection mutex poisoned");
        if let Err(e) = guard.send(data) {
            eprintln!("[midi_out] send error: {e}");
        }
    }

    fn try_clone(&self) -> Box<dyn MidiSender> {
        Box::new(MidirSender { conn: Arc::clone(&self.conn) })
    }
}

/// Open an ALSA MIDI output port.
///
/// When `port_name` is `Some(filter)`, searches for a port whose name contains
/// `filter` (case-insensitive substring match).  If no matching port is found,
/// logs a warning and falls back to the first available port.  When `port_name`
/// is `None`, opens the first available port.
///
/// Returns `None` if no ports are available or if opening the chosen port fails.
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

    // Determine which port to open.
    let chosen_idx = if let Some(filter) = port_name {
        let filter_lower = filter.to_lowercase();
        let found = ports.iter().enumerate().find(|(_, p)| {
            output.port_name(p)
                .unwrap_or_default()
                .to_lowercase()
                .contains(&filter_lower)
        });
        match found {
            Some((idx, _)) => idx,
            None => {
                eprintln!(
                    "[midi_out] no port matching '{}' found — falling back to first port",
                    filter
                );
                0
            }
        }
    } else {
        0
    };

    let port = &ports[chosen_idx];
    let chosen_name = output.port_name(port).unwrap_or_else(|_| "<unknown>".to_owned());
    println!("[midi_out] opening port: {chosen_name}");

    match output.connect(port, "midi-man-mk3-out") {
        Ok(conn) => {
            println!("[midi_out] connected to port: {chosen_name}");
            Some(Box::new(MidirSender { conn: Arc::new(Mutex::new(conn)) }))
        }
        Err(e) => {
            eprintln!("[midi_out] failed to connect to port '{chosen_name}': {e}");
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
/// When `port_name` is `Some(filter)`, searches for a port whose name contains
/// `filter` (case-insensitive substring match); falls back to the first port if
/// no match is found.  When `None`, opens the first available port.
///
/// Loops dispatching `MidiEvent` values received on `rx`. If no ports are
/// available, logs an error and returns without panicking. Exits when `rx` is
/// disconnected (sender dropped).
///
/// Requires the `hw-io` feature (ALSA/midir).
#[cfg(feature = "hw-io")]
pub fn run_midi_out(rx: Receiver<MidiEvent>, port_name: Option<String>) {
    let mut sender = match open_port(port_name.as_deref()) {
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

    // --- NoteOff timing boundary ---

    /// Verify that the spawned note-off thread does NOT fire before `duration_nanos`
    /// has elapsed. We use a 50 ms duration and sample the log after only 10 ms —
    /// only the NoteOn bytes should be present. After 100 ms the NoteOff bytes
    /// must also be present.
    #[test]
    fn note_off_not_sent_before_duration_elapses() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);
        let duration_nanos: u64 = 50_000_000; // 50 ms
        dispatch(
            &mut sender,
            MidiEvent::NoteOn { channel: 0, note: 48, velocity: 64, duration_nanos },
        );
        // Sample before duration: only NoteOn (3 bytes).
        thread::sleep(Duration::from_millis(10));
        {
            let bytes = log.lock().expect("lock").clone();
            assert_eq!(
                bytes.len(),
                3,
                "NoteOff must not be sent before duration_nanos ({duration_nanos} ns) elapses"
            );
        }
        // Sample after duration: NoteOff (3 more bytes) must now be present.
        thread::sleep(Duration::from_millis(100));
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(
            bytes.len(),
            6,
            "NoteOff must be sent after duration_nanos ({duration_nanos} ns) elapses"
        );
        assert_eq!(&bytes[3..6], &[0x80, 48, 0], "NoteOff bytes incorrect after duration");
    }

    // --- Multiple concurrent NoteOn events ---

    /// Send three NoteOn events with distinct durations (10 ms, 30 ms, 60 ms)
    /// concurrently. After all durations have elapsed every NoteOff must be
    /// present and must appear in deadline order (shortest duration first).
    #[test]
    fn concurrent_note_ons_all_note_offs_arrive_in_deadline_order() {
        let (mock, log) = MockSender::new();
        let mut sender = boxed(mock);

        // Dispatch three NoteOn events back-to-back before any duration elapses.
        dispatch(
            &mut sender,
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100, duration_nanos: 10_000_000 }, // 10 ms
        );
        dispatch(
            &mut sender,
            MidiEvent::NoteOn { channel: 0, note: 62, velocity: 100, duration_nanos: 30_000_000 }, // 30 ms
        );
        dispatch(
            &mut sender,
            MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100, duration_nanos: 60_000_000 }, // 60 ms
        );

        // All three NoteOn bytes should be present immediately (9 bytes total).
        {
            let bytes = log.lock().expect("lock").clone();
            assert_eq!(bytes.len(), 9, "all three NoteOn messages should be sent immediately");
        }

        // Wait for all note-off threads to complete (longest = 60 ms + margin).
        thread::sleep(Duration::from_millis(150));

        let bytes = log.lock().expect("lock").clone();
        // 3 NoteOn (9 bytes) + 3 NoteOff (9 bytes) = 18 bytes total.
        assert_eq!(bytes.len(), 18, "all three NoteOff messages should have arrived");

        // Collect NoteOff bytes (positions 9, 12, 15).
        let off0 = &bytes[9..12];
        let off1 = &bytes[12..15];
        let off2 = &bytes[15..18];

        // NoteOff status byte is 0x80 for channel 0.
        assert_eq!(off0[0], 0x80, "first NoteOff status byte");
        assert_eq!(off1[0], 0x80, "second NoteOff status byte");
        assert_eq!(off2[0], 0x80, "third NoteOff status byte");

        // Notes must arrive in ascending deadline order: 60, 62, 64.
        assert_eq!(off0[1], 60, "first NoteOff (shortest duration) note");
        assert_eq!(off1[1], 62, "second NoteOff note");
        assert_eq!(off2[1], 64, "third NoteOff (longest duration) note");

        // Velocity byte must be 0 for all NoteOff messages.
        assert_eq!(off0[2], 0);
        assert_eq!(off1[2], 0);
        assert_eq!(off2[2], 0);
    }

    // --- run_midi_out with no ALSA ports ---

    /// Verify that `run_midi_out` returns without panicking when the channel is
    /// already closed and (on this CI host) no ALSA ports are available.
    /// Even if a port were available, dropping the sender before calling
    /// `run_midi_out` causes the receive loop to exit immediately.
    ///
    /// Requires the hw-io feature because `run_midi_out` calls `open_port`
    /// which uses `midir`. Without hw-io, the no-ports path is tested implicitly
    /// by `loop_exits_when_channel_closes` via `run_midi_out_with_sender`.
    #[cfg(feature = "hw-io")]
    #[test]
    fn run_midi_out_no_ports_does_not_panic() {
        let (tx, rx) = std::sync::mpsc::channel::<MidiEvent>();
        // Drop the sender so that if the function opens a port and enters the
        // receive loop it exits immediately rather than blocking.
        drop(tx);
        // Must not panic regardless of whether ALSA ports are present.
        run_midi_out(rx, None);
    }
}
