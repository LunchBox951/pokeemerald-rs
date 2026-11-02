//! `xATTA`/`xDECA`/`xSUST`/`xRELE` rewrite only the issuing track's local tone copy.
//!
//! Upstream `ply_xatta`..`ply_xrele` store one byte into `track->tone`
//! (`m4a.c:1567`..`:1589`, dispatched at `m4a_tables.c:297`..`:300`);
//! `ply_voice` overwrites that copy (`m4a_1.s:945`..`:965`) and `ply_note`
//! snapshots its ADSR into the new channel (`m4a_1.s:1750`..`:1758`).
//! Each test proves an override by rendering the same song twice: once with
//! the XCMD against a bank tone, once with no XCMD against a bank tone whose
//! ADSR already holds the overridden byte. The two renders must be identical,
//! and both must differ from the unoverridden bank tone.

use std::sync::Arc;

use super::test_support::tied_note;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::song::{KeySplit, Rhythm, RhythmChild, SquareTone, ToneData, KEY_SLOTS};

const XCMD_ATTA: u8 = 0x04;
const XCMD_DECA: u8 = 0x05;
const XCMD_SUST: u8 = 0x06;
const XCMD_RELE: u8 = 0x07;

const FRAMES: usize = 60;

/// `(kind, field setter)` for each override.
type FieldSetter<A> = fn(&mut A, u8);

const DIRECT_BASE: Adsr = Adsr {
    attack: 60,
    decay: 200,
    sustain: 120,
    release: 100,
};
const DIRECT_OVERRIDES: [(u8, u8, FieldSetter<Adsr>); 4] = [
    (XCMD_ATTA, 255, |a, v| a.attack = v),
    (XCMD_DECA, 90, |a, v| a.decay = v),
    (XCMD_SUST, 30, |a, v| a.sustain = v),
    (XCMD_RELE, 245, |a, v| a.release = v),
];

const CGB_BASE: CgbAdsr = CgbAdsr {
    attack: 1,
    decay: 1,
    sustain: 8,
    release: 1,
};
const CGB_OVERRIDES: [(u8, u8, FieldSetter<CgbAdsr>); 4] = [
    (XCMD_ATTA, 7, |a, v| a.attack = v),
    (XCMD_DECA, 6, |a, v| a.decay = v),
    (XCMD_SUST, 3, |a, v| a.sustain = v),
    (XCMD_RELE, 7, |a, v| a.release = v),
];

fn direct_tone(adsr: Adsr) -> Instrument {
    let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
    Instrument::DirectSound(ToneData::new(wave, adsr))
}

fn square_tone(adsr: CgbAdsr) -> Instrument {
    Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: 0,
        adsr,
        fixed_rate: false,
    })
}

fn xcmd(kind: u8, value: u8) -> Event {
    Event::Xcmd {
        kind,
        value: u32::from(value),
    }
}

fn short_note() -> Event {
    Event::Note {
        key: 60,
        velocity: 127,
        gate: 30,
    }
}

/// `Voice(0)`, the given prefix, one short note, then a long wait and `Fine`.
fn note_track(prefix: &[Event]) -> Vec<Event> {
    let mut track = vec![Event::Voice(0), Event::Volume(127)];
    track.extend_from_slice(prefix);
    track.extend([short_note(), Event::Wait(96), Event::Fine]);
    track
}

fn render_bits(voices: Vec<Instrument>, tracks: Vec<Vec<Event>>) -> Vec<u32> {
    let mut seq = Sequencer::new(Song::new(voices, tracks, 150));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    let mut all = Vec::new();
    for _ in 0..FRAMES {
        seq.render_frame(&mut out);
        all.extend(out.iter().map(|sample| sample.to_bits()));
    }
    all
}

