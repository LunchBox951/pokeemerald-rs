//! Unit tests for [`super`]/[`super::player`]: a synthetic, pack-free
//! [`MusicPlayer`] round-trip (continuous playback, restart-on-finish, ring
//! prefill sizing, neither underrun nor overrun when drained each frame, and
//! the `FadeOutBGM` ramp), plus real-pack coverage of
//! [`load_song_from_pack`] against `mus_title`.
//!
//! The ignored, env-gated mGBA reference comparison (Discussion #227's
//! fidelity oracle) lives in [`oracle`] below.

use std::sync::Arc;

use audio::{Adsr, Event, Instrument, Sequencer, Song, ToneData, WaveData};
use platform::AudioOutput;

use super::{load_song_from_pack, MusicPlayer, RING_CAPACITY_FRAMES, TITLE_FADE_OUT_SPEED};

/// The ring's capacity in interleaved samples, i.e. what
/// `Producer::available_space` reports before anything is queued.
const RING_CAPACITY_SAMPLES: usize = RING_CAPACITY_FRAMES * (AudioOutput::CHANNELS as usize);

/// Drain everything currently queued in `player`'s ring, so the next
/// [`MusicPlayer::advance_frame`] plus a one-frame drain reads back exactly
/// the frame that call pushed.
fn drain_everything(player: &mut MusicPlayer) {
    let queued = RING_CAPACITY_SAMPLES - player.ring_free_for_test();
    let mut sink = vec![0.0_f32; queued];
    player.drain_null_for_test(&mut sink);
}

/// A short, loud, tied square-ish wave -- loud enough that `push`ed frames
/// are trivially distinguishable from silence.
fn loud_wave() -> Arc<WaveData> {
    Arc::new(WaveData::one_shot(1 << 20, vec![100; 64]))
}

/// A song that loops forever via its own `Goto`, exactly like a real BGM
/// (never reaching `Fine`) -- see `super`'s module docs on why continuous
/// playback needs no extra loop-restart logic beyond a song's own jump
/// commands.
fn looping_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(50),
        Event::Goto(0),
    ];
    Song::new(voices, vec![events], 150)
}

/// A song that reaches `Fine` quickly -- for
/// [`a_finished_song_restarts_instead_of_falling_permanently_silent`].
fn short_one_shot_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 4,
        },
        Event::Wait(8),
        Event::Fine,
    ];
    Song::new(voices, vec![events], 150)
}

/// A finite song whose dry voice stops before its master-mix reverb tail.
fn finite_reverbed_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 1,
        },
        Event::Wait(2),
        Event::Fine,
    ];
    Song::new(voices, vec![events], 150).with_reverb(100)
}

#[test]
fn advance_frame_produces_audible_output_and_never_underruns_when_drained_each_frame() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");
    assert!(player.is_running());

    let mut any_audible = false;
    let mut drained = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    for _ in 0..64 {
        player.advance_frame();
        player.drain_null_for_test(&mut drained);
        if drained.iter().any(|&s| s != 0.0) {
            any_audible = true;
        }
    }
    assert!(any_audible, "a looping song must produce audible output");
    assert_eq!(
        player.underruns(),
        0,
        "draining exactly one frame per advance must never starve the ring"
    );
    assert_eq!(
        player.overruns(),
        0,
        "draining exactly one frame per advance must never overflow the ring either"
    );
}

/// The prefill leaves roughly half the ring free (`super::player`'s module
/// docs): headroom for game-loop/device clock drift, and half the latency a
/// fill-to-the-brim prefill would add.
#[test]
fn start_prefills_about_half_the_ring_and_leaves_the_rest_as_headroom() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    assert_eq!(
        output.producer().available_space(),
        RING_CAPACITY_SAMPLES,
        "a fresh ring is empty, so this test's capacity constant is the real one"
    );
    let player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");

    let free = player.ring_free_for_test();
    let queued = RING_CAPACITY_SAMPLES - free;
    let half = RING_CAPACITY_SAMPLES / 2;
    assert!(
        queued <= half && queued + Sequencer::FRAME_SAMPLES > half,
        "prefill queued {queued} samples: expected the largest whole number of \
         {}-sample frames that fits in half of {RING_CAPACITY_SAMPLES}",
        Sequencer::FRAME_SAMPLES
    );
    assert!(
        free >= half,
        "prefill must leave at least half the ring ({half} samples) free as drift headroom, \
         left {free}"
    );
    assert_eq!(player.overruns(), 0, "the prefill must never drop a sample");
}

