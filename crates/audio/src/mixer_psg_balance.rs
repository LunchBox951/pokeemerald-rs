//! PSG-to-DirectSound balance, pinned with literal hardware-derived values.
//!
//! Emerald mixes the PSG at 100% (`SOUND_ALL_MIX_FULL`) with NR50 = 0x77
//! (`pokeemerald/src/m4a.c:275,370-372`). One PSG volume or nibble unit is then
//! `8 * (1 + 7) >> (4 - 2) = 16` mGBA hardware units (`mgba/src/gb/audio.c:778-793`,
//! `mgba/src/gba/audio.c:362-366`) against `4` per FIFO s8 unit
//! (`mgba/src/gba/audio.c:368-384`): four accumulator units, which the mixer
//! renders as `n / 128`. Every expectation here is a literal, never derived from
//! the production scale constants.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;
use crate::psg::WaveChannel;

const TRACK: usize = 0;
const KEY: u8 = 60;
/// Track volume whose flat envelope settles on hardware volume 1.
const HARDWARE_VOLUME_1_TRACK: u8 = 16;
/// Track volume whose flat envelope settles on hardware volume 15.
const HARDWARE_VOLUME_15_TRACK: u8 = 128;
/// `constant_voice` source level that renders exactly one FIFO unit
/// (`voice.rs:132-157`: full-gain 254 * 2 >> 8 = 1).
const ONE_FIFO_UNIT_SOURCE: i8 = 2;

fn unit(n: i16) -> f32 {
    f32::from(n) / OUTPUT_SCALE
}

/// A mixed sample in whole FIFO units; every output is an exact multiple of 1/128.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a rounded sample within a few hundred units fits i32"
)]
fn units(sample: f32) -> i32 {
    (sample * OUTPUT_SCALE).round() as i32
}

fn frame(mixer: &mut Mixer) -> Vec<f32> {
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    out
}

fn mixer() -> Mixer {
    Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES)
}

/// Lowest and highest rendered sample.
fn extrema(out: &[f32]) -> (f32, f32) {
    out.iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &s| (lo.min(s), hi.max(s)))
}

fn assert_extrema(out: &[f32], low: i16, high: i16) {
    let (lo, hi) = extrema(out);
    assert_eq!(lo.to_bits(), unit(low).to_bits(), "low {lo} vs {low}/128");
    assert_eq!(
        hi.to_bits(),
        unit(high).to_bits(),
        "high {hi} vs {high}/128"
    );
}

fn assert_every_sample(out: &[f32], n: i16) {
    assert!(out.iter().all(|s| s.to_bits() == unit(n).to_bits()));
}

fn pcm_voice(source: i8) -> Voice {
    constant_voice(source, TRACK)
}

fn wave_voice(nibbles: u8, track_volume: u8) -> CgbVoice {
    wave_asset_voice("balance", nibbles, track_volume)
}

fn wave_asset_voice(asset: &str, nibbles: u8, track_volume: u8) -> CgbVoice {
    CgbVoice::wave(
        Arc::from(asset),
        WaveChannel::decode_wave_ram(&[nibbles; 16]),
        CgbAdsr::flat(),
        false,
        KEY,
        0,
        track_volume,
        track_volume,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        KEY,
        TRACK,
        0,
        0,
        0,
    )
}

fn square_voice(track_volume: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        None,
        CgbAdsr::flat(),
        KEY,
        0,
        track_volume,
        track_volume,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        KEY,
        TRACK,
        0,
        0,
        0,
    )
}

fn noise_voice(width_selector: u8, track_volume: u8) -> CgbVoice {
    CgbVoice::noise(
        CgbAdsr::flat(),
        KEY,
        width_selector,
        track_volume,
        track_volume,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        KEY,
        TRACK,
        0,
        0,
        0,
    )
}

fn render_cgb(voice: CgbVoice) -> Vec<f32> {
    let mut mixer = mixer();
    assert!(mixer.add_cgb_voice(voice));
    frame(&mut mixer)
}

fn render_cgb_with_pcm(voice: CgbVoice, pcm_source: i8) -> Vec<f32> {
    let mut mixer = mixer();
    assert!(mixer.add_voice(pcm_voice(pcm_source)));
    assert!(mixer.add_cgb_voice(voice));
    frame(&mut mixer)
}

