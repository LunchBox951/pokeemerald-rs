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
        let mut track = vec![Event::Voice(0), Event::Volume(127)];
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
        let mut track = vec![Event::Voice(0), Event::Volume(127)];
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

// --- xWAIT ---------------------------------------------------------------

/// A one-voice song whose track is `events`; the wave outlives the frames the
/// xWAIT tests render so a started note stays visible.
fn xwait_song(events: Vec<Event>) -> Sequencer {
    let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME * 8]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    Sequencer::new(Song::new(voices, vec![events], 150))
}

/// The first rendered frame (counted from 0) in which a key-60 voice sounds.
fn first_frame_with_note(seq: &mut Sequencer, frames: usize) -> Option<usize> {
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    (0..frames).find(|_| {
        seq.render_frame(&mut out);
        seq.mixer.voices().iter().any(|v| v.midi_key() == 60)
    })
}

fn note_60() -> Event {
    Event::Note {
        key: 60,
        velocity: 127,
        gate: 0,
    }
}

#[test]
fn xcmd_xwait_delays_the_following_note() {
    // `ply_xwait` (`m4a.c:1615`..`:1636`) re-runs itself with a one-tick wait
    // until its timer reaches `len`, so the next command runs on tick `len`.
    let mut seq = xwait_song(vec![
        Event::Voice(0),
        Event::Xcmd {
            kind: XCMD_WAIT,
            value: 3,
        },
        note_60(),
        Event::Wait(10),
        Event::Fine,
    ]);
    assert_eq!(first_frame_with_note(&mut seq, 8), Some(3));
}

#[test]
fn xcmd_xwait_zero_does_not_delay() {
    let mut seq = xwait_song(vec![
        Event::Voice(0),
        Event::Xcmd {
            kind: XCMD_WAIT,
            value: 0,
        },
        note_60(),
        Event::Wait(10),
        Event::Fine,
    ]);
    assert_eq!(first_frame_with_note(&mut seq, 4), Some(0));
}

#[test]
fn consecutive_xwaits_each_restart_the_timer() {
    // `ply_xwait` zeroes `timer` when it completes (`m4a.c:1634`).
    let xwait = |value| Event::Xcmd {
        kind: XCMD_WAIT,
        value,
    };
    let mut seq = xwait_song(vec![
        Event::Voice(0),
        xwait(2),
        xwait(2),
        note_60(),
        Event::Wait(10),
        Event::Fine,
    ]);
    assert_eq!(first_frame_with_note(&mut seq, 8), Some(4));
}

#[test]
fn xcmd_xwait_length_is_sixteen_bits() {
    let mut seq = xwait_song(vec![
        Event::Voice(0),
        Event::Xcmd {
            kind: XCMD_WAIT,
            value: 0x0102,
        },
        note_60(),
        Event::Wait(10),
        Event::Fine,
    ]);
    assert_eq!(first_frame_with_note(&mut seq, 300), Some(0x0102));
}

#[test]
fn decoded_xwait_bytes_hold_the_following_note() {
    const VOICE: u8 = 0xBD;
    const XCMD: u8 = 0xCD;
    const NOTE_1: u8 = 0xD0;
    const WAIT_10: u8 = 0x8A;
    const FINE: u8 = 0xB1;
    let bytes = [
        VOICE, 0, XCMD, 0x0C, 0x03, 0x00, NOTE_1, 60, 127, WAIT_10, FINE,
    ];
    let events = crate::sequence::decode_track(&bytes).unwrap();
    let mut seq = xwait_song(events);
    assert_eq!(first_frame_with_note(&mut seq, 8), Some(3));
}
