//! Pins the fixtures this suite's seams share across files: ring-draining
//! and a sustained song for the playback and PSG-fade seams, and the
//! synthetic-pack byte-layout helpers for the packed-song conversion seam.

use std::sync::Arc;

use assets::{
    AssetPack, DirectSoundMode, DirectSoundSample, DirectSoundVoice, Envelope, ProgrammableWave,
    ProgrammableWaveVoice, Sample, SampleId, SongEvent, Square1Voice, Square2Voice, VoiceEntry,
    VoiceGroup, VoiceGroupId,
};
use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};
use platform::AudioOutput;

use super::super::{MusicPlayer, RING_CAPACITY_FRAMES};

pub(super) const RING_CAPACITY_SAMPLES: usize =
    RING_CAPACITY_FRAMES * (AudioOutput::CHANNELS as usize);

pub(super) fn drain_everything(player: &mut MusicPlayer) {
    let queued = RING_CAPACITY_SAMPLES - player.ring_free_for_test();
    let mut sink = vec![0.0_f32; queued];
    player.drain_null_for_test(&mut sink);
}

/// `tracks` copies of one voice sustaining the same looping note. `Wait`
/// must outlast every test using this song, or `Goto` restarts the untied
/// note early, doubling the voice. Shared by the playback and PSG-fade
/// seams, whose fade-schedule tests both hold a note through a fade.
pub(super) fn sustained_song(tracks: usize) -> Song {
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

pub(super) struct TempPackGuard {
    path: std::path::PathBuf,
}

impl TempPackGuard {
    fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }

    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempPackGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) fn write_pack(test_name: &str, entries: &[(&str, Vec<u8>)]) -> TempPackGuard {
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

pub(super) fn flat_envelope() -> Envelope {
    Envelope {
        attack: 255,
        decay: 0,
        sustain: 255,
        release: 0,
    }
}

/// `CgbAdsr::flat()` as a pack envelope; [`flat_envelope`]'s
/// out-of-domain values are valid only for `DirectSound`'s unmasked envelope.
pub(super) fn flat_cgb_envelope() -> Envelope {
    Envelope {
        attack: 0,
        decay: 0,
        sustain: 15,
        release: 0,
    }
}

pub(super) fn fixed_rate_voicegroup(wave_id: &str) -> VoiceGroup {
    VoiceGroup::new(vec![
        VoiceEntry::Square1(Square1Voice {
            base_key: 60,
            length: 0,
            sweep: 0,
            duty: 2,
            envelope: flat_cgb_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::Square2(Square2Voice {
            base_key: 60,
            length: 0,
            duty: 2,
            envelope: flat_cgb_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
            base_key: 60,
            length: 0,
            wave: SampleId(wave_id.to_owned()),
            envelope: flat_cgb_envelope(),
            fixed_rate: true,
        }),
        VoiceEntry::Square1(Square1Voice {
            base_key: 60,
            length: 0,
            sweep: 0,
            duty: 2,
            envelope: flat_cgb_envelope(),
            fixed_rate: false,
        }),
    ])
    .expect("four slots is well under VOICE_SLOT_COUNT")
}

// The returned guard outlives the pack in the caller's scope so the
// scratch file is removed on every exit path, panics included.
pub(super) fn pack_with_song(test_name: &str, reverb: Option<u8>) -> (AssetPack, TempPackGuard) {
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

pub(super) fn direct_sound_voicegroup(wave_id: &str) -> VoiceGroup {
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
pub(super) fn pack_with_direct_sound_sample(
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

// The returned guard outlives the pack in the caller's scope so the
// scratch file is removed on every exit path, panics included.
pub(super) fn pack_with_priority(test_name: &str, priority: u8) -> (AssetPack, TempPackGuard) {
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

pub(super) fn occupant_track() -> Vec<SongEvent> {
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
pub(super) fn evictor_track(key: u8) -> Vec<SongEvent> {
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
