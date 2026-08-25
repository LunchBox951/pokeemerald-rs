//! Audio reader tests over a synthetic ROM.
//!
//! The fixture plants two `WaveData` samples, one programmable wave, one
//! key-split table, and two voicegroups: a child with a `starting_note`
//! bias and a song-selected parent whose slots cover every `ToneData` kind.
//! Each entry is decoded back through `assets` and compared to the value
//! the reader should have built, so the tests pin the field mapping rather
//! than the wire bytes.

use assets::{
    DirectSoundMode, DirectSoundVoice, Envelope, KeySplitVoice, NoiseVoice, ProgrammableWaveVoice,
    RhythmVoice, Sample, SampleId, Square1Voice, Square2Voice, VoiceEntry, VoiceGroup,
    VoiceGroupId,
};
use pack_format::PackWriter;

use super::{direct_sound, programmable_wave, voicegroup, write};
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, ROM_BASE};
use crate::rom::Rom;
use crate::roots::{AudioRoots, KeysplitRoot, Roots, SampleRoot, SongRoot, VoicegroupRoot};

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

/// A looping `DirectSound` sample.
const LOOPED: u32 = 0x1000;
/// A one-shot `DirectSound` sample.
const ONE_SHOT: u32 = 0x1100;
/// A DPCM-compressed sample.
const DPCM: u32 = 0x1200;
/// A programmable wave table.
const WAVE: u32 = 0x1300;
/// A key-split table, addressed before its first note.
const KEYSPLIT: u32 = 0x1400;
/// The child voicegroup, addressed 2 slots before its first declared one.
const CHILD: u32 = 0x2000;
/// The parent voicegroup a song selects.
const PARENT: u32 = 0x3000;
/// A song header pointing at the parent.
const SONG: u32 = 0x4000;
/// A voicegroup whose first slot has an unmodelled type.
const ODD_TYPE: u32 = 0x5000;
/// A voicegroup whose first slot points at an unrecorded sample.
const DANGLING: u32 = 0x5100;
/// An address four bytes before the end of a 16 MiB image: no header and
/// no slot fits.
const PAST_THE_ROM: u32 = 0x00FF_FFFC;

const LOOPED_PCM: [u8; 6] = [0x00, 0x7F, 0x80, 0xFF, 0x01, 0xFE];
const ONE_SHOT_PCM: [u8; 3] = [1, 2, 3];
const WAVE_TABLE: [u8; 16] = [
    0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF,
];
/// Three notes starting at note 2.
const KEYSPLIT_TABLE: [u8; 3] = [0, 0, 1];

fn wave_data(kind: u16, status: u16, freq: u32, loop_start: u32, pcm: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(kind.to_le_bytes());
    out.extend(status.to_le_bytes());
    out.extend(freq.to_le_bytes());
    out.extend(loop_start.to_le_bytes());
    out.extend(u32::try_from(pcm.len()).unwrap().to_le_bytes());
    out.extend(pcm);
    out
}

/// One `ToneData` slot.
fn slot(kind: u8, key: u8, length: u8, pan_sweep: u8, word: u32, env: [u8; 4]) -> [u8; 12] {
    let w = word.to_le_bytes();
    [
        kind, key, length, pan_sweep, w[0], w[1], w[2], w[3], env[0], env[1], env[2], env[3],
    ]
}

/// A `voice_keysplit` slot: child pointer in `wav`, table pointer in the
/// envelope bytes.
fn keysplit_slot(child: GbaPtr, table: GbaPtr) -> [u8; 12] {
    slot(0x40, 0, 0, 0, child.raw(), table.raw().to_le_bytes())
}

