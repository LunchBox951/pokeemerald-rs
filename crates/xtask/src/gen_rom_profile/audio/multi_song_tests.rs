//! Locating several songs at once, over a synthetic ROM.
//!
//! The world holds two songs on different music players. `title` plays
//! through the group `title` and its own sample. `other` plays through the
//! group `other`, whose slots are a sample stored twice in the ROM, a rhythm
//! child and a key-split child, both the group `drums`, which plays a third
//! sample. Only the second copy of `tone2` is pointed at.

use std::collections::BTreeMap;
use std::path::PathBuf;

use assets::audio::{
    DirectSoundMode, DirectSoundVoice, Envelope, KeySplitVoice, MusicPlayerId, RhythmVoice,
    SampleId, Song, SongEvent, Square1Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
    VOICE_SLOT_COUNT,
};
use pack_format::{raw_entry, PackWriter};
use rom_import::fixture::RomFixture;

use super::locate;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::pack_source::PackSource;
use crate::gen_rom_profile::plan::AudioPlan;
use crate::gen_rom_profile::search::{Lz77Search, PointerIndex, RawSearch};
use crate::gen_rom_profile::tests::{scratch, synthetic_bag_ids};
use crate::gen_rom_profile::Context;

const SAMPLE_TONE: &str = "audio/sample/direct-sound/tone";
const SAMPLE_TONE2: &str = "audio/sample/direct-sound/tone2";
const SAMPLE_DRUM: &str = "audio/sample/direct-sound/drum";

const TONE_AT: usize = 0x10_0000;
const TONE2_UNUSED_AT: usize = 0x11_0000;
const TONE2_AT: usize = 0x13_0000;
const DRUM_AT: usize = 0x14_0000;
const TITLE_GROUP_AT: usize = 0x20_0000;
const OTHER_GROUP_AT: usize = 0x21_0000;
const DRUMS_GROUP_AT: usize = 0x22_0000;
const TABLE_AT: usize = 0x23_0000;
const TITLE_HEADER_AT: usize = 0x30_0000;
const OTHER_HEADER_AT: usize = 0x31_0000;
const TITLE_TRACK_AT: usize = 0x40_0000;
const OTHER_TRACK_AT: usize = 0x41_0000;
const SONG_TABLE_AT: usize = 0x50_0000;

const BUS: u32 = 0x0800_0000;
const ENVELOPE: Envelope = Envelope {
    attack: 255,
    decay: 0,
    sustain: 255,
    release: 165,
};
/// The notes the key-split table maps, from note 0.
const SPLIT_TABLE: [u8; 8] = [0, 0, 0, 0, 1, 1, 1, 1];

fn bus(offset: usize) -> u32 {
    BUS + u32::try_from(offset).expect("a fixture offset fits in u32")
}

/// Sample `n`'s PCM, distinct per sample and distinctive in a fixture image.
fn pcm(n: u32) -> Vec<u8> {
    (0..64u32)
        .map(|i| u8::try_from((i * (31 + 2 * n) + 7 * n + 3) % 241).expect("fits in u8"))
        .collect()
}

fn guarded(pcm: &[u8]) -> Vec<u8> {
    let mut out = pcm.to_vec();
    out.push(0x2A);
    out
}

fn sample_payload(pcm: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8];
    payload.extend_from_slice(&0x2000u32.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(&u32::try_from(pcm.len()).expect("small").to_le_bytes());
    payload.extend_from_slice(&guarded(pcm));
    payload
}

/// The 16-byte `WaveData` header of a sample of `pcm`.
fn wave_header(pcm: &[u8]) -> Vec<u8> {
    let mut header = vec![0u8; 4];
    header.extend_from_slice(&0x2000u32.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&u32::try_from(pcm.len()).expect("small").to_le_bytes());
    header
}

fn plant_sample(rom: RomFixture, at: usize, pcm: &[u8]) -> RomFixture {
    rom.write(at, &wave_header(pcm))
        .write(at + 16, &guarded(pcm))
}