/// The overrun counter (`super::player`'s "Stream health" docs) surfaces the
/// `push` return value the player used to discard: a producer nobody drains
/// fills the ring and then starts losing samples, and that has to be visible.
#[test]
fn overruns_count_the_samples_a_full_ring_drops() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");
    assert_eq!(player.overruns(), 0);

    // Never drained: the ring fills, then every further push loses whatever
    // does not fit.
    let free_at_start = player.ring_free_for_test();
    let frames_that_fit = free_at_start / Sequencer::FRAME_SAMPLES;
    for _ in 0..frames_that_fit {
        player.advance_frame();
    }
    assert_eq!(
        player.overruns(),
        0,
        "frames that still fit must not be counted as dropped"
    );

    player.advance_frame();
    let first_drop = player.overruns();
    assert_eq!(
        first_drop,
        (Sequencer::FRAME_SAMPLES - free_at_start % Sequencer::FRAME_SAMPLES) as u64,
        "the first overflowing push must drop exactly the part that did not fit"
    );

    player.advance_frame();
    assert_eq!(
        player.overruns(),
        first_drop + Sequencer::FRAME_SAMPLES as u64,
        "a completely full ring drops a whole frame"
    );
    assert_eq!(
        player.underruns(),
        0,
        "overflowing is not underflowing -- the two counters must stay independent"
    );
}

