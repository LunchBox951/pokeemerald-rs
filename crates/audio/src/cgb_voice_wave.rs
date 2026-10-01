use super::fixture::*;
use super::*;

#[test]
fn wave_note_with_active_envelope_is_audible() {
    let mut voice = wave_voice(false, TestNote::default());
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
    assert_eq!(cgb3_wave_gain_256(31), FULL_GAIN_256);
}

#[test]
fn fixed_rate_wave_note_on_audibly_differs_from_the_uncorrected_register() {
    let edge_note = TestNote {
        fine_pitch: 167,
        ..TestNote::at_key(54)
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
