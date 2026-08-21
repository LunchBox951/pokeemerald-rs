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
//! checkout. `voicegroup_title` itself declares only 89 of the 128
//! addressable slots (89 `voice_*` body lines under `title.inc`'s
//! `voice_group title` header), and this pipeline materializes the
//! undeclared tail as `assets::audio::voicegroup::VoiceEntry::Empty`
//! (`resolve::pad_to_128`).
//!
//! **Modeled link-adjacency read (issue #201):** slot 127 -- the one
//! `mus_title.mid`'s own MIDI source selects on one channel (see
//! `crates/assets/src/audio.rs`'s module docs) -- is undeclared *in
//! `title.inc`*, but not silent upstream. `sound/voice_groups.inc:66-67`
//! links `intro.inc` contiguously after `title.inc` (whose 89 entries are
//! 1068 bytes, already 4-aligned), and the mixer's unchecked
//! `voicegroup + voice * 12` fetch (`src/m4a_1.s`, `ply_voice`) resolves
//! slot 127 to offset 1524 = byte 456 of `voicegroup_intro` = its entry 38,
//! `voice_square_1 60, 0, 0, 2, 0, 0, 15, 0` -- a real playable CGB voice.
//! [`link_order_successors`] parses `sound/voice_groups.inc`'s own linked
//! order to learn what follows `title`'s file, and
//! `resolve::resolve_voice_groups_with_link_successors` materializes
//! `title`'s undeclared tail (slots 89..=127) from that successor's own
//! entries instead of [`resolve::VoiceSlot::Empty`] -- see that function's
//! module docs' "Link adjacency" section for the full mechanism, including
//! the fail-closed case (a top-level group last in the linked order, or
//! whose successors run out first, still gets `Empty` padding: this
//! pipeline cannot know what bytes, if any, genuinely follow, so silence is
//! the honest fallback, not a guess). This is modeled only for `title`
//! itself, never for a key-split/rhythm child it references (e.g.
//! `rs_drumset`'s own `starting_note` under-range and any child's trailing
//! over-range stay `Empty` -- a different mechanism, the keysplit table
//! itself, not file adjacency, and out of scope here) `(behavioral-fidelity)`.
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
//! 3. [`link_order_successors`] parses `sound/voice_groups.inc`
//!    ([`parser::parse_link_order`]) and, using step 1's own path-to-label
//!    map, returns the labels linked immediately after `"title"`'s file --
//!    empty if none (see the "Modeled link-adjacency read" section above).
//! 4. [`resolve::resolve_voice_groups_with_link_successors`] walks from
//!    `"title"`, cycle-safe (a currently-being-resolved stack, checked
//!    before any recursive call) and rejecting a key-split/rhythm child
//!    that itself carries further indirection (upstream's own single-level
//!    limit -- see that module's docs), producing one fully-linked
//!    [`resolve::ResolvedVoiceGroup`] per distinct label reached --
//!    `"title"`'s own undeclared tail filled from step 3's successors first
//!    (issue #201), any remainder left `Empty`.
//! 5. Each resolved group is [`encode::encode_voice_group`]d to
//!    `crates/assets/src/audio/voicegroup.rs`'s exact wire shape (this
//!    crate cannot depend on that one -- see `encode`'s module docs) and
//!    pushed as a [`pack_format::EntryKind::Raw`] entry under
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
//! [`resolve`]'s module docs cover the `SampleId` derivation scheme: an
//! `audio/sample/direct-sound/<name>` / `audio/sample/programmable-wave/<nn>`
//! id per referenced sample, mirroring `xtask::extract::audio_samples`'s
//! own ids verbatim so the two halves link up. Extracting those payloads is
//! issue `#183`, a separate `#115` child -- this module writes only the
//! references.

mod encode;
mod error;
mod parser;
mod resolve;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{read_text, ExtractError};
use pack_format::PackWriter;
use parser::RawVoiceGroup;

pub(crate) use error::VoiceGroupError;

/// The maximum number of slots any voicegroup may declare -- every raw
/// `VOICE`/key-split-table command byte addresses a slot in `0..=127`.
/// Duplicated from `crates/assets/src/audio/voicegroup.rs`'s
/// `VOICE_SLOT_COUNT` rather than imported (this crate never depends on
/// `crates/assets` -- see `pack_format`'s module docs).
pub(super) const VOICE_SLOT_COUNT: usize = 128;