/// [`MusicPlayer::fade_out`] reproduces `m4aMPlayFadeOut`'s schedule for
/// upstream's title-screen speed of 4 (`title_screen.c:784`): the volume
/// holds at 64/64 for the first 4 frames, then drops 4/64 every 4 frames,
/// reaching zero -- and the fade's terminal "player paused" state -- on
/// frame 64. See [`super::player`]'s `FadeOut` docs for the arithmetic.
///
/// Asserted against an identically-seeded unfaded player rather than against
/// hand-picked amplitudes: both sequencers are deterministic, so every
/// sample must be exactly the unfaded one scaled by that frame's gain.
#[test]
fn fade_out_follows_upstreams_speed_4_volume_schedule_and_then_stops() {
    /// 16 steps of 4/64, one every `TITLE_FADE_OUT_SPEED` frames.
    const FADE_FRAMES: u32 = 64;

    let mut plain = MusicPlayer::start(looping_song(), AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    let mut fading = MusicPlayer::start(looping_song(), AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    drain_everything(&mut plain);
    drain_everything(&mut fading);

    assert!(!fading.fade_finished(), "no fade has been started yet");
    fading.fade_out(TITLE_FADE_OUT_SPEED);
    // Idempotent: a second call must not restart the ramp at full volume.
    fading.fade_out(TITLE_FADE_OUT_SPEED);

    let mut plain_frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut fading_frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    for frame in 1..=FADE_FRAMES {
        plain.advance_frame();
        fading.advance_frame();
        plain.drain_null_for_test(&mut plain_frame);
        fading.drain_null_for_test(&mut fading_frame);

        // `fadeOV` starts at 64 and loses 4 on every frame divisible by the
        // speed (`m4a.c:715`), so `volX` is 64 - 4 * (frame / speed).
        let vol_x = 64 - 4 * (frame / u32::from(TITLE_FADE_OUT_SPEED));
        #[allow(clippy::cast_precision_loss)] // both sides are 0..=64.
        let gain = vol_x as f32 / 64.0;
        for (i, (&dry, &wet)) in plain_frame.iter().zip(&fading_frame).enumerate() {
            assert!(
                (wet - dry * gain).abs() < 1e-6,
                "frame {frame}, sample {i}: expected {dry} * {gain} = {}, got {wet}",
                dry * gain
            );
            if dry != 0.0 {
                any_audible = true;
            }
        }

        assert_eq!(
            fading.fade_finished(),
            frame == FADE_FRAMES,
            "frame {frame}: the fade must finish on frame {FADE_FRAMES}, not before or after"
        );
    }
    assert!(
        any_audible,
        "the reference player must actually have been producing sound to fade"
    );
    assert!(
        fading_frame.iter().all(|&s| s == 0.0),
        "the last fade frame must be silent"
    );
}

#[test]
fn a_finished_song_restarts_instead_of_falling_permanently_silent() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player =
        MusicPlayer::start(short_one_shot_song(), output).expect("null backend never errors");

    // Advance well past when the short song's note (and its release tail)
    // would have finished, draining every frame so the ring never just
    // accumulates the prefill's own leftover audio.
    let mut drained = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut audible_after_expected_finish = false;
    for frame in 0..200 {
        player.advance_frame();
        player.drain_null_for_test(&mut drained);
        if frame > 100 && drained.iter().any(|&s| s != 0.0) {
            audible_after_expected_finish = true;
        }
    }
    assert!(
        audible_after_expected_finish,
        "a one-shot song must restart rather than staying silent forever once finished"
    );
}

#[test]
fn finite_reverbed_song_restarts_only_after_tail_drains() {
    let song = finite_reverbed_song();
    // Exactly one rendered frame fits, while the half-ring prefill target is
    // too small for a frame. Playback and the reference therefore both begin
    // at frame zero, with no queued prefill to account for.
    let capacity_frames = Sequencer::FRAME_SAMPLES / usize::from(AudioOutput::CHANNELS);
    let output = AudioOutput::null(capacity_frames);
    let mut player = MusicPlayer::start(song.clone(), output).expect("null backend never errors");
    let mut reference = Sequencer::new(song.clone());
    let mut expected = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut actual = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut heard_tail_without_voice = false;

    for frame in 0..1000 {
        reference.render_frame(&mut expected);
        player.advance_frame();
        player.drain_null_for_test(&mut actual);
        assert_eq!(
            actual, expected,
            "MusicPlayer restarted before the reference tail drained on frame {frame}"
        );

        if reference.voice_count() == 0 && expected.iter().any(|&sample| sample != 0.0) {
            heard_tail_without_voice = true;
        }
        if reference.is_finished() {
            break;
        }
    }

    assert!(
        heard_tail_without_voice,
        "the finite song must render a wet tail after its dry voice stops"
    );
    assert!(
        reference.is_finished(),
        "the finite song's reverb tail must eventually drain"
    );

    let mut restarted = Sequencer::new(song);
    restarted.render_frame(&mut expected);
    player.advance_frame();
    player.drain_null_for_test(&mut actual);
    assert_eq!(
        actual, expected,
        "MusicPlayer must restart on the frame after the drained tail completes"
    );
}

/// Synthetic-pack coverage of [`load_song_from_pack`]'s conversion edges
/// (PR #276 review): a minimal but *real* pack file -- the same on-disk
/// format `cargo xtask extract` writes, built from the assets crate's own
/// public encoders -- so the loader's CGB `fixed_rate` threading and its
/// inherit-vs-explicit-zero reverb mapping are pinned in CI, not only on
/// the ignored real-pack lane.
mod synthetic_pack {
    use assets::{
        AssetPack, Envelope, ProgrammableWave, ProgrammableWaveVoice, Sample, SampleId,
        Square1Voice, Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
    };
    use audio::Instrument;

    use crate::music::load_song_from_pack;

    /// Serialize `entries` (id -> raw payload) into the pack container
    /// format (`assets::pack::format`: magic, version, directory of Raw
    /// entries, payloads) and write it to a per-test scratch path.
    fn write_pack(test_name: &str, entries: &[(&str, Vec<u8>)]) -> std::path::PathBuf {
        const RAW_KIND: u8 = 2;
        // The reader binary-searches the directory, so ids must be sorted --
        // the same determinism guarantee `xtask`'s writer upholds.
        let mut entries: Vec<&(&str, Vec<u8>)> = entries.iter().collect();
        entries.sort_by_key(|(id, _)| *id);
        let entries = entries;
        let header_len = 8 + 4 + 4;
        let dir_len: usize = entries.iter().map(|(id, _)| 2 + id.len() + 1 + 8 + 8).sum();
        let mut payload_offset = header_len + dir_len;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PKMRPACK");
        bytes.extend_from_slice(&6u32.to_le_bytes());
        bytes.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
        for (id, payload) in &entries {
            bytes.extend_from_slice(&u16::try_from(id.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(id.as_bytes());
            bytes.push(RAW_KIND);
            bytes.extend_from_slice(&(payload_offset as u64).to_le_bytes());
            bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
            payload_offset += payload.len();
        }
        for (_, payload) in &entries {
            bytes.extend_from_slice(payload);
        }

        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-music-test-{}-{test_name}.pack",
            std::process::id()
        ));
        std::fs::write(&path, bytes).expect("scratch pack must be writable");
        path
    }

    fn flat_envelope() -> Envelope {
        Envelope {
            attack: 255,
            decay: 0,
            sustain: 255,
            release: 0,
        }
    }

    /// A voicegroup whose first four slots exercise every CGB kind that
    /// carries `TONEDATA_TYPE_FIX`, `fixed_rate` alternating true/false so
    /// both a dropped flag and an invented one fail the assertions below.
    fn fix_voicegroup(wave_id: &str) -> VoiceGroup {
        VoiceGroup::new(vec![
            VoiceEntry::Square1(Square1Voice {
                base_key: 60,
                length: 0,
                sweep: 0,
                duty: 2,
                envelope: flat_envelope(),
                fixed_rate: true,
            }),
            VoiceEntry::Square2(Square2Voice {
                base_key: 60,
                length: 0,
                duty: 2,
                envelope: flat_envelope(),
                fixed_rate: true,
            }),
            VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
                base_key: 60,
                length: 0,
                wave: SampleId(wave_id.to_owned()),
                envelope: flat_envelope(),
                fixed_rate: true,
            }),
            VoiceEntry::Square1(Square1Voice {
                base_key: 60,
                length: 0,
                sweep: 0,
                duty: 2,
                envelope: flat_envelope(),
                fixed_rate: false,
            }),
        ])
        .expect("four slots is well under VOICE_SLOT_COUNT")
    }

    fn pack_with_song(test_name: &str, reverb: Option<u8>) -> AssetPack {
        let vg_id = "audio/voicegroup/fixtest";
        let wave_id = "audio/sample/fixtest_wave";
        let song = assets::Song::new(VoiceGroupId(vg_id.to_owned()), 0, reverb, vec![vec![]])
            .expect("a one-empty-track song is well-formed");
        let sample = Sample::ProgrammableWave(ProgrammableWave { table: [0x88; 16] });
        let path = write_pack(
            test_name,
            &[
                ("audio/song/fixtest", song.encode()),
                (vg_id, fix_voicegroup(wave_id).encode()),
                (wave_id, sample.encode()),
            ],
        );
        AssetPack::load(&path).expect("the synthetic pack must parse")
    }

    /// The `TONEDATA_TYPE_FIX` tag must survive the asset -> engine
    /// conversion for every CGB kind that carries it (`convert_square1`,
    /// `convert_square2`, `convert_programmable_wave`) -- and must not be
    /// invented where the instrument left it clear.
    #[test]
    fn cgb_fixed_rate_tags_survive_loading() {
        let pack = pack_with_song("fixed-rate", None);
        let song = load_song_from_pack(&pack, "fixtest").expect("the synthetic song loads");

        match song.voice(0) {
            Some(Instrument::CgbSquare1(tone)) => {
                assert!(tone.fixed_rate, "square 1's FIX tag must survive loading");
            }
            other => panic!("slot 0 must convert to CgbSquare1, got {other:?}"),
        }
        match song.voice(1) {
            Some(Instrument::CgbSquare2(tone)) => {
                assert!(tone.fixed_rate, "square 2's FIX tag must survive loading");
            }
            other => panic!("slot 1 must convert to CgbSquare2, got {other:?}"),
        }
        match song.voice(2) {
            Some(Instrument::CgbWave(tone)) => {
                assert!(
                    tone.fixed_rate,
                    "the programmable wave's FIX tag must survive loading"
                );
            }
            other => panic!("slot 2 must convert to CgbWave, got {other:?}"),
        }
        match song.voice(3) {
            Some(Instrument::CgbSquare1(tone)) => {
                assert!(
                    !tone.fixed_rate,
                    "a non-FIX instrument must not grow the tag in conversion"
                );
            }
            other => panic!("slot 3 must convert to CgbSquare1, got {other:?}"),
        }
    }

    /// [`load_song_from_pack`] must preserve the header's three reverb
    /// states distinctly: unset stays *no override* (inherit at start
    /// time), explicit `0` stays an explicit disable, and a real level
    /// carries through -- the `unwrap_or(0)` collapse this distinction
    /// replaced must not come back.
    #[test]
    fn loading_preserves_the_inherit_vs_explicit_zero_reverb_distinction() {
        let unset = load_song_from_pack(&pack_with_song("reverb-unset", None), "fixtest")
            .expect("the synthetic song loads");
        assert_eq!(
            unset.reverb_override(),
            None,
            "a header with reverb unset must load as no-override, not as an explicit 0"
        );

        let zero = load_song_from_pack(&pack_with_song("reverb-zero", Some(0)), "fixtest")
            .expect("the synthetic song loads");
        assert_eq!(zero.reverb_override(), Some(0));

        let level = load_song_from_pack(&pack_with_song("reverb-77", Some(77)), "fixtest")
            .expect("the synthetic song loads");
        assert_eq!(level.reverb_override(), Some(77));
    }
}