#[test]
fn the_pcm_reference_voice_renders_exactly_one_fifo_unit() {
    let mut mixer = mixer();
    assert!(mixer.add_voice(pcm_voice(ONE_FIFO_UNIT_SOURCE)));
    assert_every_sample(&frame(&mut mixer), 1);
}

#[test]
fn one_full_volume_wave_nibble_step_is_four_fifo_units() {
    // Cold latch, then the settled nibble: 9 -> 9 - 8 = +1 step, 4 units.
    let nine = render_cgb(wave_voice(0x99, HARDWARE_VOLUME_15_TRACK));
    assert_eq!(nine[nine.len() - 2].to_bits(), unit(4).to_bits());
    assert_eq!(nine[nine.len() - 1].to_bits(), unit(4).to_bits());
    let eight = render_cgb(wave_voice(0x88, HARDWARE_VOLUME_15_TRACK));
    assert_eq!(eight[eight.len() - 1].to_bits(), unit(0).to_bits());
}

#[test]
fn the_full_wave_range_spans_sixty_fifo_units() {
    // Nibbles 0..15 at full volume span 15 steps * 4 = 60 units (-32..+28).
    let low = render_cgb(wave_voice(0x00, HARDWARE_VOLUME_15_TRACK));
    assert_extrema(&low, -32, -32);
    let high = render_cgb(wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK));
    assert_eq!(high[high.len() - 1].to_bits(), unit(28).to_bits());
}

#[test]
fn a_wave_nibble_step_adds_to_a_fifo_unit_by_the_same_ratio() {
    let mixed = render_cgb_with_pcm(
        wave_voice(0x99, HARDWARE_VOLUME_15_TRACK),
        ONE_FIFO_UNIT_SOURCE,
    );
    assert_eq!(mixed[mixed.len() - 1].to_bits(), unit(5).to_bits());
}

#[test]
fn a_volume_one_square_swings_four_fifo_units() {
    let out = render_cgb(square_voice(HARDWARE_VOLUME_1_TRACK));
    assert_extrema(&out, -2, 2);
}

#[test]
fn a_volume_fifteen_square_swings_sixty_fifo_units() {
    let out = render_cgb(square_voice(HARDWARE_VOLUME_15_TRACK));
    assert_extrema(&out, -30, 30);
}

#[test]
fn a_square_mixes_with_a_fifo_unit_at_the_hardware_ratio() {
    let out = render_cgb_with_pcm(square_voice(HARDWARE_VOLUME_1_TRACK), ONE_FIFO_UNIT_SOURCE);
    assert_extrema(&out, -1, 3);
}

#[test]
fn a_volume_one_noise_swings_four_fifo_units_at_either_width() {
    for width_selector in [0, 1] {
        let out = render_cgb(noise_voice(width_selector, HARDWARE_VOLUME_1_TRACK));
        assert_extrema(&out, -2, 2);
    }
}

#[test]
fn a_volume_fifteen_noise_swings_sixty_fifo_units() {
    let out = render_cgb(noise_voice(0, HARDWARE_VOLUME_15_TRACK));
    assert_extrema(&out, -30, 30);
}

#[test]
fn a_noise_mixes_with_a_fifo_unit_at_the_hardware_ratio() {
    let out = render_cgb_with_pcm(
        noise_voice(0, HARDWARE_VOLUME_1_TRACK),
        ONE_FIFO_UNIT_SOURCE,
    );
    assert_extrema(&out, -1, 3);
}

/// Mixes `voice` over the constant PCM level `source` and returns
/// `(pcm_only, lo, hi)` in FIFO units after the mixer's output scale.
fn pcm_level_and_swing(voice: CgbVoice, source: i8) -> (i32, i32, i32) {
    let pcm_only = {
        let mut mixer = mixer();
        assert!(mixer.add_voice(pcm_voice(source)));
        frame(&mut mixer)
    };
    let (p_lo, p_hi) = extrema(&pcm_only);
    assert_eq!(p_lo.to_bits(), p_hi.to_bits(), "constant PCM");
    let pcm = units(p_lo);
    let (lo, hi) = extrema(&render_cgb_with_pcm(voice, source));
    (pcm, units(lo), units(hi))
}

