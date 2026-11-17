//! Real-pack pinning for `xtask::extract::voicegroups`: decodes real `cargo
//! xtask extract` output through [`assets::VoiceGroup::decode`] and compares it
//! with expectations written in this file.
//!
//! # What these tests establish
//!
//! For each of the seven groups in [`EXPECTED_GROUPS`], each `*_records_equal_*`
//! test compares every slot the upstream declarations reach as a whole
//! [`VoiceEntry`] against a literal written here from the `.inc` source it
//! cites (`pokeemerald/sound/voicegroups/**/*.inc`, `sound/keysplit_tables.inc`,
//! `sound/voice_groups.inc` for link order): sample or wave id, base key, pan
//! override, envelope, playback mode or `_alt` flag, duty and sweep, and each
//! key split's start note, table, and child group. That covers `title`'s 89
//! declared slots plus the 39 `intro.inc` records it reads by link adjacency,
//! `rs_drumset`'s 36 aliased `dummy.inc` records plus its 29 own, and each
//! key-split child's declared records plus the one linked record a widened
//! table selects past them (strings, tuba). In the five key-split children
//! every other slot must be [`VoiceEntry::Empty`]. A pinned slot repointed to
//! another existing sample therefore fails, as the pack-free
//! `the_group_comparison_rejects_a_slot_repointed_to_another_existing_sample`
//! shows; the one exception is the unpinned `rs_drumset` tail listed below. The
//! literals are transcribed from the `.inc` files by hand, never computed from
//! the pack or from `xtask`.
//!
//! The separate id walk adds only that every id a group references exists in
//! the pack.
//!
//! # What they do not establish
//!
//! * The encoded bytes are not compared with expected bytes. The comparison is
//!   on the decoded view, so a bytes-level difference the decoder ignores
//!   would pass; this is not a byte-for-byte equality.
//! * `rs_drumset` slots 65..128 are not pinned. Upstream's rhythm lookup reads
//!   them from the linked `frlg.inc` records, which are not transcribed here.
//! * The sample and wave payloads the ids name, and every group outside the
//!   seven, are not checked here.
//!
//! See `crate::lib`'s module docs for why this lives here.

use assets::pack::AssetPack;
use assets::{
    DirectSoundMode, DirectSoundVoice, Envelope, KeySplitVoice, ProgrammableWaveVoice, RhythmVoice,
    SampleId, Square1Voice, Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
};

/// `MUS_TITLE`'s voicegroup plus every key-split/rhythm child it transitively
/// references (`xtask::extract::voicegroups` docs explain why exactly these seven).
const EXPECTED_GROUPS: [&str; 7] = [
    "title",
    "rs_drumset",
    "piano_keysplit",
    "strings_keysplit",
    "trumpet_keysplit",
    "tuba_keysplit",
    "french_horn_keysplit",
];

/// A program byte addresses 128 slots (`ply_voice`, `src/m4a_1.s`); pinned
/// apart from `assets::VOICE_SLOT_COUNT` so a schema change cannot move it.
const ADDRESSABLE_SLOTS: usize = 128;

fn decode(pack: &AssetPack, label: &str) -> VoiceGroup {
    let bytes = pack
        .raw(&format!("audio/voicegroup/{label}"))
        .unwrap_or_else(|e| panic!("audio/voicegroup/{label} should be in the pack: {e}"));
    VoiceGroup::decode(bytes)
        .unwrap_or_else(|e| panic!("audio/voicegroup/{label} should decode cleanly: {e}"))
}

/// What the slots past a group's expected records must hold.
#[derive(Clone, Copy)]
enum PastExpected {
    /// [`VoiceEntry::Empty`]: nothing reaches them.
    Empty,
    /// Not pinned: upstream's rhythm lookup reads linked records there that
    /// these tests do not transcribe.
    Unpinned,
}

