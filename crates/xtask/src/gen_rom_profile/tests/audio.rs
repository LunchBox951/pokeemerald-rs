//! Audio fixture, pack-entry, song, voicegroup and bytecode regression tests.

use std::path::{Path, PathBuf};

use assets::audio::{
    DirectSoundMode, DirectSoundVoice, Envelope, SampleId, Square1Voice, VoiceEntry, VoiceGroup,
    VOICE_SLOT_COUNT,
};
use pack_format::PackWriter;
use rom_import::fixture::RomFixture;

use super::super::audio;
use super::super::error::GenRomProfileError;
use super::super::pack_source::PackSource;
use super::super::search::{Lz77Search, PointerIndex, RawSearch};
use super::super::Context;
use super::{image_twin, scratch, synthetic_bag_ids};

/// A synthetic `pokeemerald/` holding just the `sound/` files
/// [`audio::locate`] reads, plus the `MUS_*` constants a multi-song pack
/// would name. Not a real upstream checkout: `title.inc` declares one slot,
/// enough to anchor the run [`audio_rom`] plants.
fn audio_upstream(dir: &Path) -> PathBuf {
    let upstream = dir.join("upstream");
    std::fs::create_dir_all(upstream.join("sound/voicegroups")).expect("voicegroup dir");
    std::fs::create_dir_all(upstream.join("include/constants")).expect("constants dir");
    std::fs::write(
        upstream.join("sound/voicegroups/voicegroup_title.inc"),
        "voice_group title\n\tvoice_directsound 60, 0, DirectSoundWaveData_demo, 255, 0, 255, 165\n",
    )
    .expect("write voicegroup");
    std::fs::write(upstream.join("sound/keysplit_tables.inc"), "").expect("write keysplits");
    std::fs::write(
        upstream.join("include/constants/songs.h"),
        "#define MUS_TITLE 1\n#define MUS_OTHER 2\n",
    )
    .expect("write songs.h");
    upstream
}

/// A distinctive byte appended past `pcm`'s declared count: the retained
/// interpolation guard `extract::wav` always keeps (its module docs).
const AUDIO_GUARD_SAMPLE: u8 = 0x2A;

/// `pcm` with [`AUDIO_GUARD_SAMPLE`] appended, matching the buffer both the
/// checkout extractor and the ROM importer retain past the logical count.
fn with_audio_guard(pcm: &[u8]) -> Vec<u8> {
    let mut out = pcm.to_vec();
    out.push(AUDIO_GUARD_SAMPLE);
    out
}

/// The pack payload of one `DirectSound` sample, in the wire format
/// `extract::audio_samples` writes: kind, freq, loop flag, loop start,
/// logical sample count, PCM, and the retained guard byte.
fn audio_direct_sound_payload(frequency: u32, pcm: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8];
    payload.extend_from_slice(&frequency.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(&u32::try_from(pcm.len()).expect("small").to_le_bytes());
    payload.extend_from_slice(&with_audio_guard(pcm));
    payload
}

/// The 12 bytes of a square-1 slot with no pointer, padding the addressable
/// slots past `voicegroup_title`'s one declared slot.
const AUDIO_FILLER_SLOT: [u8; 12] = [1, 60, 0, 0, 2, 0, 0, 0, 0, 0, 15, 0];

/// The slots [`audio_rom`] holds for `voicegroup_title`, as the importer
/// reads them.
pub(super) fn title_voicegroup_slots() -> Vec<VoiceEntry> {
    let mut slots = vec![VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId("audio/sample/direct-sound/demo".to_owned()),
        envelope: Envelope {
            attack: 255,
            decay: 0,
            sustain: 255,
            release: 165,
        },
        mode: DirectSoundMode::Resampled,
    })];
    slots.extend((1..VOICE_SLOT_COUNT).map(|_| {
        VoiceEntry::Square1(Square1Voice {
            base_key: 60,
            length: 0,
            sweep: 0,
            duty: 2,
            envelope: Envelope {
                attack: 0,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            fixed_rate: false,
        })
    }));
    slots
}

/// The encoded pack voicegroup [`audio_rom`]'s title group yields.
pub(super) fn title_voicegroup_payload() -> Vec<u8> {
    VoiceGroup::new(title_voicegroup_slots())
        .expect("128 slots are a valid voicegroup")
        .encode()
}

