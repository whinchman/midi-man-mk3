use engine::midi_out::{
    dispatch, run_midi_out_with_open_fn, run_midi_out_with_sender, select_port_idx, MidiCtrlMsg,
    MidiSender,
};
use engine::state::MidiEvent;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn dummy_log_tx() -> std::sync::mpsc::SyncSender<(bool, String)> {
    std::sync::mpsc::sync_channel::<(bool, String)>(1).0
}

/// Shared byte log accessible from both the main thread and spawned note-off threads.
type Log = Arc<Mutex<Vec<u8>>>;

/// Test double: records all bytes passed to `send_bytes`.
struct MockSender {
    log: Log,
}

impl MockSender {
    fn new() -> (Self, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                log: Arc::clone(&log),
            },
            log,
        )
    }
}

impl MidiSender for MockSender {
    fn send_bytes(&mut self, data: &[u8]) {
        self.log.lock().expect("mock lock").extend_from_slice(data);
    }

    fn try_clone(&self) -> Box<dyn MidiSender> {
        Box::new(MockSender {
            log: Arc::clone(&self.log),
        })
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
        MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
            duration_nanos: 0,
        },
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
        MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
            duration_nanos: 1_000_000,
        }, // 1 ms
    );
    // Before duration elapses: only NoteOn (3 bytes).
    {
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(
            bytes.len(),
            3,
            "only NoteOn should be present before duration elapses"
        );
    }
    // After duration: NoteOff bytes appended by spawned thread.
    thread::sleep(Duration::from_millis(50));
    let bytes = log.lock().expect("lock").clone();
    assert_eq!(
        bytes.len(),
        6,
        "NoteOff should be appended after duration elapses"
    );
    assert_eq!(&bytes[3..6], &[0x80, 60, 0], "NoteOff bytes incorrect");
}

#[test]
fn note_on_channel_bits_masked_correctly() {
    let (mock, log) = MockSender::new();
    let mut sender = boxed(mock);
    dispatch(
        &mut sender,
        MidiEvent::NoteOn {
            channel: 3,
            note: 72,
            velocity: 80,
            duration_nanos: 0,
        },
    );
    thread::sleep(Duration::from_millis(20));
    let bytes = log.lock().expect("lock").clone();
    assert_eq!(
        bytes[0], 0x93,
        "NoteOn status byte for channel 3 should be 0x93"
    );
    assert_eq!(
        bytes[3], 0x83,
        "NoteOff status byte for channel 3 should be 0x83"
    );
}

// --- NoteOff ---

#[test]
fn note_off_sends_correct_bytes() {
    let (mock, log) = MockSender::new();
    let mut sender = boxed(mock);
    dispatch(
        &mut sender,
        MidiEvent::NoteOff {
            channel: 0,
            note: 60,
        },
    );
    let bytes = log.lock().expect("lock").clone();
    assert_eq!(&bytes[..], &[0x80, 60, 0], "NoteOff bytes incorrect");
}

#[test]
fn note_off_channel_bits_masked_correctly() {
    let (mock, log) = MockSender::new();
    let mut sender = boxed(mock);
    dispatch(
        &mut sender,
        MidiEvent::NoteOff {
            channel: 9,
            note: 36,
        },
    );
    let bytes = log.lock().expect("lock").clone();
    assert_eq!(
        bytes[0], 0x89,
        "NoteOff channel 9 status byte should be 0x89"
    );
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
        MidiEvent::NoteOn {
            channel: 0,
            note: 48,
            velocity: 64,
            duration_nanos,
        },
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
    assert_eq!(
        &bytes[3..6],
        &[0x80, 48, 0],
        "NoteOff bytes incorrect after duration"
    );
}

// --- Multiple concurrent NoteOn events ---

/// Send three NoteOn events with distinct durations concurrently.
#[test]
fn concurrent_note_ons_all_note_offs_arrive_in_deadline_order() {
    let (mock, log) = MockSender::new();
    let mut sender = boxed(mock);

    dispatch(
        &mut sender,
        MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
            duration_nanos: 10_000_000,
        }, // 10 ms
    );
    dispatch(
        &mut sender,
        MidiEvent::NoteOn {
            channel: 0,
            note: 62,
            velocity: 100,
            duration_nanos: 30_000_000,
        }, // 30 ms
    );
    dispatch(
        &mut sender,
        MidiEvent::NoteOn {
            channel: 0,
            note: 64,
            velocity: 100,
            duration_nanos: 60_000_000,
        }, // 60 ms
    );

    // All three NoteOn bytes should be present immediately (9 bytes total).
    {
        let bytes = log.lock().expect("lock").clone();
        assert_eq!(
            bytes.len(),
            9,
            "all three NoteOn messages should be sent immediately"
        );
    }

    // Wait for all note-off threads to complete (longest = 60 ms + margin).
    thread::sleep(Duration::from_millis(150));

    let bytes = log.lock().expect("lock").clone();
    // 3 NoteOn (9 bytes) + 3 NoteOff (9 bytes) = 18 bytes total.
    assert_eq!(
        bytes.len(),
        18,
        "all three NoteOff messages should have arrived"
    );

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
    let log_tx = dummy_log_tx();
    assert_eq!(select_port_idx(&[], None, &log_tx), None);
    assert_eq!(select_port_idx(&[], Some("anything"), &log_tx), None);
}