/// `MUS_TITLE`'s own voicegroup label -- see the module docs' "Why
/// `MUS_TITLE`".
const TOP_LEVEL_LABEL: &str = "title";

/// [`build_label_index`]'s return shape: every parsed group keyed by its
/// declared label, alongside the path (relative to `sound/voicegroups/`,
/// forward-slashed) each one was parsed from, keyed the same way --
/// see that function's own docs for why both maps are needed.
type LabelIndex = (HashMap<String, RawVoiceGroup>, HashMap<String, String>);

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
///
/// Also returns each file's path *relative to `sound/voicegroups/`*
/// (forward-slashed regardless of host path separator), keyed to its
/// declared label -- what [`link_order_successors`] needs to turn
/// `sound/voice_groups.inc`'s own `.include` targets (paths, not labels)
/// into the label sequence [`resolve::resolve_voice_groups_with_link_successors`]
/// walks. A label need not match its filename (e.g. `drumsets/rs.inc`
/// declares `rs_drumset`), so this mapping can't be reconstructed from
/// `raw_groups` alone.
fn build_label_index(upstream: &Path) -> Result<LabelIndex, ExtractError> {
    let dir = upstream.join("sound/voicegroups");
    let mut raw_groups: HashMap<String, RawVoiceGroup> = HashMap::new();
    let mut label_paths: HashMap<String, PathBuf> = HashMap::new();
    let mut path_labels: HashMap<String, String> = HashMap::new();

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
        let relative = path
            .strip_prefix(&dir)
            .expect("collect_inc_files_sorted only yields paths under dir")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        path_labels.insert(relative, raw.label.clone());
        raw_groups.insert(raw.label.clone(), raw);
    }
    Ok((raw_groups, path_labels))
}

