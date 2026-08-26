//! Real-pack pinning for `xtask::extract::voicegroups` (S-4, issue #182,
//! `#115` child 3): loads the actual `cargo xtask extract` output and
//! decodes its `audio/voicegroup/*` entries with the real
//! [`assets::VoiceGroup::decode`] -- proof the extraction pipeline's
//! hand-rolled encoder (duplicated there rather than depending on this
//! crate -- see `xtask::extract::voicegroups::encode`'s module docs) agrees
//! byte-for-byte with this crate's schema, not just with itself.
//!
//! See `crate::lib`'s module docs on why this lives here rather than in
//! `crates/xtask`/`crates/assets` directly.

use assets::pack::AssetPack;
use assets::{Envelope, VoiceEntry, VoiceGroup};

/// `MUS_TITLE`'s voicegroup plus every key-split/rhythm child it
/// transitively references (see `xtask::extract::voicegroups`'s module
/// docs for why exactly these seven).
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
    // `mus_title.mid` selects instrument 127 on one channel
    // (`crates/assets/src/audio.rs`'s module docs), and `title.inc` only
    // declares 89 of the 128 addressable slots. Upstream this is NOT
    // silence: `sound/voice_groups.inc:66-67` lays `intro.inc` contiguously
    // after `title.inc`, and the mixer's unchecked `voicegroup + voice * 12`
    // fetch (`ply_voice`, `src/m4a_1.s`) resolves a slot in 89..=127 into
    // `voicegroup_intro`'s own entries 0..=38
    // (`xtask::extract::voicegroups`'s "Modeled link-adjacency read" docs,
    // issue #201). This pins the adjacency mapping itself, not just one
    // slot: slot 89 is intro's entry 0
    // (`sound/voicegroups/intro.inc:2`, `voice_keysplit_all
    // voicegroup_rs_drumset` -- already one of `title`'s own rhythm
    // children, so it resolves to the same `rs_drumset` group rather than a
    // new one), and slot 127 is intro's entry 38
    // (`sound/voicegroups/intro.inc:40`, `voice_square_1 60, 0, 0, 2, 0, 0,
    // 15, 0` -- a real, playable CGB square-1 voice, not fixed-rate).
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let title = decode(&pack, "title");

    match title.slot(89) {
        Some(VoiceEntry::Rhythm(rhythm)) => {
            assert_eq!(rhythm.children.0, "audio/voicegroup/rs_drumset");
        }
        other => panic!("expected slot 89 to be a Rhythm indirection, got {other:?}"),
    }

    assert_eq!(
        title.slot(127),
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
    assert!(title.slot(128).is_none());
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn titles_rhythm_slot_resolves_through_rs_drumset_with_its_starting_note_bias() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let title = decode(&pack, "title");
    match title.slot(0) {
        Some(VoiceEntry::Rhythm(rhythm)) => {
            assert_eq!(rhythm.children.0, "audio/voicegroup/rs_drumset");
        }
        other => panic!("expected title's slot 0 to be a Rhythm indirection, got {other:?}"),
    }

    let rs_drumset = decode(&pack, "rs_drumset");
    // `drumsets/rs.inc` declares `voice_group rs_drumset, 36` -- the
    // leading 36 slots are the starting_note bias, not real content.
    for index in 0..36 {
        assert_eq!(
            rs_drumset.slot(index),
            Some(&VoiceEntry::Empty),
            "slot {index} should be the starting_note bias's leading gap"
        );
    }
    assert_ne!(rs_drumset.slot(36), Some(&VoiceEntry::Empty));
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn titles_key_split_slots_resolve_their_tables_and_children() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let title = decode(&pack, "title");
    match title.slot(1) {
        Some(VoiceEntry::KeySplit(key_split)) => {
            assert_eq!(key_split.starting_note, 36);
            // notes 36..=54 -> 0 (19), 55..=69 -> 1 (15), 70..=90 -> 2 (21),
            // 91..=107 -> 3 (17): 72 entries total.
            assert_eq!(key_split.table().len(), 72);
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
    // The sample lists in `xtask::extract::audio_samples` are
    // hand-maintained while that pass runs *before* the voicegroup pass,
    // and `VoiceGroup::decode` is deliberately structural-only -- so
    // nothing else fails when a resolver change (say, a new borrowed
    // link-adjacency slot, issue #201) starts referencing a sample the
    // lists miss. This walks every slot of every emitted group and
    // requires each referenced id to name a real pack entry.
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