/// A ROM holding one sample, `voicegroup_title`'s one slot, one song header
/// that plays through it, and the `gSongTable` entry at `MUS_TITLE`'s index.
fn audio_rom(pcm: &[u8]) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&0x2000u32.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&u32::try_from(pcm.len()).expect("small").to_le_bytes());

    let mut slot = vec![0x00, 60, 0, 0];
    slot.extend_from_slice(&0x0810_0000u32.to_le_bytes());
    slot.extend_from_slice(&[255, 0, 255, 165]);

    let mut song_header = vec![1u8, 0, 0, 0];
    song_header.extend_from_slice(&0x0820_0000u32.to_le_bytes());
    song_header.extend_from_slice(&0x0840_0000u32.to_le_bytes());

    RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &header)
        .write(0x10_0010, &with_audio_guard(pcm))
        .write(0x20_0000, &slot)
        // Title is song-selected, so every one of its 128 slots is read.
        .write(0x20_0000 + 12, &AUDIO_FILLER_SLOT.repeat(127))
        .write(0x30_0000, &song_header)
        // The one track: WAIT 1 tick (0x81), then FINE (0xB1).
        .write(0x40_0000, &[0x81, 0xB1])
        // gSongTable at 0x0850_0000; MUS_TITLE is index 1.
        .write(0x50_0008, &0x0830_0000u32.to_le_bytes())
        // `Song.ms` is BGM (0); `Song.me` is a distinct value, so only `ms` matches the pack.
        .write(0x50_000C, &[0, 0, 7, 0])
        .finish()
}