/// Learn `sound/voice_groups.inc`'s own concatenation order for every
/// voicegroup `.inc` file it links (see [`parser::parse_link_order`]), and
/// return the labels immediately following `top_label` in that order.
///
/// Empty if `top_label` isn't linked at all, or is the last voicegroup file
/// linked -- both fail closed to `resolve::pad_to_128`'s ordinary trailing
/// `Empty` pad (see `resolve`'s "Link adjacency" module docs): this
/// pipeline has no way to know what, if anything, follows in the real
/// linked binary in either case.
///
/// # Errors
///
/// [`ExtractError::ReadFailed`] if `sound/voice_groups.inc` can't be read;
/// [`ExtractError::VoiceGroup`] wrapping
/// [`VoiceGroupError::UnindexedLinkOrderFile`] if a linked path names a file
/// `path_labels` (i.e. `build_label_index`'s own directory walk) never saw
/// -- an internal mismatch, not expected against the pinned reference
/// checkout.
fn link_order_successors(
    upstream: &Path,
    top_label: &str,
    path_labels: &HashMap<String, String>,
) -> Result<Vec<String>, ExtractError> {
    let path = upstream.join("sound/voice_groups.inc");
    let text = read_text(&path)?;

    let mut ordered = Vec::new();
    for item in parser::parse_link_order(&text) {
        match item {
            parser::LinkOrderItem::VoiceGroup(relative) => {
                let label = path_labels.get(&relative).ok_or_else(|| {
                    ExtractError::VoiceGroup(VoiceGroupError::UnindexedLinkOrderFile(
                        relative.clone(),
                    ))
                })?;
                ordered.push(Some(label.clone()));
            }
            // A non-voicegroup include is an adjacency barrier (see
            // `parser::LinkOrderItem::Foreign`): successors stop there.
            parser::LinkOrderItem::Foreign => ordered.push(None),
        }
    }

    let Some(index) = ordered
        .iter()
        .position(|label| label.as_deref() == Some(top_label))
    else {
        return Ok(Vec::new());
    };
    Ok(ordered[index + 1..]
        .iter()
        .map_while(Clone::clone)
        .collect())
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
/// -- see [`parser::VoiceGroupError`]'s variants) or an unindexed
/// link-order file (see [`link_order_successors`]);
/// [`ExtractError::ReadFailed`] if a source file can't be read;
/// [`ExtractError::Pack`] if assembling the pack entries fails (an
/// internal-bug case, since every id here is generated by this module).
pub(super) fn extract_voicegroups(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let (raw_groups, path_labels) = build_label_index(upstream)?;
    let link_successors = link_order_successors(upstream, TOP_LEVEL_LABEL, &path_labels)?;

    let keysplit_path = upstream.join("sound/keysplit_tables.inc");
    let keysplit_text = read_text(&keysplit_path)?;
    let keysplit_tables = parser::parse_keysplit_tables(&keysplit_text)
        .map_err(|e| ExtractError::VoiceGroupFile(keysplit_path, e))?;

    let groups = resolve::resolve_voice_groups_with_link_successors(
        TOP_LEVEL_LABEL,
        &raw_groups,
        &keysplit_tables,
        &link_successors,
    )
    .map_err(ExtractError::VoiceGroup)?;

    for group in &groups {
        writer.push(pack_format::raw_entry(
            resolve::voice_group_pack_id(&group.label),
            encode::encode_voice_group(group),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_label_index, collect_inc_files_sorted, extract_voicegroups, link_order_successors,
        TOP_LEVEL_LABEL,
    };
    use pack_format::PackWriter;

    // Real-checkout tests: see `crate::extract`'s own test module docs on
    // why these are `#[ignore]`d and how to run them.

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn every_voicegroups_inc_file_parses() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let (index, path_labels) =
            build_label_index(&upstream).expect("every real .inc file should parse");
        // 180 top-level + 10 drumsets + 5 keysplits (see
        // `crates/assets/src/audio/voicegroup.rs`'s module docs, "195 in
        // the reference checkout: 180 at top level plus 15 under
        // `drumsets/`/`keysplits/`").
        assert_eq!(index.len(), 195);
        assert_eq!(path_labels.len(), 195);
        assert!(index.contains_key(TOP_LEVEL_LABEL));
        assert_eq!(
            path_labels.get("title.inc").map(String::as_str),
            Some("title")
        );
        assert_eq!(
            path_labels.get("drumsets/rs.inc").map(String::as_str),
            Some("rs_drumset")
        );
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn title_is_immediately_followed_by_intro_in_the_real_link_order() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let (_raw_groups, path_labels) = build_label_index(&upstream).unwrap();
        let successors = link_order_successors(&upstream, TOP_LEVEL_LABEL, &path_labels).unwrap();
        // `sound/voice_groups.inc:66-67` -- see the module docs' "Modeled
        // link-adjacency read".
        assert_eq!(successors.first().map(String::as_str), Some("intro"));
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn mus_titles_full_dependency_tree_resolves_to_seven_groups() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let (raw_groups, path_labels) = build_label_index(&upstream).unwrap();
        let link_successors =
            link_order_successors(&upstream, TOP_LEVEL_LABEL, &path_labels).unwrap();
        let keysplit_text =
            std::fs::read_to_string(upstream.join("sound/keysplit_tables.inc")).unwrap();
        let keysplit_tables = super::parser::parse_keysplit_tables(&keysplit_text).unwrap();

        let groups = super::resolve::resolve_voice_groups_with_link_successors(
            TOP_LEVEL_LABEL,
            &raw_groups,
            &keysplit_tables,
            &link_successors,
        )
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
        // `title.inc` declares only 89 of the 128 slots (see the module
        // docs' "Why MUS_TITLE"), but slots 89..=127 are no longer invented
        // silence: they are `voicegroup_intro`'s own entries 0..=38, read
        // through the modeled link-adjacency (issue #201). Slot 89 ==
        // intro's entry 0, `voice_keysplit_all voicegroup_rs_drumset`
        // (`sound/voicegroups/intro.inc:2`) -- already one of `title`'s own
        // rhythm children, so no new group is emitted for it. Slot 127 ==
        // intro's entry 38, `voice_square_1 60, 0, 0, 2, 0, 0, 15, 0`
        // (`sound/voicegroups/intro.inc:40`) -- a real playable CGB voice.
        assert_eq!(
            title.slots[89],
            super::resolve::VoiceSlot::Rhythm {
                children_id: "audio/voicegroup/rs_drumset".to_owned(),
            }
        );
        assert_eq!(
            *title.slots.last().unwrap(),
            super::resolve::VoiceSlot::Square1 {
                base_key: 60,
                length: 0,
                sweep: 0,
                duty: 2,
                envelope: super::parser::Envelope {
                    attack: 0,
                    decay: 0,
                    sustain: 15,
                    release: 0,
                },
                fixed_rate: false,
            }
        );

        let rs_drumset = groups.iter().find(|g| g.label == "rs_drumset").unwrap();
        assert_eq!(rs_drumset.slots.len(), super::VOICE_SLOT_COUNT);
        // `drumsets/rs.inc` declares `voice_group rs_drumset, 36` with 29
        // body lines -- the first 36 slots are the `starting_note` bias,
        // slots 36..=64 are the real entries, and everything past 64 is
        // trailing padding. `rs_drumset` is reached only as a key-split/
        // rhythm child, never a top-level group itself, so it keeps this
        // plain padding regardless of link order (see `resolve`'s "Link
        // adjacency" module docs).
        for slot in &rs_drumset.slots[0..36] {
            assert_eq!(*slot, super::resolve::VoiceSlot::Empty);
        }
        for slot in &rs_drumset.slots[36..=64] {
            assert_ne!(*slot, super::resolve::VoiceSlot::Empty);
        }
        for slot in &rs_drumset.slots[65..] {
            assert_eq!(*slot, super::resolve::VoiceSlot::Empty);
        }
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn extraction_emits_every_expected_voicegroup_pack_entry() {
        assert!(super::super::upstream_present(), "run ./init.sh first");
        let upstream = super::super::repo_root().join("pokeemerald");
        let mut writer = PackWriter::new();
        extract_voicegroups(&upstream, &mut writer)
            .expect("extraction should succeed against a real checkout");
        // Still exactly `title` plus its six key-split/rhythm children --
        // `intro` itself is never emitted as its own pack entry: only a
        // handful of its entries are borrowed into `title`'s overflow tail
        // (see the module docs' "Modeled link-adjacency read").
        assert_eq!(writer.len(), 7);
    }

    #[test]
    fn collect_inc_files_sorted_rejects_missing_dir() {
        let err = collect_inc_files_sorted(std::path::Path::new("/does/not/exist")).unwrap_err();
        assert!(matches!(err, crate::extract::ExtractError::ReadFailed(..)));
    }

    /// Crafted fixture (the real checkout has no such collision): two
    /// `.inc` files declaring the same label would make
    /// [`build_label_index`]'s map -- and so every `voice_keysplit`
    /// reference resolved through it -- depend on which file `read_dir`
    /// happened to yield last. That fails closed instead.
    #[test]
    fn two_inc_files_declaring_the_same_label_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-voicegroup-duplicate-label-test-{}",
            std::process::id()
        ));
        let dir = root.join("sound/voicegroups");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).unwrap();
        let body = "voice_group demo\n\tvoice_square_1 60, 0, 0, 2, 0, 0, 15, 0\n";
        std::fs::write(dir.join("a.inc"), body).unwrap();
        std::fs::write(dir.join("b.inc"), body).unwrap();

        let err = build_label_index(&root).unwrap_err();
        std::fs::remove_dir_all(&root).unwrap();

        // `collect_inc_files_sorted` sorts by full path, so `a.inc` is
        // always the first-discovered file and `b.inc` the collision.
        match err {
            crate::extract::ExtractError::DuplicateVoiceGroupLabel {
                label,
                first_path,
                second_path,
            } => {
                assert_eq!(label, "demo");
                assert_eq!(first_path, dir.join("a.inc"));
                assert_eq!(second_path, dir.join("b.inc"));
            }
            other => panic!("expected DuplicateVoiceGroupLabel, got {other:?}"),
        }
    }

    /// Synthetic fixture (no real `./init.sh` checkout needed --
    /// [`link_order_successors`] only reads `sound/voice_groups.inc` and
    /// consults an already-built `path_labels` map): a group followed by
    /// another in a hand-written link order, mirroring
    /// `sound/voice_groups.inc:66-67`'s `title.inc`/`intro.inc` shape at
    /// arm's length.
    #[test]
    fn link_order_successors_returns_every_file_after_top_label_in_order() {
        let mut path_labels = std::collections::HashMap::new();
        path_labels.insert("a.inc".to_owned(), "a".to_owned());
        path_labels.insert("b.inc".to_owned(), "b".to_owned());
        path_labels.insert("c.inc".to_owned(), "c".to_owned());
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-voicegroup-link-order-test-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(root.join("sound")).unwrap();
        std::fs::write(
            root.join("sound/voice_groups.inc"),
            ".include \"sound/voicegroups/a.inc\"\n.include \"sound/voicegroups/b.inc\"\n.include \"sound/voicegroups/c.inc\"\n",
        )
        .unwrap();

        let successors = link_order_successors(&root, "a", &path_labels).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(successors, vec!["b".to_owned(), "c".to_owned()]);
    }

    /// The fail-closed case the module docs promise: a top-level group last
    /// in the linker's own order has no successor at all -- upstream would
    /// read into whatever bytes follow `sound/voice_groups.inc`'s very last
    /// linked file, which this pipeline cannot know, so an empty list (and
    /// so `resolve`'s ordinary `Empty` trailing pad) is the honest answer,
    /// not an error.
    #[test]
    fn link_order_successors_is_empty_when_top_label_is_last() {
        let mut path_labels = std::collections::HashMap::new();
        path_labels.insert("a.inc".to_owned(), "a".to_owned());
        path_labels.insert("b.inc".to_owned(), "b".to_owned());
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-voicegroup-link-order-last-test-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(root.join("sound")).unwrap();
        std::fs::write(
            root.join("sound/voice_groups.inc"),
            ".include \"sound/voicegroups/a.inc\"\n.include \"sound/voicegroups/b.inc\"\n",
        )
        .unwrap();

        let successors = link_order_successors(&root, "b", &path_labels).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(successors, Vec::<String>::new());
    }

    /// A foreign include (e.g. `sound/cry_tables.inc`) is an adjacency
    /// *barrier*: the group before it is followed in memory by that
    /// table's bytes, so the successor list must stop there -- same
    /// fail-closed `Empty` padding as being last -- and a group *after*
    /// the barrier still gets its own onward successors.
    #[test]
    fn link_order_successors_stop_at_a_foreign_include_barrier() {
        let mut path_labels = std::collections::HashMap::new();
        path_labels.insert("a.inc".to_owned(), "a".to_owned());
        path_labels.insert("b.inc".to_owned(), "b".to_owned());
        path_labels.insert("c.inc".to_owned(), "c".to_owned());
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-voicegroup-link-order-barrier-test-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(root.join("sound")).unwrap();
        std::fs::write(
            root.join("sound/voice_groups.inc"),
            ".include \"sound/voicegroups/a.inc\"\n.include \"sound/cry_tables.inc\"\n.include \"sound/voicegroups/b.inc\"\n.include \"sound/voicegroups/c.inc\"\n",
        )
        .unwrap();

        let before_barrier = link_order_successors(&root, "a", &path_labels).unwrap();
        let after_barrier = link_order_successors(&root, "b", &path_labels).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(before_barrier, Vec::<String>::new());
        assert_eq!(after_barrier, vec!["c".to_owned()]);
    }

    /// Also fail-closed, not an error: `top_label` never appearing in the
    /// linked order at all (e.g. a synthetic top-level group with no
    /// corresponding `.include` line) gets the same empty answer as being
    /// last -- there is no "next file" to read from either way.
    #[test]
    fn link_order_successors_is_empty_when_top_label_is_unlinked() {
        let path_labels = std::collections::HashMap::new();
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-voicegroup-link-order-unlinked-test-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(root.join("sound")).unwrap();
        std::fs::write(root.join("sound/voice_groups.inc"), "").unwrap();

        let successors = link_order_successors(&root, "solo", &path_labels).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(successors, Vec::<String>::new());
    }

    /// A linked path with no matching parsed label -- a mismatch between
    /// `sound/voice_groups.inc`'s own `.include` list and
    /// [`build_label_index`]'s directory walk that should never happen
    /// against the pinned reference checkout, but fails closed rather than
    /// silently dropping the file from the link order.
    #[test]
    fn link_order_successors_rejects_an_unindexed_linked_file() {
        let path_labels = std::collections::HashMap::new();
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-voicegroup-link-order-unindexed-test-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(root.join("sound")).unwrap();
        std::fs::write(
            root.join("sound/voice_groups.inc"),
            ".include \"sound/voicegroups/ghost.inc\"\n",
        )
        .unwrap();

        let err = link_order_successors(&root, "ghost", &path_labels).unwrap_err();
        std::fs::remove_dir_all(&root).unwrap();

        match err {
            crate::extract::ExtractError::VoiceGroup(
                super::VoiceGroupError::UnindexedLinkOrderFile(path),
            ) => assert_eq!(path, "ghost.inc"),
            other => panic!("expected UnindexedLinkOrderFile, got {other:?}"),
        }
    }
}
