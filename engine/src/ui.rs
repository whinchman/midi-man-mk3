//! Terminal UI event loop — ratatui render loop + crossterm keyboard handling.
//!
//! This module requires the `hw-io` feature because it uses the crossterm
//! backend which needs a real terminal.  Render logic lives in `ui_render`
//! (always compiled) so it can be unit-tested with `TestBackend`.
//!
//! # Threading model
//!
//! `run_ui` blocks the calling thread.  It is designed to be the main thread's
//! blocking point (Step 9 joins on it).  When the function returns, the process
//! should exit.
//!
//! # Exit behaviour
//!
//! On Ctrl-C (or when the notify channel closes), `run_ui` exits cleanly.
//! It does *not* send a `MidiEvent::Stop` — the caller (Step 9 / main.rs) is
//! responsible for stopping the sequencer and joining the clock/MIDI threads
//! after `run_ui` returns.  This keeps the UI thread free of back-channels into
//! the clock domain.
//!
//! # Render timing
//!
//! The render loop wakes on either:
//! - A message on the `notify` channel (sent by the clock or HID thread after
//!   each state mutation), or
//! - A forced 50 ms timeout (~20 FPS) for playhead animation.
//!
//! The read lock is acquired, state is cloned, and the lock is released *before*
//! rendering starts.  The render never holds the lock.

use std::collections::VecDeque;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::input::{FocusPanel, InputCommand};
use crate::midi_out::MidiCtrlMsg;
use crate::music_theory::{note_name, parse_note_name};
use crate::pattern::{
    pattern_dir, song_dir,
    save_pattern, load_pattern,
    save_song, load_song,
    pattern_from_state,
    Song, PatternRef,
};
use crate::state::PlayMode;
use crate::ui_render::{LogEntry, LogTag};

// ── HELP_ENTRIES ──────────────────────────────────────────────────────────────

/// All CLI commands with brief descriptions shown by `help`.
pub(crate) const HELP_ENTRIES: &[(&str, &str)] = &[
    ("port <name>", "connect to MIDI output port by name"),
    ("port list", "list available MIDI output ports"),
    ("channel <1-16>", "set MIDI output channel"),
    ("seed <hex>", "set random seed (e.g. 0xDEAD)"),
    ("rand all", "randomise notes and velocities"),
    ("rand velo", "randomise velocities only"),
    ("rand notes", "randomise note sequence"),
    (
        "note set <1-16> <note> [vel]",
        "set a step's note and velocity",
    ),
    ("clear", "clear the CLI log"),
    ("ok", "alias of clear"),
    ("help", "show this help"),
    ("pattern save <name>", "save current pattern to <name>.pat.toml"),
    ("pattern load <name>", "load pattern from <name>.pat.toml into current state"),
    ("pattern list", "list saved pattern files"),
    ("song new <name>", "create a new empty song"),
    ("song load <name>", "load song from <name>.song.toml"),
    ("song save <name>", "save current song to <name>.song.toml"),
    ("song list", "list saved song files"),
    ("song add <filename>", "append a pattern slot to the current song"),
    ("song remove <n>", "remove slot at 1-indexed position n"),
    ("song set-repeats <n> <r>", "set repeat count for slot n to r"),
];

// ── UiState ───────────────────────────────────────────────────────────────────

/// Local UI state — lives entirely in the UI thread, never shared.
// Fields are used by run_ui (hw-io) and by tests; silence dead-code warnings in
// non-hw-io lib builds where run_ui is not compiled.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) struct UiState {
    /// Which panel currently holds keyboard focus.
    pub focus: FocusPanel,
    /// Currently selected step index (0–15) for F1 panel navigation.
    pub selected_step: usize,
    /// Currently selected param index (0–7) for the F2 · SEQ PARAMS panel.
    pub seq_param_idx: u8,
    /// Currently selected param index (0–7) for the F3 · RANDOM PARAMS panel.
    pub rand_param_idx: u8,
    /// Current contents of the F4 CLI input line (max 256 chars).
    pub cli_line: String,
    /// Ring buffer of CLI log entries (max `CLI_LOG_CAPACITY`).
    pub cli_log: VecDeque<LogEntry>,
    /// Name of the connected MIDI output port (echoed from `port` CLI command).
    pub midi_device_name: String,
    /// MIDI channel display value (1-indexed, echoed from `channel` CLI command).
    pub midi_channel_display: u8,
    /// Current play mode: Pattern or Song.
    pub play_mode: PlayMode,
    /// Current song being edited/played, if any.
    pub song: Option<Song>,
    /// Cursor position within the song slot list.
    pub song_cursor: usize,
    /// Startup instant used for log entry timestamps.
    pub start_time: Instant,
}

#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) const CLI_LOG_CAPACITY: usize = 200;

impl UiState {
    /// Create a new `UiState` with default values.
    #[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
    pub(crate) fn new() -> Self {
        Self {
            focus: FocusPanel::Sequencer,
            selected_step: 0,
            seq_param_idx: 0,
            rand_param_idx: 0,
            cli_line: String::new(),
            cli_log: VecDeque::with_capacity(CLI_LOG_CAPACITY),
            midi_device_name: String::new(),
            midi_channel_display: 1,
            play_mode: PlayMode::Pattern,
            song: None,
            song_cursor: 0,
            start_time: Instant::now(),
        }
    }
}

// ── CLI helpers ────────────────────────────────────────────────────────────────

/// Push a log entry to `log`, dropping the oldest entry if at capacity.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) fn push_log(log: &mut VecDeque<LogEntry>, timestamp_ms: u64, tag: LogTag, text: String) {
    if log.len() >= CLI_LOG_CAPACITY {
        log.pop_front();
    }
    log.push_back(LogEntry {
        timestamp_ms,
        tag,
        text,
    });
}

/// Handle `pattern save|load|list` CLI sub-commands.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
fn handle_cli_pattern_cmd(
    parts: &[&str],
    ui: &mut UiState,
    state: &crate::state::SequencerState,
    _cmd_tx: &SyncSender<InputCommand>,
    _arc_song: &Arc<RwLock<Option<Song>>>,
) {
    let ts = ui.start_time.elapsed().as_millis() as u64;
    match parts.get(1).copied() {
        Some("save") => {
            if let Some(name) = parts.get(2).copied() {
                let data = pattern_from_state(state, name);
                let filename = format!("{name}.pat.toml");
                match save_pattern(&data, &filename) {
                    Ok(()) => push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("pattern saved: {filename}")),
                    Err(e) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("pattern save error: {e}")),
                }
            } else {
                push_log(&mut ui.cli_log, ts, LogTag::Err, "pattern save: missing name".into());
            }
        }
        Some("load") => {
            if let Some(name) = parts.get(2).copied() {
                let filename = format!("{name}.pat.toml");
                match load_pattern(&filename) {
                    Ok(_data) => {
                        // NOTE: Applying to live state requires a write lock on Arc<RwLock<SequencerState>>.
                        // Full wiring (apply_pattern_to_state + send state update) is done in the
                        // song-mode-wiring task which has access to the Arc. We confirm success here.
                        push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("pattern loaded: {filename}"));
                    }
                    Err(e) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("pattern load error: {e}")),
                }
            } else {
                push_log(&mut ui.cli_log, ts, LogTag::Err, "pattern load: missing name".into());
            }
        }
        Some("list") => {
            match std::fs::read_dir(pattern_dir()) {
                Ok(entries) => {
                    let mut found = false;
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.ends_with(".pat.toml") {
                            push_log(&mut ui.cli_log, ts, LogTag::Info, name_str.into_owned());
                            found = true;
                        }
                    }
                    if !found {
                        push_log(&mut ui.cli_log, ts, LogTag::Info, "(no pattern files)".into());
                    }
                }
                Err(e) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("pattern list error: {e}")),
            }
        }
        _ => {
            push_log(&mut ui.cli_log, ts, LogTag::Err, format!("unknown pattern command: {}", parts.get(1).unwrap_or(&"")));
        }
    }
}

