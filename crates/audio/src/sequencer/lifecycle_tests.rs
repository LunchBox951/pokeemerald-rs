//! Note release and fade propagation respect track termination and allocation timing.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::sequence::decode_track;
use crate::song::{SquareTone, ToneData};

/// A song whose instrument reads a wave at frequency `0`, so a tied voice
/// never advances through the sample and only stops on an explicit
/// note-off — isolating end-of-tie behaviour from wave exhaustion.
fn held_note_song(track: Vec<Event>) -> Song {
    let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    Song::new(voices, vec![track], 150)
}

#[test]
fn end_of_tie_without_operand_stops_only_the_last_keyed_note() {
    // The omitted-operand `EOT` must resolve to key 64 (the current
    // key), not 60 and not every sounding voice.
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Note {
            key: 64,
            velocity: 127,
            gate: 0,
        },
        Event::EndOfTie { key: None },
        Event::Wait(2),
        Event::EndOfTie { key: Some(60) },
        Event::Wait(2),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(held_note_song(track));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame(&mut out);
    assert_eq!(
        seq.voice_count(),
        1,
        "only the last-keyed note (64) retires"
    );

    for _ in 0..4 {
        seq.render_frame(&mut out);
    }
    assert_eq!(seq.voice_count(), 0, "the key-60 survivor now retires too");
}

#[test]
fn fine_releases_a_tied_voice_and_the_song_finishes() {
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(2),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(held_note_song(track));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    let mut frames = 0;
    while !seq.is_finished() && frames < 500 {
        seq.render_frame(&mut out);
        frames += 1;
    }
    assert!(seq.is_finished(), "FINE must release the tied voice");
}

#[test]
fn eof_without_fine_releases_a_tied_voice_and_the_song_finishes() {
    // VOICE 0; TIE key60 vel127 (gate 0); W02 -- no trailing FINE.
    let bytes = [0xBD, 0x00, 0xCF, 60, 127, 0x82];
    let events = decode_track(&bytes).unwrap();
    assert!(
        !events.contains(&Event::Fine),
        "this stream must end by falling off the end, not by FINE"
    );

    let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let mut seq = Sequencer::new(Song::new(voices, vec![events], 150));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    let mut frames = 0;
    while !seq.is_finished() && frames < 500 {
        seq.render_frame(&mut out);
        frames += 1;
    }
    assert_eq!(seq.voice_count(), 0);
    assert!(seq.is_finished(), "EOF must release the tied voice");
}

#[test]
fn command_cap_releases_a_tied_voice_and_the_song_finishes() {
    // A self-GOTO with no Wait ends only via the command guard, which
    // must release the tied voice too, or `is_finished()` never returns.
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Goto(2),
    ];
    let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    let mut frames = 0;
    while !seq.is_finished() && frames < 500 {
        seq.render_frame(&mut out);
        frames += 1;
    }
    assert_eq!(seq.voice_count(), 0, "the command cap must release voices");
    assert!(seq.is_finished(), "the command cap must end the song");
}

#[test]
fn volume_change_updates_a_held_notes_gains() {
    let track = vec![
        Event::Voice(0),
        Event::Volume(127),
        tied_note(60),
        Event::Wait(4),
        Event::Volume(20),
        Event::Wait(4),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(held_note_song(track));
    render_frames(&mut seq, 1);
    let (loud_r, loud_l) = seq.mixer.voices()[0].base_volume();

    render_frames(&mut seq, 5);
    let (soft_r, soft_l) = seq.mixer.voices()[0].base_volume();
    assert!(
        soft_r < loud_r && soft_l < loud_l,
        "VOL must lower the held note ({soft_r},{soft_l} vs {loud_r},{loud_l})",
    );
    assert!(soft_r > 0 && soft_l > 0);
}

/// A slow, nonzero release survives past the `Fine` frame so the voice
/// can still be inspected there, unlike `Adsr::flat()`'s `release: 0`,
/// which retires (and drops) the voice within that very frame.
fn slow_release_song(track: Vec<Event>) -> Song {
    slow_release_song_tracks(vec![track])
}

/// [`slow_release_song`] with one track per entry, so a track that ended
/// early can be observed beside one still playing. The wave loops, so a
/// released voice keeps producing samples while it decays.
fn slow_release_song_tracks(tracks: Vec<Vec<Event>>) -> Song {
    let adsr = Adsr {
        attack: 255,
        decay: 255,
        sustain: 255,
        release: 250,
    };
    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, adsr))];
    Song::new(voices, tracks, 150)
}