/// Compares the group's 128 slots with `declared` slot by slot, then the slots
/// past `declared` as `past` says.
fn assert_group_matches(
    label: &str,
    group: &VoiceGroup,
    declared: &[VoiceEntry],
    past: PastExpected,
) {
    assert!(
        declared.len() <= ADDRESSABLE_SLOTS,
        "{label}: expectation has {} slots, more than are addressable",
        declared.len()
    );
    assert_eq!(
        group.slots().len(),
        ADDRESSABLE_SLOTS,
        "{label}: slot count"
    );
    for (index, want) in declared.iter().enumerate() {
        assert_eq!(group.slot(index), Some(want), "{label} slot {index}");
    }
    if matches!(past, PastExpected::Empty) {
        for index in declared.len()..ADDRESSABLE_SLOTS {
            assert_eq!(
                group.slot(index),
                Some(&VoiceEntry::Empty),
                "{label} slot {index}"
            );
        }
    }
}

fn assert_pack_group_matches(label: &str, declared: &[VoiceEntry], past: PastExpected) {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    assert_group_matches(label, &decode(&pack, label), declared, past);
}

const DIRECT_SOUND_ID_PREFIX: &str = "audio/sample/direct-sound/";
const PROGRAMMABLE_WAVE_ID_PREFIX: &str = "audio/sample/programmable-wave/";
const VOICEGROUP_ID_PREFIX: &str = "audio/voicegroup/";

fn envelope([attack, decay, sustain, release]: [u8; 4]) -> Envelope {
    Envelope {
        attack,
        decay,
        sustain,
        release,
    }
}

/// `voice_directsound base_key, pan, DirectSoundWaveData_<sample>, a, d, s, r`
/// (`asm/macros/music_voice.inc:1-4`); pan `0` is no override, as upstream's
/// `_voice_directsound` treats it.
fn direct(base_key: u8, pan: u8, sample: &str, adsr: [u8; 4]) -> VoiceEntry {
    direct_in_mode(base_key, pan, sample, adsr, DirectSoundMode::Resampled)
}

/// `voice_directsound_no_resample`.
fn direct_fixed(base_key: u8, pan: u8, sample: &str, adsr: [u8; 4]) -> VoiceEntry {
    direct_in_mode(base_key, pan, sample, adsr, DirectSoundMode::Fixed)
}

fn direct_in_mode(
    base_key: u8,
    pan: u8,
    sample: &str,
    adsr: [u8; 4],
    mode: DirectSoundMode,
) -> VoiceEntry {
    VoiceEntry::DirectSound(DirectSoundVoice {
        base_key,
        pan: (pan != 0).then_some(pan),
        sample: SampleId(format!("{DIRECT_SOUND_ID_PREFIX}{sample}")),
        envelope: envelope(adsr),
        mode,
    })
}

/// `voice_square_1[_alt] base_key, 0, sweep, duty, a, d, s, r`; `fixed_rate` is
/// the `_alt` flavour. Every record here declares pan `0`.
fn sq1(base_key: u8, sweep: u8, duty: u8, adsr: [u8; 4], fixed_rate: bool) -> VoiceEntry {
    VoiceEntry::Square1(Square1Voice {
        base_key,
        length: 0,
        sweep,
        duty,
        envelope: envelope(adsr),
        fixed_rate,
    })
}

/// `voice_square_2[_alt] base_key, 0, duty, a, d, s, r`.
fn sq2(base_key: u8, duty: u8, adsr: [u8; 4], fixed_rate: bool) -> VoiceEntry {
    VoiceEntry::Square2(Square2Voice {
        base_key,
        length: 0,
        duty,
        envelope: envelope(adsr),
        fixed_rate,
    })
}

/// `voice_programmable_wave[_alt] base_key, 0, ProgrammableWaveData_<n>, a, d, s, r`.
fn wave(base_key: u8, wave_number: u8, adsr: [u8; 4], fixed_rate: bool) -> VoiceEntry {
    VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
        base_key,
        length: 0,
        wave: SampleId(format!("{PROGRAMMABLE_WAVE_ID_PREFIX}{wave_number:02}")),
        envelope: envelope(adsr),
        fixed_rate,
    })
}

/// `voice_keysplit_all voicegroup_<child>`.
fn rhythm(child: &str) -> VoiceEntry {
    VoiceEntry::Rhythm(RhythmVoice {
        children: VoiceGroupId(format!("{VOICEGROUP_ID_PREFIX}{child}")),
    })
}

