use super::fixture::*;
use super::*;

const SAFE_KEY: u8 = 48;
const OVERFLOW_KEY: u8 = 120;
const FRAMES: usize = 4;
const FRAME_SAMPLES: usize = 64;

fn paced_attack() -> CgbAdsr {
    CgbAdsr {
        attack: 1,
        decay: 1,
        sustain: MAX_ADSR_LEVEL,
        release: 0,
    }
}

fn sweeping_voice(start_key: u8, retune_key: u8) -> CgbVoice {
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        Some(upward_sweep(3, 1)),
        paced_attack(),
        TestNote::at_key(start_key),
    );
    voice.set_track_pitch(i32::from(retune_key) - i32::from(start_key), 0);
    voice
}

fn first_frame_sounded(voice: &mut CgbVoice) -> bool {
    let mut sounded = false;
    for _ in 0..FRAMES {
        voice.begin_frame(false);
        let mut acc = vec![(0i32, 0i32); FRAME_SAMPLES];
        voice.render(&mut acc, &[]);
        sounded |= acc.iter().any(|&(l, r)| l != 0 || r != 0);
    }
    sounded
}

fn sweep_shadow(voice: &CgbVoice) -> Option<u16> {
    voice
        .square_oscillator()
        .and_then(SquareChannel::sweep_frequency)
}

/// The note-on trigger lands after the pre-render pitch write
/// (`m4a.c:1197-1202,1219-1225`), so the sweep reloads from the retuned frequency.
#[test]
fn a_safe_retune_before_the_first_frame_reloads_the_sweep_shadow_and_plays() {
    let mut voice = sweeping_voice(36, SAFE_KEY);
    let mut reference = sweeping_voice(SAFE_KEY, SAFE_KEY);
    assert!(first_frame_sounded(&mut voice));
    assert!(first_frame_sounded(&mut reference));
    assert_eq!(sweep_shadow(&voice), sweep_shadow(&reference));
}

#[test]
fn a_retune_into_overflow_before_the_first_frame_mutes_the_channel() {
    let mut voice = sweeping_voice(SAFE_KEY, OVERFLOW_KEY);
    assert!(!first_frame_sounded(&mut voice));
    assert!(voice.is_active(), "the envelope outlives a hardware mute");
}

#[test]
fn a_retune_out_of_overflow_before_the_first_frame_unmutes_the_channel() {
    let mut voice = sweeping_voice(OVERFLOW_KEY, SAFE_KEY);
    assert!(first_frame_sounded(&mut voice));
}

/// A note stopped before its first pass bypasses the note-on hardware writes
/// (`m4a.c:1043-1056`), so it owes no sweep reload.
#[test]
fn a_note_stopped_before_its_first_frame_owes_no_sweep_trigger() {
    for (start_key, retune_key) in [(OVERFLOW_KEY, SAFE_KEY), (SAFE_KEY, OVERFLOW_KEY)] {
        for extra_iteration in [false, true] {
            let mut voice = sweeping_voice(start_key, retune_key);
            let shadow_before = sweep_shadow(&voice);
            voice.note_off();
            voice.begin_frame(extra_iteration);
            let mut acc = vec![(0i32, 0i32); FRAME_SAMPLES];
            voice.render(&mut acc, &[]);
            assert!(!voice.is_active());
            assert!(acc.iter().all(|&(l, r)| l == 0 && r == 0));
            assert_eq!(sweep_shadow(&voice), shadow_before);
        }
    }
}
