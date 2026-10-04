//! Pitch/volume arithmetic and LFO reset, delay, and allocation timing match M4A.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::song::{SquareTone, ToneData};

#[test]
fn bend_changes_a_held_notes_frequency() {
    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let track = vec![
        Event::Voice(0),
        Event::BendRange(2),
        tied_note(60),
        Event::Wait(4),
        Event::Bend(63),
        Event::Wait(4),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
    render_frames(&mut seq, 1);
    let base_freq = seq.mixer.voices()[0].frequency();

    render_frames(&mut seq, 5);
    let bent_freq = seq.mixer.voices()[0].frequency();
    assert!(
        bent_freq > base_freq,
        "BEND up must raise the held note's frequency ({bent_freq} vs {base_freq})",
    );
}

#[test]
fn track_pitch_key_m_stays_full_width_within_a_signed_byte() {
    let mut track = TrackState::new();
    track.key_shift = 127;
    assert_eq!(track_pitch(&track).0, 127);
    track.key_shift = -128;
    assert_eq!(track_pitch(&track).0, -128);
}

#[test]
fn track_pitch_key_m_wraps_through_a_signed_byte() {
    let mut track = TrackState::new();
    track.key_shift = 127;
    track.bend = 1;
    track.bend_range = 64;
    assert_eq!(track_pitch(&track).0, -128);
}

#[test]
fn track_volume_in_range_channels_pass_through() {
    let track = TrackState::new();
    assert_eq!(track_volume(&track), (127, 126));
}

#[test]
fn track_volume_tremolo_peak_wraps_through_a_byte() {
    let mut track = TrackState::new();
    track.vol = 127;
    track.modulation_target = ModulationTarget::AMPLITUDE;
    track.modulation = 127;
    track.pan = 63;
    assert_eq!(track_volume(&track), (246, 1));
}

#[test]
fn modulation_target_selects_pitch_amplitude_or_pan() {
    let mut track = TrackState::new();
    track.modulation = 32;

    track.modulation_target = ModulationTarget::PITCH;
    assert_eq!(track_pitch(&track), (2, 0));
    assert_eq!(track_volume(&track), (127, 126));

    track.modulation_target = ModulationTarget::AMPLITUDE;
    assert_eq!(track_pitch(&track), (0, 0));
    assert_eq!(track_volume(&track), (158, 157));

    track.modulation_target = ModulationTarget::PAN;
    assert_eq!(track_pitch(&track), (0, 0));
    assert_eq!(track_volume(&track), (158, 94));
}

// --- LFO/vibrato -----------------------------------------------------

#[test]
fn lfo_pitch_modulation_changes_a_held_notes_frequency_over_time() {
    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let track = vec![
        Event::Voice(0),
        Event::Modulation(40),
        Event::LfoSpeed(30),
        tied_note(60),
        Event::Wait(96),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
    render_frames(&mut seq, 1);
    let base_freq = seq.mixer.voices()[0].frequency();

    let mut changed = false;
    for _ in 0..40 {
        render_frames(&mut seq, 1);
        if seq.mixer.voices()[0].frequency() != base_freq {
            changed = true;
            break;
        }
    }
    assert!(changed, "LFO should eventually bend the held note's pitch");
}

#[test]
fn lfo_measurably_changes_the_rendered_output_vs_no_lfo() {
    let make_wave = || {
        Arc::new(WaveData::looping(
            1 << 20,
            0,
            vec![100, -100, 50, -50, 30, -30, 10, -10],
        ))
    };
    let make_track = |with_lfo: bool| {
        let mut track = vec![Event::Voice(0)];
        if with_lfo {
            track.push(Event::Modulation(60));
            track.push(Event::LfoSpeed(40));
        }
        track.push(tied_note(60));
        track.push(Event::Wait(96));
        track.push(Event::Fine);
        track
    };
    let render = |with_lfo: bool| {
        let voices = vec![Instrument::DirectSound(ToneData::new(
            make_wave(),
            Adsr::flat(),
        ))];
        let mut seq = Sequencer::new(Song::new(voices, vec![make_track(with_lfo)], 150));
        let mut buf = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..25 {
            seq.render_frame(&mut buf);
        }
        buf
    };

    assert_ne!(
        render(false),
        render(true),
        "an active LFO must audibly diverge from the unmodulated render"
    );
}

#[test]
fn lfo_delay_holds_off_modulation_until_it_elapses() {
    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let track = vec![
        Event::Voice(0),
        Event::Modulation(60),
        Event::LfoSpeed(80),
        Event::LfoDelay(10),
        tied_note(60),
        Event::Wait(96),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
    render_frames(&mut seq, 1);
    let base_freq = seq.mixer.voices()[0].frequency();
    for _ in 0..9 {
        render_frames(&mut seq, 1);
        assert_eq!(
            seq.mixer.voices()[0].frequency(),
            base_freq,
            "frequency must not move during the LFO delay"
        );
    }
    assert_eq!(seq.tracks[0].lfo_delay_remaining, 0);

    render_frames(&mut seq, 1);
    assert_eq!(seq.tracks[0].lfo_phase, 80);
    assert_ne!(seq.mixer.voices()[0].frequency(), base_freq);
}

#[test]
fn cgb_note_resets_modulation_only_after_allocation() {
    let instruments = vec![
        direct_sound(100),
        Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr::flat(),
            fixed_rate: false,
        }),
    ];
    let song = Song::new(instruments, vec![vec![], vec![]], 150);
    let mut seq = Sequencer::with_config(song, DEFAULT_MASTER_VOLUME, 2);

    seq.tracks[0].voice = 1;
    seq.tracks[0].priority = 10;
    apply_test_event(&mut seq, 0, &tied_note(50));

    seq.tracks[1].voice = 0;
    seq.tracks[1].modulation_target = ModulationTarget::AMPLITUDE;
    seq.tracks[1].modulation = 64;
    apply_test_event(&mut seq, 1, &tied_note(60));
    let modulated_volume = seq.mixer.voices()[0].base_volume();

    seq.tracks[1].voice = 1;
    seq.tracks[1].lfo_delay = 7;
    seq.tracks[1].lfo_delay_remaining = 3;
    seq.tracks[1].lfo_phase = 91;
    apply_test_event(&mut seq, 1, &tied_note(70));

    assert_eq!(seq.tracks[1].key, 70, "the raw track key still updates");
    assert_eq!(seq.tracks[1].lfo_delay_remaining, 3);
    assert_eq!(seq.tracks[1].lfo_phase, 91);
    assert_eq!(seq.tracks[1].modulation, 64);
    assert_eq!(seq.mixer.voices()[0].base_volume(), modulated_volume);
    let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("the original square-1 occupant must remain");
    assert_eq!(square1.track(), 0);
    assert_eq!(square1.midi_key(), 50);

    seq.tracks[1].priority = 20;
    apply_test_event(&mut seq, 1, &tied_note(70));
    assert_eq!(seq.tracks[1].lfo_delay_remaining, 7);
    assert_eq!(seq.tracks[1].lfo_phase, 0);
    assert_eq!(seq.tracks[1].modulation, 0);
    let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("the higher-priority note must replace square 1");
    assert_eq!(square1.track(), 1);
    assert_eq!(square1.midi_key(), 70);
}

#[test]
fn accepted_cgb_sweep_note_uses_reset_pitch_modulation() {
    const ADDING_SWEEP_PERIOD_1_SHIFT_1: u8 = 0x11;

    let instruments = vec![Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: ADDING_SWEEP_PERIOD_1_SHIFT_1,
        adsr: CgbAdsr::flat(),
        fixed_rate: false,
    })];
    let song = Song::new(instruments, vec![vec![]], 150);
    let mut seq = Sequencer::new(song);

    seq.tracks[0].modulation_target = ModulationTarget::PITCH;
    seq.tracks[0].modulation = 127;
    seq.tracks[0].lfo_delay = 7;
    seq.tracks[0].lfo_phase = 91;
    apply_test_event(&mut seq, 0, &tied_note(48));

    assert_eq!(seq.tracks[0].lfo_delay_remaining, 7);
    assert_eq!(seq.tracks[0].lfo_phase, 0);
    assert_eq!(seq.tracks[0].modulation, 0);
    let square1 = seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("the accepted square-1 note must occupy its channel");
    assert!(
        square1.is_active(),
        "the sweep must initialize from reset pitch modulation, without trigger overflow"
    );
}