/// A `voice_keysplit` record: the table as `(child slot, run length)` runs,
/// the first run covering note `starting_note`.
fn key_split(starting_note: u8, runs: &[(u8, usize)], child: &str) -> VoiceEntry {
    let table = runs
        .iter()
        .flat_map(|&(slot, len)| std::iter::repeat_n(slot, len))
        .collect();
    VoiceEntry::KeySplit(
        KeySplitVoice::new(
            starting_note,
            table,
            VoiceGroupId(format!("{VOICEGROUP_ID_PREFIX}{child}")),
        )
        .expect("the expected key-split table is in range"),
    )
}

// The five key-split tables, as the bytes upstream really reads
// (`sound/keysplit_tables.inc`): `keysplit <label>, N` puts the label N bytes
// before its first data byte, and the blocks are emitted back to back, so a
// note outside a table's declared notes reads its neighbour's byte (header
// comment, lines 1-12). Emitted in file order (the notes each block declares in parentheses):
//   piano   (36..=107): 19 of child 0 (36..=54), 15 of 1 (55..=69), 21 of 2 (70..=90), 17 of 3 (91..=107)
//   strings (36..=107): 33 of child 0 (..=68), 12 of 1 (..=80), 27 of 2 (..=107)
//   trumpet (36..=107): 30 of child 0 (..=65), 18 of 1 (..=83), 24 of 2 (..=107)
//   tuba    (24..=107): 18 of child 0 (..=41), 66 of 1 (..=107)
//   french_horn (36..=107): 30 of child 0 (..=65), 42 of 1 (..=107)
// A table's note `n` reads byte `block_start - N + n` of that stream, for the
// notes `0..128` whose byte lies in this file's emitted tables (none is invented
// past either end).

/// Piano's table starts at note 36, the notes after its own 72 bytes read
/// strings' first 20 (all child 0).
fn key_split_piano() -> VoiceEntry {
    key_split(
        36,
        &[(0, 19), (1, 15), (2, 21), (3, 17), (0, 20)],
        "piano_keysplit",
    )
}

/// Strings' table reaches back before its block: notes 0..36 read the last 36
/// of piano's bytes, and 108..128 trumpet's first 20.
fn key_split_strings() -> VoiceEntry {
    key_split(
        0,
        &[(2, 19), (3, 17), (0, 33), (1, 12), (2, 27), (0, 20)],
        "strings_keysplit",
    )
}

/// Notes 0..36 read the last 36 of strings' bytes, 108..128 the first 20 of
/// tuba's (18 of child 0, then 2 of child 1).
fn key_split_trumpet() -> VoiceEntry {
    key_split(
        0,
        &[(1, 9), (2, 27), (0, 30), (1, 18), (2, 24), (0, 18), (1, 2)],
        "trumpet_keysplit",
    )
}

/// Notes 0..24 read the last 24 of trumpet's bytes, 108..128 the first 20 of
/// `french_horn`'s (all child 0).
fn key_split_tuba() -> VoiceEntry {
    key_split(0, &[(2, 24), (0, 18), (1, 66), (0, 20)], "tuba_keysplit")
}

/// Notes 0..36 read the last 36 of tuba's bytes; `french_horn` is the last block
/// in the file, so the table ends at note 107.
fn key_split_french_horn() -> VoiceEntry {
    key_split(0, &[(1, 36), (0, 30), (1, 42)], "french_horn_keysplit")
}

/// Collects a group's declared records in `.inc` order.
#[derive(Default)]
struct Slots(Vec<VoiceEntry>);

impl Slots {
    fn one(&mut self, entry: VoiceEntry) {
        self.0.push(entry);
    }

    /// `count` copies of `voice_square_1 60, 0, 0, 2, 0, 0, 15, 0`, the record
    /// upstream writes for every unused instrument.
    fn fill(&mut self, count: usize) {
        for _ in 0..count {
            self.0.push(sq1(60, 0, 2, [0, 0, 15, 0], false));
        }
    }
}

