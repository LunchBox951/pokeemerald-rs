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

fn unit(n: i8) -> f32 {
    f32::from(n) / OUTPUT_SCALE
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

fn assert_extrema(out: &[f32], low: i8, high: i8) {
    let (lo, hi) = extrema(out);
    assert_eq!(lo.to_bits(), unit(low).to_bits(), "low {lo} vs {low}/128");
    assert_eq!(
        hi.to_bits(),
        unit(high).to_bits(),
        "high {hi} vs {high}/128"
    );
}

fn assert_every_sample(out: &[f32], n: i8) {
    assert!(out.iter().all(|s| s.to_bits() == unit(n).to_bits()));
}

fn pcm_voice(source: i8) -> Voice {
    constant_voice(source, TRACK)
}

fn wave_voice(nibbles: u8, track_volume: u8) -> CgbVoice {
    CgbVoice::wave(
        Arc::from("balance"),
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

#[test]
fn the_summed_output_clips_at_the_s8_bounds() {
    // Settled full-NR32 nibble 15 is +28; source 100 renders 99 and reaches 127.
    let at = |source: i8| {
        let out = render_cgb_with_pcm(wave_voice(0xFF, HARDWARE_VOLUME_15_TRACK), source);
        out[out.len() - 1]
    };
    assert_eq!(at(99).to_bits(), unit(126).to_bits());
    assert_eq!(at(100).to_bits(), unit(127).to_bits());
    assert_eq!(at(101).to_bits(), unit(127).to_bits());
}