#[test]
fn direct_sound_note_resets_modulation_only_after_allocation() {
    let song = Song::new(vec![direct_sound(100)], vec![vec![], vec![]], 150);
    let mut seq = Sequencer::with_config(song, DEFAULT_MASTER_VOLUME, 2);

    seq.tracks[1].priority = 10;
    seq.tracks[1].modulation = 32;
    apply_test_event(&mut seq, 1, &tied_note(60));
    seq.tracks[0].priority = 10;
    apply_test_event(&mut seq, 0, &tied_note(50));
    let modulated_frequency = seq
        .mixer
        .voices()
        .into_iter()
        .find(|voice| voice.track() == Some(1))
        .expect("track 1 must own its original voice")
        .frequency();
    let occupants: Vec<_> = seq
        .mixer
        .voices()
        .into_iter()
        .map(|voice| (voice.track(), voice.midi_key()))
        .collect();

    seq.tracks[1].priority = 0;
    seq.tracks[1].lfo_delay = 7;
    seq.tracks[1].lfo_delay_remaining = 3;
    seq.tracks[1].lfo_phase = 91;
    apply_test_event(&mut seq, 1, &tied_note(70));

    assert_eq!(seq.tracks[1].key, 70, "the raw track key still updates");
    assert_eq!(seq.tracks[1].lfo_delay_remaining, 3);
    assert_eq!(seq.tracks[1].lfo_phase, 91);
    assert_eq!(seq.tracks[1].modulation, 32);
    let voices = seq.mixer.voices();
    assert_eq!(
        voices
            .iter()
            .find(|voice| voice.track() == Some(1))
            .expect("the original track-1 voice must remain")
            .frequency(),
        modulated_frequency
    );
    assert_eq!(
        voices
            .iter()
            .map(|voice| (voice.track(), voice.midi_key()))
            .collect::<Vec<_>>(),
        occupants,
        "a refused note must not replace either pool occupant"
    );

    seq.tracks[1].priority = 20;
    apply_test_event(&mut seq, 1, &tied_note(70));
    assert_eq!(seq.tracks[1].lfo_delay_remaining, 7);
    assert_eq!(seq.tracks[1].lfo_phase, 0);
    assert_eq!(seq.tracks[1].modulation, 0);
    assert!(seq
        .mixer
        .voices()
        .iter()
        .any(|voice| voice.track() == Some(1) && voice.midi_key() == 70));
}

#[test]
fn lfo_triangle_uses_the_wide_phase_sum_before_byte_truncation() {
    let wide_phase_sum = 400;
    let triangle = lfo_triangle(wide_phase_sum);
    assert_eq!(triangle, -272);
    assert_eq!(scale_lfo(40, triangle), 86);

    assert_eq!(lfo_triangle(0x80), 0);
    assert_eq!(lfo_triangle(0x20), 0x20);
    assert_eq!(lfo_triangle(0xC0), -64);
}
