use std::sync::Arc;

use super::Mixer;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::{CgbChannelNumber, CgbVoice};
use crate::envelope::Adsr;
use crate::mixer::DEFAULT_MASTER_VOLUME;
use crate::pitch::{self, SAMPLES_PER_FRAME};
use crate::sample::WaveData;
use crate::voice::Voice;

const FULL_TRACK_VOLUME: u8 = u8::MAX;
const TEST_VELOCITY: u8 = 127;
const TIED_GATE_TIME: u16 = 0;

fn unity_freq() -> u32 {
    (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ
}

fn voice(track: usize, priority: u8, key: u8) -> Voice {
    let frame_long_samples = vec![50; SAMPLES_PER_FRAME + 4];
    let wave = Arc::new(WaveData::one_shot(0, frame_long_samples));
    Voice::new(
        wave,
        Adsr::flat(),
        unity_freq(),
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        key,
        track,
        0,
        0,
    )
    .with_priority(priority)
}

fn cgb_voice(track: usize, priority: u8, key: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        None,
        CgbAdsr::flat(),
        key,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        key,
        track,
        0,
        0,
        0,
    )
    .with_priority(priority)
}

fn mixer_with_full_pool(voices: Vec<Voice>) -> Mixer {
    let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, voices.len());
    for voice in voices {
        assert!(mixer.add_voice(voice), "setup voices must all fit");
    }
    mixer
}

fn live_keys(mixer: &Mixer) -> Vec<u8> {
    mixer.voices().into_iter().map(Voice::midi_key).collect()
}

fn occupied_slot(mixer: &Mixer, slot: usize) -> &Voice {
    mixer.direct_sound_slots[slot]
        .as_ref()
        .expect("slot must be occupied")
}

fn release_slot(mixer: &mut Mixer, slot: usize) {
    mixer.direct_sound_slots[slot]
        .as_mut()
        .expect("slot must be occupied")
        .note_off();
}

fn end_slot(mixer: &mut Mixer, slot: usize) {
    release_slot(mixer, slot);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
}

#[test]
fn a_free_channel_is_used_before_any_steal() {
    let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, 3);
    assert!(mixer.add_voice(voice(0, 200, 60)));
    assert!(mixer.add_voice(voice(1, 1, 61)));
    assert!(mixer.add_voice(voice(2, 250, 62)));
    assert_eq!(live_keys(&mixer), vec![60, 61, 62]);
}

