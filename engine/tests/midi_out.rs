use engine::midi_out::{dispatch, run_midi_out_with_sender, select_port_idx, MidiSender};
use engine::state::MidiEvent;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
/// has elapsed.
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

/// Send three NoteOn events with distinct durations concurrently.
#[test]
fn concurrent_note_ons_all_note_offs_arrive_in_deadline_order() {
    let (mock, log) = MockSender::new();
    let mut sender = boxed(mock);

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

// --- select_port_idx ---

/// Empty port list returns None.
#[test]
fn select_port_idx_empty_list_returns_none() {
    assert_eq!(select_port_idx(&[], None), None);
    assert_eq!(select_port_idx(&[], Some("anything")), None);
}

/// No filter: always returns index 0.
#[test]
fn select_port_idx_no_filter_returns_first() {
    let ports = ["PortA", "PortB", "PortC"];
    assert_eq!(select_port_idx(&ports, None), Some(0));
}

/// Case-insensitive substring match finds the correct port.
#[test]
fn select_port_idx_case_insensitive_match() {
    let ports = ["FluidSynth virtual port", "USB MIDI", "Timidity"];
    // Lowercase filter matches uppercase port name.
    assert_eq!(select_port_idx(&ports, Some("fluidsynth")), Some(0));
    // Uppercase filter matches mixed-case port name.
    assert_eq!(select_port_idx(&ports, Some("MIDI")), Some(1));
    // Partial substring match.
    assert_eq!(select_port_idx(&ports, Some("imid")), Some(2));
}

/// Exact match (full port name) is found.
#[test]
fn select_port_idx_exact_name_match() {
    let ports = ["Alpha", "Beta", "Gamma"];
    assert_eq!(select_port_idx(&ports, Some("Beta")), Some(1));
}

/// When no port matches the filter, falls back to index 0.
#[test]
fn select_port_idx_no_match_falls_back_to_zero() {
    let ports = ["PortA", "PortB"];
    assert_eq!(select_port_idx(&ports, Some("ZZZ_nonexistent")), Some(0));
}

/// Single-port list with matching filter.
#[test]
fn select_port_idx_single_port_matches() {
    let ports = ["OnlyPort"];
    assert_eq!(select_port_idx(&ports, Some("only")), Some(0));
}

/// Single-port list with non-matching filter still returns 0 (fallback).
#[test]
fn select_port_idx_single_port_no_match_falls_back() {
    let ports = ["OnlyPort"];
    assert_eq!(select_port_idx(&ports, Some("other")), Some(0));
}

/// Filter matches the last port in the list.
#[test]
fn select_port_idx_matches_last_port() {
    let ports = ["Alpha", "Beta", "Gamma", "Delta"];
    assert_eq!(select_port_idx(&ports, Some("delta")), Some(3));
}

// -----------------------------------------------------------------------
// BUG-016 acceptance: select_port_idx fallback edge cases.
//
// choose_midi_port (hw-io path) mirrors select_port_idx logic: when a
// non-None filter matches no port it falls back to index 0 and emits
// an eprintln warning. The following tests lock down every edge case of
// the fallback branch in select_port_idx, which is the pure testable proxy.
// -----------------------------------------------------------------------

/// Empty-string filter is a substring of every port name: always matches
/// index 0 (the first port), not the fallback path.
#[test]
fn select_port_idx_empty_filter_matches_first_port() {
    let ports = ["Alpha", "Beta", "Gamma"];
    // "" is contained in every string, so port 0 matches directly.
    assert_eq!(select_port_idx(&ports, Some("")), Some(0));
}

/// When the filter matches ports at indices 1 and 2, the *first* match
/// (index 1) is returned — not index 0 or 2.
#[test]
fn select_port_idx_filter_matches_multiple_returns_first_match() {
    let ports = ["Alpha", "BetaMIDI", "GammaMIDI"];
    // Both ports 1 and 2 contain "midi" — port 1 must win.
    assert_eq!(select_port_idx(&ports, Some("midi")), Some(1));
}

/// Filter is all uppercase; port names are all lowercase. Case-insensitive
/// match must still succeed.
#[test]
fn select_port_idx_uppercase_filter_matches_lowercase_port() {
    let ports = ["fluidsynth", "timidity", "usb"];
    assert_eq!(select_port_idx(&ports, Some("FLUID")), Some(0));
    assert_eq!(select_port_idx(&ports, Some("TIMIDITY")), Some(1));
}

/// With five ports and a non-matching filter, fallback is still index 0
/// regardless of list length.
#[test]
fn select_port_idx_fallback_with_many_ports_returns_zero() {
    let ports = ["PortA", "PortB", "PortC", "PortD", "PortE"];
    assert_eq!(select_port_idx(&ports, Some("ZZZ_nonexistent")), Some(0));
}

/// Filter matches exactly one port that is not index 0 in a longer list.
#[test]
fn select_port_idx_unique_match_not_at_zero() {
    let ports = ["Alpha", "Beta", "SpecialSynth", "Delta", "Epsilon"];
    assert_eq!(select_port_idx(&ports, Some("special")), Some(2));
}