/// `sound/voicegroups/title.inc`'s 89 declared slots (slot = line - 2), then
/// the 39 slots `intro.inc` lines 2-40 supply by link adjacency
/// (`xtask::extract::voicegroups` docs).
fn title_declared() -> Vec<VoiceEntry> {
    let mut s = Slots::default();
    s.one(rhythm("rs_drumset")); // title.inc:2
    s.one(key_split_piano()); // title.inc:3
    s.fill(2);
    s.one(sq2(60, 1, [1, 1, 6, 2], true)); // title.inc:6
    s.one(wave(60, 1, [0, 7, 15, 0], true)); // title.inc:7
    s.one(wave(60, 5, [0, 7, 15, 0], true)); // title.inc:8
    s.fill(2);
    s.one(direct(60, 0, "sc88pro_glockenspiel", [255, 165, 51, 242])); // title.inc:11
    s.fill(4);
    s.one(direct(60, 0, "sc88pro_tubular_bell", [255, 165, 90, 216])); // title.inc:16
    s.fill(31);
    s.one(direct(60, 0, "sc88pro_harp", [255, 242, 51, 242])); // title.inc:48
    s.one(direct(60, 0, "sc88pro_timpani", [255, 0, 180, 239])); // title.inc:49
    s.one(key_split_strings()); // title.inc:50
    s.fill(7);
    s.one(key_split_trumpet()); // title.inc:58
    s.fill(1);
    s.one(key_split_tuba()); // title.inc:60
    s.fill(1);
    s.one(key_split_french_horn()); // title.inc:62
    s.fill(12);
    s.one(direct(60, 0, "sc88pro_flute", [255, 0, 255, 165])); // title.inc:75
    s.fill(6);
    s.one(sq1(60, 0, 2, [0, 1, 6, 2], true)); // title.inc:82
    s.one(sq2(60, 2, [0, 1, 6, 0], true)); // title.inc:83
    s.one(wave(60, 2, [0, 7, 15, 0], true)); // title.inc:84
    s.one(sq1(60, 0, 1, [0, 1, 4, 1], true)); // title.inc:85
    s.one(sq1(60, 0, 1, [1, 2, 4, 0], true)); // title.inc:86
    s.fill(2);
    s.one(wave(60, 6, [0, 7, 15, 0], true)); // title.inc:89
    s.one(sq2(60, 3, [0, 1, 6, 2], true)); // title.inc:90
    assert_eq!(s.0.len(), 89, "title.inc declares 89 slots");
    s.one(rhythm("rs_drumset")); // intro.inc:2
    s.one(key_split_piano()); // intro.inc:3
    s.fill(11);
    s.one(direct(60, 0, "sc88pro_xylophone", [255, 235, 0, 204])); // intro.inc:15
    s.fill(25);
    s.0
}

/// The 36 records `drumsets/rs.inc`'s `voice_group rs_drumset, 36` aliases:
/// the last 36 declared in `dummy.inc` (lines 64-99, which links right before
/// it in `sound/voice_groups.inc`).
fn dummy_tail_aliased_by_rs_drumset() -> Vec<VoiceEntry> {
    let mut s = Slots::default();
    s.fill(8);
    s.one(sq1(60, 0, 2, [0, 1, 6, 0], false)); // dummy.inc:72
    s.fill(2);
    s.one(wave(60, 1, [0, 7, 15, 1], false)); // dummy.inc:75
    s.fill(6);
    s.one(sq2(60, 2, [0, 1, 6, 0], false)); // dummy.inc:82
    s.one(wave(60, 3, [0, 7, 15, 1], false)); // dummy.inc:83
    s.one(sq1(60, 0, 2, [0, 1, 6, 0], false)); // dummy.inc:84
    s.one(sq2(60, 3, [0, 1, 6, 0], false)); // dummy.inc:85
    s.fill(3);
    s.one(sq1(60, 0, 0, [0, 1, 6, 0], false)); // dummy.inc:89
    s.fill(2);
    s.one(direct(
        60,
        0,
        "classical_choir_voice_ahhs",
        [255, 0, 255, 0],
    )); // dummy.inc:92
    s.one(direct(60, 0, "sd90_classical_oboe", [255, 165, 154, 127])); // dummy.inc:93
    s.one(direct(60, 0, "unused_sd90_oboe", [255, 165, 154, 127])); // dummy.inc:94
    s.one(direct(
        60,
        0,
        "unused_guitar_separates_power_chord",
        [255, 165, 206, 127],
    )); // dummy.inc:95
    s.one(direct(
        60,
        0,
        "unused_sc88pro_unison_slap",
        [255, 165, 206, 127],
    )); // dummy.inc:96
    s.one(direct(60, 0, "unknown_snare", [255, 0, 255, 0])); // dummy.inc:97
    s.one(direct(60, 0, "ethnic_flavours_ohtsuzumi", [255, 0, 255, 0])); // dummy.inc:98
    s.one(direct(60, 0, "ethnic_flavours_hyoushigi", [255, 0, 255, 0])); // dummy.inc:99
    assert_eq!(s.0.len(), 36);
    s.0
}

