use super::fixture::*;
use super::*;

#[test]
fn noise_control_byte_sets_width_bit_only_for_odd_period() {
    assert_eq!(midi_key_to_noise_control(TEST_KEY) & NOISE_WIDTH_BIT, 0);
    assert_eq!(
        noise_control_byte(TEST_KEY, NARROW_NOISE) & NOISE_WIDTH_BIT,
        NOISE_WIDTH_BIT
    );
    assert_eq!(
        noise_control_byte(TEST_KEY, WIDE_NOISE) & NOISE_WIDTH_BIT,
        0
    );
}

#[test]
fn noise_period_bit_drives_narrow_lfsr_mode() {
    let narrow = noise_voice(CgbAdsr::flat(), NARROW_NOISE, TestNote::default());
    assert_eq!(narrow.noise_is_narrow(), Some(true));
    let wide = noise_voice(CgbAdsr::flat(), WIDE_NOISE, TestNote::default());
    assert_eq!(wide.noise_is_narrow(), Some(false));
}

#[test]
fn noise_retune_preserves_the_width_bit() {
    let mut narrow = noise_voice(CgbAdsr::flat(), NARROW_NOISE, TestNote::default());
    let octave_up = 12;
    narrow.set_track_pitch(octave_up, 0);
    assert_eq!(narrow.noise_is_narrow(), Some(true));
}

#[test]
fn release_start_volume_write_retriggers_the_noise_lfsr() {
    // Release start is a volume write; upstream's channel-4 trigger
    // resets the LFSR (`m4a.c:1060-1069,1219-1226`; `mgba/src/gb/audio.c:374`).
    let adsr = CgbAdsr {
        attack: 0,
        decay: 0,
        sustain: MAX_ADSR_LEVEL,
        release: 4,
    };
    let mut voice = noise_voice(adsr, WIDE_NOISE, TestNote::default());
    let at_note_on = voice.noise_lfsr();

    let mut acc = vec![(0i32, 0i32); 64];
    for _ in 0..3 {
        voice.begin_frame(false);
        voice.render(&mut acc, &[]);
    }
    let walked_away = voice.noise_lfsr();
    assert_ne!(
        walked_away, at_note_on,
        "sanity: a sustained note must have walked the LFSR away from note-on"
    );

    voice.note_off();
    assert_eq!(
        voice.noise_lfsr(),
        walked_away,
        "sanity: note_off must not retrigger before the next begin_frame -- upstream applies \
         a tick's writes inside the following CgbSound call, not synchronously"
    );
    voice.begin_frame(false);
    assert_eq!(
        voice.noise_lfsr(),
        at_note_on,
        "note_off's release-start volume write (release != 0) must retrigger channel 4 at \
         the next begin_frame, clearing the LFSR"
    );
}
