/// Key represents the 12 chromatic pitch classes.
/// C4 = MIDI note 60.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    C,
    Cs,
    D,
    Ds,
    E,
    F,
    Fs,
    G,
    Gs,
    A,
    As,
    B,
}

impl Key {
    /// Number of Key variants.
    pub const COUNT: usize = 12;

    /// Convert a zero-based index (mod 12) to the corresponding Key variant.
    pub fn from_index(i: usize) -> Self {
        match i % Self::COUNT {
            0 => Key::C,
            1 => Key::Cs,
            2 => Key::D,
            3 => Key::Ds,
            4 => Key::E,
            5 => Key::F,
            6 => Key::Fs,
            7 => Key::G,
            8 => Key::Gs,
            9 => Key::A,
            10 => Key::As,
            _ => Key::B,
        }
    }

    /// Return the zero-based index of this Key variant.
    pub fn to_index(self) -> usize {
        key_index(self)
    }
}

/// Mode represents the available scales.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Major,
    NaturalMinor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    HarmonicMinor,
    MelodicMinor,
}

impl Mode {
    /// Number of Mode variants.
    pub const COUNT: usize = 9;

    /// Convert a zero-based index (mod 9) to the corresponding Mode variant.
    pub fn from_index(i: usize) -> Self {
        match i % Self::COUNT {
            0 => Mode::Major,
            1 => Mode::NaturalMinor,
            2 => Mode::Dorian,
            3 => Mode::Phrygian,
            4 => Mode::Lydian,
            5 => Mode::Mixolydian,
            6 => Mode::Locrian,
            7 => Mode::HarmonicMinor,
            _ => Mode::MelodicMinor,
        }
    }

    /// Return the zero-based index of this Mode variant.
    pub fn to_index(self) -> usize {
        mode_index(self)
    }
}

/// Semitone intervals between successive scale degrees for each mode.
/// Each row sums to 12 (one octave).
pub const SCALE_INTERVALS: [[u8; 7]; 9] = [
    [2, 2, 1, 2, 2, 2, 1], // Major
    [2, 1, 2, 2, 1, 2, 2], // NaturalMinor
    [2, 1, 2, 2, 2, 1, 2], // Dorian
    [1, 2, 2, 2, 1, 2, 2], // Phrygian
    [2, 2, 2, 1, 2, 2, 1], // Lydian
    [2, 2, 1, 2, 2, 1, 2], // Mixolydian
    [1, 2, 2, 1, 2, 2, 2], // Locrian
    [2, 1, 2, 2, 1, 3, 1], // HarmonicMinor
    [2, 1, 2, 2, 2, 2, 1], // MelodicMinor (ascending)
];

/// MIDI root note for each Key, anchored at octave 4 (C4 = 60).
const KEY_ROOT: [u8; 12] = [
    60, // C
    61, // C#
    62, // D
    63, // D#
    64, // E
    65, // F
    66, // F#
    67, // G
    68, // G#
    69, // A
    70, // A#
    71, // B
];

fn key_index(key: Key) -> usize {
    match key {
        Key::C => 0,
        Key::Cs => 1,
        Key::D => 2,
        Key::Ds => 3,
        Key::E => 4,
        Key::F => 5,
        Key::Fs => 6,
        Key::G => 7,
        Key::Gs => 8,
        Key::A => 9,
        Key::As => 10,
        Key::B => 11,
    }
}

fn mode_index(mode: Mode) -> usize {
    match mode {
        Mode::Major => 0,
        Mode::NaturalMinor => 1,
        Mode::Dorian => 2,
        Mode::Phrygian => 3,
        Mode::Lydian => 4,
        Mode::Mixolydian => 5,
        Mode::Locrian => 6,
        Mode::HarmonicMinor => 7,
        Mode::MelodicMinor => 8,
    }
}

/// Returns the 7 MIDI note numbers for one octave of the given key and mode,
/// starting at the key root in octave 4 (C4 = 60).
pub fn notes_in_key(key: Key, mode: Mode) -> [u8; 7] {
    let root = KEY_ROOT[key_index(key)];
    let intervals = SCALE_INTERVALS[mode_index(mode)];
    let mut notes = [0u8; 7];
    notes[0] = root;
    for i in 1..7 {
        notes[i] = notes[i - 1].saturating_add(intervals[i - 1]);
    }
    notes
}

static CHROMA_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