/// `rs_drumset`'s slots 0..65: the aliased `dummy.inc` tail, then rs.inc's own
/// 29 records.
fn rs_drumset_declared() -> Vec<VoiceEntry> {
    let mut s = Slots(dummy_tail_aliased_by_rs_drumset());
    s.one(direct_fixed(60, 64, "sc88pro_rnd_kick", [255, 0, 255, 242])); // rs.inc:2
    s.fill(1);
    s.one(direct_fixed(
        60,
        64,
        "sc88pro_rnd_snare",
        [255, 0, 255, 242],
    )); // rs.inc:4
    s.one(direct_fixed(
        60,
        64,
        "sc88pro_tr909_hand_clap",
        [255, 255, 255, 127],
    )); // rs.inc:5
    s.one(direct_fixed(
        60,
        64,
        "sc88pro_orchestra_snare",
        [255, 0, 255, 242],
    )); // rs.inc:6
    s.one(direct(48, 44, "unused_sc55_tom", [255, 210, 77, 204])); // rs.inc:7
    s.one(direct_fixed(
        60,
        79,
        "unknown_close_hihat",
        [255, 127, 0, 188],
    )); // rs.inc:8
    s.one(direct(51, 54, "unused_sc55_tom", [255, 216, 77, 204])); // rs.inc:9
    s.fill(1);
    s.one(direct(54, 64, "unused_sc55_tom", [255, 216, 77, 204])); // rs.inc:11
    s.one(direct_fixed(
        60,
        79,
        "unknown_open_hihat",
        [255, 242, 141, 0],
    )); // rs.inc:12
    s.one(direct(57, 69, "unused_sc55_tom", [255, 210, 77, 204])); // rs.inc:13
    s.one(direct(60, 79, "unused_sc55_tom", [255, 204, 77, 204])); // rs.inc:14
    s.fill(1);
    s.one(direct(62, 84, "unused_sc55_tom", [255, 204, 77, 204])); // rs.inc:16
    s.fill(2);
    s.one(direct_fixed(70, 49, "unknown_bell", [255, 165, 103, 231])); // rs.inc:19
    s.one(direct_fixed(
        32,
        34,
        "sc88pro_tambourine",
        [255, 127, 77, 204],
    )); // rs.inc:20
    s.one(direct_fixed(
        60,
        14,
        "trinity_cymbal_crash",
        [255, 235, 0, 165],
    )); // rs.inc:21
    s.fill(1);
    s.one(direct_fixed(
        30,
        54,
        "sc88pro_orchestra_cymbal_crash",
        [255, 246, 0, 216],
    )); // rs.inc:23
    s.fill(1);
    s.one(direct_fixed(
        30,
        54,
        "sc88pro_orchestra_cymbal_crash",
        [255, 246, 0, 216],
    )); // rs.inc:25
    s.one(direct_fixed(
        30,
        64,
        "sc88pro_orchestra_cymbal_crash",
        [8, 0, 255, 216],
    )); // rs.inc:26
    s.fill(1);
    s.one(direct_fixed(
        72,
        104,
        "sc88pro_mute_high_conga",
        [255, 0, 255, 0],
    )); // rs.inc:28
    s.fill(1);
    s.one(direct_fixed(
        72,
        94,
        "sc88pro_open_low_conga",
        [255, 0, 255, 0],
    )); // rs.inc:30
    s.0
}