/// `ply_fine` releases a track's voices rather than retiring them, and
/// `SoundMain` keeps mixing every frame regardless of
/// `MUSICPLAYER_STATUS_PAUSE` (`m4a_1.s:20`-`:119`), so a voice still in
/// release when the terminal step stops the surviving tracks rings out
/// instead of being cut short with them. This song carries no reverb, so
/// the voice is the only thing that can still sound.
#[test]
fn a_paused_sequencer_still_sounds_a_voice_an_ended_track_left_in_release() {
    let ending = vec![Event::Voice(0), tied_note(60), Event::Wait(1), Event::Fine];
    let surviving = vec![Event::Voice(0), tied_note(72), Event::Wait(200)];
    let mut seq = Sequencer::new(slow_release_song_tracks(vec![ending, surviving]));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame_with_fade(&mut out, None);
    seq.render_frame_with_fade(&mut out, None);
    assert!(
        seq.tracks[0].ended && !seq.tracks[1].ended,
        "sanity: the first track must end via Fine while the second plays on"
    );

    seq.render_frame_with_fade(&mut out, Some(0));
    assert!(
        seq.is_sounding(),
        "the terminal step stops the surviving track, not the voice the ended one released"
    );

    seq.render_frame_with_fade(&mut out, Some(0));
    assert!(
        out.iter().any(|&sample| sample != 0.0),
        "the paused mixer must keep rendering that released voice"
    );
}

/// Upstream marks every existing track's `volX` before that frame's
/// track pass (`FadeOutBody`, `m4a.c:750`-`:757`), but only propagates it
/// into a channel afterward (`TrkVolPitSet`/`ChnVolSetAsm`,
/// `m4a_1.s:1361`-`:1400`); `ply_fine` clears a track's flags first
/// (`m4a_1.s:750`-`:777`), so a track ending via `Fine` on the same tick
/// as a fade step never receives that step.
#[test]
fn a_track_that_ends_via_fine_this_tick_never_receives_that_ticks_fade_step() {
    let track = vec![Event::Voice(0), tied_note(60), Event::Wait(1), Event::Fine];
    let mut seq = Sequencer::new(slow_release_song(track));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame_with_fade(&mut out, None);
    let before = seq.mixer.voices()[0].base_volume();

    seq.render_frame_with_fade(&mut out, Some(32));
    assert!(
        seq.tracks[0].ended,
        "sanity: Fine must end the track this frame"
    );
    assert_eq!(
        seq.mixer.voices()[0].base_volume(),
        before,
        "a track ending via Fine this tick must not receive that tick's fade step"
    );
}

