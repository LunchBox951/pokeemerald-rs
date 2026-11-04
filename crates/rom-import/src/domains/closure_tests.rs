//! Several songs and everything they play through, over one synthetic ROM.
//!
//! `title` plays through the group `title` and its own sample on music player
//! 0. `other` plays through the group `other` on player 3: a sample, a rhythm
//! child and a key-split child, both the group `drums`, which plays a third
//! sample. Every test that breaks something breaks the second song's side, so a
//! domain that stopped after its first root would pass.

use std::collections::BTreeMap;

use assets::{MusicPlayerId, Sample, Song, VoiceEntry, VoiceGroup};
use pack_format::{parse_directory, EntryKind, PackWriter};

use super::{audio, song};
use crate::error::{ImportError, SongFault};
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{AudioRoots, KeysplitRoot, Roots, SampleRoot, SongRoot, VoicegroupRoot};

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

const TONE: u32 = 0x10_0000;
const TONE2: u32 = 0x11_0000;
const DRUM: u32 = 0x12_0000;
const TITLE_GROUP: u32 = 0x20_0000;
const OTHER_GROUP: u32 = 0x21_0000;
const DRUMS_GROUP: u32 = 0x22_0000;
const TABLE: u32 = 0x23_0000;
const TITLE_HEADER: u32 = 0x30_0000;
const OTHER_HEADER: u32 = 0x31_0000;
const TITLE_TRACK: u32 = 0x40_0000;
const OTHER_TRACK: u32 = 0x41_0000;
const SONG_TABLE: u32 = 0x50_0000;

const SAMPLE_LEN: u32 = 8;
/// A square-1 record: padding for a song group's undeclared slots.
const FILLER: [u8; 12] = [1, 60, 0, 0, 2, 0, 0, 0, 0, 0, 15, 0];

static DIRECT_SOUND: [SampleRoot; 3] = [
    sample("audio/sample/direct-sound/tone", TONE),
    sample("audio/sample/direct-sound/tone2", TONE2),
    sample("audio/sample/direct-sound/drum", DRUM),
];
static VOICEGROUPS: [VoicegroupRoot; 3] = [
    group("audio/voicegroup/title", TITLE_GROUP, 1),
    group("audio/voicegroup/other", OTHER_GROUP, 3),
    group("audio/voicegroup/drums", DRUMS_GROUP, 1),
];
static KEYSPLITS: [KeysplitRoot; 1] = [KeysplitRoot {
    label: "halves",
    addr: at(TABLE),
    starting_note: 0,
    len: 8,
}];
static SONGS: [SongRoot; 2] = [
    SongRoot {
        id: "audio/song/title",
        index: 1,
        header: at(TITLE_HEADER),
        track_count: 1,
        voicegroup: at(TITLE_GROUP),
    },
    SongRoot {
        id: "audio/song/other",
        index: 2,
        header: at(OTHER_HEADER),
        track_count: 1,
        voicegroup: at(OTHER_GROUP),
    },
];

const fn sample(id: &'static str, addr: u32) -> SampleRoot {
    SampleRoot {
        id,
        addr: at(addr),
        header_len: 16,
        data_len: SAMPLE_LEN,
    }
}

const fn group(id: &'static str, addr: u32, declared: u16) -> VoicegroupRoot {
    VoicegroupRoot {
        id,
        label: "g",
        addr: at(addr),
        starting_note: 0,
        declared_slots: declared,
        addressable_slots: 128,
    }
}

fn audio_roots(
    songs: &'static [SongRoot],
    voicegroups: &'static [VoicegroupRoot],
    keysplits: &'static [KeysplitRoot],
    direct_sound: &'static [SampleRoot],
) -> AudioRoots {
    AudioRoots {
        song_table: at(SONG_TABLE),
        songs,
        voicegroups,
        keysplits,
        direct_sound,
        programmable_wave: &[],
        cry: None,
    }
}

fn all_roots() -> AudioRoots {
    audio_roots(&SONGS, &VOICEGROUPS, &KEYSPLITS, &DIRECT_SOUND)
}