/// A `voice_directsound` slot's 12 bytes.
fn direct_slot(sample_header: usize) -> Vec<u8> {
    let mut slot = vec![0x00, 60, 0, 0];
    slot.extend_from_slice(&bus(sample_header).to_le_bytes());
    slot.extend_from_slice(&[255, 0, 255, 165]);
    slot
}

/// A rhythm or key-split slot's 12 bytes.
fn indirect_slot(kind: u8, child: usize, table: usize) -> Vec<u8> {
    let mut slot = vec![kind, 0, 0, 0];
    slot.extend_from_slice(&bus(child).to_le_bytes());
    slot.extend_from_slice(&bus(table).to_le_bytes());
    slot
}

/// The 12 bytes padding a song-selected group's unused addressable slots.
const FILLER: [u8; 12] = [1, 60, 0, 0, 2, 0, 0, 0, 0, 0, 15, 0];

fn filler_voice() -> VoiceEntry {
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
}

fn direct_voice(sample: &str) -> VoiceEntry {
    VoiceEntry::DirectSound(DirectSoundVoice {
        base_key: 60,
        pan: None,
        sample: SampleId(sample.to_owned()),
        envelope: ENVELOPE,
        mode: DirectSoundMode::Resampled,
    })
}

/// A voicegroup of `leading`, then Empty or filler slots to 128.
fn group_payload(leading: Vec<VoiceEntry>, pad: Option<VoiceEntry>) -> Vec<u8> {
    let mut slots = leading;
    slots.resize(VOICE_SLOT_COUNT, pad.unwrap_or(VoiceEntry::Empty));
    VoiceGroup::new(slots).expect("a valid group").encode()
}

fn song_payload(group: &str, player: u16, priority: u8, reverb: Option<u8>, wait: u8) -> Vec<u8> {
    Song::new(
        VoiceGroupId(format!("audio/voicegroup/{group}")),
        MusicPlayerId(player),
        priority,
        reverb,
        vec![vec![SongEvent::Wait(wait), SongEvent::Fine]],
    )
    .expect("a one-track song is valid")
    .encode()
}

fn song_header(priority: u8, reverb: u8, group: usize, track: usize) -> Vec<u8> {
    let mut header = vec![1u8, 0, priority, reverb];
    header.extend_from_slice(&bus(group).to_le_bytes());
    header.extend_from_slice(&bus(track).to_le_bytes());
    header
}

fn table_entry(header: usize, ms: u8, me: u8) -> Vec<u8> {
    let mut entry = bus(header).to_le_bytes().to_vec();
    entry.extend_from_slice(&[ms, 0, me, 0]);
    entry
}

/// Everything a test may edit before the fixture is built.
struct World {
    rom: Vec<u8>,
    pack: BTreeMap<String, Vec<u8>>,
    songs_h: String,
}

