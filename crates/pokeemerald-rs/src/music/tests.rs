//! Covers packed-song conversion and frame-driven playback.

use std::sync::Arc;

use audio::song::SquareTone;
use audio::{Adsr, CgbAdsr, Event, Instrument, Sequencer, Song, ToneData, WaveData};
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

/// `tracks` copies of one voice sustaining the same looping note, unlike
/// [`looping_song`]'s one-shot wave, which falls silent between repeats.
/// `Wait` must outlast every test using this song, or `Goto` restarts the
/// untied note early, doubling the voice.
fn sustained_song(tracks: usize) -> Song {
    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; 64]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(200),
        Event::Goto(0),
    ];
    Song::new(voices, vec![track; tracks], 150)
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
    // A faded sample is now `dry * gain` truncated through several integer
    // `>>` stages (`sequencer::track_volume`, `voice::channel_volume`)
    // instead of one exact float multiply (issue #1243); `1.5/128` covers
    // this fixture's worst observed truncation (`1.4375/128` at `volX ==
    // 4`) with headroom.
    const TRUNCATION_TOLERANCE: f32 = 1.5 / 128.0;
    // (frame, left, right) mix units at representative steps, independently
    // derived from those same integer stages rather than observed from this
    // player: catches a wrong stage, or a reintroduced post-mix scale, that
    // TRUNCATION_TOLERANCE's headroom alone would miss.
    const EXACT_MIX_UNITS: [(u32, u8, u8); 3] = [(4, 36, 37), (32, 19, 19), (60, 1, 1)];

    let mut plain = MusicPlayer::start(sustained_song(1), AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    let mut fading = MusicPlayer::start(sustained_song(1), AudioOutput::null(RING_CAPACITY_FRAMES))
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
                (wet - dry * gain).abs() < TRUNCATION_TOLERANCE,
                "frame {frame}, sample {i}: expected about {dry} * {gain} = {}, got {wet}",
                dry * gain
            );
            if dry != 0.0 {
                any_audible = true;
            }
        }
        if let Some(&(_, left_units, right_units)) =
            EXACT_MIX_UNITS.iter().find(|&&(f, ..)| f == frame)
        {
            assert_eq!(
                (fading_frame[0], fading_frame[1]),
                (f32::from(left_units) / 128.0, f32::from(right_units) / 128.0),
                "frame {frame}: exact fade level check (independent of TRUNCATION_TOLERANCE) failed"
            );
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

/// Renders `song` for `frames` game frames past the prefill, fading from the
/// first frame when `fade_speed` is given, and returns the last frame's
/// first sample.
fn first_sample_after_frames(song: Song, frames: u32, fade_speed: Option<u16>) -> f32 {
    let mut player = MusicPlayer::start(song, AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    drain_everything(&mut player);
    if let Some(speed) = fade_speed {
        player.fade_out(speed);
    }
    let mut frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    for _ in 0..frames {
        player.advance_frame();
        player.drain_null_for_test(&mut frame);
    }
    frame[0]
}

/// Upstream scales each voice, then sums, then clips (`FadeOutBody`,
/// `m4a.c:750`-`:757`; `TrkVolPitSet`, `m4a.c:765`-`:788`); four full-volume
/// tracks overflow signed 8-bit together, so halving their volume must
/// relieve that clipping, not just halve its clipped remainder.
#[test]
fn a_fade_scales_each_voice_before_the_mixer_clips_their_sum() {
    const CLIPPING_TRACKS: u8 = 4;
    const FADE_SPEED: u16 = 1;
    const HALF_VOLUME_FRAME: u32 = 8;
    const FULL_SCALE: f32 = 127.0 / 128.0;

    let one_voice = first_sample_after_frames(sustained_song(1), 1, None);
    let unclipped_sum = one_voice * f32::from(CLIPPING_TRACKS);
    assert!(
        unclipped_sum > FULL_SCALE,
        "the test needs voices whose sum overflows the mix: \
         {CLIPPING_TRACKS} x {one_voice} = {unclipped_sum}"
    );

    // Per-voice volume truncation costs at most one mix unit per voice.
    let tolerance = f32::from(CLIPPING_TRACKS) / 128.0;
    let unfaded = first_sample_after_frames(sustained_song(usize::from(CLIPPING_TRACKS)), 1, None);
    assert!(
        (unfaded - FULL_SCALE).abs() < tolerance,
        "the unfaded frame must sit at clipped full scale, got {unfaded}"
    );

    let faded = first_sample_after_frames(
        sustained_song(usize::from(CLIPPING_TRACKS)),
        HALF_VOLUME_FRAME,
        Some(FADE_SPEED),
    );
    let expected = unclipped_sum * 0.5;
    assert!(
        expected < FULL_SCALE,
        "at half volume the faded voices must fit in the mix without clipping"
    );
    assert!(
        (faded - expected).abs() < tolerance,
        "half-volume fade of {CLIPPING_TRACKS} clipping voices: expected about {expected}, \
         got {faded} (scaling the clipped frame instead would give {})",
        FULL_SCALE * 0.5
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
        AssetPack, DirectSoundMode, DirectSoundSample, DirectSoundVoice, Envelope, KeySplitVoice,
        ProgrammableWave, ProgrammableWaveVoice, Sample, SampleId, SongEvent, Square1Voice,
        Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
    };
    use audio::{Instrument, Sequencer, DEFAULT_MASTER_VOLUME};

    use crate::music::load_song_from_pack;

    struct TempPackGuard {
        path: std::path::PathBuf,
    }

    impl TempPackGuard {
        fn new(path: std::path::PathBuf) -> Self {
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempPackGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn write_pack(test_name: &str, entries: &[(&str, Vec<u8>)]) -> TempPackGuard {
        const PACK_MAGIC: &[u8; 8] = b"PKMRPACK";
        // Bound to the live format version rather than a hardcoded number so
        // this synthetic pack keeps matching what `pack_format`'s reader
        // accepts as the format evolves.
        const PACK_VERSION: u32 = assets::pack::FORMAT_VERSION;
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
        // Own the path before writing so a write panic still cleans up.
        let temp_pack = TempPackGuard::new(path);
        std::fs::write(temp_pack.path(), bytes).expect("scratch pack must be writable");
        temp_pack
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

    // The returned guard outlives the pack in the caller's scope so the
    // scratch file is removed on every exit path, panics included.
    fn pack_with_song(test_name: &str, reverb: Option<u8>) -> (AssetPack, TempPackGuard) {
        let vg_id = "audio/voicegroup/fixtest";
        let wave_id = "audio/sample/fixtest_wave";
        let song = assets::Song::new(VoiceGroupId(vg_id.to_owned()), 0, reverb, vec![vec![]])
            .expect("a one-empty-track song is well-formed");
        let sample = Sample::ProgrammableWave(ProgrammableWave { table: [0x88; 16] });
        let temp_pack = write_pack(
            test_name,
            &[
                ("audio/song/fixtest", song.encode()),
                (vg_id, fixed_rate_voicegroup(wave_id).encode()),
                (wave_id, sample.encode()),
            ],
        );
        let pack = AssetPack::load(temp_pack.path()).expect("the synthetic pack must parse");
        (pack, temp_pack)
    }

    #[test]
    fn cgb_fixed_rate_tags_survive_loading() {
        let (pack, _pack_guard) = pack_with_song("fixed-rate", None);
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

    fn direct_sound_voicegroup(wave_id: &str) -> VoiceGroup {
        VoiceGroup::new(vec![VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 60,
            pan: None,
            sample: SampleId(wave_id.to_owned()),
            envelope: flat_envelope(),
            mode: DirectSoundMode::Resampled,
        })])
        .expect("one slot is well under VOICE_SLOT_COUNT")
    }

    // The returned guard outlives the pack in the caller's scope so the
    // scratch file is removed on every exit path, panics included.
    fn pack_with_direct_sound_sample(
        test_name: &str,
        sample: DirectSoundSample,
    ) -> (AssetPack, TempPackGuard) {
        let vg_id = "audio/voicegroup/fixtest_direct_sound";
        let wave_id = "audio/sample/fixtest_direct_sound_wave";
        let song = assets::Song::new(VoiceGroupId(vg_id.to_owned()), 0, None, vec![vec![]])
            .expect("a one-empty-track song is well-formed");
        let temp_pack = write_pack(
            test_name,
            &[
                ("audio/song/fixtest", song.encode()),
                (vg_id, direct_sound_voicegroup(wave_id).encode()),
                (wave_id, Sample::DirectSound(sample).encode()),
            ],
        );
        let pack = AssetPack::load(temp_pack.path()).expect("the synthetic pack must parse");
        (pack, temp_pack)
    }

    #[test]
    fn direct_sound_conversion_narrows_the_wave_to_its_logical_sample_count() {
        // `data`'s last value (99) is the retained interpolation guard past
        // `sample_count` (2); the pack-to-runtime conversion must narrow the
        // `WaveData` back to that logical length so the mixer's loop/one-shot
        // boundary (`crates/audio/src/voice.rs`) never treats the guard as a
        // genuine playable sample.
        let sample = DirectSoundSample::new(1 << 20, Some(0), 2, vec![10, -10, 99])
            .expect("a two-sample looping wave with a retained guard is well-formed");
        let (pack, _pack_guard) = pack_with_direct_sound_sample("direct-sound-logical-len", sample);
        let song = load_song_from_pack(&pack, "fixtest").expect("the synthetic song loads");
        match song.voice(0) {
            Some(Instrument::DirectSound(tone)) => {
                assert_eq!(
                    tone.wave.len(),
                    3,
                    "the buffer must keep the retained guard"
                );
                assert_eq!(
                    tone.wave.logical_len(),
                    2,
                    "the wave must be narrowed to the sample's logical count, not its buffer \
                     length"
                );
            }
            other => panic!("slot 0 must convert to DirectSound, got {other:?}"),
        }
    }

    // The returned guard outlives the pack in the caller's scope so the
    // scratch file is removed on every exit path, panics included.
    fn pack_with_priority(test_name: &str, priority: u8) -> (AssetPack, TempPackGuard) {
        let vg_id = "audio/voicegroup/fixtest";
        let wave_id = "audio/sample/fixtest_wave";
        let song = assets::Song::new(VoiceGroupId(vg_id.to_owned()), priority, None, vec![vec![]])
            .expect("a one-empty-track song is well-formed");
        let sample = Sample::ProgrammableWave(ProgrammableWave { table: [0x88; 16] });
        let temp_pack = write_pack(
            test_name,
            &[
                ("audio/song/fixtest", song.encode()),
                (vg_id, fixed_rate_voicegroup(wave_id).encode()),
                (wave_id, sample.encode()),
            ],
        );
        let pack = AssetPack::load(temp_pack.path()).expect("the synthetic pack must parse");
        (pack, temp_pack)
    }

    #[test]
    fn loading_carries_the_header_priority_into_the_runtime_song() {
        let (plain_pack, _plain_guard) = pack_with_priority("prio-zero", 0);
        let plain = load_song_from_pack(&plain_pack, "fixtest").expect("the synthetic song loads");
        assert_eq!(plain.priority(), 0);

        let (raised_pack, _raised_guard) = pack_with_priority("prio-200", 200);
        let raised =
            load_song_from_pack(&raised_pack, "fixtest").expect("the synthetic song loads");
        assert_eq!(raised.priority(), 200);
    }

    #[test]
    fn loading_preserves_the_inherit_vs_explicit_zero_reverb_distinction() {
        let (unset_pack, _unset_guard) = pack_with_song("reverb-unset", None);
        let unset = load_song_from_pack(&unset_pack, "fixtest").expect("the synthetic song loads");
        assert_eq!(
            unset.reverb_override(),
            None,
            "a header with reverb unset must load as no-override, not as an explicit 0"
        );

        let (zero_pack, _zero_guard) = pack_with_song("reverb-zero", Some(0));
        let zero = load_song_from_pack(&zero_pack, "fixtest").expect("the synthetic song loads");
        assert_eq!(zero.reverb_override(), Some(0));

        let (level_pack, _level_guard) = pack_with_song("reverb-77", Some(77));
        let level = load_song_from_pack(&level_pack, "fixtest").expect("the synthetic song loads");
        assert_eq!(level.reverb_override(), Some(77));
    }

    #[test]
    fn synthetic_pack_helpers_leave_no_scratch_file_behind_on_panic() {
        // Every synthetic pack these helpers write must be gone once the test
        // body exits, unwinding included, so the shared temp dir never
        // accumulates leftover fixtures across runs.
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-music-test-{}-unwind-cleanup.pack",
            std::process::id()
        ));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (_pack, _guard) = pack_with_song("unwind-cleanup", None);
            assert!(path.exists(), "the scratch pack must exist while in scope");
            panic!("deliberate panic to exercise unwind cleanup");
        }));

        assert!(result.is_err(), "the inner closure must have panicked");
        assert!(
            !path.exists(),
            "the guard must remove the scratch pack even when the test panics"
        );
    }

    fn occupant_track() -> Vec<SongEvent> {
        vec![
            SongEvent::Priority(0),
            SongEvent::Voice(0),
            SongEvent::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            SongEvent::Wait(48),
            SongEvent::Fine,
        ]
    }

    /// Selects a silent key-split child at higher priority; must not evict
    /// [`occupant_track`]'s note, since a silent child produces no note before allocation.
    fn evictor_track(key: u8) -> Vec<SongEvent> {
        vec![
            SongEvent::Priority(90),
            SongEvent::Voice(1),
            SongEvent::Note {
                key,
                velocity: 127,
                gate: 8,
            },
            SongEvent::Wait(48),
            SongEvent::Fine,
        ]
    }

    #[test]
    fn empty_and_nested_key_split_children_do_not_evict_an_occupied_voice() {
        const WAVE_ID: &str = "audio/sample/keysplit_wave";
        const TOP_VG_ID: &str = "audio/voicegroup/keysplit_top";
        const CHILD_VG_ID: &str = "audio/voicegroup/keysplit_children";

        let wave = Sample::DirectSound(
            DirectSoundSample::new(1 << 20, Some(0), 63, vec![100; 64])
                .expect("a looping 64-sample wave is well-formed"),
        );
        // `voice(1)` splits on the played key: 60 selects child 0 (`Empty`), 61 selects
        // child 1 (a nested key split) -- the two silent cases.
        let top_group = VoiceGroup::new(vec![
            VoiceEntry::DirectSound(DirectSoundVoice {
                base_key: 60,
                pan: None,
                sample: SampleId(WAVE_ID.to_owned()),
                envelope: flat_envelope(),
                mode: DirectSoundMode::Resampled,
            }),
            VoiceEntry::KeySplit(
                KeySplitVoice::new(60, vec![0, 1], VoiceGroupId(CHILD_VG_ID.to_owned()))
                    .expect("a two-entry table is well under VOICE_SLOT_COUNT"),
            ),
        ])
        .expect("two slots is well under VOICE_SLOT_COUNT");
        let child_group = VoiceGroup::new(vec![
            VoiceEntry::Empty,
            VoiceEntry::KeySplit(
                // Never resolved: nested children map straight to `None`,
                // so this target id need not exist in the pack.
                KeySplitVoice::new(
                    0,
                    vec![0],
                    VoiceGroupId("audio/voicegroup/unresolved".to_owned()),
                )
                .expect("a one-entry table is well under VOICE_SLOT_COUNT"),
            ),
        ])
        .expect("two slots is well under VOICE_SLOT_COUNT");

        let control = assets::Song::new(
            VoiceGroupId(TOP_VG_ID.to_owned()),
            0,
            None,
            vec![occupant_track()],
        )
        .expect("one track is well-formed");
        let empty_child = assets::Song::new(
            VoiceGroupId(TOP_VG_ID.to_owned()),
            0,
            None,
            vec![occupant_track(), evictor_track(60)],
        )
        .expect("two tracks is well-formed");
        let nested_child = assets::Song::new(
            VoiceGroupId(TOP_VG_ID.to_owned()),
            0,
            None,
            vec![occupant_track(), evictor_track(61)],
        )
        .expect("two tracks is well-formed");

        let temp_pack = write_pack(
            "keysplit-silent-children",
            &[
                ("audio/song/keysplit_control", control.encode()),
                ("audio/song/keysplit_empty_child", empty_child.encode()),
                ("audio/song/keysplit_nested_child", nested_child.encode()),
                (TOP_VG_ID, top_group.encode()),
                (CHILD_VG_ID, child_group.encode()),
                (WAVE_ID, wave.encode()),
            ],
        );
        let pack = AssetPack::load(temp_pack.path()).expect("the synthetic pack must parse");

        let render_first_frame = |name: &str| {
            let song = load_song_from_pack(&pack, name).expect("the synthetic song loads");
            // One DirectSound slot: any note reaching allocation evicts the occupant
            // (`mixer::select_direct_sound_slot`), which a silent child must never do.
            let mut seq = Sequencer::with_config(song, DEFAULT_MASTER_VOLUME, 1);
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            seq.render_frame(&mut out);
            (seq.voice_count(), out)
        };

        let (control_voices, control_frame) = render_first_frame("keysplit_control");
        assert_eq!(
            control_voices, 1,
            "sanity: the occupant must claim the sole DirectSound slot"
        );
        assert!(
            control_frame.iter().any(|&s| s != 0.0),
            "sanity: the occupant note must be audible"
        );

        for (name, label) in [
            ("keysplit_empty_child", "an empty key-split child"),
            ("keysplit_nested_child", "a nested key-split child"),
        ] {
            let (voices, frame) = render_first_frame(name);
            assert_eq!(
                voices, 1,
                "{label} must not add a second voice to the full DirectSound pool"
            );
            assert_eq!(
                frame, control_frame,
                "{label} must leave the occupied DirectSound slot's output untouched"
            );
        }
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

/// The PSG counterpart to [`sustained_song`]: one CGB square voice holding a
/// tied note, so a fade's terminal frame can be inspected on a channel whose
/// gain runs through the CGB envelope rather than the `DirectSound` mixer.
fn sustained_cgb_song() -> Song {
    let voices = vec![Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: 0,
        adsr: CgbAdsr::flat(),
        fixed_rate: false,
    })];
    let track = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(200),
        Event::Goto(0),
    ];
    Song::new(voices, vec![track], 150)
}