/// Upstream's `ply_pan` only raises `MPT_FLG_VOLCHG`; every volume
/// recomputation is deferred to the single post-tick `TrkVolPitSet`
/// (`m4a_1.s:1361`-`:1400`), which `ply_fine`'s flag clear
/// (`m4a_1.s:750`-`:777`) skips for a track that ended this tick. So an
/// intervening volume-affecting command must not leak this tick's fade
/// step into the released voice either. `Pan(0)` here leaves `pan` at its
/// default 0, so `vol_x` is the only volume input that changed.
#[test]
fn an_intervening_pan_must_not_leak_this_ticks_fade_step_into_a_fine_ending_track() {
    let track = vec![
        Event::Voice(0),
        tied_note(60),
        Event::Wait(1),
        Event::Pan(0),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(slow_release_song(track));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame_with_fade(&mut out, None);
    let before = seq.mixer.voices()[0].base_volume();

    seq.render_frame_with_fade(&mut out, Some(32));
    assert!(
        seq.tracks[0].ended,
        "sanity: Fine must end the track this frame"
    );
    assert_eq!(
        seq.mixer.voices()[0].base_volume(),
        before,
        "an intervening volume command must not propagate this tick's fade step to a track \
         ending via Fine"
    );
}

/// `ply_fine` unlinks every channel and zeroes the flags before the
/// post-tick pass runs (`m4a_1.s:750`-`:777`), so a same-tick `PAN`
/// never reaches the released voice.
#[test]
fn a_pan_command_in_the_same_tick_as_fine_must_not_reach_the_released_voice() {
    let track = vec![
        Event::Voice(0),
        tied_note(60),
        Event::Wait(1),
        Event::Pan(-64),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(slow_release_song(track));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame(&mut out);
    let before = seq.mixer.voices()[0].base_volume();

    seq.render_frame(&mut out);
    assert!(
        seq.tracks[0].ended,
        "sanity: Fine must end the track this frame"
    );
    assert_eq!(
        seq.mixer.voices()[0].base_volume(),
        before,
        "a PAN reached in the same tick as FINE must not propagate to the released voice"
    );
}

/// The pitch-domain twin of the `PAN` case above (`m4a_1.s:994`-`:1005`).
#[test]
fn a_bend_command_in_the_same_tick_as_fine_must_not_reach_the_released_voice() {
    let track = vec![
        Event::Voice(0),
        Event::BendRange(2),
        tied_note(60),
        Event::Wait(1),
        Event::Bend(63),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(slow_release_song(track));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame(&mut out);
    let before = seq.mixer.voices()[0].frequency();

    seq.render_frame(&mut out);
    assert!(
        seq.tracks[0].ended,
        "sanity: Fine must end the track this frame"
    );
    assert_eq!(
        seq.mixer.voices()[0].frequency(),
        before,
        "a BEND reached in the same tick as FINE must not propagate to the released voice"
    );
}

/// Propagation runs once per `MPlayMain` call after every tick of the
/// frame (`m4a_1.s:1169`-`:1175`), so a control from the first of two
/// same-frame ticks still never reaches a voice the second tick's `Fine`
/// releases.
#[test]
fn a_control_from_an_earlier_tick_in_the_same_frame_as_fine_must_not_reach_the_released_voice() {
    let track = vec![
        Event::Voice(0),
        tied_note(60),
        Event::Wait(1),
        Event::Pan(-64),
        Event::Wait(1),
        Event::Fine,
    ];
    let mut seq = Sequencer::new(slow_release_song(track));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    // Frame 1, at the song's default tempo, runs exactly one tick:
    // starts the note and immediately consumes the first `Wait(1)`.
    seq.render_frame(&mut out);
    let before = seq.mixer.voices()[0].base_volume();

    // Frame 2 runs two ticks: the first reaches `Pan`, the second `Fine`.
    seq.tempo_i = 2 * TEMPO_UNIT;
    seq.render_frame(&mut out);
    assert!(
        seq.tracks[0].ended,
        "sanity: Fine must end the track this frame"
    );
    assert_eq!(
        seq.mixer.voices()[0].base_volume(),
        before,
        "a control from an earlier tick in the same frame as FINE must not propagate to the \
         released voice"
    );
}

/// A successful `ply_note` applies the track's current controls to the
/// new voice and masks the flags (`m4a_1.s:1802`-`:1805`), so a same-tick
/// control never also reaches an older voice on that track.
#[test]
fn a_note_started_the_same_tick_as_a_pending_control_consumes_it_before_older_voices_see_it() {
    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let track = vec![
        Event::Voice(0),
        tied_note(60),
        Event::Wait(1),
        Event::Pan(-64),
        tied_note(72),
        Event::Wait(4),
        Event::Fine,
    ];
    let mut seq = Sequencer::with_config(
        Song::new(voices, vec![track], 150),
        DEFAULT_MASTER_VOLUME,
        2,
    );
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame(&mut out);
    assert_eq!(
        seq.voice_count(),
        1,
        "sanity: only the first note has started"
    );
    let older_before = seq
        .mixer
        .voices()
        .iter()
        .find(|voice| voice.midi_key() == 60)
        .expect("the first note's voice must exist")
        .base_volume();

    seq.render_frame(&mut out);
    assert_eq!(
        seq.voice_count(),
        2,
        "sanity: the second note must also start"
    );
    let older_after = seq
        .mixer
        .voices()
        .iter()
        .find(|voice| voice.midi_key() == 60)
        .expect("the first note's voice must still exist")
        .base_volume();
    assert_eq!(
        older_after, older_before,
        "a control consumed by a same-tick Note must not reach the track's other voices"
    );
}

/// A fade step raises the same flag as a `VOL` command (`m4a.c:753`-`:757`),
/// so a same-tick `Note` consumes it like any other control.
#[test]
fn a_note_started_the_same_tick_as_a_fade_step_consumes_it_before_older_voices_see_it() {
    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let track = vec![
        Event::Voice(0),
        tied_note(60),
        Event::Wait(1),
        tied_note(72),
        Event::Wait(4),
        Event::Fine,
    ];
    let mut seq = Sequencer::with_config(
        Song::new(voices, vec![track], 150),
        DEFAULT_MASTER_VOLUME,
        2,
    );
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame(&mut out);
    assert_eq!(
        seq.voice_count(),
        1,
        "sanity: only the first note has started"
    );
    let older_before = seq
        .mixer
        .voices()
        .iter()
        .find(|voice| voice.midi_key() == 60)
        .expect("the first note's voice must exist")
        .base_volume();

    // Frame 2 reaches a new `Note` with a fade step landing the same frame.
    seq.render_frame_with_fade(&mut out, Some(32));
    assert_eq!(
        seq.voice_count(),
        2,
        "sanity: the second note must also start"
    );
    let older_after = seq
        .mixer
        .voices()
        .iter()
        .find(|voice| voice.midi_key() == 60)
        .expect("the first note's voice must still exist")
        .base_volume();
    assert_eq!(
        older_after, older_before,
        "a fade step consumed by a same-tick Note must not reach the track's other voices"
    );
}

/// `FadeOutBody`'s terminal step stops every track outright
/// (`m4a.c:715`-`:743`) and `TrackStop` turns the CGB channel off as it
/// goes (`m4a_1.s:1490`-`:1493`), so the PSG voice is gone after that
/// frame -- not merely scaled to zero in the output buffer.
#[test]
fn a_terminal_fade_step_retires_a_sustained_cgb_voice() {
    let voices = vec![Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: 0,
        adsr: CgbAdsr::flat(),
        fixed_rate: false,
    })];
    let track = vec![Event::Voice(0), tied_note(60), Event::Wait(200)];
    let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

    seq.render_frame_with_fade(&mut out, Some(64));
    assert!(
        seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some(),
        "sanity: the CGB voice must be sounding before the terminal step"
    );

    seq.render_frame_with_fade(&mut out, Some(0));
    assert!(
        seq.mixer.cgb_voices()[CgbChannelNumber::Square1.slot()].is_none(),
        "the terminal fade step must retire the CGB voice, not just silence its output"
    );
}

/// A fade-end `stop_track` leaves a slow noise note's off-write tail
/// rendering until the restarted LFSR's first clock settles it, so the
/// sequencer stays sounding for it (`Mixer::is_idle`'s doc).
#[test]
fn a_sequencer_keeps_sounding_for_a_slow_noise_off_write_tail() {
    const SLOW_KEY: u8 = 21;
    const MAX_FRAMES: usize = 40;
    let song = test_song(vec![vec![Event::Wait(200)]], 150);
    let mut seq = Sequencer::new(song);
    let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
    let voice = crate::cgb_voice::CgbVoice::noise(
        CgbAdsr::flat(),
        SLOW_KEY,
        0,
        u8::MAX,
        u8::MAX,
        127,
        0,
        SLOW_KEY,
        0,
        0,
        0,
        0,
    );
    assert!(seq.mixer.add_cgb_voice(voice));
    let mut frames = 0;
    // Run until the first clock raises the latch, then retire the voice.
    while seq.is_sounding() && frames < MAX_FRAMES {
        seq.render_frame(&mut out);
        frames += 1;
        if frames == 14 {
            seq.mixer.stop_track(0);
            break;
        }
    }
    assert!(seq.is_sounding(), "the unsettled latch is a pending tail");
    let mut tail_frames = 0;
    while seq.is_sounding() && tail_frames < MAX_FRAMES {
        seq.render_frame(&mut out);
        tail_frames += 1;
    }
    assert!(
        !seq.is_sounding(),
        "the tail settles and the sequencer finishes"
    );
    assert!(tail_frames > 1, "the tail rendered for several frames");
}