impl World {
    fn new() -> Self {
        let mut rom = RomFixture::new().emerald_header();
        rom = plant_sample(rom, TONE_AT, &pcm(1));
        rom = plant_sample(rom, TONE2_UNUSED_AT, &pcm(2));
        rom = plant_sample(rom, TONE2_AT, &pcm(2));
        rom = plant_sample(rom, DRUM_AT, &pcm(3));
        let rom = rom
            .write(TITLE_GROUP_AT, &direct_slot(TONE_AT))
            .write(TITLE_GROUP_AT + 12, &FILLER.repeat(127))
            .write(OTHER_GROUP_AT, &direct_slot(TONE2_AT))
            .write(OTHER_GROUP_AT + 12, &indirect_slot(0x80, DRUMS_GROUP_AT, 0))
            .write(
                OTHER_GROUP_AT + 24,
                &indirect_slot(0x40, DRUMS_GROUP_AT, TABLE_AT),
            )
            .write(OTHER_GROUP_AT + 36, &FILLER.repeat(125))
            .write(DRUMS_GROUP_AT, &direct_slot(DRUM_AT))
            .write(TABLE_AT, &SPLIT_TABLE)
            .write(
                TITLE_HEADER_AT,
                &song_header(0, 0, TITLE_GROUP_AT, TITLE_TRACK_AT),
            )
            .write(
                OTHER_HEADER_AT,
                &song_header(3, 0x8A, OTHER_GROUP_AT, OTHER_TRACK_AT),
            )
            .write(TITLE_TRACK_AT, &[0x81, 0xB1])
            .write(OTHER_TRACK_AT, &[0x82, 0xB1])
            .write(SONG_TABLE_AT + 8, &table_entry(TITLE_HEADER_AT, 0, 7))
            .write(SONG_TABLE_AT + 16, &table_entry(OTHER_HEADER_AT, 3, 9))
            .finish();

        let mut pack = BTreeMap::new();
        pack.insert(SAMPLE_TONE.to_owned(), sample_payload(&pcm(1)));
        pack.insert(SAMPLE_TONE2.to_owned(), sample_payload(&pcm(2)));
        pack.insert(SAMPLE_DRUM.to_owned(), sample_payload(&pcm(3)));
        pack.insert(
            "audio/voicegroup/title".to_owned(),
            group_payload(vec![direct_voice(SAMPLE_TONE)], Some(filler_voice())),
        );
        let children = VoiceGroupId("audio/voicegroup/drums".to_owned());
        pack.insert(
            "audio/voicegroup/other".to_owned(),
            group_payload(
                vec![
                    direct_voice(SAMPLE_TONE2),
                    VoiceEntry::Rhythm(RhythmVoice {
                        children: children.clone(),
                    }),
                    VoiceEntry::KeySplit(
                        KeySplitVoice::new(0, SPLIT_TABLE.to_vec(), children)
                            .expect("a valid table"),
                    ),
                ],
                Some(filler_voice()),
            ),
        );
        pack.insert(
            "audio/voicegroup/drums".to_owned(),
            group_payload(vec![direct_voice(SAMPLE_DRUM)], None),
        );
        pack.insert(
            "audio/song/mus_title".to_owned(),
            song_payload("title", 0, 0, None, 1),
        );
        pack.insert(
            "audio/song/mus_other".to_owned(),
            song_payload("other", 3, 3, Some(10), 2),
        );
        Self {
            rom,
            pack,
            songs_h: "#define MUS_TITLE 1\n#define MUS_OTHER 2\n".to_owned(),
        }
    }