fn piano_keysplit_declared() -> Vec<VoiceEntry> {
    let mut s = Slots::default();
    s.one(direct(60, 0, "sc88pro_piano1_48", [255, 252, 0, 239])); // piano.inc:2
    s.one(direct(60, 0, "sc88pro_piano1_60", [255, 250, 0, 221])); // piano.inc:3
    s.one(direct(60, 0, "sc88pro_piano1_72", [255, 250, 0, 221])); // piano.inc:4
    s.one(direct(60, 0, "sc88pro_piano1_84", [255, 247, 0, 221])); // piano.inc:5
    s.0
}

fn strings_keysplit_declared() -> Vec<VoiceEntry> {
    let mut s = Slots::default();
    s.one(direct(
        60,
        0,
        "sc88pro_string_ensemble_60",
        [255, 0, 255, 196],
    )); // strings.inc:2
    s.one(direct(
        60,
        0,
        "sc88pro_string_ensemble_72",
        [255, 0, 255, 196],
    )); // strings.inc:3
    s.one(direct(
        60,
        0,
        "sc88pro_string_ensemble_84",
        [255, 0, 255, 196],
    )); // strings.inc:4
        // Strings' widened table (`key_split_strings`) selects child 3, past its three
        // records, so upstream plays the record linked right after them: the first
        // of `trumpet.inc` (`sound/voice_groups.inc:11-12`).
    s.one(direct(60, 0, "sc88pro_trumpet_60", [255, 0, 193, 127])); // trumpet.inc:2
    s.0
}

fn trumpet_keysplit_declared() -> Vec<VoiceEntry> {
    let mut s = Slots::default();
    s.one(direct(60, 0, "sc88pro_trumpet_60", [255, 0, 193, 127])); // trumpet.inc:2
    s.one(direct(60, 0, "sc88pro_trumpet_72", [255, 0, 193, 127])); // trumpet.inc:3
    s.one(direct(60, 0, "sc88pro_trumpet_84", [255, 0, 193, 127])); // trumpet.inc:4
    s.0
}

fn tuba_keysplit_declared() -> Vec<VoiceEntry> {
    let mut s = Slots::default();
    s.one(direct(60, 0, "sc88pro_tuba_39", [255, 0, 255, 165])); // tuba.inc:2
    s.one(direct(60, 0, "sc88pro_tuba_51", [255, 0, 255, 165])); // tuba.inc:3
                                                                 // Tuba's widened table (`key_split_tuba`) selects child 2, past its two
                                                                 // records: the first of `french_horn.inc` follows (`sound/voice_groups.inc:14-15`).
    s.one(direct(60, 0, "sc88pro_french_horn_60", [255, 0, 224, 165])); // french_horn.inc:2
    s.0
}

fn french_horn_keysplit_declared() -> Vec<VoiceEntry> {
    let mut s = Slots::default();
    s.one(direct(60, 0, "sc88pro_french_horn_60", [255, 0, 224, 165])); // french_horn.inc:2
    s.one(direct(60, 0, "sc88pro_french_horn_72", [255, 0, 218, 165])); // french_horn.inc:3
    s.0
}

/// A piano group exactly as declared, optionally with slot 1 replaced.
fn piano_group_with_slot_1(replacement: Option<VoiceEntry>) -> (VoiceGroup, Vec<VoiceEntry>) {
    let declared = piano_keysplit_declared();
    let mut slots = declared.clone();
    slots.resize(ADDRESSABLE_SLOTS, VoiceEntry::Empty);
    if let Some(entry) = replacement {
        slots[1] = entry;
    }
    (
        VoiceGroup::new(slots).expect("the group is valid"),
        declared,
    )
}

#[test]
fn the_group_comparison_accepts_the_declared_records() {
    // Needs no pack.
    let (group, declared) = piano_group_with_slot_1(None);
    assert_group_matches("piano_keysplit", &group, &declared, PastExpected::Empty);
}

#[test]
#[should_panic(expected = "piano_keysplit slot 1")]
fn the_group_comparison_rejects_a_slot_repointed_to_another_existing_sample() {
    // Needs no pack: slot 1 plays slot 2's sample, an id that exists in the
    // wrong place.
    let (group, declared) = piano_group_with_slot_1(Some(piano_keysplit_declared()[2].clone()));
    assert_group_matches("piano_keysplit", &group, &declared, PastExpected::Empty);
}