fn assert_override_matches_bank<A: Copy>(
    base: A,
    overrides: &[(u8, u8, FieldSetter<A>)],
    make: fn(A) -> Instrument,
) {
    for &(kind, value, set) in overrides {
        let mut patched = base;
        set(&mut patched, value);
        let overridden = render_bits(vec![make(base)], vec![note_track(&[xcmd(kind, value)])]);
        let in_bank = render_bits(vec![make(patched)], vec![note_track(&[])]);
        let ignored = render_bits(vec![make(base)], vec![note_track(&[])]);
        assert_eq!(
            overridden, in_bank,
            "XCMD kind {kind:#04x} must act like a bank tone holding that byte"
        );
        assert_ne!(
            overridden, ignored,
            "XCMD kind {kind:#04x} must change the rendered note"
        );
    }
}

#[test]
fn each_override_changes_a_direct_sound_note_like_the_bank_byte_would() {
    assert_override_matches_bank(DIRECT_BASE, &DIRECT_OVERRIDES, direct_tone);
}

#[test]
fn each_override_changes_a_cgb_note_like_the_bank_byte_would() {
    assert_override_matches_bank(CGB_BASE, &CGB_OVERRIDES, square_tone);
}

#[test]
fn an_override_on_one_track_leaves_another_track_on_the_bank_tone() {
    let release_override = [xcmd(XCMD_RELE, 245)];
    let voices = || vec![direct_tone(DIRECT_BASE)];
    let silent = || vec![Event::Fine];
    let both = render_bits(
        voices(),
        vec![note_track(&release_override), note_track(&[])],
    );
    let first_only = render_bits(voices(), vec![note_track(&release_override), silent()]);
    let second_only = render_bits(voices(), vec![silent(), note_track(&[])]);
    let both_plain = render_bits(voices(), vec![note_track(&[]), note_track(&[])]);
    assert_ne!(both, both_plain, "the overriding track must change");
    for ((&mixed, &first), &second) in both.iter().zip(&first_only).zip(&second_only) {
        let expected = f32::from_bits(first) + f32::from_bits(second);
        assert!(
            (f32::from_bits(mixed) - expected).abs() < 1e-4,
            "track 1 must render exactly as it does without track 0's override"
        );
    }
}

#[test]
fn a_new_voice_replaces_the_local_tone_and_its_overrides() {
    let reset = render_bits(
        vec![direct_tone(DIRECT_BASE)],
        vec![note_track(&[
            xcmd(XCMD_RELE, 245),
            xcmd(XCMD_ATTA, 255),
            Event::Voice(0),
        ])],
    );
    let plain = render_bits(vec![direct_tone(DIRECT_BASE)], vec![note_track(&[])]);
    assert_eq!(reset, plain, "VOICE must discard every ADSR override");

    let reselected_cgb = render_bits(
        vec![square_tone(CGB_BASE)],
        vec![note_track(&[xcmd(XCMD_SUST, 3), Event::Voice(0)])],
    );
    let plain_cgb = render_bits(vec![square_tone(CGB_BASE)], vec![note_track(&[])]);
    assert_eq!(reselected_cgb, plain_cgb);
}

#[test]
fn an_override_does_not_alter_a_note_that_is_already_playing() {
    let held = |prefix: &[Event]| {
        let mut track = vec![
            Event::Voice(0),
            Event::Volume(127),
            tied_note(60),
            Event::Wait(12),
        ];
        track.extend_from_slice(prefix);
        track.extend([Event::EndOfTie { key: None }, Event::Wait(96), Event::Fine]);
        track
    };
    let overridden = render_bits(
        vec![direct_tone(DIRECT_BASE)],
        vec![held(&[xcmd(XCMD_RELE, 245), xcmd(XCMD_SUST, 30)])],
    );
    let plain = render_bits(vec![direct_tone(DIRECT_BASE)], vec![held(&[])]);
    assert_eq!(
        overridden, plain,
        "the sounding note keeps the ADSR it was started with"
    );
}

