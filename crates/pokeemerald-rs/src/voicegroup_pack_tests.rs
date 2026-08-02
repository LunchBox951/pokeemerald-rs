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
use assets::{VoiceEntry, VoiceGroup};

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
    let pack = AssetPack::load_default().expect("run `cargo xtask extract` first");
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
fn slot_127_is_the_effective_slot_mus_titles_own_midi_selects() {
    // `mus_title.mid` selects instrument 127 on one channel
    // (`crates/assets/src/audio.rs`'s module docs), but `title.inc` itself
    // only declares 89 of the 128 addressable slots -- slot 127 has no
    // `ToneData` in the source at all. The pipeline represents that
    // honestly as `VoiceEntry::Empty` rather than inventing content for
    // it (issue #182's "128-slot normalization").
    let pack = AssetPack::load_default().expect("run `cargo xtask extract` first");
    let title = decode(&pack, "title");
    assert_eq!(title.slot(127), Some(&VoiceEntry::Empty));
    assert!(title.slot(128).is_none());
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn titles_rhythm_slot_resolves_through_rs_drumset_with_its_starting_note_bias() {
    let pack = AssetPack::load_default().expect("run `cargo xtask extract` first");
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
    let pack = AssetPack::load_default().expect("run `cargo xtask extract` first");
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
