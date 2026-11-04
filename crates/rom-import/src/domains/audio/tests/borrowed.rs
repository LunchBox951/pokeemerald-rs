//! Records a voicegroup borrows from its linked neighbours.
//!
//! A group declared with a `starting_note` bias aliases the records before its
//! first declared slot, and a song's group reads the records after its last
//! declared one. Upstream resolves one level of indirection (`ply_note` aborts
//! on a second), so a key-split or rhythm record only counts where a song
//! plays through the group that holds it.

use assets::{DirectSoundMode, DirectSoundVoice, SampleId, VoiceEntry, VoiceGroup};

use super::super::voicegroup;
use super::{
    at, env, group, keysplit_slot, slot, wave_data, DIRECT_SOUND, LOOPED, LOOPED_PCM, ONE_SHOT,
    ONE_SHOT_PCM, PROGRAMMABLE_WAVE,
};
use crate::fixture::RomFixture;
use crate::reader::GbaPtr;
use crate::rom::Rom;
use crate::roots::{AudioRoots, KeysplitRoot, SongRoot, VoicegroupRoot};

/// `leaf`, then a key-split record, then a rhythm record.
const PRED: u32 = 0x8000;
/// Aliases the key-split and rhythm records before its one declared leaf.
const ALIAS: u32 = PRED + 12;
const TABLE: u32 = 0x8100;

static KEYSPLITS: [KeysplitRoot; 1] = [KeysplitRoot {
    label: "t",
    addr: at(TABLE),
    starting_note: 0,
    len: 2,
}];
static GROUPS: [VoicegroupRoot; 2] = [
    group("audio/voicegroup/pred", PRED, 0, 3),
    group("audio/voicegroup/alias", ALIAS, 2, 1),
];
/// A song playing through the alias group.
static SELECTING_ALIAS: [SongRoot; 1] = [song(ALIAS)];

const fn song(voicegroup: u32) -> SongRoot {
    SongRoot {
        id: "audio/song/s",
        index: 0,
        header: at(0x8F00),
        track_count: 0,
        voicegroup: at(voicegroup),
    }
}

/// A square-1 record: padding for the slots a song's group addresses past its declared ones.
const FILLER: [u8; 12] = [1, 60, 0, 0, 2, 0, 0, 0, 0, 0, 15, 0];

fn leaf(sample: u32) -> [u8; 12] {
    slot(0x00, 60, 0, 0, at(sample).raw(), [255, 0, 255, 0])
}

fn samples(rom: RomFixture) -> RomFixture {
    rom.write(
        LOOPED as usize,
        &wave_data(0, 0x4000, 3_425_024, 2, &LOOPED_PCM),
    )
    .write(
        ONE_SHOT as usize,
        &wave_data(0, 0, 13_700_096, 0, &ONE_SHOT_PCM),
    )
}

fn alias_rom() -> Rom {
    let bytes = samples(RomFixture::new().emerald_header())
        .write(TABLE as usize, &[0, 0])
        .write(PRED as usize, &leaf(LOOPED))
        .write(PRED as usize + 12, &keysplit_slot(at(ALIAS), at(TABLE)))
        .write(
            PRED as usize + 24,
            &slot(0x80, 0, 0, 0, at(ALIAS).raw(), [0; 4]),
        )
        .write(ALIAS as usize + 24, &leaf(ONE_SHOT))
        .write(ALIAS as usize + 36, &FILLER.repeat(5))
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

fn alias_group(songs: &'static [SongRoot]) -> VoiceGroup {
    let audio = AudioRoots {
        song_table: GbaPtr::AT_BASE,
        songs,
        voicegroups: &GROUPS,
        keysplits: &KEYSPLITS,
        direct_sound: &DIRECT_SOUND,
        programmable_wave: &PROGRAMMABLE_WAVE,
        cry: None,
    };
    let entry = voicegroup(&alias_rom().reader(), &audio, &GROUPS[1]).expect("the alias group");
    VoiceGroup::decode(&entry.payload).unwrap()
}

#[test]
fn a_childs_aliased_indirection_records_are_silent() {
    let group = alias_group(&[]);
    assert_eq!(
        group.slot(0),
        Some(&VoiceEntry::Empty),
        "the key-split record"
    );
    assert_eq!(group.slot(1), Some(&VoiceEntry::Empty), "the rhythm record");
    assert!(
        matches!(group.slot(2), Some(VoiceEntry::DirectSound(_))),
        "the declared leaf is read"
    );
}

#[test]
fn a_songs_group_keeps_the_indirection_records_it_aliases() {
    let group = alias_group(&SELECTING_ALIAS);
    assert!(matches!(group.slot(0), Some(VoiceEntry::KeySplit(_))));
    assert!(matches!(group.slot(1), Some(VoiceEntry::Rhythm(_))));
}

/// A song's group whose only key-split record lies past its declared slots.
const ROOT: u32 = 0x9000;
const ROOT_TABLE: u32 = 0x9100;
const KID: u32 = 0x9200;
/// Linked right after the child: the leaf the table selects through the child.
const NEXT: u32 = KID + 12;

static ROOT_KEYSPLITS: [KeysplitRoot; 1] = [KeysplitRoot {
    label: "r",
    addr: at(ROOT_TABLE),
    starting_note: 0,
    len: 2,
}];
static ROOT_GROUPS: [VoicegroupRoot; 3] = [
    group("audio/voicegroup/root", ROOT, 0, 1),
    group("audio/voicegroup/kid", KID, 0, 1),
    group("audio/voicegroup/next", NEXT, 0, 1),
];
static ROOT_SONGS: [SongRoot; 1] = [song(ROOT)];

#[test]
fn a_songs_borrowed_key_split_selects_its_childs_linked_tail() {
    // The table maps note 1 to child index 1, past the child's declared slot.
    let bytes = samples(RomFixture::new().emerald_header())
        .write(ROOT_TABLE as usize, &[0, 1])
        .write(ROOT as usize, &leaf(LOOPED))
        .write(ROOT as usize + 12, &keysplit_slot(at(KID), at(ROOT_TABLE)))
        .write(ROOT as usize + 24, &FILLER.repeat(6))
        .write(KID as usize, &leaf(LOOPED))
        .write(NEXT as usize, &leaf(ONE_SHOT))
        .finish();
    let rom = Rom::from_bytes(bytes).expect("the fixture header is valid");
    let audio = AudioRoots {
        song_table: GbaPtr::AT_BASE,
        songs: &ROOT_SONGS,
        voicegroups: &ROOT_GROUPS,
        keysplits: &ROOT_KEYSPLITS,
        direct_sound: &DIRECT_SOUND,
        programmable_wave: &PROGRAMMABLE_WAVE,
        cry: None,
    };
    let entry = voicegroup(&rom.reader(), &audio, &ROOT_GROUPS[1]).expect("the child group");
    let kid = VoiceGroup::decode(&entry.payload).unwrap();
    assert_eq!(
        kid.slot(1),
        Some(&VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 60,
            pan: None,
            sample: SampleId("audio/sample/direct-sound/one_shot".into()),
            envelope: env(255, 0, 255, 0),
            mode: DirectSoundMode::Resampled,
        }))
    );
}
