//! Voicegroup resolver + 128-slot normalization (S-4, issue #182, `#115`
//! child 3): resolves every voice `MUS_TITLE` uses -- its own voicegroup
//! plus every key-split/rhythm child it transitively references -- into
//! `crates/assets/src/audio/voicegroup.rs`'s backend-neutral schema, and
//! emits each as its own `audio/voicegroup/<label>` pack entry.
//!
//! # Why `MUS_TITLE`
//!
//! `MUS_TITLE`'s voicegroup is `voicegroup_title`
//! (`pokeemerald/sound/songs/midi/midi.cfg`'s `mus_title.mid: ... -G_title`
//! entry feeds `tools/mid2agb`'s `-G` option, which
//! `pokeemerald/tools/mid2agb/agb.cpp` turns into the symbol
//! `voicegroup_title` -- matching `pokeemerald/sound/voicegroups/title.inc`'s
//! own `voice_group title` declaration), *not* a numbered
//! `voicegroup127` -- no such symbol exists anywhere in the reference
//! checkout. `voicegroup_title` itself declares only 90 of the 128
//! addressable slots; slot 127 (the one `mus_title.mid`'s own MIDI source
//! selects on one channel -- see `crates/assets/src/audio.rs`'s module
//! docs) has no `ToneData` in the source at all. This pipeline represents
//! that honestly as `assets::audio::voicegroup::VoiceEntry::Empty`
//! rather than inventing content for it -- see `resolve::pad_to_128`.
//!
//! # Pipeline
//!
//! 1. [`build_label_index`] reads and fully parses (`parser::parse_voice_group`)
//!    every `.inc` file under `sound/voicegroups/` (all three levels: the
//!    ~180 top-level groups plus `drumsets/`/`keysplits/`), keyed by each
//!    file's own declared label -- this is what lets a `voice_keysplit`/
//!    `voice_keysplit_all` reference (which names a label, not a path) be
//!    resolved, and lets a reference to a label nothing declares fail
//!    closed as [`parser::VoiceGroupError::DanglingVoiceGroupReference`]
//!    rather than silently doing nothing.
//! 2. `sound/keysplit_tables.inc` is parsed once
//!    ([`parser::parse_keysplit_tables`]) into every `keysplit` block.
//! 3. [`resolve::resolve_voice_groups`] walks from `"title"`, cycle-safe
//!    (a currently-being-resolved stack, checked before any recursive
//!    call) and rejecting a key-split/rhythm child that itself carries
//!    further indirection (upstream's own single-level limit -- see that
//!    module's docs), producing one fully-linked
//!    [`resolve::ResolvedVoiceGroup`] per distinct label reached.
//! 4. Each resolved group is [`encode::encode_voice_group`]d to
//!    `crates/assets/src/audio/voicegroup.rs`'s exact wire shape (this
//!    crate cannot depend on that one -- see `encode`'s module docs) and
//!    pushed as a [`crate::extract::pack::PackKind::Raw`] entry under
//!    `audio/voicegroup/<label>`.
//!
//! # Scope: `MUS_TITLE`'s own dependency tree only
//!
//! Only `title` and the groups reachable from it are emitted (seven total
//! in the current reference checkout: `title`, `rs_drumset`,
//! `piano_keysplit`, `strings_keysplit`, `trumpet_keysplit`,
//! `tuba_keysplit`, `french_horn_keysplit`) -- not the other ~188 `.inc`
//! files under `sound/voicegroups/`, which stay `pending` in the coverage
//! ledger. [`build_label_index`] still reads and fully parses all of them
//! (needed to tell a real dangling reference from one merely outside this
//! slice's scope), so a malformed file anywhere in the tree fails
//! extraction even if `title` never reaches it -- a deliberate fail-closed
//! choice given the reference checkout is a pinned, already-building C
//! project (`./init.sh`), so this is not expected to fire in practice.
//!
//! # Samples: ids only, no payload
//!
//! [`resolve`]'s module docs cover the `SampleId` derivation scheme. No
//! `audio/sample/*` pack entry exists yet -- normalizing the actual sample
//! payloads is issue `#183`, a separate `#115` child.

mod encode;
mod error;
mod parser;
mod resolve;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::pack::{PackEntry, PackKind, PackWriter};
use super::{read_text, ExtractError};
use parser::RawVoiceGroup;

pub(crate) use error::VoiceGroupError;

/// The maximum number of slots any voicegroup may declare -- every raw
/// `VOICE`/key-split-table command byte addresses a slot in `0..=127`.
/// Duplicated from `crates/assets/src/audio/voicegroup.rs`'s
/// `VOICE_SLOT_COUNT` rather than imported (this crate never depends on
/// `crates/assets` -- see `crate::extract::pack`'s module docs).
pub(super) const VOICE_SLOT_COUNT: usize = 128;

/// `MUS_TITLE`'s own voicegroup label -- see the module docs' "Why
/// `MUS_TITLE`".
const TOP_LEVEL_LABEL: &str = "title";

/// Recursively collect every `*.inc` file under `dir`, sorted by full path
/// (deterministic regardless of `read_dir`'s unspecified order -- mirrors
/// `super::collect_pngs_sorted`, duplicated rather than shared since that
/// helper is PNG-specific).
fn collect_inc_files_sorted(dir: &Path) -> Result<Vec<PathBuf>, ExtractError> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(&path, out)?;
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("inc"))
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, &mut out).map_err(|e| ExtractError::ReadFailed(dir.to_path_buf(), e.to_string()))?;
    out.sort();
    Ok(out)
}

