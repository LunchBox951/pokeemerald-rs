//! CGB output must remain outside the DirectSound reverb ring.

use super::test_support::*;
use super::*;

/// CGB output must never reach the DirectSound reverb ring
/// (`crate::reverb`'s module doc carries the upstream citation).
#[test]
fn a_cgb_only_mix_never_feeds_the_directsound_reverb_ring() {
    const REVERB_LEVEL: u8 = 100;
    const FRAMES: usize = 4;

    let mut mixer =
        Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES).with_reverb_level(REVERB_LEVEL);
    assert!(mixer.add_cgb_voice(cgb_keyed_voice(0, 60)));

    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut heard_cgb = false;
    for _ in 0..FRAMES {
        mixer.mix_frame(&mut out);
        heard_cgb |= out.iter().any(|&sample| sample != 0.0);
    }

    assert!(heard_cgb, "the CGB voice must actually be audible");
    assert!(
        !mixer.has_pending_reverb(),
        "CGB output must not enter the DirectSound reverb ring",
    );
}

/// A CGB voice sounding alongside DirectSound must still leave the reverb
/// ring exactly as a DirectSound-only mix would, once both retire: the tail
/// echoed back must match bit-for-bit against a twin mixer that never had a
/// CGB voice at all.
#[test]
fn a_cgb_voice_alongside_directsound_leaves_the_reverb_tap_directsound_only() {
    const REVERB_LEVEL: u8 = 100;
    const DIRECT_SAMPLE: i8 = 40;
    const DIRECT_TRACK: usize = 0;
    const CGB_TRACK: usize = 1;
    const CGB_KEY: u8 = 60;
    const MAX_SETTLE_FRAMES: usize = 32;

    let mut combined =
        Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES).with_reverb_level(REVERB_LEVEL);
    let mut direct_only =
        Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES).with_reverb_level(REVERB_LEVEL);
    assert!(combined.add_voice(constant_voice(DIRECT_SAMPLE, DIRECT_TRACK)));
    assert!(direct_only.add_voice(constant_voice(DIRECT_SAMPLE, DIRECT_TRACK)));
    assert!(combined.add_cgb_voice(cgb_keyed_voice(CGB_TRACK, CGB_KEY)));

    let mut combined_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut direct_out = vec![0.0; SAMPLES_PER_FRAME * 2];

    combined.mix_frame(&mut combined_out);
    direct_only.mix_frame(&mut direct_out);
    assert_ne!(
        combined_out, direct_out,
        "the CGB voice must audibly change the combined output"
    );

    combined.note_off_track(CGB_TRACK, CGB_KEY);

    let mut settled = false;
    for _ in 0..MAX_SETTLE_FRAMES {
        combined.mix_frame(&mut combined_out);
        direct_only.mix_frame(&mut direct_out);
        if combined.is_idle() && direct_only.is_idle() {
            settled = true;
            break;
        }
    }
    assert!(
        settled,
        "both mixers must fall silent within the settle window"
    );

    let ring_frames = crate::reverb::DELAY_SAMPLES.div_ceil(SAMPLES_PER_FRAME);
    for offset in 0..ring_frames {
        combined.mix_frame(&mut combined_out);
        direct_only.mix_frame(&mut direct_out);
        assert_eq!(
            combined_out, direct_out,
            "reverb tail must match a DirectSound-only mix at offset {offset}"
        );
    }
}
