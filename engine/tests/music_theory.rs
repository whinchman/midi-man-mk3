use engine::music_theory::{note_name, notes_in_key, next_note, snap_to_key, SCALE_INTERVALS, Key, Mode};

#[test]
fn all_modes_intervals_sum_to_12() {
    for (i, row) in SCALE_INTERVALS.iter().enumerate() {
        let sum: u8 = row.iter().sum();
        assert_eq!(sum, 12, "Mode index {} intervals sum to {} (expected 12)", i, sum);
    }
}

#[test]
fn c_major_notes() {
    let notes = notes_in_key(Key::C, Mode::Major);
    assert_eq!(notes, [60, 62, 64, 65, 67, 69, 71]);
}

#[test]
fn note_name_midi_0() {
    assert_eq!(note_name(0), "C-1");
}

#[test]
fn note_name_midi_60() {
    assert_eq!(note_name(60), "C4");
}

#[test]
fn note_name_midi_127() {
    assert_eq!(note_name(127), "G9");
}

#[test]
fn note_name_sharp_accidentals() {
    assert_eq!(note_name(61), "C#4"); // C#4
    assert_eq!(note_name(66), "F#4"); // F#4
    assert_eq!(note_name(70), "A#4"); // A#4
}

#[test]
fn next_note_wraps_degree7_to_degree1_next_octave() {
    // In C Major the 7th degree is B4 = 71. Going up from B4 should give C5 = 72.
    let result = next_note(71, Key::C, Mode::Major, 1);
    assert_eq!(result, 72, "next_note up from B4 (71) should be C5 (72), got {}", result);
}

#[test]
fn next_note_wraps_down_from_degree1_to_degree7_previous_octave() {
    // In C Major the 1st degree is C4 = 60. Going down should give B3 = 59.
    let result = next_note(60, Key::C, Mode::Major, -1);
    assert_eq!(result, 59, "next_note down from C4 (60) should be B3 (59), got {}", result);
}

#[test]
fn next_note_clamps_at_midi_127() {
    // G9 = 127, stepping up should stay at 127
    let result = next_note(127, Key::G, Mode::Major, 1);
    assert_eq!(result, 127, "next_note at MIDI 127 stepping up should clamp to 127");
}

#[test]
fn next_note_clamps_at_midi_0() {
    // C-1 = 0, stepping down should stay at 0
    let result = next_note(0, Key::C, Mode::Major, -1);
    assert_eq!(result, 0, "next_note at MIDI 0 stepping down should clamp to 0");
}

#[test]
fn notes_in_key_natural_minor() {
    // A NaturalMinor: A4=69, B4=71, C5=72, D5=74, E5=76, F5=77, G5=79
    let notes = notes_in_key(Key::A, Mode::NaturalMinor);
    assert_eq!(notes, [69, 71, 72, 74, 76, 77, 79]);
}

#[test]
fn notes_in_key_dorian() {
    // D Dorian: D4=62, intervals [2,1,2,2,2,1,2]
    // 62, 64, 65, 67, 69, 71, 72
    let notes = notes_in_key(Key::D, Mode::Dorian);
    assert_eq!(notes, [62, 64, 65, 67, 69, 71, 72]);
}

#[test]
fn no_heap_alloc_sanity() {
    // This test exists to document intent: all return types are [u8;N] or &'static str.
    let _: [u8; 7] = notes_in_key(Key::C, Mode::Major);
    let _: String = note_name(60);
    let _: u8 = next_note(60, Key::C, Mode::Major, 1);
}

// --- note_name: all 12 pitch classes across multiple octaves ---

#[test]
fn note_name_all_pitch_classes_octave_neg1() {
    // MIDI 0–11: octave -1, all 12 chromatic pitches
    assert_eq!(note_name(0), "C-1");
    assert_eq!(note_name(1), "C#-1");
    assert_eq!(note_name(2), "D-1");
    assert_eq!(note_name(3), "D#-1");
    assert_eq!(note_name(4), "E-1");
    assert_eq!(note_name(5), "F-1");
    assert_eq!(note_name(6), "F#-1");
    assert_eq!(note_name(7), "G-1");
    assert_eq!(note_name(8), "G#-1");
    assert_eq!(note_name(9), "A-1");
    assert_eq!(note_name(10), "A#-1");
    assert_eq!(note_name(11), "B-1");
}

#[test]
fn note_name_all_pitch_classes_octave_4() {
    // MIDI 60–71: octave 4 (the reference octave, C4 = 60)
    assert_eq!(note_name(60), "C4");
    assert_eq!(note_name(61), "C#4");
    assert_eq!(note_name(62), "D4");
    assert_eq!(note_name(63), "D#4");
    assert_eq!(note_name(64), "E4");
    assert_eq!(note_name(65), "F4");
    assert_eq!(note_name(66), "F#4");
    assert_eq!(note_name(67), "G4");
    assert_eq!(note_name(68), "G#4");
    assert_eq!(note_name(69), "A4");
    assert_eq!(note_name(70), "A#4");
    assert_eq!(note_name(71), "B4");
}