/// No filter: always returns index 0.
#[test]
fn select_port_idx_no_filter_returns_first() {
    let log_tx = dummy_log_tx();
    let ports = ["PortA", "PortB", "PortC"];
    assert_eq!(select_port_idx(&ports, None, &log_tx), Some(0));
}

/// Case-insensitive substring match finds the correct port.
#[test]
fn select_port_idx_case_insensitive_match() {
    let log_tx = dummy_log_tx();
    let ports = ["FluidSynth virtual port", "USB MIDI", "Timidity"];
    // Lowercase filter matches uppercase port name.
    assert_eq!(select_port_idx(&ports, Some("fluidsynth"), &log_tx), Some(0));
    // Uppercase filter matches mixed-case port name.
    assert_eq!(select_port_idx(&ports, Some("MIDI"), &log_tx), Some(1));
    // Partial substring match.
    assert_eq!(select_port_idx(&ports, Some("imid"), &log_tx), Some(2));
}

/// Exact match (full port name) is found.
#[test]
fn select_port_idx_exact_name_match() {
    let log_tx = dummy_log_tx();
    let ports = ["Alpha", "Beta", "Gamma"];
    assert_eq!(select_port_idx(&ports, Some("Beta"), &log_tx), Some(1));
}

/// When no port matches the filter, falls back to index 0.
#[test]
fn select_port_idx_no_match_falls_back_to_zero() {
    let log_tx = dummy_log_tx();
    let ports = ["PortA", "PortB"];
    assert_eq!(select_port_idx(&ports, Some("ZZZ_nonexistent"), &log_tx), Some(0));
}

/// Single-port list with matching filter.
#[test]
fn select_port_idx_single_port_matches() {
    let log_tx = dummy_log_tx();
    let ports = ["OnlyPort"];
    assert_eq!(select_port_idx(&ports, Some("only"), &log_tx), Some(0));
}

/// Single-port list with non-matching filter still returns 0 (fallback).
#[test]
fn select_port_idx_single_port_no_match_falls_back() {
    let log_tx = dummy_log_tx();
    let ports = ["OnlyPort"];
    assert_eq!(select_port_idx(&ports, Some("other"), &log_tx), Some(0));
}

/// Filter matches the last port in the list.
#[test]
fn select_port_idx_matches_last_port() {
    let log_tx = dummy_log_tx();
    let ports = ["Alpha", "Beta", "Gamma", "Delta"];
    assert_eq!(select_port_idx(&ports, Some("delta"), &log_tx), Some(3));
}

// -----------------------------------------------------------------------
// BUG-016 acceptance: select_port_idx fallback edge cases.
//
// select_port_idx is a pure function: when a non-None filter matches no port
// it falls back to index 0 and emits an eprintln warning. The following tests
// lock down every edge case of the fallback branch.
// -----------------------------------------------------------------------

/// Empty-string filter is a substring of every port name: always matches
/// index 0 (the first port), not the fallback path.
#[test]
fn select_port_idx_empty_filter_matches_first_port() {
    let log_tx = dummy_log_tx();
    let ports = ["Alpha", "Beta", "Gamma"];
    // "" is contained in every string, so port 0 matches directly.
    assert_eq!(select_port_idx(&ports, Some(""), &log_tx), Some(0));
}

/// When the filter matches ports at indices 1 and 2, the *first* match
/// (index 1) is returned — not index 0 or 2.
#[test]
fn select_port_idx_filter_matches_multiple_returns_first_match() {
    let log_tx = dummy_log_tx();
    let ports = ["Alpha", "BetaMIDI", "GammaMIDI"];
    // Both ports 1 and 2 contain "midi" — port 1 must win.
    assert_eq!(select_port_idx(&ports, Some("midi"), &log_tx), Some(1));
}