/// Real-pack coverage of [`load_song_from_pack`] (S-3, issue #185): resolves
/// `mus_title` end to end (voicegroup indirection, samples, reverb) and
/// proves the result behaves like continuous BGM -- audible, self-looping,
/// never reaching `Sequencer::is_finished`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn mus_title_resolves_and_plays_continuously_with_its_real_reverb_level() {
    let pack = assets::AssetPack::load_default().expect("run `cargo xtask extract` first");
    let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");

    // `-R50` in `pokeemerald/sound/songs/midi/midi.cfg`'s `mus_title.mid`
    // entry (cited in this crate's `music` module docs).
    assert_eq!(song.reverb(), 50);

    let mut seq = Sequencer::new(song);
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    // A few seconds of frames: long enough for the real song's intro to
    // start sounding, nowhere near long enough to reach a real BGM's loop
    // point if it had one -- the point here is "does not finish", not
    // "audible on every single frame" (rests are legitimate).
    for _ in 0..300 {
        seq.render_frame(&mut buffer);
        if buffer.iter().any(|&s| s != 0.0) {
            any_audible = true;
        }
        assert!(
            !seq.is_finished(),
            "mus_title must keep looping via its own jump commands, never reach Fine"
        );
    }
    assert!(any_audible, "mus_title must actually produce sound");
}