/// Run `body` against the [`audio_rom`]/[`audio_upstream`] fixture, with
/// `songs` as the pack's `audio/song/` ids.
fn with_audio_context<T>(name: &str, songs: &[&str], body: impl FnOnce(&Context<'_>) -> T) -> T {
    with_audio_song_payload(name, songs, &title_song_payload(0), body)
}

/// The encoded pack song [`audio_rom`]'s header describes: one track, the
/// given priority, no reverb, playing through `voicegroup_title`.
pub(super) fn title_song_payload(priority: u8) -> Vec<u8> {
    assets::audio::Song::new(
        assets::audio::VoiceGroupId("audio/voicegroup/title".to_owned()),
        assets::audio::MusicPlayerId(0),
        priority,
        None,
        vec![vec![
            assets::audio::SongEvent::Wait(1),
            assets::audio::SongEvent::Fine,
        ]],
    )
    .expect("a one-track song is valid")
    .encode()
}

/// [`with_audio_context`] with every pack song carrying `song_payload`.
fn with_audio_song_payload<T>(
    name: &str,
    songs: &[&str],
    song_payload: &[u8],
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    with_audio_song_rom(name, songs, song_payload, |_| {}, body)
}

/// [`with_audio_song_payload`] after `mutate` has edited the ROM image.
pub(super) fn with_audio_song_rom<T>(
    name: &str,
    songs: &[&str],
    song_payload: &[u8],
    mutate: impl FnOnce(&mut Vec<u8>),
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    let voicegroup = title_voicegroup_payload();
    with_audio_pack(name, songs, song_payload, Some(&voicegroup), mutate, body)
}

/// The deepest fixture: `voicegroup` is the payload of the pack's
/// `audio/voicegroup/title` entry, or `None` to leave the entry out.
pub(super) fn with_audio_pack<T>(
    name: &str,
    songs: &[&str],
    song_payload: &[u8],
    voicegroup: Option<&[u8]>,
    mutate: impl FnOnce(&mut Vec<u8>),
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    let entries = voicegroup
        .map(|payload| {
            pack_format::raw_entry("audio/voicegroup/title".to_owned(), payload.to_vec())
        })
        .into_iter()
        .collect();
    with_audio_pack_entry(name, songs, song_payload, entries, mutate, body)
}

/// [`with_audio_pack`] with the `audio/voicegroup/` entries given whole, so a
/// test can choose their kind and add groups the plan never reaches.
pub(super) fn with_audio_pack_entry<T>(
    name: &str,
    songs: &[&str],
    song_payload: &[u8],
    voicegroups: Vec<pack_format::PackEntry>,
    mutate: impl FnOnce(&mut Vec<u8>),
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    with_audio_pack_extra(
        name,
        songs,
        song_payload,
        voicegroups,
        Vec::new(),
        &[],
        mutate,
        body,
    )
}

/// [`with_audio_pack_entry`] with `extra` entries appended whole, and any
/// `songs` or sample id listed in `skip` left out of the Raw defaults so
/// `extra` can supply it under another kind.
#[expect(
    clippy::too_many_arguments,
    reason = "a flat fixture signature keeps each audio test one call"
)]
fn with_audio_pack_extra<T>(
    name: &str,
    songs: &[&str],
    song_payload: &[u8],
    voicegroups: Vec<pack_format::PackEntry>,
    extra: Vec<pack_format::PackEntry>,
    skip: &[&str],
    mutate: impl FnOnce(&mut Vec<u8>),
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    let pcm: Vec<u8> = (0..64u32)
        .map(|index| u8::try_from((index * 43 + 7) % 199).expect("modulo 199 fits in u8"))
        .collect();
    let mut rom = audio_rom(&pcm);
    mutate(&mut rom);
    let dir = scratch(name);
    let upstream = audio_upstream(&dir);

    let mut writer = PackWriter::new();
    if !skip.contains(&"audio/sample/direct-sound/demo") {
        writer.push(pack_format::raw_entry(
            "audio/sample/direct-sound/demo".to_owned(),
            audio_direct_sound_payload(0x2000, &pcm),
        ));
    }
    for song in songs.iter().filter(|song| !skip.contains(song)) {
        writer.push(pack_format::raw_entry(
            (*song).to_owned(),
            song_payload.to_vec(),
        ));
    }
    for entry in voicegroups.into_iter().chain(extra) {
        writer.push(entry);
    }
    let pack_path = dir.join("test.pack");
    std::fs::write(&pack_path, writer.finish().expect("pack")).expect("write pack");
    let pack = PackSource::load(&pack_path).expect("load pack");

    let ctx = Context {
        rom: &rom,
        pack: &pack,
        raw: RawSearch::new(&rom),
        lz77: Lz77Search::new(&rom),
        pointers: PointerIndex::build(&rom),
        upstream,
        bag_ids: synthetic_bag_ids(),
    };
    let out = body(&ctx);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn the_audio_fixture_locates_its_one_song() {
    // The sanity check the refusal test below leans on: the fixture itself
    // resolves the header, the table, and exactly one voicegroup when the
    // pack holds only the one song this locator supports.
    with_audio_context("audio-one-song", &["audio/song/mus_title"], |ctx| {
        let mut report = Vec::new();
        let plan = audio::locate(ctx, &mut report).expect("the one-song pack locates");
        assert_eq!(plan.songs.len(), 1);
        assert_eq!(plan.songs[0].id, "audio/song/mus_title");
        assert_eq!(plan.songs[0].header, 0x0830_0000);
        assert_eq!(plan.song_table, 0x0850_0000);
        assert_eq!(plan.voicegroups.len(), 1);
    });
}

fn wrong_kind_id(err: &GenRomProfileError) -> Option<&str> {
    match err {
        GenRomProfileError::WrongPackEntryKind { id, .. } => Some(id),
        _ => None,
    }
}

#[test]
fn a_direct_sound_entry_of_the_wrong_kind_is_refused() {
    let pcm: Vec<u8> = (0..64u32)
        .map(|index| u8::try_from((index * 43 + 7) % 199).expect("modulo 199 fits in u8"))
        .collect();
    let id = "audio/sample/direct-sound/demo";
    let err = with_audio_pack_extra(
        "audio-direct-sound-wrong-kind",
        &["audio/song/mus_title"],
        &title_song_payload(0),
        vec![pack_format::raw_entry(
            "audio/voicegroup/title".to_owned(),
            title_voicegroup_payload(),
        )],
        vec![image_twin(id, &audio_direct_sound_payload(0x2000, &pcm))],
        &[id],
        |_| {},
        |ctx| audio::locate(ctx, &mut Vec::new()),
    )
    .expect_err("an image-kind sample entry is refused");
    assert_eq!(wrong_kind_id(&err), Some(id), "{err:?}");
}

#[test]
fn a_programmable_wave_entry_of_the_wrong_kind_is_refused() {
    let id = "audio/sample/programmable-wave/00";
    let err = with_audio_pack_extra(
        "audio-programmable-wave-wrong-kind",
        &["audio/song/mus_title"],
        &title_song_payload(0),
        vec![pack_format::raw_entry(
            "audio/voicegroup/title".to_owned(),
            title_voicegroup_payload(),
        )],
        vec![image_twin(id, &[0u8; 16])],
        &[],
        |_| {},
        |ctx| audio::locate(ctx, &mut Vec::new()),
    )
    .expect_err("an image-kind programmable-wave entry is refused");
    assert_eq!(wrong_kind_id(&err), Some(id), "{err:?}");
}

#[test]
fn a_song_entry_of_the_wrong_kind_is_refused() {
    let id = "audio/song/mus_title";
    let err = with_audio_pack_extra(
        "audio-song-wrong-kind",
        &[id],
        &title_song_payload(0),
        vec![pack_format::raw_entry(
            "audio/voicegroup/title".to_owned(),
            title_voicegroup_payload(),
        )],
        vec![image_twin(id, &title_song_payload(0))],
        &[id],
        |_| {},
        |ctx| audio::locate(ctx, &mut Vec::new()),
    )
    .expect_err("an image-kind song entry is refused");
    assert_eq!(wrong_kind_id(&err), Some(id), "{err:?}");
}

#[test]
fn a_song_payload_that_disagrees_with_the_rom_header_is_refused() {
    // Same header, pointers, and sample as the passing fixture; only the
    // pack song's priority differs from the ROM's, so a refusal can only
    // come from validating the payload.
    with_audio_song_payload(
        "audio-song-mismatch",
        &["audio/song/mus_title"],
        &title_song_payload(5),
        |ctx| {
            let mut report = Vec::new();
            let err = audio::locate(ctx, &mut report)
                .expect_err("a song the ROM does not hold must be refused");
            let GenRomProfileError::StructMismatch { id, reason } = &err else {
                panic!("{err:?}");
            };
            assert_eq!(id, "audio/song/mus_title");
            assert!(reason.contains("priority"), "{reason}");
        },
    );
}

#[test]
fn a_malformed_song_payload_is_refused_as_an_entry_shape() {
    with_audio_song_payload(
        "audio-song-garbage",
        &["audio/song/mus_title"],
        &[1, 2, 3, 4],
        |ctx| {
            let mut report = Vec::new();
            let err = audio::locate(ctx, &mut report).expect_err("garbage is not a song");
            assert!(
                matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == "audio/song/mus_title"),
                "{err:?}"
            );
        },
    );
}

