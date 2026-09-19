//! Real-pack pinning for `xtask::extract::voicegroups`: decodes real `cargo
//! xtask extract` output through [`assets::VoiceGroup::decode`], checking it
//! agrees byte-for-byte with `xtask`'s independently maintained encoder (its
//! module docs). See `crate::lib`'s module docs for why this lives here.

use assets::pack::AssetPack;
use assets::{Envelope, VoiceEntry, VoiceGroup};

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

fn decode(pack: &AssetPack, label: &str) -> VoiceGroup {
    let bytes = pack
        .raw(&format!("audio/voicegroup/{label}"))
        .unwrap_or_else(|e| panic!("audio/voicegroup/{label} should be in the pack: {e}"));
    VoiceGroup::decode(bytes)
        .unwrap_or_else(|e| panic!("audio/voicegroup/{label} should decode cleanly: {e}"))
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
fn titles_rhythm_slot_resolves_through_rs_drumset_with_its_starting_note_bias() {
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
    for index in 0..RS_DRUMSET_FIRST_DECLARED_SLOT {
        assert_eq!(
            rs_drumset.slot(index),
            Some(&VoiceEntry::Empty),
            "slot {index} should be the starting_note bias's leading gap"
        );
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

            assert_eq!(key_split.starting_note, 36);
            assert_eq!(key_split.table().len(), PIANO_KEYSPLIT_TABLE_LEN);
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