/// Read and fully parse every voicegroup `.inc` file under
/// `sound/voicegroups/`, keyed by each file's own declared label. See the
/// module docs' "Scope" section for why this reads the whole tree rather
/// than only `title`'s reachable set.
fn build_label_index(upstream: &Path) -> Result<HashMap<String, RawVoiceGroup>, ExtractError> {
    let dir = upstream.join("sound/voicegroups");
    let mut raw_groups: HashMap<String, RawVoiceGroup> = HashMap::new();
    let mut label_paths: HashMap<String, PathBuf> = HashMap::new();

    for path in collect_inc_files_sorted(&dir)? {
        let text = read_text(&path)?;
        let raw = parser::parse_voice_group(&text)
            .map_err(|e| ExtractError::VoiceGroupFile(path.clone(), e))?;
        if let Some(first_path) = label_paths.insert(raw.label.clone(), path.clone()) {
            return Err(ExtractError::DuplicateVoiceGroupLabel {
                label: raw.label,
                first_path,
                second_path: path,
            });
        }
        raw_groups.insert(raw.label.clone(), raw);
    }
    Ok(raw_groups)
}

/// Extract `MUS_TITLE`'s voicegroup and every group it transitively
/// references (see the module docs).
///
/// # Errors
///
/// [`ExtractError::VoiceGroupFile`] if a `.inc` source under
/// `sound/voicegroups/` (or `keysplit_tables.inc`) fails to parse;
/// [`ExtractError::DuplicateVoiceGroupLabel`] if two `.inc` files declare
/// the same label; [`ExtractError::VoiceGroup`] for any resolution failure
/// (dangling reference, cycle, nested indirection, an over-long group/table
/// -- see [`parser::VoiceGroupError`]'s variants); [`ExtractError::ReadFailed`]
/// if a source file can't be read; [`ExtractError::Pack`] if assembling the
/// pack entries fails (an internal-bug case, since every id here is
/// generated by this module).
pub(super) fn extract_voicegroups(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let raw_groups = build_label_index(upstream)?;

    let keysplit_path = upstream.join("sound/keysplit_tables.inc");
    let keysplit_text = read_text(&keysplit_path)?;
    let keysplit_tables = parser::parse_keysplit_tables(&keysplit_text)
        .map_err(|e| ExtractError::VoiceGroupFile(keysplit_path, e))?;

    let groups = resolve::resolve_voice_groups(TOP_LEVEL_LABEL, &raw_groups, &keysplit_tables)
        .map_err(ExtractError::VoiceGroup)?;

    for group in &groups {
        writer.push(PackEntry {
            id: resolve::voice_group_pack_id(&group.label),
            kind: PackKind::Raw,
            payload: encode::encode_voice_group(group),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_label_index, collect_inc_files_sorted, extract_voicegroups, TOP_LEVEL_LABEL,
    };
    use crate::extract::pack::PackWriter;

    // Real-checkout tests: see `crate::extract`'s own test module docs on
    // why these are `#[ignore]`d and how to run them.

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn every_voicegroups_inc_file_parses() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let index = build_label_index(&upstream).expect("every real .inc file should parse");
        // 180 top-level + 10 drumsets + 5 keysplits (see
        // `crates/assets/src/audio/voicegroup.rs`'s module docs, "195 in
        // the reference checkout: 180 at top level plus 15 under
        // `drumsets/`/`keysplits/`").
        assert_eq!(index.len(), 195);
        assert!(index.contains_key(TOP_LEVEL_LABEL));
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn mus_titles_full_dependency_tree_resolves_to_seven_groups() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let raw_groups = build_label_index(&upstream).unwrap();
        let keysplit_text =
            std::fs::read_to_string(upstream.join("sound/keysplit_tables.inc")).unwrap();
        let keysplit_tables = super::parser::parse_keysplit_tables(&keysplit_text).unwrap();

        let groups =
            super::resolve::resolve_voice_groups(TOP_LEVEL_LABEL, &raw_groups, &keysplit_tables)
                .expect("MUS_TITLE's real dependency tree should resolve cleanly");

        let mut labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
        labels.sort_unstable();
        assert_eq!(
            labels,
            [
                "french_horn_keysplit",
                "piano_keysplit",
                "rs_drumset",
                "strings_keysplit",
                "title",
                "trumpet_keysplit",
                "tuba_keysplit",
            ]
        );

        let title = groups.iter().find(|g| g.label == "title").unwrap();
        assert_eq!(title.slots.len(), super::VOICE_SLOT_COUNT);
        // `title.inc` declares only 90 of the 128 slots (see the module
        // docs' "Why MUS_TITLE") -- slot 127 is trailing padding, not
        // invented content.
        assert_eq!(
            *title.slots.last().unwrap(),
            super::resolve::VoiceSlot::Empty
        );

        let rs_drumset = groups.iter().find(|g| g.label == "rs_drumset").unwrap();
        assert_eq!(rs_drumset.slots.len(), super::VOICE_SLOT_COUNT);
        // `drumsets/rs.inc` declares `voice_group rs_drumset, 36` -- the
        // first 36 slots are the `starting_note` bias, not real content.
        for slot in &rs_drumset.slots[0..36] {
            assert_eq!(*slot, super::resolve::VoiceSlot::Empty);
        }
        assert_ne!(rs_drumset.slots[36], super::resolve::VoiceSlot::Empty);
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn extraction_emits_every_expected_voicegroup_pack_entry() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let mut writer = PackWriter::new();
        extract_voicegroups(&upstream, &mut writer)
            .expect("extraction should succeed against a real checkout");
        assert_eq!(writer.len(), 7);
    }

    #[test]
    fn collect_inc_files_sorted_rejects_missing_dir() {
        let err = collect_inc_files_sorted(std::path::Path::new("/does/not/exist")).unwrap_err();
        assert!(matches!(err, crate::extract::ExtractError::ReadFailed(..)));
    }
}