static OCTAVE_NAMES: [&str; 11] = ["-1", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Returns the note name for a MIDI note number (0–127).
/// Uses sharp notation (e.g. "C#4", "A#5"). C4 = MIDI 60, C-1 = MIDI 0.
/// Returns "?" for out-of-range values.
pub fn note_name(midi_note: u8) -> String {
    let octave = (midi_note / 12) as usize;
    let chroma = (midi_note % 12) as usize;
    if octave < OCTAVE_NAMES.len() {
        format!("{}{}", CHROMA_NAMES[chroma], OCTAVE_NAMES[octave])
    } else {
        "?".to_string()
    }
}

/// Parse a note name string (e.g. "C4", "F#3", "Bb5", "A-1") into a MIDI note number.
pub fn parse_note_name(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    // Step 1: peel leading letter (A–G, case-insensitive) → chroma
    let chroma: i32 = match bytes[0].to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let mut pos = 1usize;
    // Step 2: peel optional accidental
    let accidental: i32 = if pos < bytes.len() {
        match bytes[pos] {
            b'#' | b's' => { pos += 1; 1 }
            b'b' => { pos += 1; -1 }
            _ => 0,
        }
    } else {
        0
    };
    // Step 3: parse remaining as i8 octave (must have at least one char)
    if pos >= bytes.len() {
        return None;
    }
    let octave_str = core::str::from_utf8(&bytes[pos..]).ok()?;
    let octave: i32 = octave_str.parse::<i8>().ok()? as i32;
    // Step 4: compute MIDI note
    let midi = (octave + 1) * 12 + chroma + accidental;
    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

/// Snap `midi_note` to the nearest note in the scale defined by `key` and `mode`.
///
/// Ties resolve to the lower note. Result is always in 0–127.
pub fn snap_to_key(midi_note: u8, key: Key, mode: Mode) -> u8 {
    let intervals = SCALE_INTERVALS[mode_index(mode)];

    // Build cumulative semitone offsets within one octave: [0, i0, i0+i1, ...]
    let mut cum: [i32; 7] = [0; 7];
    for i in 1..7 {
        cum[i] = cum[i - 1] + intervals[i - 1] as i32;
    }

    let note_i32 = midi_note as i32;
    let mut best_note: i32 = 0;
    let mut best_dist: i32 = i32::MAX;

    // anchor is the key root in octave 4 (e.g. C4 = 60)
    let anchor = KEY_ROOT[key_index(key)] as i32;
    let oct_min = -((anchor + 11) / 12);
    let oct_max = (127 - anchor) / 12 + 1;

    for oct in oct_min..=oct_max {
        for &c in cum.iter() {
            let candidate = anchor + oct * 12 + c;
            if !(0..=127).contains(&candidate) {
                continue;
            }
            let dist = (note_i32 - candidate).abs();
            // Strict < so the first (lower) candidate encountered wins ties.
            // Candidates are iterated low-to-high within each octave,
            // and octaves are iterated in ascending order.
            if dist < best_dist {
                best_dist = dist;
                best_note = candidate;
            }
        }
    }

    best_note.clamp(0, 127) as u8
}

/// Advances or retreats within the 7-note scale, wrapping across octaves.
///
/// Finds the closest scale degree to `current` in the given key/mode, then
/// steps `direction` degrees (positive = up, negative = down), wrapping
/// octaves as needed. The result is clamped to MIDI 0–127.
pub fn next_note(current: u8, key: Key, mode: Mode, direction: i8) -> u8 {
    let scale = notes_in_key(key, mode);
    let intervals = SCALE_INTERVALS[mode_index(mode)];
    let root = scale[0] as i32;

    // Determine the offset of `current` from the root in semitones.
    let current_i32 = current as i32;
    // Number of octaves below/above root
    let semitones_from_root = current_i32 - root;
    // Find which octave block we are in (can be negative)
    let octave_offset = if semitones_from_root >= 0 {
        semitones_from_root / 12
    } else {
        (semitones_from_root - 11) / 12
    };
    let within_octave = semitones_from_root - octave_offset * 12;

    // Build cumulative offsets within one octave: [0, i0, i0+i1, ...]
    let mut cum: [i32; 7] = [0; 7];
    cum[0] = 0;
    for i in 1..7 {
        cum[i] = cum[i - 1] + intervals[i - 1] as i32;
    }

    // Find the closest scale degree index for `within_octave`
    let mut best_degree: usize = 0;
    let mut best_dist = i32::MAX;
    for (i, &c) in cum.iter().enumerate() {
        let dist = (within_octave - c).abs();
        if dist < best_dist {
            best_dist = dist;
            best_degree = i;
        }
    }

    // Step by direction
    let total_degree = octave_offset * 7 + best_degree as i32 + direction as i32;

    // Convert back to MIDI note
    let target_octave = if total_degree >= 0 {
        total_degree / 7
    } else {
        (total_degree - 6) / 7
    };
    let target_degree_in_oct = (total_degree - target_octave * 7) as usize;

    let midi = root + target_octave * 12 + cum[target_degree_in_oct];
    midi.clamp(0, 127) as u8
}

#[cfg(test)]
mod tests {
    use super::parse_note_name;

    #[test]
    fn test_parse_note_name_c4() {
        assert_eq!(parse_note_name("C4"), Some(60));
    }

    #[test]
    fn test_parse_note_name_lowercase() {
        assert_eq!(parse_note_name("c4"), Some(60));
    }

    #[test]
    fn test_parse_note_name_sharp() {
        assert_eq!(parse_note_name("F#3"), Some(54));
    }

    #[test]
    fn test_parse_note_name_flat() {
        // Bb2: B-flat in octave 2 → chroma 10, (2+1)*12+10 = 46
        assert_eq!(parse_note_name("Bb2"), Some(46));
    }

    #[test]
    fn test_parse_note_name_negative_octave() {
        assert_eq!(parse_note_name("A-1"), Some(9));
    }

    #[test]
    fn test_parse_note_name_g9() {
        assert_eq!(parse_note_name("G9"), Some(127));
    }

    #[test]
    fn test_parse_note_name_c_minus1() {
        assert_eq!(parse_note_name("C-1"), Some(0));
    }

    #[test]
    fn test_parse_note_name_out_of_range() {
        assert_eq!(parse_note_name("G#9"), None);
    }

    #[test]
    fn test_parse_note_name_empty() {
        assert_eq!(parse_note_name(""), None);
    }

    #[test]
    fn test_parse_note_name_invalid_letter() {
        assert_eq!(parse_note_name("X4"), None);
    }

    #[test]
    fn test_parse_note_name_missing_octave() {
        assert_eq!(parse_note_name("C"), None);
    }
}
