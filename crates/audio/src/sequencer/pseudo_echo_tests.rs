//! Echo settings snapshot on note allocation and extend DirectSound/PSG lifetimes.

use std::sync::Arc;

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::song::{SquareTone, ToneData};

// --- xIECV/xIECL pseudo-echo XCMDs --------------------------------------

#[test]
fn xcmd_iecv_and_iecl_only_affect_subsequently_started_voices() {
    let pre_echo_key = 60;
    let post_echo_key = 64;
    let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: pre_echo_key,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(2),
        Event::Xcmd {
            kind: XCMD_IECV,
            value: 200,
        },
        Event::Xcmd {
            kind: XCMD_IECL,
            value: 5,
        },
        Event::Note {
            key: post_echo_key,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(2),
        Event::EndOfTie {
            key: Some(pre_echo_key),
        },
        Event::EndOfTie {
            key: Some(post_echo_key),
        },
        Event::Wait(64),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    let mut pre_echo_seen = false;
    let mut post_echo_seen = false;
    let mut pre_echo_gone_at = None;
    let mut post_echo_gone_at = None;
    for frame in 0..64 {
        seq.render_frame(&mut out);
        let has_pre_echo = seq
            .mixer
            .voices()
            .iter()
            .any(|v| v.midi_key() == pre_echo_key);
        let has_post_echo = seq
            .mixer
            .voices()
            .iter()
            .any(|v| v.midi_key() == post_echo_key);
        pre_echo_seen |= has_pre_echo;
        post_echo_seen |= has_post_echo;
        if pre_echo_seen && !has_pre_echo && pre_echo_gone_at.is_none() {
            pre_echo_gone_at = Some(frame);
        }
        if post_echo_seen && !has_post_echo && post_echo_gone_at.is_none() {
            post_echo_gone_at = Some(frame);
        }
    }
    let pre_echo_gone = pre_echo_gone_at.expect("the pre-echo voice must eventually retire");
    let post_echo_gone = post_echo_gone_at.expect("the post-xIECV voice must eventually retire");
    assert!(
        post_echo_gone > pre_echo_gone,
        "the xIECV/xIECL voice must outlive the voice started before them ({post_echo_gone} vs {pre_echo_gone})"
    );
}

#[test]
fn xcmd_iecv_and_iecl_extend_a_directsound_voices_lifetime() {
    let make_track = |with_echo: bool| {
        let mut track = vec![Event::Voice(0)];
        if with_echo {
            track.push(Event::Xcmd {
                kind: XCMD_IECV,
                value: 200,
            });
            track.push(Event::Xcmd {
                kind: XCMD_IECL,
                value: 10,
            });
        }
        track.push(Event::Note {
            key: 60,
            velocity: 127,
            gate: 4,
        });
        track.push(Event::Wait(200));
        track.push(Event::Fine);
        track
    };
    let frames_to_silence = |with_echo: bool| {
        let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
        let song = Song::new(voices, vec![make_track(with_echo)], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames = 0;
        loop {
            seq.render_frame(&mut out);
            frames += 1;
            if seq.voice_count() == 0 || frames >= 200 {
                break;
            }
        }
        frames
    };
    assert!(
        frames_to_silence(true) > frames_to_silence(false),
        "xIECV/xIECL must extend the DirectSound voice's lifetime via its pseudo-echo tail"
    );
}

#[test]
fn xcmd_iecv_and_iecl_extend_a_cgb_voices_lifetime() {
    let make_track = |with_echo: bool| {
        let mut track = vec![Event::Voice(0)];
        if with_echo {
            track.push(Event::Xcmd {
                kind: XCMD_IECV,
                value: 200,
            });
            track.push(Event::Xcmd {
                kind: XCMD_IECL,
                value: 10,
            });
        }
        track.push(Event::Note {
            key: 60,
            velocity: 127,
            gate: 4,
        });
        track.push(Event::Wait(200));
        track.push(Event::Fine);
        track
    };
    let frames_to_silence = |with_echo: bool| {
        let voices = vec![Instrument::CgbSquare1(SquareTone {
            duty: 2,
            sweep: 0,
            adsr: CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            fixed_rate: false,
        })];
        let song = Song::new(voices, vec![make_track(with_echo)], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames = 0;
        loop {
            seq.render_frame(&mut out);
            frames += 1;
            if seq.voice_count() == 0 || frames >= 200 {
                break;
            }
        }
        frames
    };
    assert!(
        frames_to_silence(true) > frames_to_silence(false),
        "xIECV/xIECL must extend the CGB voice's lifetime via its pseudo-echo tail"
    );
}