/// Handle `song new|load|save|list|add|remove|set-repeats` CLI sub-commands.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
fn handle_cli_song_cmd(
    parts: &[&str],
    ui: &mut UiState,
    _state: &crate::state::SequencerState,
    _cmd_tx: &SyncSender<InputCommand>,
    arc_song: &Arc<RwLock<Option<Song>>>,
) {
    let ts = ui.start_time.elapsed().as_millis() as u64;
    match parts.get(1).copied() {
        Some("new") => {
            if let Some(name) = parts.get(2).copied() {
                ui.song = Some(Song { name: name.to_string(), slots: vec![] });
                *arc_song.write().unwrap() = ui.song.clone();
                push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("song new: {name}"));
            } else {
                push_log(&mut ui.cli_log, ts, LogTag::Err, "song new: missing name".into());
            }
        }
        Some("load") => {
            if let Some(name) = parts.get(2).copied() {
                let filename = format!("{name}.song.toml");
                match load_song(&filename) {
                    Ok(song) => {
                        ui.song = Some(song);
                        *arc_song.write().unwrap() = ui.song.clone();
                        push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("song loaded: {filename}"));
                    }
                    Err(e) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song load error: {e}")),
                }
            } else {
                push_log(&mut ui.cli_log, ts, LogTag::Err, "song load: missing name".into());
            }
        }
        Some("save") => {
            if let Some(name) = parts.get(2).copied() {
                match ui.song.as_ref() {
                    Some(song) => {
                        let filename = format!("{name}.song.toml");
                        match save_song(song, &filename) {
                            Ok(()) => push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("song saved: {filename}")),
                            Err(e) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song save error: {e}")),
                        }
                    }
                    None => push_log(&mut ui.cli_log, ts, LogTag::Err, "song save: no song loaded".into()),
                }
            } else {
                push_log(&mut ui.cli_log, ts, LogTag::Err, "song save: missing name".into());
            }
        }
        Some("list") => {
            match std::fs::read_dir(song_dir()) {
                Ok(entries) => {
                    let mut found = false;
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.ends_with(".song.toml") {
                            push_log(&mut ui.cli_log, ts, LogTag::Info, name_str.into_owned());
                            found = true;
                        }
                    }
                    if !found {
                        push_log(&mut ui.cli_log, ts, LogTag::Info, "(no song files)".into());
                    }
                }
                Err(e) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song list error: {e}")),
            }
        }
        Some("add") => {
            if let Some(filename) = parts.get(2).copied() {
                match ui.song.as_mut() {
                    Some(song) => {
                        song.slots.push(PatternRef { filename: format!("{filename}.pat.toml"), repeats: 1 });
                        *arc_song.write().unwrap() = ui.song.clone();
                        push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("song add: {filename}.pat.toml"));
                    }
                    None => push_log(&mut ui.cli_log, ts, LogTag::Err, "song add: no song loaded".into()),
                }
            } else {
                push_log(&mut ui.cli_log, ts, LogTag::Err, "song add: missing filename".into());
            }
        }
        Some("remove") => {
            if let Some(n_str) = parts.get(2).copied() {
                match n_str.parse::<usize>() {
                    Ok(n) if n >= 1 => {
                        match ui.song.as_mut() {
                            Some(song) => {
                                if n <= song.slots.len() {
                                    song.slots.remove(n - 1);
                                    // Clamp cursor
                                    let max_cursor = song.slots.len().saturating_sub(1);
                                    if ui.song_cursor > max_cursor {
                                        ui.song_cursor = max_cursor;
                                    }
                                    *arc_song.write().unwrap() = ui.song.clone();
                                    push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("song remove: slot {n}"));
                                } else {
                                    push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song remove: index {n} out of range"));
                                }
                            }
                            None => push_log(&mut ui.cli_log, ts, LogTag::Err, "song remove: no song loaded".into()),
                        }
                    }
                    Ok(_) => push_log(&mut ui.cli_log, ts, LogTag::Err, "song remove: index must be >= 1".into()),
                    Err(_) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song remove: invalid index '{n_str}'")),
                }
            } else {
                push_log(&mut ui.cli_log, ts, LogTag::Err, "song remove: missing index".into());
            }
        }
        Some("set-repeats") => {
            let n_str = parts.get(2).copied();
            let r_str = parts.get(3).copied();
            match (n_str, r_str) {
                (Some(n_str), Some(r_str)) => {
                    match (n_str.parse::<usize>(), r_str.parse::<u8>()) {
                        (Ok(n), Ok(r)) if n >= 1 => {
                            match ui.song.as_mut() {
                                Some(song) => {
                                    if n <= song.slots.len() {
                                        song.slots[n - 1].repeats = r;
                                        *arc_song.write().unwrap() = ui.song.clone();
                                        push_log(&mut ui.cli_log, ts, LogTag::Cmd, format!("song set-repeats: slot {n} → {r}"));
                                    } else {
                                        push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song set-repeats: index {n} out of range"));
                                    }
                                }
                                None => push_log(&mut ui.cli_log, ts, LogTag::Err, "song set-repeats: no song loaded".into()),
                            }
                        }
                        (Ok(_), Ok(_)) => push_log(&mut ui.cli_log, ts, LogTag::Err, "song set-repeats: index must be >= 1".into()),
                        (Err(_), _) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song set-repeats: invalid index '{n_str}'")),
                        (_, Err(_)) => push_log(&mut ui.cli_log, ts, LogTag::Err, format!("song set-repeats: invalid repeat count '{r_str}'")),
                    }
                }
                _ => push_log(&mut ui.cli_log, ts, LogTag::Err, "song set-repeats: expected <n> <r>".into()),
            }
        }
        _ => {
            push_log(&mut ui.cli_log, ts, LogTag::Err, format!("unknown song command: {}", parts.get(1).unwrap_or(&"")));
        }
    }
}

/// Process the current `cli_line`, dispatch commands, append log entries, clear input.
///
/// Handles:
/// - `pattern save|load|list` → pattern file operations
/// - `song new|load|save|list|add|remove|set-repeats` → song operations
/// - `port <name>`   → `MidiCtrlMsg::ChangePort` + `InputCommand::MidiDeviceName`
/// - `channel <n>`   → `MidiCtrlMsg::ChangeChannel` + `InputCommand::ChannelSet`
/// - `seed <hex>`    → `InputCommand::SeedSet`
/// - unknown         → error log entry
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) fn handle_cli_submit(
    ui: &mut UiState,
    cmd_tx: &SyncSender<InputCommand>,
    midi_ctrl_tx: &SyncSender<MidiCtrlMsg>,
    state: &crate::state::SequencerState,
    arc_song: &Arc<RwLock<Option<Song>>>,
) {
    let line = ui.cli_line.trim().to_string();
    ui.cli_line.clear();
    let ts = ui.start_time.elapsed().as_millis() as u64;

    if line.is_empty() {
        return;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts[0] == "pattern" {
        return handle_cli_pattern_cmd(&parts, ui, state, cmd_tx, arc_song);
    }
    if parts[0] == "song" {
        return handle_cli_song_cmd(&parts, ui, state, cmd_tx, arc_song);
    }

    if line == "port list" {
        let _ = midi_ctrl_tx.send(MidiCtrlMsg::ListPorts);
        push_log(
            &mut ui.cli_log,
            ts,
            LogTag::Cmd,
            "port list (querying...)".into(),
        );
    } else if let Some(name) = line.strip_prefix("port ") {
        let name = name.trim().to_string();
        let _ = midi_ctrl_tx.send(MidiCtrlMsg::ChangePort(name.clone()));
        let _ = cmd_tx.send(InputCommand::MidiDeviceName(name.clone()));
        ui.midi_device_name = name.clone();
        push_log(
            &mut ui.cli_log,
            ts,
            LogTag::Midi,
            format!("port → {name} (requesting)"),
        );
    } else if let Some(rest) = line.strip_prefix("channel ") {
        if let Ok(n) = rest.trim().parse::<u8>() {
            if (1..=16).contains(&n) {
                let _ = midi_ctrl_tx.send(MidiCtrlMsg::ChangeChannel(n));
                let _ = cmd_tx.send(InputCommand::ChannelSet(n));
                ui.midi_channel_display = n;
                push_log(&mut ui.cli_log, ts, LogTag::Midi, format!("channel → {n}"));
            } else {
                push_log(
                    &mut ui.cli_log,
                    ts,
                    LogTag::Err,
                    "channel must be 1–16".into(),
                );
            }
        } else {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Err,
                format!("invalid channel: {rest}"),
            );
        }
    } else if let Some(rest) = line.strip_prefix("seed ") {
        let hex = rest
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            let _ = cmd_tx.send(InputCommand::SeedSet(v));
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Cmd,
                format!("seed → 0x{v:04X}"),
            );
        } else {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Err,
                format!("invalid hex: {rest}"),
            );
        }
    } else if line == "rand all" {
        let _ = cmd_tx.send(InputCommand::RandAll);
        push_log(&mut ui.cli_log, ts, LogTag::Cmd, "rand all".into());
    } else if line == "rand velo" {
        let _ = cmd_tx.send(InputCommand::RandVelocities);
        push_log(&mut ui.cli_log, ts, LogTag::Cmd, "rand velo".into());
    } else if line == "rand notes" {
        let _ = cmd_tx.send(InputCommand::GenerateRandomSequence);
        push_log(&mut ui.cli_log, ts, LogTag::Cmd, "rand notes".into());
    } else if let Some(rest) = line.strip_prefix("note set ") {
        handle_cli_note_set(ui, cmd_tx, ts, rest.trim());
    } else if line == "clear" || line == "ok" {
        ui.cli_log.clear();
    } else if line == "help" {
        for (cmd, desc) in HELP_ENTRIES {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Info,
                format!("{cmd}  —  {desc}"),
            );
        }
    } else {
        push_log(
            &mut ui.cli_log,
            ts,
            LogTag::Err,
            format!("unknown command: {line}"),
        );
    }
}