// The tests below pin the upstream order of operations. mGBA builds every PSG
// channel UNIPOLAR (`mgba/src/gb/audio.c:641,543-574,916-918`), sums it into
// the signed FIFO total (`mgba/src/gba/audio.c:360-389`) and saturates THAT sum
// in `_applyBias` (`mgba/src/gba/audio.c:343-351`). The port renders each
// channel about its midpoint, so a saturating sum has to see the midpoint
// restored first and lose it again afterwards: it saturates `centred + offset`
// and subtracts the offset (`Mixer::mix_frame`). A volume-15 square (0 or 60
// units, midpoint 30) therefore has `127 - P` units of headroom over PCM `P`
// and not the `30 + (127 - P)` that clipping the centred value would leave.

#[test]
fn a_loud_square_keeps_upstream_clip_headroom_over_loud_pcm() {
    // Source 100 renders P = 99. Upstream: lo = 99, hi = min(99 + 60, 127) = 127,
    // swing 28; DC removal then subtracts the 30-unit midpoint: 69 and 97.
    let (pcm, lo, hi) = pcm_level_and_swing(square_voice(HARDWARE_VOLUME_15_TRACK), 100);
    assert_eq!(pcm, 99);
    assert_eq!(
        hi - lo,
        60.min(127 - pcm),
        "upstream square swing over PCM {pcm}"
    );
    assert_eq!((lo, hi), (69, 97));
}

#[test]
fn a_loud_noise_keeps_upstream_clip_headroom_over_loud_pcm() {
    // Noise is unipolar 0 or 4 * volume as well (`mgba/src/gb/audio.c:641`).
    // Its latch starts cold at 0 with no midpoint, so judge the settled tail
    // of the frame, where both states have been latched at volume 15.
    let out = render_cgb_with_pcm(noise_voice(0, HARDWARE_VOLUME_15_TRACK), 100);
    let (lo, hi) = extrema(&out[out.len() / 2..]);
    assert_eq!((units(lo), units(hi)), (69, 97));
}

#[test]
fn a_loud_wave_keeps_upstream_clip_headroom_over_loud_pcm() {
    // Settled full-gain nibble 15 is unipolar 60 with a 32-unit midpoint
    // (`mgba/src/gb/audio.c:543-574`): 99 + 60 saturates to 127, minus 32.
    let (pcm, _, _) = pcm_level_and_swing(wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK), 100);
    assert_eq!(pcm, 99);
    let out = render_cgb_with_pcm(wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK), 100);
    assert_eq!(out[out.len() - 1].to_bits(), unit(127 - 32).to_bits());
}

#[test]
fn a_psg_over_negative_pcm_stays_within_the_transport_range() {
    // Source -128 renders P = -127. Hardware holds the square at -127 / -67
    // (unipolar, no clipping; `mgba/src/gba/audio.c:343-351` keeps the offset).
    // Removing the 30-unit midpoint would give -157 / -97, below the s8 floor
    // the output transport (`f32_to_i16` clamps at -1.0) can carry, which
    // would squash the low state anyway while f32 sinks heard a wider swing.
    // The mixer therefore removes the midpoint only down to -128: -128 / -97,
    // identical on every sink and no lower than the centred clip it replaced.
    let (pcm, lo, hi) = pcm_level_and_swing(square_voice(HARDWARE_VOLUME_15_TRACK), -128);
    assert_eq!(pcm, -127);
    assert_eq!((lo, hi), (-128, -97));
}

#[test]
fn mixed_output_never_leaves_the_transport_range() {
    // Every PSG kind over every PCM extreme stays in [-1.0, 127/128].
    for source in [-128, -100, -50, 0, 50, 100, 127] {
        for voice in [
            square_voice(HARDWARE_VOLUME_15_TRACK),
            noise_voice(0, HARDWARE_VOLUME_15_TRACK),
            wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK),
            wave_voice(0x00, HARDWARE_VOLUME_15_TRACK),
        ] {
            let out = render_cgb_with_pcm(voice, source);
            let (lo, hi) = extrema(&out);
            assert!(lo >= -1.0, "source {source}: low {lo}");
            assert!(hi < 1.0, "source {source}: high {hi}");
        }
    }
}

