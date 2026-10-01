use super::fixture::*;
use super::*;

#[test]
fn with_pitch_key_overrides_the_base_used_for_mid_note_bend() {
    let rhythm_child_key = 72;
    assert_eq!(
        noise_voice(CgbAdsr::flat(), WIDE_NOISE, TestNote::default())
            .with_pitch_key(rhythm_child_key)
            .midi_key(),
        TEST_KEY,
        "EOT identity stays the played key"
    );

    let mut square = square_voice(CgbChannelNumber::Square1, None, TestNote::default())
        .with_pitch_key(rhythm_child_key);
    square.set_track_pitch(0, 0);
    let mut expected = square_voice(
        CgbChannelNumber::Square1,
        None,
        TestNote::at_key(rhythm_child_key),
    );
    let mut acc_a = vec![(0i32, 0i32); 16];
    let mut acc_b = vec![(0i32, 0i32); 16];
    square.begin_frame(false);
    square.render(&mut acc_a, &[]);
    expected.begin_frame(false);
    expected.render(&mut acc_b, &[]);
    assert_eq!(acc_a, acc_b);
}

#[test]
fn fixed_rate_dac_correction_matches_emeralds_8_bit_formula() {
    let correction = DacCorrection::FixedRate8Bit;
    assert_eq!(correction.apply(0), 0);
    assert_eq!(correction.apply(1), 2);
    assert_eq!(correction.apply(2), 2);
    assert_eq!(correction.apply(EVEN_FREQUENCY_REGISTER_MASK), 0x7FE);
    assert_eq!(DacCorrection::None.apply(0x555), 0x555);
}

#[test]
fn fixed_rate_note_on_applies_the_dac_correction_before_the_sweep_mute_check() {
    let edge_note = TestNote {
        fine_pitch: 167,
        ..TestNote::at_key(54)
    };
    let raw_frequency = midi_key_to_cgb_freq_reg(edge_note.note_key, edge_note.fine_pitch);
    assert_eq!(raw_frequency, 0x555);
    assert_eq!(DacCorrection::FixedRate8Bit.apply(raw_frequency), 0x556);

    let sweep = upward_sweep(0, 1);
    let mut plain = square_voice(CgbChannelNumber::Square1, Some(sweep), edge_note);
    let mut plain_frame = vec![(0i32, 0i32); 8];
    plain.begin_frame(false);
    plain.render(&mut plain_frame, &[]);
    assert!(
        plain_frame.iter().any(|&(l, r)| l != 0 || r != 0),
        "the uncorrected sum sits exactly at the threshold, not over it, so the note sounds"
    );

    let mut fixed = fixed_square_voice(CgbChannelNumber::Square1, Some(sweep), edge_note);
    let mut fixed_frame = vec![(0i32, 0i32); 8];
    fixed.begin_frame(false);
    fixed.render(&mut fixed_frame, &[]);
    assert!(
        fixed_frame.iter().all(|&(l, r)| l == 0 && r == 0),
        "the DAC-corrected sum must overflow the sweep and mute the channel"
    );
    assert!(
        fixed.is_active(),
        "the overflow mutes the hardware channel without retiring the voice"
    );

    let safe_key: u8 = 48;
    fixed.set_track_pitch(i32::from(safe_key) - i32::from(edge_note.note_key), 0);
    fixed.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    fixed.begin_frame(false);
    let mut revived = vec![(0i32, 0i32); 8];
    fixed.render(&mut revived, &[]);
    assert!(
        revived.iter().any(|&(l, r)| l != 0 || r != 0),
        "a later safe trigger must revive the muted fixed-rate note"
    );
}

#[test]
fn set_track_pitch_reapplies_the_dac_correction_for_a_fixed_rate_channel() {
    let target_key = 54;
    let target_fine_pitch = 167;
    let target_note = TestNote {
        fine_pitch: target_fine_pitch,
        ..TestNote::at_key(target_key)
    };
    let mut direct = fixed_square_voice(CgbChannelNumber::Square2, None, target_note);
    let mut retuned = fixed_square_voice(
        CgbChannelNumber::Square2,
        None,
        TestNote {
            played_key: target_key,
            ..TestNote::default()
        },
    )
    .with_pitch_key(target_key);
    retuned.set_track_pitch(0, target_fine_pitch);

    let mut acc_direct = vec![(0i32, 0i32); 2048];
    let mut acc_retuned = vec![(0i32, 0i32); 2048];
    direct.begin_frame(false);
    direct.render(&mut acc_direct, &[]);
    retuned.begin_frame(false);
    retuned.render(&mut acc_retuned, &[]);
    assert_eq!(acc_direct, acc_retuned);
}