/// Parse and apply a `note set <step> <note> [velocity]` CLI command.
///
/// Logs `LogTag::Err` on any parse failure and `LogTag::Cmd` on success.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
fn handle_cli_note_set(ui: &mut UiState, cmd_tx: &SyncSender<InputCommand>, ts: u64, rest: &str) {
    let mut parts = rest.split_whitespace();

    // Step is user-facing 1–16. We store it internally as 0–15.
    let user_step = match parts.next().and_then(|s| s.parse::<usize>().ok()) {
        Some(s) if (1..=16).contains(&s) => s,
        Some(_) => {
            push_log(&mut ui.cli_log, ts, LogTag::Err, "step must be 1–16".into());
            return;
        }
        None => {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Err,
                "note set: expected <step> <note> [velocity]".into(),
            );
            return;
        }
    };
    let step = user_step - 1;

    let note_str = match parts.next() {
        Some(s) => s,
        None => {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Err,
                "note set: missing note name".into(),
            );
            return;
        }
    };

    let midi_note = match parse_note_name(note_str) {
        Some(n) => n,
        None => {
            push_log(
                &mut ui.cli_log,
                ts,
                LogTag::Err,
                format!("note set: invalid note '{note_str}'"),
            );
            return;
        }
    };

    let velocity: u8 = match parts.next() {
        Some(s) => match s.parse::<u8>() {
            Ok(v) if v <= 127 => v,
            Ok(_) => {
                push_log(
                    &mut ui.cli_log,
                    ts,
                    LogTag::Err,
                    "velocity must be 0–127".into(),
                );
                return;
            }
            Err(_) => {
                push_log(
                    &mut ui.cli_log,
                    ts,
                    LogTag::Err,
                    format!("note set: invalid velocity '{s}'"),
                );
                return;
            }
        },
        None => 127,
    };

    // Reject any unexpected trailing input after the velocity field.
    if parts.next().is_some() {
        push_log(
            &mut ui.cli_log,
            ts,
            LogTag::Err,
            "note set: unexpected trailing input".into(),
        );
        return;
    }

    let _ = cmd_tx.send(InputCommand::NoteSet {
        step,
        midi_note,
        velocity,
    });
    push_log(
        &mut ui.cli_log,
        ts,
        LogTag::Cmd,
        format!(
            "note set {user_step} → {} vel {velocity}",
            note_name(midi_note)
        ),
    );
}

// ── MIDI log sentinel parsing ─────────────────────────────────────────────────

/// Parse a `[ports]` or `[ports-err]` sentinel from the MIDI output thread.
///
/// Returns `Some(entries)` when `(ok, msg)` is a recognised port-listing sentinel,
/// where each entry is a `(LogTag, String)` pair ready to push into the CLI log.
/// Returns `None` for any non-sentinel message so the caller handles it normally.
///
/// Sentinel contracts:
/// - `(true,  "[ports]<name1>\x1f<name2>…")` — one `LogTag::Info` per port name.
/// - `(true,  "[ports]")` with empty payload  — single `LogTag::Info` "no MIDI ports available".
/// - `(false, "[ports-err] <msg>")` — single `LogTag::Err` with the full message.
/// - Any other `(false, _)` falls through (`None`); caller handles it as a generic error.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) fn parse_ports_sentinel(ok: bool, msg: &str) -> Option<Vec<(LogTag, String)>> {
    if ok {
        let payload = msg.strip_prefix("[ports]")?;
        if payload.is_empty() {
            Some(vec![(LogTag::Info, "no MIDI ports available".into())])
        } else {
            Some(
                payload
                    .split('\x1f')
                    .map(|name| (LogTag::Info, name.to_string()))
                    .collect(),
            )
        }
    } else if msg.starts_with("[ports-err]") {
        Some(vec![(LogTag::Err, msg.to_string())])
    } else {
        None
    }
}

// ── Global key dispatch (feature-independent) ────────────────────────────────

/// Map a key to a global `InputCommand` that is active in any focus panel.
///
/// Returns `Some(cmd)` for: +/- (BpmDelta), P/p (PlayStop), F1–F4 (SetFocus).
/// Returns `None` for all other keys.
#[cfg_attr(not(feature = "hw-io"), allow(dead_code))]
pub(crate) fn global_key_to_command(key: crate::input::KeyCodeSimple) -> Option<InputCommand> {
    use crate::input::KeyCodeSimple;
    match key {
        KeyCodeSimple::Plus => Some(InputCommand::BpmDelta(1)),
        KeyCodeSimple::Minus => Some(InputCommand::BpmDelta(-1)),
        KeyCodeSimple::Char('p') | KeyCodeSimple::Char('P') => Some(InputCommand::PlayStop),
        KeyCodeSimple::F1 => Some(InputCommand::SetFocus(FocusPanel::Sequencer)),
        KeyCodeSimple::F2 => Some(InputCommand::SetFocus(FocusPanel::SeqParams)),
        KeyCodeSimple::F3 => Some(InputCommand::SetFocus(FocusPanel::RandParams)),
        KeyCodeSimple::F4 => Some(InputCommand::SetFocus(FocusPanel::Cli)),
        _ => None,
    }
}

// ── hw-io–only items (crossterm, terminal, run_ui) ────────────────────────────

#[cfg(feature = "hw-io")]
use std::io;
#[cfg(feature = "hw-io")]
use std::sync::mpsc::Receiver;
#[cfg(feature = "hw-io")]
use std::time::Duration;

#[cfg(feature = "hw-io")]
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
#[cfg(feature = "hw-io")]
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
#[cfg(feature = "hw-io")]
use crossterm::ExecutableCommand;
#[cfg(feature = "hw-io")]
use ratatui::backend::CrosstermBackend;
#[cfg(feature = "hw-io")]
use ratatui::Terminal;

#[cfg(feature = "hw-io")]
use crate::input::{cli_key_to_char, panel_key_to_command, KeyCodeSimple};
#[cfg(feature = "hw-io")]
use crate::state::SequencerState;
#[cfg(feature = "hw-io")]
use crate::ui_render::{render_frame, UiLocalSnapshot};