fn wave(pcm: u8) -> Vec<u8> {
    let mut out = vec![0u8; 4];
    out.extend(0x2000u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(SAMPLE_LEN.to_le_bytes());
    // The samples' data, then the interpolation-guard byte.
    out.extend(std::iter::repeat_n(
        pcm,
        usize::try_from(SAMPLE_LEN).unwrap(),
    ));
    out.push(0x2A);
    out
}

fn direct(sample: u32) -> Vec<u8> {
    let mut slot = vec![0x00, 60, 0, 0];
    slot.extend(at(sample).raw().to_le_bytes());
    slot.extend([255, 0, 255, 165]);
    slot
}

fn indirect(kind: u8, child: u32, table: u32) -> Vec<u8> {
    let mut slot = vec![kind, 0, 0, 0];
    slot.extend(at(child).raw().to_le_bytes());
    slot.extend(if table == 0 { 0 } else { at(table).raw() }.to_le_bytes());
    slot
}

fn header(priority: u8, group: u32, track: u32) -> Vec<u8> {
    let mut bytes = vec![1, 0, priority, 0];
    bytes.extend(at(group).raw().to_le_bytes());
    bytes.extend(at(track).raw().to_le_bytes());
    bytes
}

fn entry(header: u32, ms: u8) -> Vec<u8> {
    let mut bytes = at(header).raw().to_le_bytes().to_vec();
    bytes.extend([ms, 0, 9, 0]);
    bytes
}

fn fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(TONE as usize, &wave(1))
        .write(TONE2 as usize, &wave(2))
        .write(DRUM as usize, &wave(3))
        .write(TITLE_GROUP as usize, &direct(TONE))
        .write(TITLE_GROUP as usize + 12, &FILLER.repeat(127))
        .write(OTHER_GROUP as usize, &direct(TONE2))
        .write(OTHER_GROUP as usize + 12, &indirect(0x80, DRUMS_GROUP, 0))
        .write(
            OTHER_GROUP as usize + 24,
            &indirect(0x40, DRUMS_GROUP, TABLE),
        )
        .write(OTHER_GROUP as usize + 36, &FILLER.repeat(125))
        .write(DRUMS_GROUP as usize, &direct(DRUM))
        .write(TABLE as usize, &[0, 0, 0, 0, 1, 1, 1, 1])
        .write(TITLE_HEADER as usize, &header(0, TITLE_GROUP, TITLE_TRACK))
        .write(OTHER_HEADER as usize, &header(3, OTHER_GROUP, OTHER_TRACK))
        .write(TITLE_TRACK as usize, &[0x81, 0xB1])
        .write(OTHER_TRACK as usize, &[0x82, 0xB1])
        .write(SONG_TABLE as usize + 8, &entry(TITLE_HEADER, 0))
        .write(SONG_TABLE as usize + 16, &entry(OTHER_HEADER, 3))
}

/// Runs the audio domain, then the song domain, as [`crate::import`] does.
fn import(rom: &Rom, audio: AudioRoots) -> Result<Vec<u8>, ImportError> {
    let roots = Roots {
        audio,
        ..Roots::NONE
    };
    let mut writer = PackWriter::new();
    audio::write(rom, &roots, &mut writer)?;
    song::write(rom, &roots, &mut writer)?;
    Ok(writer.finish().expect("the pack serializes"))
}

fn rom(fixture: RomFixture) -> Rom {
    Rom::from_bytes(fixture.finish()).expect("the fixture header is valid")
}

fn payloads(pack: &[u8]) -> BTreeMap<String, Vec<u8>> {
    parse_directory(pack)
        .expect("the pack parses")
        .into_iter()
        .map(|entry| {
            assert_eq!(entry.kind, EntryKind::Raw, "{}", entry.id);
            (
                entry.id,
                pack[entry.offset..entry.offset + entry.length].to_vec(),
            )
        })
        .collect()
}

#[test]
fn every_root_of_every_song_is_written() {
    let pack = import(&rom(fixture()), all_roots()).expect("a well-formed profile");
    let ids: Vec<String> = payloads(&pack).into_keys().collect();
    assert_eq!(
        ids,
        [
            "audio/sample/direct-sound/drum",
            "audio/sample/direct-sound/tone",
            "audio/sample/direct-sound/tone2",
            "audio/song/other",
            "audio/song/title",
            "audio/voicegroup/drums",
            "audio/voicegroup/other",
            "audio/voicegroup/title",
        ]
    );
}

#[test]
fn each_song_keeps_its_own_player_and_voicegroup() {
    let pack = import(&rom(fixture()), all_roots()).expect("a well-formed profile");
    let pack = payloads(&pack);
    let title = Song::decode(&pack["audio/song/title"]).unwrap();
    let other = Song::decode(&pack["audio/song/other"]).unwrap();
    assert_eq!(title.player(), MusicPlayerId(0));
    assert_eq!(other.player(), MusicPlayerId(3));
    assert_eq!(title.voicegroup().0, "audio/voicegroup/title");
    assert_eq!(other.voicegroup().0, "audio/voicegroup/other");
    assert_eq!(other.priority(), 3);
}