#[test]
fn the_summed_output_saturates_before_the_wave_midpoint_is_removed() {
    // Settled full-NR32 nibble 15 is unipolar 60 with a 32-unit midpoint
    // (+28 centred). Sources 99/100/101 render P = 98/99/100, so P + 60
    // reaches 158/159/160 and saturates at 127; the midpoint leaves 95.
    // (Clipping the centred +28 instead, as before, gave 126/127/127.)
    let at = |source: i8| {
        let out = render_cgb_with_pcm(wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK), source);
        out[out.len() - 1]
    };
    assert_eq!(at(99).to_bits(), unit(95).to_bits());
    assert_eq!(at(100).to_bits(), unit(95).to_bits());
    assert_eq!(at(101).to_bits(), unit(95).to_bits());
    // The saturation edge: sources 67/68/69 render P = 66/67/68 and meet
    // the 127 ceiling at P = 67.
    assert_eq!(at(67).to_bits(), unit(94).to_bits());
    assert_eq!(at(68).to_bits(), unit(95).to_bits());
    assert_eq!(at(69).to_bits(), unit(95).to_bits());
}

#[test]
fn render_with_offsets_reports_the_midpoint_that_restores_the_unipolar_output() {
    // `signal + offset` must be mGBA's unipolar output: a square is 0 or
    // twice its midpoint (`mgba/src/gb/audio.c:916-918`), at odd volumes too.
    for track_volume in [16, 48, 80, 112, HARDWARE_VOLUME_15_TRACK] {
        let mut voice = square_voice(track_volume);
        voice.begin_frame(false);
        let mut acc = vec![(0, 0); SAMPLES_PER_FRAME];
        let mut offsets = vec![(0, 0); SAMPLES_PER_FRAME];
        voice.render_with_offsets(&mut acc, Some(&mut offsets), &[], &[]);
        let midpoint = offsets[0].0;
        assert!(midpoint > 0, "track volume {track_volume}");
        for (&(signal, _), &(offset, _)) in acc.iter().zip(&offsets) {
            assert_eq!(offset, midpoint);
            assert!(
                [0, 2 * midpoint].contains(&(signal + offset)),
                "track volume {track_volume}: {signal} + {offset}"
            );
        }
    }
}

#[test]
fn a_wave_midpoint_follows_the_nr32_gain_it_latched_under() {
    // Unipolar nibble 15 at full gain is 60 (`mgba/src/gb/audio.c:543-574`):
    // +28 centred plus the 32-unit midpoint.
    let mut voice = wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK);
    voice.begin_frame(false);
    let mut acc = vec![(0, 0); SAMPLES_PER_FRAME];
    let mut offsets = vec![(0, 0); SAMPLES_PER_FRAME];
    voice.render_with_offsets(&mut acc, Some(&mut offsets), &[], &[]);
    let last = SAMPLES_PER_FRAME - 1;
    assert_eq!((acc[last].0, offsets[last].0), (28, 32));
}

#[test]
fn an_inherited_wave_latch_saturates_on_the_midpoint_it_latched_under() {
    // A settled full-gain nibble 15 over PCM P = 100 is 160 unipolar, so it
    // saturates at 127 and the 32-unit midpoint leaves 95. A replacement
    // inherits the latch together with that midpoint on both paths: the same
    // asset keeps rotating (`continue_rotation_from`), a new asset is uploaded
    // with the held output (`continue_latch_from`). Without the midpoint the
    // replacement's first sample would saturate the centred +28 and report 127.
    for asset in ["balance", "other"] {
        let mut mixer = mixer();
        assert!(mixer.add_voice(pcm_voice(101)));
        assert!(mixer.add_cgb_voice(wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK)));
        let settled = frame(&mut mixer);
        assert_eq!(
            settled[settled.len() - 1].to_bits(),
            unit(95).to_bits(),
            "{asset}: settled"
        );
        assert!(mixer.add_cgb_voice(wave_asset_voice(asset, 0xFF, HARDWARE_VOLUME_15_TRACK)));
        let replaced = frame(&mut mixer);
        assert_eq!(
            replaced[0].to_bits(),
            unit(95).to_bits(),
            "{asset}: the replacement's first sample"
        );
    }
}