/// RAII guard that restores the terminal on drop.
///
/// Using Drop ensures the terminal is cleaned up even if a panic unwinds the
/// stack, preventing a broken terminal state for the user.
#[cfg(feature = "hw-io")]
struct TerminalGuard;

#[cfg(feature = "hw-io")]
impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        Ok(Self)
    }
}

#[cfg(feature = "hw-io")]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup — ignore errors because we may already be panicking.
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
    }
}

/// Convert a crossterm `KeyCode` into our portable `KeyCodeSimple`.
#[cfg(feature = "hw-io")]
fn to_simple(code: KeyCode) -> KeyCodeSimple {
    match code {
        KeyCode::Left => KeyCodeSimple::Left,
        KeyCode::Right => KeyCodeSimple::Right,
        KeyCode::Up => KeyCodeSimple::Up,
        KeyCode::Down => KeyCodeSimple::Down,
        KeyCode::Char(' ') => KeyCodeSimple::Space,
        KeyCode::Char('+') | KeyCode::Char('=') => KeyCodeSimple::Plus,
        KeyCode::Char('-') => KeyCodeSimple::Minus,
        KeyCode::Char(c) => KeyCodeSimple::Char(c),
        KeyCode::Enter => KeyCodeSimple::Enter,
        KeyCode::Esc => KeyCodeSimple::Esc,
        KeyCode::Backspace => KeyCodeSimple::Backspace,
        KeyCode::F(1) => KeyCodeSimple::F1,
        KeyCode::F(2) => KeyCodeSimple::F2,
        KeyCode::F(3) => KeyCodeSimple::F3,
        KeyCode::F(4) => KeyCodeSimple::F4,
        KeyCode::F(9)  => KeyCodeSimple::F9,
        KeyCode::F(10) => KeyCodeSimple::F10,
        KeyCode::Delete => KeyCodeSimple::Delete,
        _ => KeyCodeSimple::Other,
    }
}

/// Dispatch a key event to the appropriate handler based on current focus.
///
/// Global keys (F9/F10 mode switch) are handled first regardless of focus.
/// Then F1–F4/+/-/P global keys apply (with CLI-focus guard for non-focus keys).
/// Focus-specific keys are then dispatched via `panel_key_to_command` or inline CLI logic.
#[cfg(feature = "hw-io")]
fn translate_key(
    event: KeyEvent,
    ui: &mut UiState,
    cmd_tx: &SyncSender<InputCommand>,
    midi_ctrl_tx: &SyncSender<MidiCtrlMsg>,
    state: &SequencerState,
    arc_song: &Arc<RwLock<Option<Song>>>,
) {
    let simple = to_simple(event.code);

    // ── F9/F10 mode-switch keys: always global, highest priority ──────────────
    if let Some(cmd) = crate::input::global_key_to_command(simple) {
        match cmd {
            InputCommand::SwitchToPatternMode => { ui.play_mode = PlayMode::Pattern; }
            InputCommand::SwitchToSongMode    => { ui.play_mode = PlayMode::Song; }
            _ => {}
        }
        let _ = cmd_tx.try_send(cmd);
        return;
    }

    // ── UI-level global keys (F1–F4, +/-, P) ─────────────────────────────────
    // When CLI has focus, only SetFocus variants (F1–F4) pass through global
    // dispatch. All other global keys (PlayStop, BpmDelta) must fall through to
    // the FocusPanel::Cli arm so that characters like 'p' are inserted into the
    // CLI line instead of firing their global actions.
    if let Some(cmd) = global_key_to_command(simple) {
        let is_set_focus = matches!(cmd, InputCommand::SetFocus(_));
        if is_set_focus || ui.focus != FocusPanel::Cli {
            // SetFocus commands update local ui.focus; other globals are sent on cmd_tx.
            match cmd {
                InputCommand::SetFocus(panel) => {
                    ui.focus = panel;
                }
                other => {
                    let _ = cmd_tx.send(other);
                }
            }
            return;
        }
        // Non-SetFocus global key in CLI mode: fall through to CLI handler below.
    }

    // ── Focus-specific keys ────────────────────────────────────────────────────
    match ui.focus {
        FocusPanel::Sequencer => {
            // Song mode navigation when in Sequencer focus + Song play mode
            if ui.play_mode == PlayMode::Song {
                match simple {
                    KeyCodeSimple::Up => {
                        ui.song_cursor = ui.song_cursor.saturating_sub(1);
                        let _ = cmd_tx.try_send(InputCommand::SongSlotCursorUp);
                        return;
                    }
                    KeyCodeSimple::Down => {
                        let max = ui.song.as_ref().map(|s| s.slots.len()).unwrap_or(0).saturating_sub(1);
                        if ui.song_cursor < max {
                            ui.song_cursor += 1;
                        }
                        let _ = cmd_tx.try_send(InputCommand::SongSlotCursorDown);
                        return;
                    }
                    KeyCodeSimple::Char('d') | KeyCodeSimple::Delete => {
                        let _ = cmd_tx.try_send(InputCommand::SongSlotDelete);
                        if ui.song.is_some() {
                            if let Some(song) = ui.song.as_mut() {
                                if !song.slots.is_empty() && ui.song_cursor < song.slots.len() {
                                    song.slots.remove(ui.song_cursor);
                                    let max = song.slots.len().saturating_sub(1);
                                    if ui.song_cursor > max {
                                        ui.song_cursor = max;
                                    }
                                }
                            }
                        }
                        return;
                    }
                    KeyCodeSimple::Char('[') => {
                        let _ = cmd_tx.try_send(InputCommand::SongSlotMoveUp);
                        return;
                    }
                    KeyCodeSimple::Char(']') => {
                        let _ = cmd_tx.try_send(InputCommand::SongSlotMoveDown);
                        return;
                    }
                    _ => {}
                }
            }

            match simple {
                // BUG-034: update ui.selected_step here so the render reflects the
                // new highlighted step immediately.  ui.selected_step must stay in
                // sync with SequencerState.selected_step (updated via cmd_tx below).
                KeyCodeSimple::Left => {
                    ui.selected_step = (ui.selected_step + 15) % 16;
                    let _ = cmd_tx.send(InputCommand::StepSelectDelta(-1));
                }
                KeyCodeSimple::Right => {
                    ui.selected_step = (ui.selected_step + 1) % 16;
                    let _ = cmd_tx.send(InputCommand::StepSelectDelta(1));
                }
                key => {
                    if let Some(cmd) = panel_key_to_command(key, FocusPanel::Sequencer) {
                        let _ = cmd_tx.send(cmd);
                    }
                }
            }
        }
        FocusPanel::SeqParams => match simple {
            KeyCodeSimple::Left => {
                ui.seq_param_idx = ui.seq_param_idx.saturating_sub(1);
                let _ = cmd_tx.send(InputCommand::PanelParamSelect(ui.seq_param_idx));
            }
            KeyCodeSimple::Right => {
                ui.seq_param_idx = (ui.seq_param_idx + 1).min(7);
                let _ = cmd_tx.send(InputCommand::PanelParamSelect(ui.seq_param_idx));
            }
            KeyCodeSimple::Up => {
                let _ = cmd_tx.send(InputCommand::PanelParamDelta(1));
            }
            KeyCodeSimple::Down => {
                let _ = cmd_tx.send(InputCommand::PanelParamDelta(-1));
            }
            _ => {}
        },
        FocusPanel::RandParams => match simple {
            KeyCodeSimple::Left => {
                ui.rand_param_idx = ui.rand_param_idx.saturating_sub(1);
                let _ = cmd_tx.send(InputCommand::RandParamSelect(ui.rand_param_idx));
            }
            KeyCodeSimple::Right => {
                ui.rand_param_idx = (ui.rand_param_idx + 1).min(7);
                let _ = cmd_tx.send(InputCommand::RandParamSelect(ui.rand_param_idx));
            }
            KeyCodeSimple::Up => {
                let _ = cmd_tx.send(InputCommand::RandParamDelta(1));
            }
            KeyCodeSimple::Down => {
                let _ = cmd_tx.send(InputCommand::RandParamDelta(-1));
            }
            _ => {}
        },
        FocusPanel::Cli => match simple {
            KeyCodeSimple::Enter => {
                handle_cli_submit(ui, cmd_tx, midi_ctrl_tx, state, arc_song);
            }
            KeyCodeSimple::Backspace => {
                ui.cli_line.pop();
            }
            key => {
                if let Some(c) = cli_key_to_char(key) {
                    if ui.cli_line.len() < 256 {
                        ui.cli_line.push(c);
                    }
                }
            }
        },
    }
}