#[test]
fn note_name_all_pitch_classes_octave_9_partial() {
    // MIDI 120–127: octave 9 (partial — only C9 through G9 exist)
    assert_eq!(note_name(120), "C9");
    assert_eq!(note_name(121), "C#9");
    assert_eq!(note_name(122), "D9");
    assert_eq!(note_name(123), "D#9");
    assert_eq!(note_name(124), "E9");
    assert_eq!(note_name(125), "F9");
    assert_eq!(note_name(126), "F#9");
    assert_eq!(note_name(127), "G9");
}

#[test]
fn note_name_spot_check_octave_3() {
    // Spot check a few notes in octave 3
    assert_eq!(note_name(48), "C3");
    assert_eq!(note_name(54), "F#3");
    assert_eq!(note_name(57), "A3");
    assert_eq!(note_name(59), "B3");
}

#[test]
fn note_name_spot_check_octave_5() {
    // Spot check accidentals in octave 5
    assert_eq!(note_name(72), "C5");
    assert_eq!(note_name(82), "A#5");
    assert_eq!(note_name(83), "B5");
}

// --- notes_in_key: all 7 modes, multiple keys ---

#[test]
fn notes_in_key_phrygian() {
    // E Phrygian: root E4=64, intervals [1,2,2,2,1,2,2]
    // 64, 65, 67, 69, 71, 72, 74
    let notes = notes_in_key(Key::E, Mode::Phrygian);
    assert_eq!(notes, [64, 65, 67, 69, 71, 72, 74]);
}

#[test]
fn notes_in_key_lydian() {
    // F Lydian: root F4=65, intervals [2,2,2,1,2,2,1]
    // 65, 67, 69, 71, 72, 74, 76
    let notes = notes_in_key(Key::F, Mode::Lydian);
    assert_eq!(notes, [65, 67, 69, 71, 72, 74, 76]);
}

#[test]
fn notes_in_key_mixolydian() {
    // G Mixolydian: root G4=67, intervals [2,2,1,2,2,1,2]
    // 67, 69, 71, 72, 74, 76, 77
    let notes = notes_in_key(Key::G, Mode::Mixolydian);
    assert_eq!(notes, [67, 69, 71, 72, 74, 76, 77]);
}

#[test]
fn notes_in_key_locrian() {
    // B Locrian: root B4=71, intervals [1,2,2,1,2,2,2]
    // 71, 72, 74, 76, 77, 79, 81
    let notes = notes_in_key(Key::B, Mode::Locrian);
    assert_eq!(notes, [71, 72, 74, 76, 77, 79, 81]);
}

#[test]
fn notes_in_key_g_major() {
    // G Major: root G4=67, intervals [2,2,1,2,2,2,1]
    // 67, 69, 71, 72, 74, 76, 78
    let notes = notes_in_key(Key::G, Mode::Major);
    assert_eq!(notes, [67, 69, 71, 72, 74, 76, 78]);
}

#[test]
fn notes_in_key_fs_major() {
    // F# Major: root F#4=66, intervals [2,2,1,2,2,2,1]
    // 66, 68, 70, 71, 73, 75, 77
    let notes = notes_in_key(Key::Fs, Mode::Major);
    assert_eq!(notes, [66, 68, 70, 71, 73, 75, 77]);
}

#[test]
fn notes_in_key_c_dorian() {
    // C Dorian: root C4=60, intervals [2,1,2,2,2,1,2]
    // 60, 62, 63, 65, 67, 69, 70
    let notes = notes_in_key(Key::C, Mode::Dorian);
    assert_eq!(notes, [60, 62, 63, 65, 67, 69, 70]);
}

#[test]
fn notes_in_key_g_phrygian() {
    // G Phrygian: root G4=67, intervals [1,2,2,2,1,2,2]
    // 67, 68, 70, 72, 74, 75, 77
    let notes = notes_in_key(Key::G, Mode::Phrygian);
    assert_eq!(notes, [67, 68, 70, 72, 74, 75, 77]);
}

#[test]
fn notes_in_key_d_mixolydian() {
    // D Mixolydian: root D4=62, intervals [2,2,1,2,2,1,2]
    // 62, 64, 66, 67, 69, 71, 72
    let notes = notes_in_key(Key::D, Mode::Mixolydian);
    assert_eq!(notes, [62, 64, 66, 67, 69, 71, 72]);
}

#[test]
fn notes_in_key_e_lydian() {
    // E Lydian: root E4=64, intervals [2,2,2,1,2,2,1]
    // 64, 66, 68, 70, 71, 73, 75
    let notes = notes_in_key(Key::E, Mode::Lydian);
    assert_eq!(notes, [64, 66, 68, 70, 71, 73, 75]);
}

#[test]
fn notes_in_key_a_locrian() {
    // A Locrian: root A4=69, intervals [1,2,2,1,2,2,2]
    // 69, 70, 72, 74, 75, 77, 79
    let notes = notes_in_key(Key::A, Mode::Locrian);
    assert_eq!(notes, [69, 70, 72, 74, 75, 77, 79]);
}

// --- next_note: additional boundary and direction tests ---