    fn write(&mut self, at: usize, bytes: &[u8]) {
        self.rom[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// Locates against this world, with `name` naming the scratch directory.
    fn locate(&self, name: &str) -> Result<AudioPlan, GenRomProfileError> {
        let dir = scratch(name);
        let upstream = write_upstream(&dir, &self.songs_h);
        let mut writer = PackWriter::new();
        for (id, payload) in &self.pack {
            writer.push(raw_entry(id.clone(), payload.clone()));
        }
        let pack_path = dir.join("test.pack");
        std::fs::write(&pack_path, writer.finish().expect("pack")).expect("write pack");
        let pack = PackSource::load(&pack_path).expect("load pack");
        let ctx = Context {
            rom: &self.rom,
            pack: &pack,
            raw: RawSearch::new(&self.rom),
            lz77: Lz77Search::new(&self.rom),
            pointers: PointerIndex::build(&self.rom),
            upstream,
            bag_ids: synthetic_bag_ids(),
        };
        let out = locate(&ctx, &mut Vec::new());
        let _ = std::fs::remove_dir_all(&dir);
        out
    }
}

/// The upstream `sound/` files the world reads.
fn write_upstream(dir: &std::path::Path, songs_h: &str) -> PathBuf {
    let upstream = dir.join("upstream");
    let groups = upstream.join("sound/voicegroups");
    std::fs::create_dir_all(&groups).expect("voicegroup dir");
    std::fs::create_dir_all(upstream.join("include/constants")).expect("constants dir");
    let files = [
        (
            "voicegroup_title.inc",
            "voice_group title\n\tvoice_directsound 60, 0, DirectSoundWaveData_tone, 255, 0, 255, 165\n",
        ),
        (
            "voicegroup_other.inc",
            "voice_group other\n\
             \tvoice_directsound 60, 0, DirectSoundWaveData_tone2, 255, 0, 255, 165\n\
             \tvoice_keysplit_all voicegroup_drums\n\
             \tvoice_keysplit voicegroup_drums, keysplit_halves\n",
        ),
        (
            "voicegroup_drums.inc",
            "voice_group drums\n\tvoice_directsound 60, 0, DirectSoundWaveData_drum, 255, 0, 255, 165\n",
        ),
    ];
    for (name, text) in files {
        std::fs::write(groups.join(name), text).expect("write voicegroup");
    }
    std::fs::write(
        upstream.join("sound/keysplit_tables.inc"),
        "keysplit halves, 0\n\tsplit 0, 4\n\tsplit 1, 8\n",
    )
    .expect("write keysplits");
    std::fs::write(upstream.join("include/constants/songs.h"), songs_h).expect("write songs.h");
    upstream
}

fn plan(name: &str) -> AudioPlan {
    World::new().locate(name).expect("the world locates")
}

#[test]
fn every_song_and_what_it_plays_through_is_located() {
    let plan = plan("multi-locates");
    assert_eq!(plan.song_table, bus(SONG_TABLE_AT));

    let songs: Vec<(&str, u16, u32, u32)> = plan
        .songs
        .iter()
        .map(|song| (song.id.as_str(), song.index, song.header, song.voicegroup))
        .collect();
    assert_eq!(
        songs,
        [
            (
                "audio/song/mus_other",
                2,
                bus(OTHER_HEADER_AT),
                bus(OTHER_GROUP_AT)
            ),
            (
                "audio/song/mus_title",
                1,
                bus(TITLE_HEADER_AT),
                bus(TITLE_GROUP_AT)
            ),
        ]
    );
    let groups: Vec<(&str, u32)> = plan
        .voicegroups
        .iter()
        .map(|group| (group.label.as_str(), group.addr))
        .collect();
    assert_eq!(
        groups,
        [
            ("drums", bus(DRUMS_GROUP_AT)),
            ("other", bus(OTHER_GROUP_AT)),
            ("title", bus(TITLE_GROUP_AT)),
        ]
    );
    assert_eq!(plan.keysplits.len(), 1);
    assert_eq!(plan.keysplits[0].label, "halves");
    assert_eq!(plan.keysplits[0].addr, bus(TABLE_AT));
    assert_eq!(plan.keysplits[0].len, 8);
}

#[test]
fn a_sample_stored_twice_is_the_copy_a_voicegroup_slot_points_at() {
    // The unreferenced copy sits at the lower address, so a first-match
    // choice would take it.
    let plan = plan("multi-duplicate-sample");
    let tone2 = plan
        .direct_sound
        .iter()
        .find(|sample| sample.id == SAMPLE_TONE2)
        .expect("tone2 is planned");
    assert_eq!(tone2.addr, bus(TONE2_AT));
    assert_ne!(tone2.addr, bus(TONE2_UNUSED_AT));
    assert_eq!(plan.direct_sound.len(), 3);
}

#[test]
fn slots_pointing_at_two_copies_of_one_sample_are_refused() {
    let mut world = World::new();
    // The `drums` slot now plays the other copy of `tone2` too.
    world.write(DRUMS_GROUP_AT, &direct_slot(TONE2_UNUSED_AT));
    world.pack.insert(
        "audio/voicegroup/drums".to_owned(),
        group_payload(vec![direct_voice(SAMPLE_TONE2)], None),
    );
    world.pack.remove(SAMPLE_DRUM);
    let err = world.locate("multi-two-copies").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, reason }
            if id == SAMPLE_TONE2 && reason.contains("two copies")),
        "{err:?}"
    );
}