/// `FadeOutBody` stops every existing track outright on the step that reaches
/// `volX == 0` (`m4a.c:750`-`:757`), and `TrackStop` turns each CGB channel
/// off as it goes -- so the frame mixed after that step carries no PSG sound.
#[test]
fn a_finished_fade_silences_a_sustained_cgb_voice_on_its_terminal_frame() {
    const FADE_FRAMES: u32 = 64;

    let mut player = MusicPlayer::start(
        sustained_cgb_song(),
        AudioOutput::null(RING_CAPACITY_FRAMES),
    )
    .expect("null backend never errors");
    drain_everything(&mut player);
    player.fade_out(TITLE_FADE_OUT_SPEED);

    let mut frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    for _ in 1..=FADE_FRAMES {
        player.advance_frame();
        player.drain_null_for_test(&mut frame);
        if frame.iter().any(|&s| s != 0.0) {
            any_audible = true;
        }
    }

    assert!(
        player.fade_finished(),
        "sanity: the fade must finish on frame {FADE_FRAMES}"
    );
    assert!(
        any_audible,
        "sanity: the CGB voice must actually have been producing sound to fade"
    );
    assert!(
        frame.iter().all(|&s| s == 0.0),
        "the terminal fade frame must be silent, got {:?}",
        &frame[..4]
    );
}