#[test]
fn an_override_before_any_voice_edits_the_cleared_startup_tone() {
    // The startup tone is a zero-ADSR Square1 (`m4a_1.s:1218`..`:1230`):
    // sustain 0 is silent until `xSUST` raises it.
    let track = |prefix: &[Event]| {
        let mut events = prefix.to_vec();
        events.extend([
            Event::Volume(127),
            short_note(),
            Event::Wait(96),
            Event::Fine,
        ]);
        events
    };
    let silent = render_bits(Vec::new(), vec![track(&[])]);
    let sustained = render_bits(Vec::new(), vec![track(&[xcmd(XCMD_SUST, 15)])]);
    assert_ne!(silent, sustained);
}

#[test]
fn indirect_children_do_not_receive_a_parent_tone_override() {
    let overrides = [
        xcmd(XCMD_ATTA, 255),
        xcmd(XCMD_DECA, 90),
        xcmd(XCMD_SUST, 30),
        xcmd(XCMD_RELE, 245),
    ];
    let split = Instrument::KeySplit(KeySplit {
        table: [0; KEY_SLOTS],
        children: vec![Some(direct_tone(DIRECT_BASE))],
    });
    let rhythm = || {
        let mut children = vec![None; KEY_SLOTS];
        children[60] = Some(RhythmChild {
            instrument: direct_tone(DIRECT_BASE),
            base_key: 60,
            pan: None,
        });
        Instrument::Rhythm(Rhythm { children })
    };
    let cgb_split = Instrument::KeySplit(KeySplit {
        table: [0; KEY_SLOTS],
        children: vec![Some(square_tone(CGB_BASE))],
    });
    let cgb_overrides = [
        xcmd(XCMD_ATTA, 7),
        xcmd(XCMD_DECA, 6),
        xcmd(XCMD_SUST, 3),
        xcmd(XCMD_RELE, 7),
    ];
    for (parent, overrides) in [
        (split, &overrides),
        (rhythm(), &overrides),
        (cgb_split, &cgb_overrides),
    ] {
        let overridden = render_bits(vec![parent.clone()], vec![note_track(overrides)]);
        let plain = render_bits(vec![parent], vec![note_track(&[])]);
        assert_eq!(
            overridden, plain,
            "a key-split or rhythm child reads the bank, never the parent's local tone"
        );
    }
}

#[test]
fn release_override_then_tie_then_end_of_tie_sustains_the_release_tail() {
    // `gPokemonCrySongTemplate` runs `xRELE` before `TIE` (`m4a_tables.c:274`..`:286`).
    let cry = |prefix: &[Event]| {
        let mut track = vec![Event::Voice(0), Event::Volume(127)];
        track.extend_from_slice(prefix);
        track.extend([
            tied_note(60),
            Event::Wait(12),
            Event::EndOfTie { key: None },
            Event::Wait(96),
            Event::Fine,
        ]);
        track
    };
    let instant_release = Adsr {
        release: 0,
        ..DIRECT_BASE
    };
    let lifetime = |prefix: &[Event]| {
        let song = Song::new(vec![direct_tone(instant_release)], vec![cry(prefix)], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames_with_voice = 0;
        for _ in 0..FRAMES {
            seq.render_frame(&mut out);
            if seq.voice_count() > 0 {
                frames_with_voice += 1;
            }
        }
        frames_with_voice
    };
    let ignored = lifetime(&[]);
    let released = lifetime(&[xcmd(XCMD_RELE, 125)]);
    assert!(
        released > ignored,
        "release 125 must outlast release 0 (lived {released} vs {ignored} frames)"
    );

    let pcm = render_bits(
        vec![direct_tone(instant_release)],
        vec![cry(&[xcmd(XCMD_RELE, 125)])],
    );
    let bank = render_bits(
        vec![direct_tone(Adsr {
            release: 125,
            ..instant_release
        })],
        vec![cry(&[])],
    );
    assert_eq!(pcm, bank, "xRELE must equal a bank tone with release 125");
    assert_ne!(
        pcm,
        render_bits(vec![direct_tone(instant_release)], vec![cry(&[])])
    );
}