#[test]
fn a_sample_no_voicegroup_plays_is_refused() {
    let mut world = World::new();
    world.pack.insert(
        "audio/sample/direct-sound/orphan".to_owned(),
        sample_payload(&pcm(9)),
    );
    world.write(0x15_0000, &wave_header(&pcm(9)));
    world.write(0x15_0010, &guarded(&pcm(9)));
    let err = world.locate("multi-orphan-sample").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. }
            if id == "audio/sample/direct-sound/orphan"),
        "{err:?}"
    );
}

#[test]
fn a_song_whose_table_entry_names_another_music_player_is_refused() {
    let mut world = World::new();
    // The second song's `ms` is 3 in the pack; the ROM says 2.
    world.write(SONG_TABLE_AT + 16 + 4, &[2]);
    let err = world.locate("multi-player").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, reason }
            if id == "audio/song/mus_other" && reason.contains("ms")),
        "{err:?}"
    );
}

#[test]
fn a_song_whose_table_entry_is_empty_is_refused() {
    let mut world = World::new();
    world.write(SONG_TABLE_AT + 16, &[0; 8]);
    let err = world.locate("multi-empty-entry").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/song/mus_other"),
        "{err:?}"
    );
}

#[test]
fn a_song_header_naming_another_voicegroup_than_the_pack_is_refused() {
    let mut world = World::new();
    // The pack's `other` plays through `title`, but the header points at `other`.
    world.pack.insert(
        "audio/song/mus_other".to_owned(),
        song_payload("title", 3, 3, Some(10), 2),
    );
    let err = world.locate("multi-wrong-voicegroup").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, reason }
            if id == "audio/song/mus_title" && reason.contains("earlier song")),
        "{err:?}"
    );
}

#[test]
fn two_songs_sharing_one_table_entry_are_refused() {
    let mut world = World::new();
    // `MUS_OTHER` now names the title's entry, whose header is the title's.
    world.songs_h = "#define MUS_TITLE 1\n#define MUS_OTHER 1\n".to_owned();
    let err = world.locate("multi-same-index").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/song/mus_other"),
        "{err:?}"
    );
}

#[test]
fn a_rhythm_slot_that_is_not_a_rhythm_record_is_refused() {
    let mut world = World::new();
    world.write(OTHER_GROUP_AT + 12, &[0x00]);
    let err = world.locate("multi-slot-type").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, reason }
            if id == "audio/voicegroup/other" && reason.contains("slot 1")),
        "{err:?}"
    );
}

#[test]
fn a_key_split_table_the_source_does_not_declare_is_refused() {
    let mut world = World::new();
    world.write(TABLE_AT, &[1, 1, 1, 1, 0, 0, 0, 0]);
    let err = world.locate("multi-unknown-table").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { reason, .. }
            if reason.contains("a source key-split table")),
        "{err:?}"
    );
}

#[test]
fn a_voicegroup_the_pack_holds_but_no_song_reaches_is_refused() {
    let mut world = World::new();
    world.pack.insert(
        "audio/voicegroup/stale".to_owned(),
        group_payload(vec![direct_voice(SAMPLE_DRUM)], None),
    );
    let err = world.locate("multi-stale-group").unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/voicegroup/*"),
        "{err:?}"
    );
}

#[test]
fn planning_is_independent_of_the_order_the_pack_lists_songs() {
    // The pack directory is sorted, so reorder the world instead: the plan
    // must not depend on which song's constant is smaller.
    let mut world = World::new();
    world.songs_h = "#define MUS_OTHER 1\n#define MUS_TITLE 2\n".to_owned();
    world.write(SONG_TABLE_AT + 8, &table_entry(OTHER_HEADER_AT, 3, 9));
    world.write(SONG_TABLE_AT + 16, &table_entry(TITLE_HEADER_AT, 0, 7));
    let plan = world
        .locate("multi-swapped")
        .expect("the swapped world locates");
    let indices: Vec<(&str, u16)> = plan
        .songs
        .iter()
        .map(|song| (song.id.as_str(), song.index))
        .collect();
    assert_eq!(
        indices,
        [("audio/song/mus_other", 1), ("audio/song/mus_title", 2)]
    );
    assert_eq!(plan.song_table, bus(SONG_TABLE_AT));
}