/// The `DirectSound` counterpart to
/// [`a_finished_fade_silences_a_sustained_cgb_voice_on_its_terminal_frame`]:
/// a reverbed song, whose master-mix feedback ring still holds seven frames
/// of delayed `DirectSound` samples when the terminal fade step stops every
/// track (`m4a.c:720`-`:735`). Upstream's mixer keeps running through the
/// paused player (`SoundMain` and `SoundMainRAM_Reverb`, `m4a_1.s:20`-`:119`),
/// so those delayed samples keep sounding as a decaying wet tail; stopping the
/// tracks silences the dry voices, not the ring. The player is droppable only
/// once that tail has rung down.
#[test]
fn a_finished_fade_keeps_sounding_the_reverb_tail_the_ring_still_holds() {
    const TITLE_REVERB_LEVEL: u8 = 50;
    const FRAMES: u32 = 96;

    let mut player = MusicPlayer::start(
        sustained_song(1).with_reverb(TITLE_REVERB_LEVEL),
        AudioOutput::null(RING_CAPACITY_FRAMES),
    )
    .expect("null backend never errors");
    drain_everything(&mut player);
    player.fade_out(TITLE_FADE_OUT_SPEED);

    let mut frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut audible_through_terminal = false;
    let mut audible_after_terminal = false;
    let mut past_terminal = false;
    for _ in 0..FRAMES {
        // The contract `App::advance_music` is built on -- and which
        // `app::tests::a_faded_reverbed_songs_tail_keeps_sounding_past_the_terminal_fade_step`
        // drives through the app itself: render frames until the fade has
        // finished and its tail is silent, then only poll `drained`.
        if player.fade_finished() && !player.tail_sounding() {
            let _ = player.drained();
        } else {
            player.advance_frame();
        }
        player.drain_null_for_test(&mut frame);
        let audible = frame.iter().any(|&sample| sample != 0.0);
        if past_terminal {
            audible_after_terminal |= audible;
        } else {
            audible_through_terminal |= audible;
        }
        past_terminal |= player.fade_finished();
    }

    assert!(
        past_terminal,
        "sanity: the fade must finish within {FRAMES} frames"
    );
    assert!(
        audible_through_terminal,
        "sanity: the song must actually have been sounding through the fade"
    );
    assert!(
        audible_after_terminal,
        "the reverb ring's delayed samples must keep sounding after the terminal fade step, \
         not be truncated when the fade finishes"
    );
    assert!(
        !player.tail_sounding(),
        "the tail must ring down within {FRAMES} frames, so the paused player can be dropped"
    );
}