/// Filter is all uppercase; port names are all lowercase. Case-insensitive
/// match must still succeed.
#[test]
fn select_port_idx_uppercase_filter_matches_lowercase_port() {
    let log_tx = dummy_log_tx();
    let ports = ["fluidsynth", "timidity", "usb"];
    assert_eq!(select_port_idx(&ports, Some("FLUID"), &log_tx), Some(0));
    assert_eq!(select_port_idx(&ports, Some("TIMIDITY"), &log_tx), Some(1));
}

/// With five ports and a non-matching filter, fallback is still index 0
/// regardless of list length.
#[test]
fn select_port_idx_fallback_with_many_ports_returns_zero() {
    let log_tx = dummy_log_tx();
    let ports = ["PortA", "PortB", "PortC", "PortD", "PortE"];
    assert_eq!(select_port_idx(&ports, Some("ZZZ_nonexistent"), &log_tx), Some(0));
}

/// Filter matches exactly one port that is not index 0 in a longer list.
#[test]
fn select_port_idx_unique_match_not_at_zero() {
    let log_tx = dummy_log_tx();
    let ports = ["Alpha", "Beta", "SpecialSynth", "Delta", "Epsilon"];
    assert_eq!(select_port_idx(&ports, Some("special"), &log_tx), Some(2));
}

// -----------------------------------------------------------------------
// MidiCtrlMsg / run_midi_out_with_open_fn integration tests
//
// These tests cover the dual-channel polling loop behaviour without
// requiring ALSA hardware (hw-io feature).  All port operations are
// replaced by stub closures.
// -----------------------------------------------------------------------

/// ChangeChannel is a no-op: ctrl_rx receives ChangeChannel and the loop
/// continues processing MIDI events normally without crashing or ignoring
/// subsequent messages.
#[test]
fn change_channel_is_noop_loop_continues() {
    let (midi_tx, midi_rx) = std::sync::mpsc::channel::<MidiEvent>();
    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel::<MidiCtrlMsg>();

    let (mock, log) = MockSender::new();
    let initial_sender: Option<Box<dyn MidiSender>> = Some(boxed(mock));

    // Send a ChangeChannel then a real MIDI event, then disconnect both channels.
    ctrl_tx
        .send(MidiCtrlMsg::ChangeChannel(3))
        .expect("send ChangeChannel");
    midi_tx.send(MidiEvent::Start).expect("send Start");
    drop(ctrl_tx);
    drop(midi_tx);

    run_midi_out_with_open_fn(midi_rx, ctrl_rx, initial_sender, dummy_log_tx(), |_| None);

    // The Start event must still have been dispatched — sender was not replaced.
    let bytes = log.lock().expect("lock").clone();
    assert_eq!(&bytes[..], &[0xFA], "Start must be dispatched after ChangeChannel no-op");
}

/// ChangePort with a name that matches no port: open_port_fn returns None,
/// sender becomes None, loop does not crash.
#[test]
fn change_port_no_match_sender_becomes_none_no_crash() {
    let (midi_tx, midi_rx) = std::sync::mpsc::channel::<MidiEvent>();
    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel::<MidiCtrlMsg>();

    // Start with a live sender.
    let (mock, _log) = MockSender::new();
    let initial_sender: Option<Box<dyn MidiSender>> = Some(boxed(mock));

    // ChangePort whose name matches nothing — stub returns None.
    ctrl_tx
        .send(MidiCtrlMsg::ChangePort("no-such-port".to_owned()))
        .expect("send ChangePort");
    // Send a MIDI event after the failed port-change; it should be silently dropped.
    midi_tx.send(MidiEvent::Stop).expect("send Stop");
    drop(ctrl_tx);
    drop(midi_tx);

    // If the loop panics this test fails via join().
    let handle = std::thread::spawn(move || {
        run_midi_out_with_open_fn(midi_rx, ctrl_rx, initial_sender, dummy_log_tx(), |_name| {
            None // port not found
        });
    });

    handle.join().expect("loop must not panic on unmatched ChangePort");
}

