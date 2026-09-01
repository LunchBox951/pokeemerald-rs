//! Covers packed-song conversion and frame-driven playback.

use std::sync::Arc;

use audio::{Adsr, Event, Instrument, Sequencer, Song, ToneData, WaveData};
use platform::AudioOutput;

use super::{load_song_from_pack, MusicPlayer, RING_CAPACITY_FRAMES, TITLE_FADE_OUT_SPEED};

const RING_CAPACITY_SAMPLES: usize = RING_CAPACITY_FRAMES * (AudioOutput::CHANNELS as usize);

fn drain_everything(player: &mut MusicPlayer) {
    let queued = RING_CAPACITY_SAMPLES - player.ring_free_for_test();
    let mut sink = vec![0.0_f32; queued];
    player.drain_null_for_test(&mut sink);
}

fn loud_wave() -> Arc<WaveData> {
    Arc::new(WaveData::one_shot(1 << 20, vec![100; 64]))
}

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

#[test]
fn overruns_count_the_samples_a_full_ring_drops() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");
    assert_eq!(player.overruns(), 0);

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

#[test]
fn fade_out_follows_upstreams_speed_4_volume_schedule_and_then_stops() {
    const FULL_VOLUME: u32 = 64;
    const VOLUME_PER_STEP: u32 = 4;
    const FADE_FRAMES: u32 = 64;

    let mut plain = MusicPlayer::start(looping_song(), AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    let mut fading = MusicPlayer::start(looping_song(), AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    drain_everything(&mut plain);
    drain_everything(&mut fading);

    assert!(!fading.fade_finished(), "no fade has been started yet");
    fading.fade_out(TITLE_FADE_OUT_SPEED);
    fading.fade_out(TITLE_FADE_OUT_SPEED);

    let mut plain_frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut fading_frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    for frame in 1..=FADE_FRAMES {
        plain.advance_frame();
        fading.advance_frame();
        plain.drain_null_for_test(&mut plain_frame);
        fading.drain_null_for_test(&mut fading_frame);

        let volume = FULL_VOLUME - VOLUME_PER_STEP * (frame / u32::from(TITLE_FADE_OUT_SPEED));
        #[expect(
            clippy::cast_precision_loss,
            reason = "fade volume values from zero through 64 are exact in f32"
        )]
        let gain = volume as f32 / FULL_VOLUME as f32;
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

mod synthetic_pack {
    use assets::{
        AssetPack, Envelope, ProgrammableWave, ProgrammableWaveVoice, Sample, SampleId,
        Square1Voice, Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
    };
    use audio::Instrument;

    use crate::music::load_song_from_pack;

    fn write_pack(test_name: &str, entries: &[(&str, Vec<u8>)]) -> std::path::PathBuf {
        const PACK_MAGIC: &[u8; 8] = b"PKMRPACK";
        const PACK_VERSION: u32 = 6;
        const RAW_ENTRY_KIND: u8 = 2;

        // AssetPack binary-searches directory entries by ID.
        let mut entries: Vec<&(&str, Vec<u8>)> = entries.iter().collect();
        entries.sort_by_key(|(id, _)| *id);
        let entries = entries;
        let header_len = PACK_MAGIC.len() + size_of::<u32>() * 2;
        let dir_len: usize = entries
            .iter()
            .map(|(id, _)| size_of::<u16>() + id.len() + size_of::<u8>() + size_of::<u64>() * 2)
            .sum();
        let mut payload_offset = header_len + dir_len;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(PACK_MAGIC);
        bytes.extend_from_slice(&PACK_VERSION.to_le_bytes());
        bytes.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
        for (id, payload) in &entries {
            bytes.extend_from_slice(&u16::try_from(id.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(id.as_bytes());
            bytes.push(RAW_ENTRY_KIND);
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

    fn fixed_rate_voicegroup(wave_id: &str) -> VoiceGroup {
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
                (vg_id, fixed_rate_voicegroup(wave_id).encode()),
                (wave_id, sample.encode()),
            ],
        );
        AssetPack::load(&path).expect("the synthetic pack must parse")
    }

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

    fn pack_with_priority(test_name: &str, priority: u8) -> AssetPack {
        let vg_id = "audio/voicegroup/fixtest";
        let wave_id = "audio/sample/fixtest_wave";
        let song = assets::Song::new(VoiceGroupId(vg_id.to_owned()), priority, None, vec![vec![]])
            .expect("a one-empty-track song is well-formed");
        let sample = Sample::ProgrammableWave(ProgrammableWave { table: [0x88; 16] });
        let path = write_pack(
            test_name,
            &[
                ("audio/song/fixtest", song.encode()),
                (vg_id, fixed_rate_voicegroup(wave_id).encode()),
                (wave_id, sample.encode()),
            ],
        );
        AssetPack::load(&path).expect("the synthetic pack must parse")
    }

    #[test]
    fn loading_carries_the_header_priority_into_the_runtime_song() {
        let plain = load_song_from_pack(&pack_with_priority("prio-zero", 0), "fixtest")
            .expect("the synthetic song loads");
        assert_eq!(plain.priority(), 0);

        let raised = load_song_from_pack(&pack_with_priority("prio-200", 200), "fixtest")
            .expect("the synthetic song loads");
        assert_eq!(raised.priority(), 200);
    }

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

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn mus_title_resolves_and_plays_continuously_with_its_real_reverb_level() {
    const TITLE_REVERB_LEVEL: u8 = 50;
    const PLAYBACK_PROBE_FRAMES: usize = 300;

    let pack = assets::AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");

    assert_eq!(song.reverb(), TITLE_REVERB_LEVEL);

    let mut seq = Sequencer::new(song);
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    for _ in 0..PLAYBACK_PROBE_FRAMES {
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

mod oracle {
    //! Compares an offline `mus_title` render with a local mGBA capture.
    //!
    //! Capture the title music with the configured mGBA build, then convert it
    //! to headerless interleaved-stereo `f32` PCM at the engine mixer rate:
    //! `ffmpeg -i capture.wav -ar 13379 -ac 2 -f f32le title_ref.pcm`.
    //! Set `POKEEMERALD_RS_MGBA_TITLE_PCM` to that file and run
    //! `cargo test -p pokeemerald-rs --ignored mgba_reference -- --nocapture`.
    //!
    //! The comparison searches the first two seconds of the reference for the
    //! five-second left-channel window with the lowest RMS error. It rejects
    //! silent references and treats error above 25% of the reference RMS as
    //! gross behavioural divergence, not sample-exact inequality.

    use std::env;
    use std::fs;

    use audio::{Sequencer, MIXER_RATE};

    use super::load_song_from_pack;

    const ALIGNMENT_SEARCH_SECONDS: usize = 2;
    const COMPARE_SECONDS: usize = 5;
    const ALIGNMENT_SEARCH_WINDOW: usize = MIXER_RATE as usize * ALIGNMENT_SEARCH_SECONDS;
    const COMPARE_WINDOW: usize = MIXER_RATE as usize * COMPARE_SECONDS;
    const MAX_RMS_ERROR_FRACTION: f64 = 0.25;
    const MIN_REFERENCE_RMS: f64 = 1e-4;

    fn read_pcm_f32(path: &str) -> Vec<f32> {
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading `{path}`: {e}"));
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    fn rms(xs: &[f32]) -> f64 {
        assert!(!xs.is_empty(), "RMS of an empty window");
        let sum_sq: f64 = xs.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "the fixed comparison windows are exactly representable in f64"
        )]
        let mean = sum_sq / xs.len() as f64;
        mean.sqrt()
    }

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
        #[expect(
            clippy::cast_precision_loss,
            reason = "the fixed comparison window is exactly representable in f64"
        )]
        let mean = sum_sq / COMPARE_WINDOW as f64;
        mean.sqrt()
    }

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
        (0..ALIGNMENT_SEARCH_WINDOW)
            .map(|offset| (offset, rms_error(reference, candidate, offset)))
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).expect("RMS error is always finite"))
            .map(|(offset, _)| offset)
            .expect("ALIGNMENT_SEARCH_WINDOW is nonzero")
    }

    fn left_channel(interleaved_stereo: &[f32]) -> Vec<f32> {
        interleaved_stereo.iter().copied().step_by(2).collect()
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

        let pack = assets::AssetPack::load_repo().expect("run `cargo xtask extract` first");
        let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");
        let mut seq = Sequencer::new(song);

        let native_frames =
            (ALIGNMENT_SEARCH_WINDOW + COMPARE_WINDOW).div_ceil(audio::SAMPLES_PER_FRAME);
        let mut native = vec![0.0_f32; native_frames * Sequencer::FRAME_SAMPLES];
        seq.mix_into(&mut native);
        let native_left = left_channel(&native);

        let reference = read_pcm_f32(&path);
        let reference_left = left_channel(&reference);

        let offset = best_alignment(&reference_left, &native_left);
        let error = rms_error(&reference_left, &native_left, offset);
        let reference_rms = rms(&reference_left[offset..offset + COMPARE_WINDOW]);
        assert!(
            reference_rms > MIN_REFERENCE_RMS,
            "the reference capture's aligned window is silent (RMS {reference_rms:.3e} at offset \
             {offset}): it captured no audio, so there is nothing to compare against"
        );
        let tolerance = MAX_RMS_ERROR_FRACTION * reference_rms;
        assert!(
            error < tolerance,
            "native render diverges from the mGBA reference: RMS error {error:.4} at aligned \
             offset {offset} exceeds {MAX_RMS_ERROR_FRACTION} of the reference's own RMS \
             {reference_rms:.4} (tolerance {tolerance:.4})"
        );
    }
}