#[test]
fn a_second_song_is_refused_by_name_instead_of_mapped_through_mus_title() {
    // Pins the refusal naming both offending ids, before any root is
    // located.
    with_audio_context(
        "audio-two-songs",
        &["audio/song/mus_other", "audio/song/mus_title"],
        |ctx| {
            let mut report = Vec::new();
            let err = audio::locate(ctx, &mut report)
                .expect_err("a pack with more than the one supported song must be refused");
            let GenRomProfileError::StructMismatch { id, reason } = &err else {
                panic!("{err:?}");
            };
            assert_eq!(id, "audio/song/*");
            assert!(reason.contains("audio/song/mus_other"), "{reason}");
            assert!(reason.contains("audio/song/mus_title"), "{reason}");
            assert!(
                report.is_empty(),
                "refused before locating anything: {report:?}"
            );
        },
    );
}

#[test]
fn no_song_at_all_is_refused_by_the_same_singleton_check() {
    // Pins the same `StructMismatch` on an empty `audio/song/` id set.
    with_audio_context("audio-no-song", &[], |ctx| {
        let mut report = Vec::new();
        let err = audio::locate(ctx, &mut report)
            .expect_err("a pack with no audio/song/ entries must be refused");
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/song/*"),
            "{err:?}"
        );
    });
}

#[test]
fn a_lone_unsupported_song_is_refused_by_the_singleton_check() {
    // Pins the id half of the singleton check: one entry, but not mus_title.
    with_audio_context("audio-lone-other", &["audio/song/mus_other"], |ctx| {
        let mut report = Vec::new();
        let err = audio::locate(ctx, &mut report)
            .expect_err("a pack holding only an unsupported song must be refused");
        let GenRomProfileError::StructMismatch { id, reason } = &err else {
            panic!("{err:?}");
        };
        assert_eq!(id, "audio/song/*");
        assert!(reason.contains("audio/song/mus_other"), "{reason}");
        assert!(
            report.is_empty(),
            "refused before locating anything: {report:?}"
        );
    });
}