/// Multiple ChangePort messages in sequence: all three are processed in order
/// and a MIDI event dispatched after them goes through the last active sender.
///
/// Design: the three ChangePort messages are pre-queued on ctrl_rx. A MIDI
/// event is sent from a separate thread after 200 ms, giving the loop time
/// to drain all ctrl messages first (each loop iteration has a 50 ms
/// recv_timeout). ctrl_tx is kept alive in the helper thread until after the
/// MIDI event is sent, preventing premature Disconnected on ctrl_rx.
#[test]
fn multiple_change_port_last_port_is_active() {
    use std::sync::{Arc, Mutex};

    let (midi_tx, midi_rx) = std::sync::mpsc::channel::<MidiEvent>();
    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel::<MidiCtrlMsg>();

    // Track which port names were requested in order.
    let ports_opened: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let ports_opened_clone = Arc::clone(&ports_opened);

    // Shared log — all TrackingSenders write here.
    let shared_log: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let shared_log_clone = Arc::clone(&shared_log);

    // Pre-queue all three ChangePort messages.
    ctrl_tx
        .send(MidiCtrlMsg::ChangePort("port-alpha".to_owned()))
        .expect("send first ChangePort");
    ctrl_tx
        .send(MidiCtrlMsg::ChangePort("port-beta".to_owned()))
        .expect("send second ChangePort");
    ctrl_tx
        .send(MidiCtrlMsg::ChangePort("port-gamma".to_owned()))
        .expect("send third ChangePort");

    // A helper thread holds both ctrl_tx and midi_tx alive. After a delay long
    // enough for the loop to drain the three queued ChangePort messages, it sends
    // a ChangeChannel no-op (so the loop does one final recv_timeout pass), then
    // the Continue MIDI event, then drops both senders so the loop exits.
    //
    // Ordering:
    //   1. Loop drains ChangePort alpha/beta/gamma (3 × 50 ms recv_timeout passes).
    //   2. Thread wakes up, sends ChangeChannel(0) on ctrl_tx.
    //   3. Loop: try_recv -> ChangeChannel(0) [no-op], recv_timeout -> Continue -> dispatch.
    //   4. Thread drops ctrl_tx and midi_tx clone.
    //   5. Loop: try_recv -> Disconnected -> break.
    let midi_tx_clone = midi_tx.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        // Send a no-op ctrl message so the loop will do one more recv_timeout
        // pass, giving it a chance to pick up the Continue MIDI event.
        let _ = ctrl_tx.send(MidiCtrlMsg::ChangeChannel(0));
        let _ = midi_tx_clone.send(MidiEvent::Continue);
        // Dropping ctrl_tx here causes Disconnected on the iteration AFTER Continue.
        drop(ctrl_tx);
        // midi_tx_clone dropped when thread ends.
    });

    drop(midi_tx); // only the thread clone keeps midi_rx live

    run_midi_out_with_open_fn(midi_rx, ctrl_rx, None, dummy_log_tx(), move |name: &str| {
        ports_opened_clone
            .lock()
            .expect("ports_opened lock")
            .push(name.to_owned());

        let log_inner = Arc::clone(&shared_log_clone);
        Some(Box::new(TrackingSender {
            log: log_inner,
            name: name.to_owned(),
        }))
    });

    let opened = ports_opened.lock().expect("lock").clone();
    assert_eq!(
        opened,
        vec!["port-alpha", "port-beta", "port-gamma"],
        "open_port_fn must be called for each ChangePort in order"
    );

    // Continue (0xFB) must have been dispatched through whichever sender was
    // active at that point — all three share the same log Arc.
    let bytes = shared_log.lock().expect("lock").clone();
    assert_eq!(&bytes[..], &[0xFB], "Continue must be dispatched after all port changes");
}

/// MIDI events sent while sender is None are silently dropped — no panic.
#[test]
fn events_dropped_silently_when_no_sender() {
    let (midi_tx, midi_rx) = std::sync::mpsc::channel::<MidiEvent>();
    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel::<MidiCtrlMsg>();

    // Send several MIDI events with no sender open.
    midi_tx.send(MidiEvent::Start).expect("send Start");
    midi_tx.send(MidiEvent::Stop).expect("send Stop");
    midi_tx.send(MidiEvent::Continue).expect("send Continue");
    drop(ctrl_tx);
    drop(midi_tx);

    // initial_sender = None; open_port_fn never called (no ChangePort msgs).
    let handle = std::thread::spawn(move || {
        run_midi_out_with_open_fn(midi_rx, ctrl_rx, None, dummy_log_tx(), |_| None);
    });

    handle
        .join()
        .expect("loop must not panic when sender is None and events arrive");
}

// ── Helper sender that writes to an externally-visible log ───────────────

/// A `MidiSender` implementation that appends bytes to a shared log.
/// Used by the multiple-ChangePort test to verify which sender is active.
struct TrackingSender {
    log: Arc<Mutex<Vec<u8>>>,
    #[allow(dead_code)]
    name: String,
}

impl MidiSender for TrackingSender {
    fn send_bytes(&mut self, data: &[u8]) {
        self.log
            .lock()
            .expect("TrackingSender lock")
            .extend_from_slice(data);
    }

    fn try_clone(&self) -> Box<dyn MidiSender> {
        Box::new(TrackingSender {
            log: Arc::clone(&self.log),
            name: self.name.clone(),
        })
    }
}