fn rom() -> Rom {
    let child: Vec<u8> = [
        slot(0x00, 60, 0, 0, at(LOOPED).raw(), [255, 0, 255, 165]),
        slot(0x08, 62, 0, 0xC0, at(ONE_SHOT).raw(), [255, 0, 255, 242]),
    ]
    .concat();
    let parent: Vec<u8> = [
        slot(0x80, 0, 0, 0, at(CHILD).raw(), [0; 4]),
        keysplit_slot(at(CHILD), at(KEYSPLIT)),
        slot(0x01, 60, 0, 0, 0x0000_0002, [0, 0, 15, 0]),
        slot(0x0A, 60, 1, 0, 0x0000_0001, [1, 1, 6, 2]),
        slot(0x0B, 60, 0, 0, at(WAVE).raw(), [0, 7, 15, 0]),
        slot(0x04, 60, 0, 0, 0x0000_0001, [0, 0, 15, 0]),
        slot(0x10, 60, 0, 0x90, at(LOOPED).raw(), [255, 0, 255, 0]),
        // Past the seven declared slots: what the linker placed next.
        slot(0x01, 60, 0, 0, 0x0000_0002, [0, 0, 15, 0]),
    ]
    .concat();
    // `SongHeader`: trackCount, blockCount, priority, reverb, tone.
    let mut song = vec![1, 0, 0, 0];
    song.extend(at(PARENT).raw().to_le_bytes());
    let bytes = RomFixture::new()
        .emerald_header()
        .write(
            LOOPED as usize,
            &wave_data(0, 0x4000, 3_425_024, 2, &LOOPED_PCM),
        )
        .write(
            ONE_SHOT as usize,
            &wave_data(0, 0, 13_700_096, 0, &ONE_SHOT_PCM),
        )
        .write(DPCM as usize, &wave_data(1, 0, 8, 0, &ONE_SHOT_PCM))
        .write(WAVE as usize, &WAVE_TABLE)
        .write(KEYSPLIT as usize + 2, &KEYSPLIT_TABLE)
        .write(CHILD as usize + 2 * 12, &child)
        .write(PARENT as usize, &parent)
        .write(SONG as usize, &song)
        .write(
            ODD_TYPE as usize,
            &slot(0x20, 60, 0, 0, at(LOOPED).raw(), [0; 4]),
        )
        .write(
            DANGLING as usize,
            &slot(0x00, 60, 0, 0, at(0x9000).raw(), [0; 4]),
        )
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

const fn sample(id: &'static str, addr: u32, data_len: u32) -> SampleRoot {
    SampleRoot {
        id,
        addr: at(addr),
        header_len: 16,
        data_len,
    }
}

static DIRECT_SOUND: [SampleRoot; 2] = [
    sample("audio/sample/direct-sound/looped", LOOPED, 6),
    sample("audio/sample/direct-sound/one_shot", ONE_SHOT, 3),
];
static PROGRAMMABLE_WAVE: [SampleRoot; 1] = [SampleRoot {
    id: "audio/sample/programmable-wave/01",
    addr: at(WAVE),
    header_len: 0,
    data_len: 16,
}];
static KEYSPLITS: [KeysplitRoot; 1] = [KeysplitRoot {
    label: "k",
    addr: at(KEYSPLIT),
    starting_note: 2,
    len: 3,
}];
static SONGS: [SongRoot; 1] = [SongRoot {
    id: "audio/song/s",
    index: 0,
    header: at(SONG),
    track_count: 1,
    voicegroup: at(PARENT),
}];

const fn group(id: &'static str, addr: u32, starting_note: u8, declared: u16) -> VoicegroupRoot {
    VoicegroupRoot {
        id,
        label: "g",
        addr: at(addr),
        starting_note,
        declared_slots: declared,
        addressable_slots: 8,
    }
}

static CHILD_ROOT: VoicegroupRoot = group("audio/voicegroup/child", CHILD, 2, 2);
static PARENT_ROOT: VoicegroupRoot = group("audio/voicegroup/parent", PARENT, 0, 7);
static VOICEGROUPS: [VoicegroupRoot; 2] = [CHILD_ROOT, PARENT_ROOT];

fn audio() -> AudioRoots {
    AudioRoots {
        song_table: GbaPtr::AT_BASE,
        songs: &SONGS,
        voicegroups: &VOICEGROUPS,
        keysplits: &KEYSPLITS,
        direct_sound: &DIRECT_SOUND,
        programmable_wave: &PROGRAMMABLE_WAVE,
    }
}

fn env(attack: u8, decay: u8, sustain: u8, release: u8) -> Envelope {
    Envelope {
        attack,
        decay,
        sustain,
        release,
    }
}

fn square1() -> VoiceEntry {
    VoiceEntry::Square1(Square1Voice {
        base_key: 60,
        length: 0,
        sweep: 0,
        duty: 2,
        envelope: env(0, 0, 15, 0),
        fixed_rate: false,
    })
}

#[test]
fn a_looping_sample_keeps_its_loop_and_pitch() {
    let rom = rom();
    let entry = direct_sound(&rom.reader(), &DIRECT_SOUND[0]).expect("a plain PCM sample");
    let Sample::DirectSound(sample) = Sample::decode(&entry.payload).unwrap() else {
        panic!("a DirectSound entry")
    };
    assert_eq!(sample.base_frequency, 3_425_024);
    assert_eq!(sample.loop_start(), Some(2));
    assert_eq!(sample.data(), [0, 127, -128, -1, 1, -2]);
}

#[test]
fn a_one_shot_sample_has_no_loop() {
    let rom = rom();
    let entry = direct_sound(&rom.reader(), &DIRECT_SOUND[1]).expect("a plain PCM sample");
    let Sample::DirectSound(sample) = Sample::decode(&entry.payload).unwrap() else {
        panic!("a DirectSound entry")
    };
    assert_eq!(sample.loop_start(), None);
    assert_eq!(sample.data(), [1, 2, 3]);
}

#[test]
fn a_compressed_sample_is_refused() {
    let rom = rom();
    let root = sample("audio/sample/direct-sound/dpcm", DPCM, 3);
    let err = direct_sound(&rom.reader(), &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::CompressedSample {
                id: "audio/sample/direct-sound/dpcm"
            }
        ),
        "{err}"
    );
}