/// The ignored, env-gated end-to-end fidelity oracle (Discussion #227's
/// owner decision): compares this crate's native offline PCM render of
/// `mus_title` -- before any device resampling -- against a local mGBA
/// reference capture that is never committed to the repository (asset
/// policy, Discussion #71).
///
/// # Producing a reference capture
///
/// 1. Build `mgba` (pinned by `init.sh`, or any recent mGBA) and a Pokémon
///    Emerald ROM (not shipped by this project -- BYO, same policy as every
///    other real-pack/real-ROM tool in this repo).
/// 2. Boot to the title screen and let `MUS_TITLE` play; use mGBA's
///    "Record audio" feature (`Tools > Record A/V` or the equivalent CLI
///    front end) to capture raw PCM at its native output rate.
/// 3. Convert the capture to headerless, interleaved-stereo `f32` PCM at
///    exactly [`audio::pitch::MIXER_RATE`] (13379 Hz) -- e.g.
///    `ffmpeg -i capture.wav -ar 13379 -ac 2 -f f32le title_ref.pcm`. This
///    must be the *native* mGBA render rate resampled down to the engine's
///    own mixer rate, not mGBA's host-audio-driver output rate, so both
///    sides are compared before either one's own device resampling.
/// 4. Point `POKEEMERALD_RS_MGBA_TITLE_PCM` at the resulting file and run
///    `cargo test -p pokeemerald-rs --ignored mgba_reference -- --nocapture`.
///
/// # Alignment and tolerance
///
/// The two renders are not guaranteed to start at the same absolute sample
/// (mGBA's own boot sequence differs from where this render starts): try
/// every offset in the first
/// [`ALIGNMENT_SEARCH_WINDOW`](oracle::ALIGNMENT_SEARCH_WINDOW) samples of
/// the reference and keep the one whose
/// [`COMPARE_WINDOW`](oracle::COMPARE_WINDOW)-sample RMS error against the
/// native render is lowest, then report that offset's error. (Minimum RMS
/// error, not maximum cross-correlation: with both renders at the same
/// nominal amplitude the two pick the same offset, and the error is the
/// quantity the tolerance below is expressed in, so there is no reason to
/// compute a second statistic.) Every offset compares the *same* number of
/// samples, so a late offset cannot win by comparing a shorter tail; the
/// captures are length-checked up front instead.
///
/// Tolerance is relative to the reference's own energy, not an absolute
/// number: the native render must sit within
/// [`RMS_ERROR_FRACTION`](oracle::RMS_ERROR_FRACTION) of the reference's RMS
/// over the compared window, and the reference itself must be louder than
/// [`MIN_REFERENCE_RMS`](oracle::MIN_REFERENCE_RMS) so a silent or
/// near-silent capture fails loudly rather than passing trivially. Not
/// sample-exact (`(behavioral-fidelity)`: player-audible result, not byte
/// parity). The fraction is a deliberately generous placeholder that still
/// catches a grossly wrong render (silence, garbage, wrong pitch); per
/// Discussion #227's decision it must be re-derived and re-documented
/// against the *first* real reference comparison run.
mod oracle {
    use std::env;
    use std::fs;