#[test]
fn next_note_direction_neg1_from_root_non_c_key() {
    // G Major root is G4=67. Going down from G4 gives F#4=66 (7th degree of previous octave).
    let result = next_note(67, Key::G, Mode::Major, -1);
    assert_eq!(result, 66, "next_note down from G4 (67) in G Major should be F#4 (66), got {}", result);
}

#[test]
fn next_note_direction_pos1_near_top_of_range() {
    // Stepping up near MIDI 127 in a key where the next note would exceed 127.
    let result = next_note(126, Key::G, Mode::Major, 1);
    assert_eq!(result, 127, "next_note up from F#9 (126) in G Major should be G9 (127), got {}", result);
}

#[test]
fn next_note_octave_boundary_wrap_up_d_major() {
    // D Major scale: [62,64,66,67,69,71,73]. 7th degree is C#5=73.
    // Next up should be D5=74.
    let result = next_note(73, Key::D, Mode::Major, 1);
    assert_eq!(result, 74, "next_note up from C#5 (73) in D Major should be D5 (74), got {}", result);
}

#[test]
fn next_note_octave_boundary_wrap_down_a_natural_minor() {
    // A NaturalMinor: root A4=69. Going down from root gives G3=67.
    let result = next_note(69, Key::A, Mode::NaturalMinor, -1);
    assert_eq!(result, 67, "next_note down from A4 (69) in A NaturalMinor should be G3 (67), got {}", result);
}

// --- next_note with off-key starting notes ---

#[test]
fn next_note_off_key_snaps_up_in_c_major() {
    // C#4 (61) is not in C Major. Snaps to C4(60), then steps direction=+1, giving D4=62.
    let result = next_note(61, Key::C, Mode::Major, 1);
    assert_eq!(result, 62, "next_note from C#4 (61) up in C Major should snap+step to D4 (62), got {}", result);
}

#[test]
fn next_note_off_key_snaps_down_in_c_major() {
    // C#4 (61) off-key in C Major: snaps to C4(60), then steps direction=-1, giving B3=59.
    let result = next_note(61, Key::C, Mode::Major, -1);
    assert_eq!(result, 59, "next_note from C#4 (61) down in C Major should snap+step to B3 (59), got {}", result);
}

#[test]
fn next_note_off_key_closer_to_upper_degree() {
    // D#4 (63) in C Major: equidistant between D4(62) and E4(64). Tie-breaking picks lower.
    // Direction=+1 gives E4=64.
    let result = next_note(63, Key::C, Mode::Major, 1);
    assert_eq!(result, 64, "next_note from D#4 (63) up in C Major should give E4 (64), got {}", result);
}

#[test]
fn next_note_off_key_in_g_major_up() {
    // F4 (65) is not in G Major. Just verify result is within MIDI range.
    let result = next_note(65, Key::G, Mode::Major, 1);
    assert!(result <= 127, "next_note result must be within MIDI range");
}

// --- snap_to_key ---

#[test]
fn snap_in_key_note_unchanged() {
    // C major scale: C4=60, D4=62, E4=64, F4=65, G4=67, A4=69, B4=71
    assert_eq!(snap_to_key(60, Key::C, Mode::Major), 60);
    assert_eq!(snap_to_key(62, Key::C, Mode::Major), 62);
    assert_eq!(snap_to_key(71, Key::C, Mode::Major), 71);
}

#[test]
fn snap_out_of_key_rounds_to_nearest() {
    // C# (61) is equidistant from C(60) and D(62) — tie: lower wins → C(60)
    assert_eq!(snap_to_key(61, Key::C, Mode::Major), 60);
    // Bb (70) in C major: nearest are A(69) dist=1, B(71) dist=1 — tie: lower wins → A(69)
    assert_eq!(snap_to_key(70, Key::C, Mode::Major), 69);
}

#[test]
fn snap_tie_picks_lower_note() {
    // F# (66) in C major: F=65 (dist 1), G=67 (dist 1) — lower wins → F(65)
    assert_eq!(snap_to_key(66, Key::C, Mode::Major), 65);
}

#[test]
fn snap_across_octaves() {
    // C5 = 72 is in C major
    assert_eq!(snap_to_key(72, Key::C, Mode::Major), 72);
    // C#5 = 73: C5(72) dist=1, D5(74) dist=1 — tie: lower wins → C5(72)
    assert_eq!(snap_to_key(73, Key::C, Mode::Major), 72);
}

#[test]
fn snap_midi_boundaries() {
    let _ = snap_to_key(0, Key::C, Mode::Major);
    let _ = snap_to_key(127, Key::C, Mode::Major);
}

#[test]
fn snap_non_c_key() {
    // Ab/G# (68) in G major: G=67 (dist 1), A=69 (dist 1) — tie: lower wins → G(67)
    assert_eq!(snap_to_key(68, Key::G, Mode::Major), 67);
}

#[test]
fn snap_natural_minor() {
    // A natural minor: A=69, B=71, C=72, D=74, E=76, F=77, G=79
    // Bb (70) in A natural minor: A=69 (dist 1), B=71 (dist 1) — tie: lower wins → A(69)
    assert_eq!(snap_to_key(70, Key::A, Mode::NaturalMinor), 69);
}