#[test]
fn a_header_whose_size_disagrees_is_refused() {
    let rom = rom();
    let root = sample("audio/sample/direct-sound/looped", LOOPED, 7);
    let err = direct_sound(&rom.reader(), &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                field: "WaveData.size",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_sample_past_the_image_is_truncated() {
    let rom = rom();
    let root = sample("audio/sample/direct-sound/far", PAST_THE_ROM, 3);
    let err = direct_sound(&rom.reader(), &root).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_programmable_wave_copies_its_table() {
    let rom = rom();
    let entry = programmable_wave(&rom.reader(), &PROGRAMMABLE_WAVE[0]).expect("a table");
    assert_eq!(
        Sample::decode(&entry.payload).unwrap(),
        Sample::ProgrammableWave(assets::ProgrammableWave { table: WAVE_TABLE })
    );
}

#[test]
fn a_child_group_is_padded_around_its_declared_slots() {
    let rom = rom();
    let entry = voicegroup(&rom.reader(), &audio(), &CHILD_ROOT).expect("a child group");
    let group = VoiceGroup::decode(&entry.payload).unwrap();
    assert_eq!(group.slots().len(), 8);
    assert_eq!(group.slot(0), Some(&VoiceEntry::Empty));
    assert_eq!(group.slot(1), Some(&VoiceEntry::Empty));
    assert_eq!(
        group.slot(2),
        Some(&VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 60,
            pan: None,
            sample: SampleId("audio/sample/direct-sound/looped".into()),
            envelope: env(255, 0, 255, 165),
            mode: DirectSoundMode::Resampled,
        }))
    );
    assert_eq!(
        group.slot(3),
        Some(&VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 62,
            pan: Some(64),
            sample: SampleId("audio/sample/direct-sound/one_shot".into()),
            envelope: env(255, 0, 255, 242),
            mode: DirectSoundMode::Fixed,
        }))
    );
    assert!(group.slots()[4..].iter().all(|s| *s == VoiceEntry::Empty));
}

