//! Pins packed-song conversion (`load_song_from_pack`): CGB fixed-rate
//! tags, `DirectSound` logical-length narrowing, header priority and reverb,
//! key-split eviction, and, against the real extracted pack, `mus_title`'s
//! reverb level and continuous playback.

use assets::{
    AssetPack, DirectSoundMode, DirectSoundSample, DirectSoundVoice, KeySplitVoice, Sample,
    SampleId, VoiceEntry, VoiceGroup, VoiceGroupId,
};
use audio::{Instrument, Sequencer, DEFAULT_MASTER_VOLUME};

use super::super::load_song_from_pack;
use super::shared::{
    evictor_track, flat_envelope, occupant_track, pack_with_direct_sound_sample,
    pack_with_priority, pack_with_song, write_pack,
};

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

#[test]
fn loading_carries_the_header_priority_into_the_runtime_song() {
    let (plain_pack, _plain_guard) = pack_with_priority("prio-zero", 0);
    let plain = load_song_from_pack(&plain_pack, "fixtest").expect("the synthetic song loads");
    assert_eq!(plain.priority(), 0);

    let (raised_pack, _raised_guard) = pack_with_priority("prio-200", 200);
    let raised = load_song_from_pack(&raised_pack, "fixtest").expect("the synthetic song loads");
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

/// The real-pack counterpart to the synthetic conversion tests above: proves
/// `load_song_from_pack` also resolves the actual extracted `mus_title` song
/// with its real header reverb level, and that the converted song plays
/// continuously via its own jump commands.
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

/// `mus_title.mid` plays key 33 on program 48, the strings key split, with
/// velocity 112 and a gate of 8 ticks (channel 2, tick 2160). Upstream reads
/// the table 36 bytes back from its data, so key 33 lands on child 3: the
/// next linked record after strings' three, trumpet's first
/// (`asm/macros/m4a.inc:25-32`, `sound/keysplit_tables.inc:13-22`,
/// `sound/voicegroups/keysplits/strings.inc:1-4`, `sound/voice_groups.inc:11-12`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn mus_titles_key_33_strings_note_resolves_to_the_trumpet_voice() {
    const STRINGS_PROGRAM: u8 = 48;
    const PLAYED_KEY: u8 = 33;
    const PLAYED_VELOCITY: u8 = 112;
    const PLAYED_GATE: u8 = 8;
    const EXPECTED_CHILD: u8 = 3;

    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let song = load_song_from_pack(&pack, "mus_title").expect("mus_title must resolve cleanly");

    let played_by_strings = song.tracks().iter().any(|track| {
        let mut program = None;
        track.iter().any(|event| match *event {
            audio::Event::Voice(selected) => {
                program = Some(selected);
                false
            }
            audio::Event::Note {
                key,
                velocity,
                gate,
            } => {
                program == Some(STRINGS_PROGRAM)
                    && (key, velocity, gate) == (PLAYED_KEY, PLAYED_VELOCITY, PLAYED_GATE)
            }
            _ => false,
        })
    });
    assert!(
        played_by_strings,
        "mus_title must still play the key-33 strings note"
    );

    let Some(Instrument::KeySplit(split)) = song.voice(usize::from(STRINGS_PROGRAM)) else {
        panic!("program {STRINGS_PROGRAM} must convert to a key split");
    };
    assert_eq!(split.table[usize::from(PLAYED_KEY)], EXPECTED_CHILD);
    assert!(
        matches!(
            split.children.get(usize::from(EXPECTED_CHILD)),
            Some(Some(Instrument::DirectSound(_)))
        ),
        "key 33 must resolve to a playable voice, not silence"
    );
}