#[test]
fn every_edge_of_the_written_pack_lands_on_a_written_entry() {
    let pack = import(&rom(fixture()), all_roots()).expect("a well-formed profile");
    let pack = payloads(&pack);
    let mut edges = 0;
    for (id, payload) in &pack {
        let mut targets = Vec::new();
        if id.starts_with("audio/song/") {
            targets.push(Song::decode(payload).unwrap().voicegroup().0.clone());
        } else if id.starts_with("audio/voicegroup/") {
            for voice in VoiceGroup::decode(payload).unwrap().slots() {
                match voice {
                    VoiceEntry::DirectSound(v) => targets.push(v.sample.0.clone()),
                    VoiceEntry::KeySplit(v) => targets.push(v.children.0.clone()),
                    VoiceEntry::Rhythm(v) => targets.push(v.children.0.clone()),
                    _ => {}
                }
            }
        } else {
            Sample::decode(payload).unwrap();
        }
        for target in targets {
            assert!(pack.contains_key(&target), "{id} names {target}");
            edges += 1;
        }
    }
    // Two songs, three samples, one rhythm and one key-split child.
    assert_eq!(edges, 2 + 3 + 2);
}

#[test]
fn a_second_songs_table_entry_that_names_another_header_is_refused() {
    let rom = rom(fixture().write(SONG_TABLE as usize + 16, &entry(TITLE_HEADER, 3)));
    let err = import(&rom, all_roots()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: "audio/song/other",
                field: "Song.header"
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_second_songs_track_count_that_disagrees_is_refused() {
    let rom = rom(fixture().write(OTHER_HEADER as usize, &[2]));
    let err = import(&rom, all_roots()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: "audio/song/other",
                field: "SongHeader.trackCount"
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_second_songs_tone_pointer_to_another_group_is_refused() {
    let rom = rom(fixture().write(
        OTHER_HEADER as usize + 4,
        &at(TITLE_GROUP).raw().to_le_bytes(),
    ));
    let err = import(&rom, all_roots()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: "audio/song/other",
                field: "SongHeader.tone"
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_song_whose_voicegroup_the_profile_does_not_record_is_refused() {
    static WITHOUT_OTHER: [VoicegroupRoot; 2] = [VOICEGROUPS[0], VOICEGROUPS[2]];
    let err = import(
        &rom(fixture()),
        audio_roots(&SONGS, &WITHOUT_OTHER, &KEYSPLITS, &DIRECT_SOUND),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::UnresolvedPointer {
                root: "audio/song/other",
                what: "a voicegroup",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_child_group_the_profile_does_not_record_is_refused() {
    static WITHOUT_DRUMS: [VoicegroupRoot; 2] = [VOICEGROUPS[0], VOICEGROUPS[1]];
    let err = import(
        &rom(fixture()),
        audio_roots(&SONGS, &WITHOUT_DRUMS, &KEYSPLITS, &DIRECT_SOUND),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::UnresolvedPointer {
                root: "audio/voicegroup/other",
                slot: 1,
                what: "a voicegroup",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_sample_the_profile_does_not_record_is_refused() {
    static WITHOUT_TONE2: [SampleRoot; 2] = [DIRECT_SOUND[0], DIRECT_SOUND[2]];
    let err = import(
        &rom(fixture()),
        audio_roots(&SONGS, &VOICEGROUPS, &KEYSPLITS, &WITHOUT_TONE2),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::UnresolvedPointer {
                root: "audio/voicegroup/other",
                slot: 0,
                what: "a DirectSound sample",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_key_split_table_the_profile_does_not_record_is_refused() {
    let err = import(
        &rom(fixture()),
        audio_roots(&SONGS, &VOICEGROUPS, &[], &DIRECT_SOUND),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::UnresolvedPointer {
                root: "audio/voicegroup/other",
                slot: 2,
                what: "a key-split table",
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_second_songs_malformed_track_is_refused() {
    // PORT, a named command this importer does not model.
    let rom = rom(fixture().write(OTHER_TRACK as usize, &[0xCC, 0xB1]));
    let err = import(&rom, all_roots()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::Song {
                id: "audio/song/other",
                track: 0,
                fault: SongFault::UnknownCommand(0xCC),
                ..
            }
        ),
        "{err:?}"
    );
}
