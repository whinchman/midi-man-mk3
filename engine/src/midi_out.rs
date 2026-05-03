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
            let found = port_names.iter().enumerate().find(|(_, name)| {
                name.to_lowercase().contains(&f_lower)
            });
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

/// Enumerate available MIDI output ports and return the chosen port name.
///
/// Call this **before** starting the TUI so the prompt has clean access to
/// stdin/stdout. When `filter` is `Some`, does a substring match and returns
/// immediately without prompting. When `filter` is `None` and only one port
/// exists, auto-selects it. When `filter` is `None` and multiple ports exist,
/// prints a numbered list and reads a selection from stdin.
///
/// Returns `None` if no ports are available.
#[cfg(feature = "hw-io")]
pub fn choose_midi_port(filter: Option<&str>) -> Option<String> {
    let output = midir::MidiOutput::new("midi-man-mk3").ok()?;
    let ports = output.ports();
    if ports.is_empty() {
        eprintln!("[midi_out] no ALSA MIDI output ports found");
        return None;
    }

    let names: Vec<String> = ports.iter()
        .map(|p| output.port_name(p).unwrap_or_default())
        .collect();

    if let Some(f) = filter {
        let f_lower = f.to_lowercase();
        let matched = names.iter().find(|n| n.to_lowercase().contains(&f_lower));
        return Some(matched.unwrap_or(&names[0]).clone());
    }

    if names.len() == 1 {
        println!("MIDI output: auto-selected \"{}\"", names[0]);
        return Some(names[0].clone());
    }

    println!("\nAvailable MIDI output ports:");
    for (i, name) in names.iter().enumerate() {
        println!("  [{i}] {name}");
    }
    print!("Select port [0]: ");
    use std::io::Write;
    let _ = std::io::stdout().flush();

    let mut line = String::new();
    let idx = if std::io::stdin().read_line(&mut line).is_ok() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            0
        } else if let Ok(n) = trimmed.parse::<usize>() {
            if n < names.len() { n } else {
                eprintln!("[midi_out] invalid selection — using port 0");
                0
            }
        } else {
            eprintln!("[midi_out] invalid selection — using port 0");
            0
        }
    } else {
        0
    };

    Some(names[idx].clone())
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

    let port_name_strings: Vec<String> = ports.iter()
        .map(|p| output.port_name(p).unwrap_or_default())
        .collect();
    let port_name_refs: Vec<&str> = port_name_strings.iter().map(String::as_str).collect();

    let chosen_idx = select_port_idx(&port_name_refs, port_name)
        .expect("ports is non-empty");

    let port = &ports[chosen_idx];
    let chosen_name = output.port_name(port).unwrap_or_else(|_| "<unknown>".to_owned());

    match output.connect(port, "midi-man-mk3-out") {
        Ok(conn) => {
            println!("[midi_out] connected to: {chosen_name}");
            Some(Box::new(MidirSender { conn: Arc::new(Mutex::new(conn)) }))
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