#[test]
fn a_song_selected_group_reads_every_addressable_slot() {
    let rom = rom();
    let entry = voicegroup(&rom.reader(), &audio(), &PARENT_ROOT).expect("the parent group");
    let group = VoiceGroup::decode(&entry.payload).unwrap();
    assert_eq!(group.slots().len(), 8);
    let child = VoiceGroupId("audio/voicegroup/child".into());
    assert_eq!(
        group.slot(0),
        Some(&VoiceEntry::Rhythm(RhythmVoice {
            children: child.clone()
        }))
    );
    assert_eq!(
        group.slot(1),
        Some(&VoiceEntry::KeySplit(
            KeySplitVoice::new(2, KEYSPLIT_TABLE.to_vec(), child).unwrap()
        ))
    );
    assert_eq!(group.slot(2), Some(&square1()));
    assert_eq!(
        group.slot(3),
        Some(&VoiceEntry::Square2(Square2Voice {
            base_key: 60,
            length: 1,
            duty: 1,
            envelope: env(1, 1, 6, 2),
            fixed_rate: true,
        }))
    );
    assert_eq!(
        group.slot(4),
        Some(&VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
            base_key: 60,
            length: 0,
            wave: SampleId("audio/sample/programmable-wave/01".into()),
            envelope: env(0, 7, 15, 0),
            fixed_rate: true,
        }))
    );
    assert_eq!(
        group.slot(5),
        Some(&VoiceEntry::Noise(NoiseVoice {
            base_key: 60,
            length: 0,
            period: 1,
            envelope: env(0, 0, 15, 0),
            fixed_rate: false,
        }))
    );
    assert_eq!(
        group.slot(6),
        Some(&VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: 60,
            pan: Some(16),
            sample: SampleId("audio/sample/direct-sound/looped".into()),
            envelope: env(255, 0, 255, 0),
            mode: DirectSoundMode::Reverse,
        }))
    );
    // Slot 7 is past the declared seven: the song can still select it, and
    // gets whatever the linker placed next.
    assert_eq!(group.slot(7), Some(&square1()));
}

#[test]
fn a_slot_of_an_unmodelled_type_is_refused() {
    let rom = rom();
    let root = group("audio/voicegroup/odd", ODD_TYPE, 0, 1);
    let err = voicegroup(&rom.reader(), &audio(), &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::VoiceType {
                root: "audio/voicegroup/odd",
                slot: 0,
                kind: 0x20,
            }
        ),
        "{err}"
    );
}

#[test]
fn a_pointer_the_profile_does_not_record_is_refused() {
    let rom = rom();
    let root = group("audio/voicegroup/dangling", DANGLING, 0, 1);
    let err = voicegroup(&rom.reader(), &audio(), &root).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::UnresolvedPointer {
                root: "audio/voicegroup/dangling",
                slot: 0,
                what: "a DirectSound sample",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_group_declaring_more_slots_than_it_can_address_is_refused() {
    let rom = rom();
    let root = group("audio/voicegroup/child", CHILD, 7, 2);
    let err = voicegroup(&rom.reader(), &audio(), &root).unwrap_err();
    assert!(matches!(err, ImportError::Length { .. }), "{err}");
}

#[test]
fn a_group_past_the_image_is_truncated() {
    let rom = rom();
    let root = group("audio/voicegroup/far", PAST_THE_ROM, 0, 2);
    let err = voicegroup(&rom.reader(), &audio(), &root).unwrap_err();
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn the_domain_writes_every_root() {
    let rom = rom();
    let roots = Roots {
        audio: audio(),
        ..Roots::NONE
    };
    let mut writer = PackWriter::new();
    write(&rom, &roots, &mut writer).expect("a well-formed table");
    assert_eq!(writer.len(), 5);
}