/// Run the terminal UI event loop.
///
/// Blocks until the user presses Ctrl-C or the `ui_notify_rx` channel closes.
///
/// # Parameters
///
/// - `state`         — shared sequencer state; read lock is acquired briefly per frame.
/// - `cmd_tx`        — command channel to the state processor.
/// - `ui_notify_rx`  — wakeup channel; the clock and HID threads send `()` after each
///   state mutation.  A 50 ms timeout fires if no wakeup arrives.
/// - `midi_ctrl_tx`  — control channel to the MIDI output thread (port/channel changes).
/// - `midi_log_rx`   — log messages from the MIDI output thread; drained each frame and
///   pushed into the CLI log panel.  `true` = info/success, `false` = error.
/// - `arc_song`      — shared song state; written by CLI song commands, read by the
///   command processor for song-mode playback.
///
/// # Termination
///
/// On exit the terminal is restored via the `TerminalGuard` Drop impl.
/// The caller is responsible for stopping the sequencer (sending
/// `MidiEvent::Stop`) and joining all other threads after this returns.
#[cfg(feature = "hw-io")]
pub fn run_ui(
    state: Arc<RwLock<SequencerState>>,
    cmd_tx: SyncSender<InputCommand>,
    ui_notify_rx: Receiver<()>,
    midi_ctrl_tx: SyncSender<MidiCtrlMsg>,
    midi_log_rx: Receiver<(bool, String)>,
    arc_song: Arc<RwLock<Option<Song>>>,
) {
    let _guard = match TerminalGuard::enter() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("run_ui: failed to enter raw mode: {e}");
            return;
        }
    };

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("run_ui: failed to create terminal: {e}");
            return;
        }
    };

    let mut ui = UiState::new();

    loop {
        // ── Drain MIDI log messages ───────────────────────────────────────────
        // Route messages from the MIDI output thread into the CLI log panel
        // before rendering so this frame shows the latest MIDI status.
        // Special sentinels for port listing:
        //   (true,  "[ports]name1\x1fname2") → one LogTag::Info per port name
        //   (true,  "[ports]")               → "no MIDI ports available" LogTag::Info
        //   (false, "[ports-err] ...")        → LogTag::Err via parse_ports_sentinel
        while let Ok((ok, msg)) = midi_log_rx.try_recv() {
            let ts = ui.start_time.elapsed().as_millis() as u64;
            if let Some(entries) = parse_ports_sentinel(ok, &msg) {
                for (tag, text) in entries {
                    push_log(&mut ui.cli_log, ts, tag, text);
                }
            } else {
                let tag = if ok {
                    crate::ui_render::LogTag::Midi
                } else {
                    crate::ui_render::LogTag::Err
                };
                push_log(&mut ui.cli_log, ts, tag, msg);
            }
        }

        // ── Render ───────────────────────────────────────────────────────────
        // Acquire read lock, clone state, release lock, then render.
        let state_snapshot = { state.read().expect("run_ui: state RwLock poisoned").clone() };
        let snapshot = UiLocalSnapshot {
            focus: ui.focus,
            selected_step: ui.selected_step,
            seq_param_idx: ui.seq_param_idx,
            rand_param_idx: ui.rand_param_idx,
            cli_line: &ui.cli_line,
            cli_log: &ui.cli_log,
            midi_device_name: &ui.midi_device_name,
            midi_channel_display: ui.midi_channel_display,
            play_mode: ui.play_mode,
            song_slots: ui.song.as_ref().map(|s| s.slots.as_slice()).unwrap_or(&[]),
            song_cursor: ui.song_cursor,
            song_active_slot: state_snapshot.song_slot_index,
        };
        if let Err(e) = terminal.draw(|frame| {
            render_frame(frame, &state_snapshot, &snapshot);
        }) {
            eprintln!("run_ui: render error: {e}");
            break;
        }

        // ── Input ────────────────────────────────────────────────────────────
        // Poll for a key event with 50 ms timeout.
        let has_event = match event::poll(Duration::from_millis(50)) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("run_ui: poll error: {e}");
                break;
            }
        };

        if has_event {
            let ev = match event::read() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("run_ui: read error: {e}");
                    break;
                }
            };

            if let Event::Key(key_event) = ev {
                // Ctrl-C exits the UI thread cleanly.
                if key_event.code == KeyCode::Char('c')
                    && key_event.modifiers.contains(KeyModifiers::CONTROL)
                {
                    break;
                }

                let state_snap = { state.read().expect("run_ui: state RwLock poisoned").clone() };
                translate_key(key_event, &mut ui, &cmd_tx, &midi_ctrl_tx, &state_snap, &arc_song);
            }
        }

        // ── Notify drain ─────────────────────────────────────────────────────
        // Drain any pending wakeups so we don't fall behind if the clock fires
        // faster than we render.  `try_recv` returns Err when the channel is
        // empty; `Disconnected` means all senders have dropped — exit.
        loop {
            match ui_notify_rx.try_recv() {
                Ok(_) => continue,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return; // exit run_ui immediately.
                }
            }
        }
    }
    // TerminalGuard Drop restores the terminal.
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SequencerState;
    use std::sync::mpsc;

    fn make_channels() -> (
        mpsc::SyncSender<InputCommand>,
        mpsc::Receiver<InputCommand>,
        mpsc::SyncSender<MidiCtrlMsg>,
        mpsc::Receiver<MidiCtrlMsg>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::sync_channel(16);
        let (ctrl_tx, ctrl_rx) = mpsc::sync_channel(16);
        (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx)
    }

    fn make_arc_song() -> Arc<RwLock<Option<Song>>> {
        Arc::new(RwLock::new(None))
    }

    // ── handle_cli_submit tests ───────────────────────────────────────────────

    #[test]
    fn cli_submit_port_sends_midi_ctrl_msg() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "port MyDevice".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        // MidiCtrlMsg::ChangePort should be sent.
        let ctrl_msg = ctrl_rx.try_recv().expect("expected MidiCtrlMsg");
        assert!(matches!(ctrl_msg, MidiCtrlMsg::ChangePort(ref n) if n == "MyDevice"));

        // InputCommand::MidiDeviceName should be sent.
        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::MidiDeviceName(ref n) if n == "MyDevice"));

        // cli_line should be cleared.
        assert!(ui.cli_line.is_empty());

        // A log entry should be appended.
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Midi));
    }

    #[test]
    fn cli_submit_channel_sends_channel_set_cmd() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "channel 5".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        // MidiCtrlMsg::ChangeChannel(5) should be sent.
        let ctrl_msg = ctrl_rx.try_recv().expect("expected MidiCtrlMsg");
        assert!(matches!(ctrl_msg, MidiCtrlMsg::ChangeChannel(5)));

        // InputCommand::ChannelSet(5) should be sent.
        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::ChannelSet(5)));

        // ui state updated.
        assert_eq!(ui.midi_channel_display, 5);
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Midi));
    }

    #[test]
    fn cli_submit_channel_out_of_range_appends_error() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "channel 0".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
    }

    #[test]
    fn cli_submit_unknown_appends_error_to_log() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "foo bar baz".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
        assert!(ui.cli_log[0].text.contains("foo bar baz"));
    }

    #[test]
    fn cli_submit_seed_hex_sends_seed_set() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "seed 0xDEAD".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::SeedSet(0xDEAD)));
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
    }

    #[test]
    fn cli_submit_empty_line_is_noop() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "   ".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err());
        assert!(ctrl_rx.try_recv().is_err());
        assert!(ui.cli_log.is_empty());
    }

    #[test]
    fn cli_log_capacity_is_respected() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();

        // Submit CLI_LOG_CAPACITY + 5 unknown commands to fill the log.
        for i in 0..(CLI_LOG_CAPACITY + 5) {
            ui.cli_line = format!("unknowncmd{i}");
            handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);
        }

        assert_eq!(ui.cli_log.len(), CLI_LOG_CAPACITY);
    }

    // ── global_key_to_command tests ───────────────────────────────────────────

    #[test]
    fn bpm_plus_key_sends_bpm_delta_from_any_focus() {
        use crate::input::{FocusPanel, KeyCodeSimple};

        // Plus from Sequencer focus → BpmDelta(+1)
        let cmd = super::global_key_to_command(KeyCodeSimple::Plus);
        assert!(
            matches!(cmd, Some(InputCommand::BpmDelta(1))),
            "Plus should produce BpmDelta(1)"
        );

        // Minus from RandParams focus → BpmDelta(-1)
        // (global_key_to_command is focus-independent; we verify the key independently)
        let cmd = super::global_key_to_command(KeyCodeSimple::Minus);
        assert!(
            matches!(cmd, Some(InputCommand::BpmDelta(-1))),
            "Minus should produce BpmDelta(-1)"
        );

        // F2 key → SetFocus(SeqParams) (from any focus, including Cli)
        let cmd = super::global_key_to_command(KeyCodeSimple::F2);
        assert!(
            matches!(cmd, Some(InputCommand::SetFocus(FocusPanel::SeqParams))),
            "F2 should produce SetFocus(SeqParams)"
        );
    }

    #[test]
    fn global_key_f1_produces_set_focus_sequencer() {
        use crate::input::{FocusPanel, KeyCodeSimple};
        let cmd = super::global_key_to_command(KeyCodeSimple::F1);
        assert!(
            matches!(cmd, Some(InputCommand::SetFocus(FocusPanel::Sequencer))),
            "F1 should produce SetFocus(Sequencer)"
        );
    }

    #[test]
    fn global_key_f3_produces_set_focus_rand_params() {
        use crate::input::{FocusPanel, KeyCodeSimple};
        let cmd = super::global_key_to_command(KeyCodeSimple::F3);
        assert!(
            matches!(cmd, Some(InputCommand::SetFocus(FocusPanel::RandParams))),
            "F3 should produce SetFocus(RandParams)"
        );
    }

    #[test]
    fn global_key_f4_produces_set_focus_cli() {
        use crate::input::{FocusPanel, KeyCodeSimple};
        let cmd = super::global_key_to_command(KeyCodeSimple::F4);
        assert!(
            matches!(cmd, Some(InputCommand::SetFocus(FocusPanel::Cli))),
            "F4 should produce SetFocus(Cli)"
        );
    }

    #[test]
    fn global_key_all_f1_f4_produce_set_focus_variants() {
        use crate::input::{FocusPanel, KeyCodeSimple};
        let cases = [
            (KeyCodeSimple::F1, FocusPanel::Sequencer),
            (KeyCodeSimple::F2, FocusPanel::SeqParams),
            (KeyCodeSimple::F3, FocusPanel::RandParams),
            (KeyCodeSimple::F4, FocusPanel::Cli),
        ];
        for (key, expected_panel) in cases {
            let cmd = super::global_key_to_command(key);
            assert!(
                matches!(&cmd, Some(InputCommand::SetFocus(p)) if *p == expected_panel),
                "key {key:?} should produce SetFocus({expected_panel:?})"
            );
        }
    }

    #[test]
    fn global_key_unknown_produces_none() {
        use crate::input::KeyCodeSimple;
        // Keys that are not globally handled should return None.
        for key in [
            KeyCodeSimple::Left,
            KeyCodeSimple::Right,
            KeyCodeSimple::Up,
            KeyCodeSimple::Down,
            KeyCodeSimple::Enter,
            KeyCodeSimple::Backspace,
            KeyCodeSimple::Esc,
            KeyCodeSimple::Space,
            KeyCodeSimple::Other,
            KeyCodeSimple::Char('a'),
            KeyCodeSimple::Char('z'),
        ] {
            let cmd = super::global_key_to_command(key);
            assert!(
                cmd.is_none(),
                "key {key:?} should produce None but got {cmd:?}"
            );
        }
    }

    // ── handle_cli_submit additional edge-case tests ──────────────────────────

    #[test]
    fn cli_submit_port_alone_without_name_is_unknown_command() {
        // "port" (no trailing space and no name) trims to "port" which does not
        // match the "port " prefix, so it falls through to the unknown-command branch.
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "port".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert_eq!(
            ui.cli_log.len(),
            1,
            "unknown command should produce one error log entry"
        );
        assert!(
            matches!(ui.cli_log[0].tag, LogTag::Err),
            "bare 'port' should log an error"
        );
        assert!(
            _ctrl_rx.try_recv().is_err(),
            "no MidiCtrlMsg for bare 'port'"
        );
        assert!(
            _cmd_rx.try_recv().is_err(),
            "no InputCommand for bare 'port'"
        );
    }

    #[test]
    fn cli_submit_channel_17_is_out_of_range() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "channel 17".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
        assert!(
            _cmd_rx.try_recv().is_err(),
            "no InputCommand should be sent for channel 17"
        );
        assert!(
            _ctrl_rx.try_recv().is_err(),
            "no MidiCtrlMsg should be sent for channel 17"
        );
    }

    #[test]
    fn cli_submit_channel_255_is_out_of_range() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "channel 255".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
    }

    #[test]
    fn cli_submit_seed_invalid_hex_appends_error() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "seed ZZZZ".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(
            cmd_rx.try_recv().is_err(),
            "no command should be sent for invalid hex"
        );
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
        assert!(ui.cli_log[0].text.contains("invalid hex"));
        drop(ctrl_tx);
    }

    #[test]
    fn cli_submit_seed_0x_prefix_is_accepted() {
        // Uppercase 0X prefix should also be stripped.
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "seed 0XBEEF".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected SeedSet");
        assert!(
            matches!(cmd, InputCommand::SeedSet(0xBEEF)),
            "0X-prefixed hex should parse correctly"
        );
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
    }

    #[test]
    fn cli_submit_truly_empty_line_is_noop() {
        // Submitting a completely empty cli_line (not just whitespace) is a no-op.
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = String::new();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err(), "no command for empty line");
        assert!(ctrl_rx.try_recv().is_err(), "no ctrl msg for empty line");
        assert!(ui.cli_log.is_empty(), "no log entry for empty line");
    }

    // ── push_log tests ────────────────────────────────────────────────────────

    #[test]
    fn push_log_evicts_oldest_when_at_capacity() {
        let mut log: VecDeque<LogEntry> = VecDeque::new();
        for i in 0..CLI_LOG_CAPACITY {
            push_log(&mut log, i as u64, LogTag::Info, format!("entry {i}"));
        }
        assert_eq!(log.len(), CLI_LOG_CAPACITY);
        assert_eq!(log[0].text, "entry 0");

        push_log(&mut log, 999, LogTag::Info, "new entry".into());
        assert_eq!(log.len(), CLI_LOG_CAPACITY);
        assert_eq!(log[0].text, "entry 1");
        assert_eq!(log[log.len() - 1].text, "new entry");
    }

    // ── BUG-030: CLI focus blocks global PlayStop / BpmDelta for 'p', '+', '-' ─
    //
    // global_key_to_command('p') → PlayStop, '+' → BpmDelta(1), '-' → BpmDelta(-1).
    // These are correct for non-CLI panels. The translate_key guard (BUG-030 fix)
    // prevents these commands from being sent when FocusPanel::Cli is active;
    // they fall through to cli_key_to_char instead so the characters are inserted
    // into the CLI line.
    //
    // global_key_to_command itself is focus-independent and is tested directly.
    // The cli_key_to_char helper (always-compiled, no hw-io gate) is tested in
    // input.rs with six dedicated unit tests.

    #[test]
    fn global_key_p_maps_to_play_stop_outside_cli_focus() {
        // Verify global_key_to_command('p') produces PlayStop as expected.
        // In non-CLI focus this fires PlayStop; in CLI focus translate_key guards it.
        use crate::input::KeyCodeSimple;
        let cmd = super::global_key_to_command(KeyCodeSimple::Char('p'));
        assert!(
            matches!(cmd, Some(InputCommand::PlayStop)),
            "'p' must map to PlayStop in global_key_to_command"
        );
        let cmd_upper = super::global_key_to_command(KeyCodeSimple::Char('P'));
        assert!(
            matches!(cmd_upper, Some(InputCommand::PlayStop)),
            "'P' must map to PlayStop in global_key_to_command"
        );
    }

    #[test]
    fn cli_submit_port_log_entry_says_requesting() {
        // BUG-033: verify the port success log message contains "(requesting)"
        // so the fire-and-forget nature is explicit.
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "port MyDevice".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert_eq!(ui.cli_log.len(), 1);
        assert!(
            ui.cli_log[0].text.contains("(requesting)"),
            "port log entry must contain '(requesting)', got: {:?}",
            ui.cli_log[0].text
        );
    }

    #[test]
    fn push_log_capacity_200_enforced_with_201_entries() {
        // Explicitly verify the CLI_LOG_CAPACITY constant is 200 and that inserting
        // 201 entries results in exactly 200 entries with the first dropped.
        assert_eq!(CLI_LOG_CAPACITY, 200, "CLI_LOG_CAPACITY must be 200");

        let mut log: VecDeque<LogEntry> = VecDeque::new();
        for i in 0..201_usize {
            push_log(&mut log, i as u64, LogTag::Info, format!("msg{i}"));
        }

        assert_eq!(
            log.len(),
            200,
            "log should hold exactly 200 entries after 201 inserts"
        );
        // The first entry (msg0) should have been dropped; msg1 is now the oldest.
        assert_eq!(
            log[0].text, "msg1",
            "oldest entry (msg0) should have been evicted"
        );
        assert_eq!(log[199].text, "msg200", "newest entry should be msg200");
    }

    // ── new CLI command tests ─────────────────────────────────────────────────

    #[test]
    fn cli_submit_rand_all_sends_command_and_logs() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "rand all".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::RandAll));
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
    }

    #[test]
    fn cli_submit_rand_velo_sends_command_and_logs() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "rand velo".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::RandVelocities));
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
    }

    #[test]
    fn cli_submit_rand_notes_sends_command_and_logs() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "rand notes".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected InputCommand");
        assert!(matches!(cmd, InputCommand::GenerateRandomSequence));
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
    }

    #[test]
    fn cli_submit_port_list_sends_list_ports_and_logs() {
        let (cmd_tx, _cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "port list".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let ctrl_msg = ctrl_rx.try_recv().expect("expected MidiCtrlMsg");
        assert!(matches!(ctrl_msg, MidiCtrlMsg::ListPorts));
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
        assert!(ui.cli_log[0].text.contains("querying"));
    }

    #[test]
    fn cli_submit_clear_empties_log() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        // Prefill the log.
        push_log(&mut ui.cli_log, 0, LogTag::Info, "old entry".into());
        ui.cli_line = "clear".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(ui.cli_log.is_empty(), "clear should empty the log");
        assert!(cmd_rx.try_recv().is_err(), "clear sends no InputCommand");
        assert!(ctrl_rx.try_recv().is_err(), "clear sends no MidiCtrlMsg");
    }

    #[test]
    fn cli_submit_ok_empties_log() {
        let (cmd_tx, cmd_rx, ctrl_tx, ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        push_log(&mut ui.cli_log, 0, LogTag::Info, "old entry".into());
        ui.cli_line = "ok".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(ui.cli_log.is_empty(), "ok should empty the log");
        assert!(cmd_rx.try_recv().is_err(), "ok sends no InputCommand");
        assert!(ctrl_rx.try_recv().is_err(), "ok sends no MidiCtrlMsg");
    }

    #[test]
    fn cli_submit_help_pushes_one_info_per_entry() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "help".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert_eq!(ui.cli_log.len(), HELP_ENTRIES.len());
        for entry in &ui.cli_log {
            assert!(matches!(entry.tag, LogTag::Info));
        }
    }

    // ── parse_ports_sentinel tests ────────────────────────────────────────────

    #[test]
    fn parse_ports_sentinel_multi_port_list() {
        let result = parse_ports_sentinel(true, "[ports]Port A\x1fPort B");
        assert_eq!(
            result,
            Some(vec![
                (LogTag::Info, "Port A".to_string()),
                (LogTag::Info, "Port B".to_string()),
            ])
        );
    }

    #[test]
    fn parse_ports_sentinel_empty_port_list() {
        let result = parse_ports_sentinel(true, "[ports]");
        assert_eq!(
            result,
            Some(vec![(LogTag::Info, "no MIDI ports available".to_string())])
        );
    }

    #[test]
    fn parse_ports_sentinel_error_path() {
        let result = parse_ports_sentinel(false, "[ports-err] boom");
        assert_eq!(
            result,
            Some(vec![(LogTag::Err, "[ports-err] boom".to_string())])
        );
    }

    #[test]
    fn parse_ports_sentinel_non_sentinel_ok_returns_none() {
        let result = parse_ports_sentinel(true, "normal log message");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_ports_sentinel_non_sentinel_err_returns_none() {
        let result = parse_ports_sentinel(false, "some error");
        assert_eq!(result, None);
    }

    // ── handle_cli_note_set tests ─────────────────────────────────────────────

    #[test]
    fn note_set_valid_sends_note_set_cmd_and_logs_cmd() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        // User-facing step 4 maps to internal step 3.
        ui.cli_line = "note set 4 C4".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected NoteSet");
        assert!(
            matches!(
                cmd,
                InputCommand::NoteSet {
                    step: 3,
                    midi_note: 60,
                    velocity: 127
                }
            ),
            "expected NoteSet {{ step: 3, midi_note: 60, velocity: 127 }}, got: {cmd:?}"
        );
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
        assert!(ui.cli_log[0].text.contains("C4"));
        // The log should display the user-facing 1–16 step, not the internal 0–15.
        assert!(
            ui.cli_log[0].text.contains("note set 4"),
            "log should display user-facing step 4, got: {}",
            ui.cli_log[0].text
        );
    }

    #[test]
    fn note_set_with_velocity_uses_provided_velocity() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        // User-facing step 1 maps to internal step 0.
        ui.cli_line = "note set 1 G3 64".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected NoteSet");
        assert!(
            matches!(
                cmd,
                InputCommand::NoteSet {
                    step: 0,
                    midi_note: 55,
                    velocity: 64
                }
            ),
            "got: {cmd:?}"
        );
    }

    #[test]
    fn note_set_step_out_of_range_logs_error() {
        // Step 0 is rejected (must be 1–16).
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 0 C4".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err(), "no command for step 0");
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));

        // Step 17 is rejected (must be 1–16).
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 17 C4".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err(), "no command for step 17");
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
    }

    #[test]
    fn note_set_invalid_note_name_logs_error() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 5 X4".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err(), "no command for invalid note");
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
    }

    #[test]
    fn note_set_velocity_out_of_range_logs_error() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 2 C4 200".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err(), "no command for velocity > 127");
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
    }

    #[test]
    fn note_set_missing_note_logs_error() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 1".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err(), "no command when note missing");
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
    }

    #[test]
    fn note_set_step_16_is_valid_boundary() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        // User-facing step 16 maps to internal step 15.
        ui.cli_line = "note set 16 A4".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected NoteSet");
        assert!(matches!(cmd, InputCommand::NoteSet { step: 15, .. }));
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
    }

    #[test]
    fn note_set_rejects_trailing_tokens() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 3 C4 64 extra".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        // No InputCommand should be sent when trailing input is present.
        assert!(
            cmd_rx.try_recv().is_err(),
            "trailing tokens should suppress the InputCommand"
        );
        // Exactly one log entry, and it must be an error.
        assert_eq!(ui.cli_log.len(), 1, "expected single log entry");
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
        assert!(
            ui.cli_log[0].text.contains("unexpected trailing input"),
            "expected 'unexpected trailing input' in log, got: {}",
            ui.cli_log[0].text
        );
    }

    // ── QA additions for PR #110 Copilot fixes ────────────────────────────────
    //
    // Fix 2 (exact wording): assert the full spec-mandated message verbatim,
    // not just a substring. Catches accidental rewording in future refactors.
    #[test]
    fn note_set_trailing_tokens_uses_exact_spec_message() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 3 C4 64 extra".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        assert!(cmd_rx.try_recv().is_err());
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Err));
        // Exact-match the spec wording (Fix 2 acceptance criterion).
        assert_eq!(ui.cli_log[0].text, "note set: unexpected trailing input");
    }

    // Fix 1 (lower boundary): the spec-mandated step=1 boundary deserves an
    // explicit named test. While `note_set_with_velocity_uses_provided_velocity`
    // exercises step 1 incidentally, this test pins the boundary contract:
    // user-facing step 1 must map to internal step 0 AND the success log must
    // show "note set 1" (not "note set 0").
    #[test]
    fn note_set_step_1_is_valid_boundary() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        // User-facing step 1 maps to internal step 0.
        ui.cli_line = "note set 1 C4".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let cmd = cmd_rx.try_recv().expect("expected NoteSet");
        assert!(
            matches!(cmd, InputCommand::NoteSet { step: 0, .. }),
            "expected internal step 0 for user step 1, got: {cmd:?}"
        );
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
        assert!(
            ui.cli_log[0].text.contains("note set 1"),
            "log should display user-facing step 1, got: {}",
            ui.cli_log[0].text
        );
    }

    // Fix 1 (upper-boundary log text): existing `note_set_step_16_is_valid_boundary`
    // checks the cmd's internal step value (15) but does NOT assert the log
    // text shows the user-facing step "16". This test plugs that gap so a
    // regression where the log accidentally shows the internal 15 at the
    // upper boundary would fail loudly.
    #[test]
    fn note_set_step_16_log_displays_user_facing_step() {
        let (cmd_tx, cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "note set 16 A4".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        let _cmd = cmd_rx.try_recv().expect("expected NoteSet");
        assert_eq!(ui.cli_log.len(), 1);
        assert!(matches!(ui.cli_log[0].tag, LogTag::Cmd));
        assert!(
            ui.cli_log[0].text.contains("note set 16"),
            "log should display user-facing step 16 (not internal 15), got: {}",
            ui.cli_log[0].text
        );
        // Defence: make sure we did not accidentally emit "note set 15".
        assert!(
            !ui.cli_log[0].text.contains("note set 15"),
            "log must not leak internal 0-indexed step 15, got: {}",
            ui.cli_log[0].text
        );
    }

    // Fix 6 (HELP_ENTRIES contents): the existing help test asserts
    // `cli_log.len() == HELP_ENTRIES.len()`, which would pass even if `ok`
    // were swapped for an unrelated entry. These two tests pin the actual
    // presence of the `ok` alias entry both in the constant and in the
    // user-visible help output.
    #[test]
    fn help_entries_includes_ok_alias() {
        assert!(
            HELP_ENTRIES.iter().any(|(cmd, _)| *cmd == "ok"),
            "HELP_ENTRIES must include the `ok` alias entry"
        );
        assert!(
            HELP_ENTRIES.iter().any(|(cmd, _)| *cmd == "clear"),
            "HELP_ENTRIES must still include the `clear` entry"
        );
    }

    #[test]
    fn cli_submit_help_output_includes_ok_alias_line() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();
        ui.cli_line = "help".into();

        handle_cli_submit(&mut ui, &cmd_tx, &ctrl_tx, &state, &arc_song);

        // The `help` command renders each entry as "<cmd>  —  <desc>". Look
        // for the rendered `ok` line directly, not just the constant.
        let has_ok_line = ui
            .cli_log
            .iter()
            .any(|e| e.text.starts_with("ok  —  ") || e.text.starts_with("ok "));
        assert!(
            has_ok_line,
            "help output must include the `ok` alias line; got entries: {:?}",
            ui.cli_log.iter().map(|e| &e.text).collect::<Vec<_>>()
        );
    }

    // ── New pattern/song CLI tests ────────────────────────────────────────────

    #[test]
    fn pattern_save_pushes_cmd_log() {
        let tmp = std::env::temp_dir();
        unsafe { std::env::set_var("HOME", &tmp) };

        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();

        let parts = vec!["pattern", "save", "test-pat"];
        handle_cli_pattern_cmd(&parts, &mut ui, &state, &cmd_tx, &arc_song);

        assert!(
            ui.cli_log.iter().any(|e| matches!(e.tag, LogTag::Cmd)),
            "pattern save should push a Cmd log entry; got: {:?}",
            ui.cli_log.iter().map(|e| (&e.tag, &e.text)).collect::<Vec<_>>()
        );
        drop(ctrl_tx);
    }

    #[test]
    fn song_new_creates_empty_song() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();

        let parts = vec!["song", "new", "my-song"];
        handle_cli_song_cmd(&parts, &mut ui, &state, &cmd_tx, &arc_song);

        assert!(ui.song.is_some(), "song should be Some after 'song new'");
        assert!(
            ui.song.as_ref().unwrap().slots.is_empty(),
            "new song should have no slots"
        );
        assert_eq!(ui.song.as_ref().unwrap().name, "my-song");
        drop(ctrl_tx);
    }

    #[test]
    fn song_add_appends_slot() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();

        // First create a song
        handle_cli_song_cmd(&["song", "new", "my-song"], &mut ui, &state, &cmd_tx, &arc_song);
        // Then add a slot
        handle_cli_song_cmd(&["song", "add", "verse-A"], &mut ui, &state, &cmd_tx, &arc_song);

        let song = ui.song.as_ref().expect("song should exist");
        assert_eq!(song.slots.len(), 1, "should have 1 slot after add");
        assert_eq!(song.slots[0].filename, "verse-A.pat.toml");
        drop(ctrl_tx);
    }

    #[test]
    fn song_remove_removes_slot() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();

        // Create song and add two slots
        handle_cli_song_cmd(&["song", "new", "my-song"], &mut ui, &state, &cmd_tx, &arc_song);
        handle_cli_song_cmd(&["song", "add", "slot-A"], &mut ui, &state, &cmd_tx, &arc_song);
        handle_cli_song_cmd(&["song", "add", "slot-B"], &mut ui, &state, &cmd_tx, &arc_song);

        assert_eq!(ui.song.as_ref().unwrap().slots.len(), 2, "should have 2 slots");

        // Remove slot 1 (1-indexed)
        handle_cli_song_cmd(&["song", "remove", "1"], &mut ui, &state, &cmd_tx, &arc_song);

        let song = ui.song.as_ref().expect("song should exist");
        assert_eq!(song.slots.len(), 1, "should have 1 slot after remove");
        assert_eq!(song.slots[0].filename, "slot-B.pat.toml", "slot-B should remain");
        drop(ctrl_tx);
    }

    #[test]
    fn unknown_pattern_cmd_pushes_err() {
        let (cmd_tx, _cmd_rx, ctrl_tx, _ctrl_rx) = make_channels();
        let mut ui = UiState::new();
        let state = SequencerState::default();
        let arc_song = make_arc_song();

        let parts = vec!["pattern", "frobnicate"];
        handle_cli_pattern_cmd(&parts, &mut ui, &state, &cmd_tx, &arc_song);

        assert!(
            ui.cli_log.iter().any(|e| matches!(e.tag, LogTag::Err)),
            "unknown pattern command should push LogTag::Err"
        );
        drop(ctrl_tx);
    }
}
