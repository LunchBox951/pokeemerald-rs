use super::fixture::*;
use super::*;

#[test]
fn wave_note_with_active_envelope_is_audible() {
    let mut voice = wave_voice(false, TestNote::unmodulated());
    let mut acc = vec![(0i32, 0i32); 8];
    voice.begin_frame(false);
    voice.render(&mut acc, &[]);
    assert!(
        acc.iter().any(|&(l, r)| l != 0 || r != 0),
        "a live-envelope wave note must be audible"
    );
}

#[test]
fn cgb3_wave_level_pins_the_stepped_output_levels() {
    assert_eq!(cgb3_wave_gain_256(0), SILENT_GAIN_256);
    assert_eq!(cgb3_wave_gain_256(1), SILENT_GAIN_256);
    assert_eq!(cgb3_wave_gain_256(2), QUARTER_GAIN_256);
    assert_eq!(cgb3_wave_gain_256(6), HALF_GAIN_256);
    assert_eq!(cgb3_wave_gain_256(10), THREE_QUARTER_GAIN_256);
    assert_eq!(cgb3_wave_gain_256(15), FULL_GAIN_256);
}

// Upstream indexes `gCgb3Vol` unchecked (`pokeemerald/src/m4a.c:1211`); its
// 16 bytes are followed in `.rodata` by `gClockTable` (`m4a_tables.c:168-177`),
// whose first sixteen bytes `0x00..=0x0F` leave NR32 volume bits 5-6 and the
// 75% bit 7 clear, which mutes the wave channel. Adjacency in the built ROM
// (`rom.sha1` image): `00 00 60 .. 20 20 | 00 01 02 .. 0f 10`.
#[test]
fn cgb3_wave_level_above_fifteen_reads_the_adjacent_clock_table_and_mutes() {
    for level in 16..=31 {
        assert_eq!(cgb3_wave_gain_256(level), SILENT_GAIN_256, "level {level}");
    }
}

#[test]
fn nr32_byte_decodes_to_the_five_wave_gains() {
    assert_eq!(nr32_wave_gain_256(0x00), SILENT_GAIN_256);
    assert_eq!(nr32_wave_gain_256(0x0F), SILENT_GAIN_256);
    assert_eq!(nr32_wave_gain_256(0x20), FULL_GAIN_256);
    assert_eq!(nr32_wave_gain_256(0x40), HALF_GAIN_256);
    assert_eq!(nr32_wave_gain_256(0x60), QUARTER_GAIN_256);
    assert_eq!(nr32_wave_gain_256(0x80), THREE_QUARTER_GAIN_256);
}

// Amplitude-modulated track volumes (158, 157) (`sequencer/lfo_tests.rs`) push
// the centred goal to level 18, past the 16-entry `gCgb3Vol`.
#[test]
fn amplitude_modulated_wave_above_the_table_is_muted_like_upstream() {
    let note = TestNote {
        track_right: 158,
        track_left: 157,
        ..TestNote::default()
    };
    let mut voice = wave_voice(false, note);
    let mut acc = vec![(0i32, 0i32); 256];
    voice.begin_frame(false);
    voice.render(&mut acc, &[]);
    let level = voice.envelope_volume();
    assert!(
        level > 15,
        "envelope level {level} must exceed the 16-entry gCgb3Vol table"
    );
    assert!(
        acc.iter().all(|&(l, r)| l == 0 && r == 0),
        "upstream reads gClockTable[{}] as NR32 (mute), got audible output at level {level}",
        level - 16,
    );
}

#[test]
fn fixed_rate_wave_note_on_audibly_differs_from_the_uncorrected_register() {
    let edge_note = TestNote {
        fine_pitch: 167,
        note_key: 54,
        played_key: 54,
        ..TestNote::unmodulated()
    };
    let mut fixed = wave_voice(true, edge_note);
    let mut plain = wave_voice(false, edge_note);
    let mut acc_fixed = vec![(0i32, 0i32); 2048];
    let mut acc_plain = vec![(0i32, 0i32); 2048];
    fixed.begin_frame(false);
    fixed.render(&mut acc_fixed, &[]);
    plain.begin_frame(false);
    plain.render(&mut acc_plain, &[]);
    assert_ne!(
        acc_fixed, acc_plain,
        "the DAC-corrected register must audibly differ from the uncorrected one"
    );
}