#[test]
#[should_panic(expected = "piano_keysplit slot 4")]
fn the_group_comparison_rejects_a_record_where_a_slot_should_be_empty() {
    let declared = piano_keysplit_declared();
    let mut slots = declared.clone();
    slots.resize(ADDRESSABLE_SLOTS, VoiceEntry::Empty);
    slots[4] = declared[0].clone();
    let group = VoiceGroup::new(slots).expect("the group is valid");
    assert_group_matches("piano_keysplit", &group, &declared, PastExpected::Empty);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn title_records_equal_its_declaration_and_the_linked_intro_overflow() {
    assert_pack_group_matches("title", &title_declared(), PastExpected::Empty);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn rs_drumset_records_equal_the_aliased_dummy_tail_and_its_declaration() {
    // `rs_drumset` is a rhythm child: upstream reads any note's slot, so
    // slots past rs.inc's last record are the linked `frlg.inc` records, which
    // this test does not transcribe.
    assert_pack_group_matches("rs_drumset", &rs_drumset_declared(), PastExpected::Unpinned);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn piano_keysplit_records_equal_its_declaration() {
    assert_pack_group_matches(
        "piano_keysplit",
        &piano_keysplit_declared(),
        PastExpected::Empty,
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn strings_keysplit_records_equal_its_declaration() {
    assert_pack_group_matches(
        "strings_keysplit",
        &strings_keysplit_declared(),
        PastExpected::Empty,
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn trumpet_keysplit_records_equal_its_declaration() {
    assert_pack_group_matches(
        "trumpet_keysplit",
        &trumpet_keysplit_declared(),
        PastExpected::Empty,
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn tuba_keysplit_records_equal_its_declaration() {
    assert_pack_group_matches(
        "tuba_keysplit",
        &tuba_keysplit_declared(),
        PastExpected::Empty,
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn french_horn_keysplit_records_equal_its_declaration() {
    assert_pack_group_matches(
        "french_horn_keysplit",
        &french_horn_keysplit_declared(),
        PastExpected::Empty,
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_expected_voicegroup_is_present_and_decodes() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    for label in EXPECTED_GROUPS {
        let group = decode(&pack, label);
        assert_eq!(
            group.slots().len(),
            assets::VOICE_SLOT_COUNT,
            "audio/voicegroup/{label} must carry the full 128-slot normalization"
        );
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn overflow_slots_resolve_through_the_linked_successors_own_entries() {
    // `intro.inc` links right after `title.inc` (`xtask::extract::voicegroups`
    // docs); slot `TITLE_DECLARED_SLOT_COUNT` is intro.inc:2, the highest slot is intro.inc:40.
    const TITLE_DECLARED_SLOT_COUNT: usize = 89;
    // A program byte addresses 128 slots (`ply_voice`, `src/m4a_1.s`); pinned
    // apart from `assets::VOICE_SLOT_COUNT` so a schema change cannot move it.
    const UPSTREAM_ADDRESSABLE_SLOT_COUNT: usize = 128;

    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let title = decode(&pack, "title");

    match title.slot(TITLE_DECLARED_SLOT_COUNT) {
        Some(VoiceEntry::Rhythm(rhythm)) => {
            assert_eq!(rhythm.children.0, "audio/voicegroup/rs_drumset");
        }
        other => panic!(
            "expected slot {TITLE_DECLARED_SLOT_COUNT} to be a Rhythm indirection, got {other:?}"
        ),
    }

    assert_eq!(
        title.slot(UPSTREAM_ADDRESSABLE_SLOT_COUNT - 1),
        Some(&VoiceEntry::Square1(assets::Square1Voice {
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
        }))
    );
    assert!(title.slot(UPSTREAM_ADDRESSABLE_SLOT_COUNT).is_none());
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn titles_rhythm_slot_resolves_through_rs_drumset_aliasing_the_dummy_records_before_it() {
    // `drumsets/rs.inc`: `voice_group rs_drumset, 36`.
    const RS_DRUMSET_FIRST_DECLARED_SLOT: usize = 36;

    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let title = decode(&pack, "title");
    match title.slot(0) {
        Some(VoiceEntry::Rhythm(rhythm)) => {
            assert_eq!(rhythm.children.0, "audio/voicegroup/rs_drumset");
        }
        other => panic!("expected title's slot 0 to be a Rhythm indirection, got {other:?}"),
    }

    let rs_drumset = decode(&pack, "rs_drumset");
    // The bias aliases the 36 records linked before it: dummy.inc's last 36.
    for index in 0..RS_DRUMSET_FIRST_DECLARED_SLOT {
        assert_ne!(
            rs_drumset.slot(index),
            Some(&VoiceEntry::Empty),
            "slot {index} should be an aliased dummy.inc record"
        );
    }
    match rs_drumset.slot(RS_DRUMSET_FIRST_DECLARED_SLOT - 1) {
        Some(VoiceEntry::DirectSound(voice)) => {
            assert_eq!(
                voice.sample.0,
                "audio/sample/direct-sound/ethnic_flavours_hyoushigi"
            );
        }
        other => panic!("expected dummy.inc's last record at slot 35, got {other:?}"),
    }
    assert_ne!(
        rs_drumset.slot(RS_DRUMSET_FIRST_DECLARED_SLOT),
        Some(&VoiceEntry::Empty)
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn titles_key_split_slots_resolve_their_tables_and_children() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let title = decode(&pack, "title");
    match title.slot(1) {
        Some(VoiceEntry::KeySplit(key_split)) => {
            // `sound/keysplit_tables.inc`: piano's key-split table has this many entries.
            const PIANO_KEYSPLIT_TABLE_LEN: usize = 72;

            // The notes 108..128 read the bytes after piano's: strings' first 20,
            // all child 0 (`keysplit_tables.inc:13-22`).
            const PIANO_ALIAS_TAIL_LEN: usize = 20;

            assert_eq!(key_split.starting_note, 36);
            assert_eq!(
                key_split.table().len(),
                PIANO_KEYSPLIT_TABLE_LEN + PIANO_ALIAS_TAIL_LEN
            );
            let declared_table = [vec![0; 19], vec![1; 15], vec![2; 21], vec![3; 17]].concat();
            assert_eq!(
                key_split.table()[..PIANO_KEYSPLIT_TABLE_LEN],
                declared_table
            );
            assert!(key_split.table()[PIANO_KEYSPLIT_TABLE_LEN..]
                .iter()
                .all(|&child| child == 0));
            assert_eq!(key_split.children.0, "audio/voicegroup/piano_keysplit");
        }
        other => panic!("expected title's slot 1 to be a KeySplit indirection, got {other:?}"),
    }

    let piano_keysplit = decode(&pack, "piano_keysplit");
    assert_eq!(piano_keysplit.slots().len(), assets::VOICE_SLOT_COUNT);
    assert!(matches!(
        piano_keysplit.slot(0),
        Some(VoiceEntry::DirectSound(_))
    ));
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_id_a_voicegroup_references_resolves_to_a_pack_entry() {
    // `assets::audio::voicegroup` docs: decoding never resolves ids. `audio_samples`
    // docs: sample extraction runs first, so only this walk catches a bad reference.
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let mut dangling = Vec::new();
    let mut checked = 0usize;
    for label in EXPECTED_GROUPS {
        let group = decode(&pack, label);
        for (index, slot) in group.slots().iter().enumerate() {
            let id = match slot {
                VoiceEntry::DirectSound(voice) => Some(&voice.sample.0),
                VoiceEntry::ProgrammableWave(voice) => Some(&voice.wave.0),
                VoiceEntry::KeySplit(voice) => Some(&voice.children.0),
                VoiceEntry::Rhythm(voice) => Some(&voice.children.0),
                _ => None,
            };
            if let Some(id) = id {
                checked += 1;
                if pack.raw(id).is_err() {
                    dangling.push(format!("{label}[{index}] -> {id}"));
                }
            }
        }
    }
    assert!(dangling.is_empty(), "dangling references: {dangling:?}");
    assert!(
        checked > 0,
        "the walk checked nothing -- the group set or slots went empty"
    );
}