#[test]
fn a_higher_priority_note_steals_the_weakest_voice() {
    let mut mixer =
        mixer_with_full_pool(vec![voice(0, 40, 60), voice(1, 10, 61), voice(2, 90, 62)]);
    assert!(mixer.add_voice(voice(3, 100, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70, 62]);
}

#[test]
fn a_lower_priority_note_is_refused() {
    let mut mixer = mixer_with_full_pool(vec![voice(0, 40, 60), voice(1, 50, 61)]);
    assert!(!mixer.add_voice(voice(2, 20, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 61]);
}

#[test]
fn an_equal_priority_note_steals_the_latest_track() {
    let mut mixer =
        mixer_with_full_pool(vec![voice(0, 60, 60), voice(3, 60, 61), voice(2, 60, 62)]);
    assert!(mixer.add_voice(voice(1, 60, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70, 62]);
}

#[test]
fn an_equal_priority_note_cannot_displace_an_earlier_track() {
    let mut mixer = mixer_with_full_pool(vec![voice(0, 60, 60), voice(1, 60, 61)]);
    assert!(!mixer.add_voice(voice(2, 60, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 61]);
}

#[test]
fn an_equal_priority_note_displaces_its_own_track() {
    let mut mixer = mixer_with_full_pool(vec![voice(0, 60, 60), voice(2, 60, 61)]);
    assert!(mixer.add_voice(voice(2, 60, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn a_retired_slot_is_refilled_in_place() {
    let mut mixer =
        mixer_with_full_pool(vec![voice(3, 60, 60), voice(3, 60, 62), voice(3, 60, 64)]);
    end_slot(&mut mixer, 1);
    assert!(
        mixer.direct_sound_slots[1].is_none(),
        "the middle slot must go free"
    );
    assert_eq!(live_keys(&mixer), vec![60, 64]);

    assert!(mixer.add_voice(voice(3, 60, 66)));
    assert_eq!(
        occupied_slot(&mixer, 1).midi_key(),
        66,
        "the freed slot is the lowest free one, so it is refilled first",
    );
    assert_eq!(live_keys(&mixer), vec![60, 66, 64]);

    assert!(mixer.add_voice(voice(3, 60, 68)));
    assert_eq!(live_keys(&mixer), vec![60, 66, 68]);
}

#[test]
fn a_released_voice_is_reused_before_any_sounding_one() {
    let mut mixer = mixer_with_full_pool(vec![voice(0, 1, 60), voice(1, u8::MAX, 61)]);
    release_slot(&mut mixer, 1);
    assert!(occupied_slot(&mixer, 1).is_stopping());
    assert!(mixer.add_voice(voice(3, 2, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn a_released_voice_makes_the_note_unrefusable() {
    let mut mixer = mixer_with_full_pool(vec![voice(0, 200, 60), voice(0, 200, 61)]);
    release_slot(&mut mixer, 0);
    assert!(mixer.add_voice(voice(5, 1, 70)));
    assert_eq!(live_keys(&mixer), vec![70, 61]);
}

#[test]
fn released_voices_compete_among_themselves_on_priority() {
    let mut mixer = mixer_with_full_pool(vec![voice(0, 90, 60), voice(1, 30, 61)]);
    release_slot(&mut mixer, 0);
    release_slot(&mut mixer, 1);
    assert!(mixer.add_voice(voice(2, 50, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn a_saturated_priority_note_outranks_every_unsaturated_one() {
    let highest_unsaturated_priority = u8::MAX - 1;
    let mut mixer = mixer_with_full_pool(vec![
        voice(0, highest_unsaturated_priority, 60),
        voice(1, highest_unsaturated_priority, 61),
    ]);
    assert!(mixer.add_voice(voice(9, u8::MAX, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn saturation_collapses_two_different_sums_into_a_track_order_tie() {
    let mut mixer = mixer_with_full_pool(vec![voice(0, u8::MAX, 60), voice(4, u8::MAX, 61)]);
    assert!(mixer.add_voice(voice(2, u8::MAX, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn cgb_channel_keeps_a_higher_priority_occupant() {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, 100, 60)));
    assert!(!mixer.add_cgb_voice(cgb_voice(0, 99, 70)));
    let slot = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("occupant kept");
    assert_eq!(slot.midi_key(), 60);
}

#[test]
fn cgb_channel_yields_to_a_higher_priority_note() {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, 100, 60)));
    assert!(mixer.add_cgb_voice(cgb_voice(3, 101, 70)));
    let slot = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("replacement present");
    assert_eq!(slot.midi_key(), 70);
}

#[test]
fn cgb_equal_priority_resolves_on_track_order() {
    let mut refused = Mixer::default();
    assert!(refused.add_cgb_voice(cgb_voice(1, 100, 60)));
    assert!(!refused.add_cgb_voice(cgb_voice(2, 100, 70)));

    let mut stolen = Mixer::default();
    assert!(stolen.add_cgb_voice(cgb_voice(2, 100, 60)));
    assert!(stolen.add_cgb_voice(cgb_voice(1, 100, 70)));

    let mut same_track = Mixer::default();
    assert!(same_track.add_cgb_voice(cgb_voice(2, 100, 60)));
    assert!(same_track.add_cgb_voice(cgb_voice(2, 100, 70)));
}

#[test]
fn cgb_channel_always_reuses_a_released_occupant() {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, u8::MAX, 60)));
    mixer.cgb_slots[CgbChannelNumber::Square1.slot()]
        .as_mut()
        .expect("occupant present")
        .note_off();
    assert!(mixer.add_cgb_voice(cgb_voice(9, u8::MIN, 70)));
    let slot = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("replacement present");
    assert_eq!(slot.midi_key(), 70);
}

#[test]
fn a_cgb_note_only_contends_for_its_own_hardware_channel() {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, u8::MAX, 60)));
    let square2 = CgbVoice::square(
        CgbChannelNumber::Square2,
        2,
        None,
        CgbAdsr::flat(),
        70,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        70,
        9,
        0,
        0,
        0,
    )
    .with_priority(u8::MIN);
    assert!(mixer.add_cgb_voice(square2));
    assert_eq!(mixer.voice_count(), 2);
}