    use audio::{Sequencer, MIXER_RATE};

    use super::load_song_from_pack;

    /// How far into the reference to search for the best-aligning offset.
    pub(super) const ALIGNMENT_SEARCH_WINDOW: usize = MIXER_RATE as usize * 2; // 2 seconds
    /// How many aligned samples every candidate offset compares -- fixed, so
    /// all offsets are scored on equal terms.
    pub(super) const COMPARE_WINDOW: usize = MIXER_RATE as usize * 5; // 5 seconds
    /// Placeholder tolerance (module docs), as a fraction of the reference
    /// window's own RMS: re-derive from the first real mGBA comparison and
    /// replace this constant (and this comment) with the justified value.
    pub(super) const RMS_ERROR_FRACTION: f64 = 0.25;
    /// Floor on the reference window's RMS. Below this the capture is
    /// effectively silent, every relative tolerance becomes meaningless, and
    /// the comparison is worthless -- so it fails instead of passing.
    pub(super) const MIN_REFERENCE_RMS: f64 = 1e-4;

    /// Read a headerless, interleaved-stereo `f32` PCM file.
    fn read_pcm_f32(path: &str) -> Vec<f32> {
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading `{path}`: {e}"));
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// Root-mean-square of a slice.
    fn rms(xs: &[f32]) -> f64 {
        assert!(!xs.is_empty(), "RMS of an empty window");
        let sum_sq: f64 = xs.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
        #[allow(clippy::cast_precision_loss)] // a handful of seconds of samples.
        let mean = sum_sq / xs.len() as f64;
        mean.sqrt()
    }

    /// RMS error between `reference[offset..]` and `candidate`, over exactly
    /// [`COMPARE_WINDOW`] samples.
    ///
    /// # Panics
    ///
    /// If either side is too short for a full window at `offset` -- callers
    /// are expected to have length-checked already, so this is a bug, not a
    /// short capture.
    fn rms_error(reference: &[f32], candidate: &[f32], offset: usize) -> f64 {
        assert!(
            reference.len() >= offset + COMPARE_WINDOW && candidate.len() >= COMPARE_WINDOW,
            "rms_error called without a full {COMPARE_WINDOW}-sample window at offset {offset}"
        );
        let sum_sq: f64 = (0..COMPARE_WINDOW)
            .map(|i| {
                let diff = f64::from(reference[offset + i]) - f64::from(candidate[i]);
                diff * diff
            })
            .sum();
        #[allow(clippy::cast_precision_loss)] // `COMPARE_WINDOW` is a handful of seconds.
        let mean = sum_sq / COMPARE_WINDOW as f64;
        mean.sqrt()
    }

    /// Search `0..ALIGNMENT_SEARCH_WINDOW` for the offset into `reference`
    /// with the lowest RMS error against `candidate`'s start.
    ///
    /// Each offset's error is computed exactly once into a `Vec` before the
    /// minimum is taken: `min_by` re-evaluates both sides of every
    /// comparison, which would double an already
    /// `ALIGNMENT_SEARCH_WINDOW * COMPARE_WINDOW`-sized scan (order 10^9
    /// float operations, in a debug binary).
    ///
    /// # Panics
    ///
    /// If `reference` is shorter than `ALIGNMENT_SEARCH_WINDOW +
    /// COMPARE_WINDOW`, or `candidate` shorter than `COMPARE_WINDOW`.
    fn best_alignment(reference: &[f32], candidate: &[f32]) -> usize {
        assert!(
            reference.len() >= ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW,
            "reference capture is too short: {} samples per channel, but aligning over \
             {ALIGNMENT_SEARCH_WINDOW} and comparing {COMPARE_WINDOW} needs at least {}. Capture \
             more audio (see this module's docs) rather than comparing a shrinking window.",
            reference.len(),
            ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW
        );
        assert!(
            candidate.len() >= COMPARE_WINDOW,
            "native render is too short: {} samples per channel, need {COMPARE_WINDOW}",
            candidate.len()
        );
        let errors: Vec<f64> = (0..ALIGNMENT_SEARCH_WINDOW)
            .map(|offset| rms_error(reference, candidate, offset))
            .collect();
        errors
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).expect("RMS error is always finite"))
            .map(|(offset, _)| offset)
            .expect("ALIGNMENT_SEARCH_WINDOW is nonzero")
    }

    #[test]
    #[ignore = "needs a local pack and a local mGBA reference capture: see this module's docs"]
    fn native_render_matches_local_mgba_reference_within_tolerance() {
        let Ok(path) = env::var("POKEEMERALD_RS_MGBA_TITLE_PCM") else {
            eprintln!(
                "skipped: set POKEEMERALD_RS_MGBA_TITLE_PCM to a local mGBA reference capture \
                 (interleaved-stereo f32 PCM at MIXER_RATE) -- see this module's doc comment for \
                 how to produce one"
            );
            return;
        };

        let pack = assets::AssetPack::load_default().expect("run `cargo xtask extract` first");
        let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");
        let mut seq = Sequencer::new(song);

        let native_frames =
            (ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW).div_ceil(audio::SAMPLES_PER_FRAME);
        let mut native = vec![0.0_f32; native_frames * Sequencer::FRAME_SAMPLES];
        seq.mix_into(&mut native);
        // Left channel only, matching `rms_error`/`read_pcm_f32`'s layout.
        let native_left: Vec<f32> = native.iter().copied().step_by(2).collect();

        let reference = read_pcm_f32(&path);
        let reference_left: Vec<f32> = reference.iter().copied().step_by(2).collect();

        let offset = best_alignment(&reference_left, &native_left);
        let error = rms_error(&reference_left, &native_left, offset);
        let reference_rms = rms(&reference_left[offset..offset + COMPARE_WINDOW]);
        assert!(
            reference_rms > MIN_REFERENCE_RMS,
            "the reference capture's aligned window is silent (RMS {reference_rms:.3e} at offset \
             {offset}): it captured no audio, so there is nothing to compare against"
        );
        let tolerance = RMS_ERROR_FRACTION * reference_rms;
        assert!(
            error < tolerance,
            "native render diverges from the mGBA reference: RMS error {error:.4} at aligned \
             offset {offset} exceeds {RMS_ERROR_FRACTION} of the reference's own RMS \
             {reference_rms:.4} (tolerance {tolerance:.4})"
        );
    }
}
