//! The audio fixtures and the tests of how [`audio::locate`] treats the pack's
//! sample and song entries and the supported-song requirement. Voicegroup
//! and song-bytecode tests build on these fixtures in `audio_bytecode`.

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

/// A synthetic `pokeemerald/` holding only what [`audio::locate`] reads: the
/// `voicegroup_title` source, an empty `keysplit_tables.inc`, and the
/// `MUS_TITLE` and `MUS_OTHER` song constants. `voicegroup_title.inc` declares
/// a single slot, matching the one [`audio_rom`] plants.
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

/// The interpolation-guard byte that follows a sample's logical PCM, as
/// `extract::wav`'s module docs describe. The locator's PCM search needle
/// includes it.
const AUDIO_GUARD_SAMPLE: u8 = 0x2A;

/// `pcm` followed by [`AUDIO_GUARD_SAMPLE`]: the buffer a pack sample payload
/// and the ROM both hold for one sample.
fn with_audio_guard(pcm: &[u8]) -> Vec<u8> {
    let mut out = pcm.to_vec();
    out.push(AUDIO_GUARD_SAMPLE);
    out
}

/// The pack payload of one non-looping `DirectSound` sample at `frequency`,
/// laid out as `assets::audio::Sample::encode` does, with `pcm` as its logical
/// samples followed by the guard byte.
fn audio_direct_sound_payload(frequency: u32, pcm: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8];
    payload.extend_from_slice(&frequency.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(&u32::try_from(pcm.len()).expect("small").to_le_bytes());
    payload.extend_from_slice(&with_audio_guard(pcm));
    payload
}

/// One 12-byte square-1 voice slot, matching what [`title_voicegroup_slots`]
/// expects at each slot past `voicegroup_title`'s one declared slot. Its
/// pointer word holds the duty, not an address, so it references no sample.
const AUDIO_FILLER_SLOT: [u8; 12] = [1, 60, 0, 0, 2, 0, 0, 0, 0, 0, 15, 0];

/// The 128 slots the importer decodes from [`audio_rom`]'s `voicegroup_title`:
/// the declared `DirectSound` voice, then 127 square-1 voices decoded from
/// [`AUDIO_FILLER_SLOT`].
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

/// The pack payload of the `audio/voicegroup/title` entry that matches
/// [`audio_rom`].
pub(super) fn title_voicegroup_payload() -> Vec<u8> {
    VoiceGroup::new(title_voicegroup_slots())
        .expect("128 slots are a valid voicegroup")
        .encode()
}

/// A ROM holding one sample (`pcm` and its guard byte), `voicegroup_title`'s
/// declared slot and 127 filler slots, a one-track song header that plays
/// through that voicegroup, and the `gSongTable` entry at `MUS_TITLE`'s index.
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
        // A song's header points at the group, so the importer reads all 128
        // slots, not just the declared one.
        .write(0x20_0000 + 12, &AUDIO_FILLER_SLOT.repeat(127))
        .write(0x30_0000, &song_header)
        // The track's bytecode: WAIT 1 tick (0x81), then FINE (0xB1).
        .write(0x40_0000, &[0x81, 0xB1])
        // gSongTable starts at 0x0850_0000, so entry 1 (`MUS_TITLE`) points at
        // the song header.
        .write(0x50_0008, &0x0830_0000u32.to_le_bytes())
        // `ms` is the BGM player (0), which the pack song names; `me` is 7 so
        // that only `ms` can agree with the pack.
        .write(0x50_000C, &[0, 0, 7, 0])
        .finish()
}

/// Runs `body` over [`audio_rom`] and [`audio_upstream`] with a pack that
/// matches them and holds every id in `songs` as an `audio/song/` entry.
fn with_audio_context<T>(name: &str, songs: &[&str], body: impl FnOnce(&Context<'_>) -> T) -> T {
    with_audio_song_payload(name, songs, &title_song_payload(0), body)
}

/// The pack payload of the song [`audio_rom`] holds, except for `priority`:
/// player 0, no reverb, and one track of WAIT 1 then FINE, playing through
/// `audio/voicegroup/title`. `audio_rom` has priority 0.
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

/// [`with_audio_context`] with `song_payload` as every pack song's payload.
fn with_audio_song_payload<T>(
    name: &str,
    songs: &[&str],
    song_payload: &[u8],
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    with_audio_song_rom(name, songs, song_payload, |_| {}, body)
}

/// [`with_audio_song_payload`] with `mutate` applied to the ROM image first, so
/// a test can corrupt one ROM field the pack still describes unchanged.
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

/// [`with_audio_song_rom`] with `voicegroup` as the payload of the pack's
/// `audio/voicegroup/title` entry (`None` leaves the entry out), so a test can
/// supply a voicegroup that differs from the ROM's.
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
/// test can give an entry another kind or add a group that no song reaches.
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

/// [`with_audio_pack_entry`] with `extra` entries added to the pack as given.
/// An id in `skip` (a `songs` entry or the `DirectSound` demo sample) is left
/// out of the default `Raw` entries, so `extra` can supply it as another kind.
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

/// Proves the fixture is sound, so the refusal tests below can only fail on
/// the one field each changes.
#[test]
fn the_audio_fixture_locates_its_one_song() {
    with_audio_context("audio-one-song", &["audio/song/mus_title"], |ctx| {
        let mut report = Vec::new();
        let plan = audio::locate(ctx, &mut report).expect("the one-song pack locates");
        assert_eq!(plan.songs.len(), 1, "exactly the supported song is planned");
        assert_eq!(plan.songs[0].id, "audio/song/mus_title");
        assert_eq!(plan.songs[0].header, 0x0830_0000);
        assert_eq!(plan.song_table, 0x0850_0000);
        assert_eq!(
            plan.voicegroups.len(),
            1,
            "only the title voicegroup is planned"
        );
    });
}

/// The id a [`GenRomProfileError::WrongPackEntryKind`] names, or `None` for any
/// other error.
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
    // Priority is the only field that differs from the ROM, so the refusal
    // can only come from checking the pack song against the header.
    with_audio_song_payload(
        "audio-song-mismatch",
        &["audio/song/mus_title"],
        &title_song_payload(5),
        |ctx| {
            let mut report = Vec::new();
            let err = audio::locate(ctx, &mut report)
                .expect_err("a pack song with another priority than the ROM header is refused");
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
fn songs_beside_mus_title_are_left_to_the_checkout_instead_of_refused() {
    // The checkout pack carries the whole checkpoint manifest, but the ROM
    // backend imports only `mus_title` until #1850, so the other songs are
    // neither planned nor reported, rather than refused.
    with_audio_context(
        "audio-two-songs",
        &["audio/song/mus_other", "audio/song/mus_title"],
        |ctx| {
            let mut report = Vec::new();
            let plan = audio::locate(ctx, &mut report)
                .expect("a pack holding mus_title among other songs locates");
            assert_eq!(plan.songs.len(), 1);
            assert_eq!(plan.songs[0].id, "audio/song/mus_title");
            assert!(
                report.iter().all(|line| line.id != "audio/song/mus_other"),
                "the other song is not claimed: {report:?}"
            );
        },
    );
}

#[test]
fn no_song_at_all_is_refused_by_the_same_title_check() {
    with_audio_context("audio-no-song", &[], |ctx| {
        let mut report = Vec::new();
        let err = audio::locate(ctx, &mut report)
            .expect_err("a pack with no audio/song/ entries lacks the supported song");
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/song/*"),
            "{err:?}"
        );
    });
}

#[test]
fn a_lone_unsupported_song_is_refused_by_the_title_check() {
    with_audio_context("audio-lone-other", &["audio/song/mus_other"], |ctx| {
        let mut report = Vec::new();
        let err = audio::locate(ctx, &mut report)
            .expect_err("a song count of one is not enough: the one song must be mus_title");
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
