//! Extracts the voicegroups every checkpoint song needs into the asset pack.
//!
//! `sound/songs/midi/midi.cfg` assigns each manifest song a voicegroup (see
//! `super::midi::manifest_voicegroup_labels`). A root group may declare fewer
//! than the 128 addressable slots: `sound/voice_groups.inc` links the next
//! group immediately after it, and the unchecked lookup in
//! `src/m4a_1.s::ply_voice` reads the undeclared slots from that adjacent
//! data. Each root therefore borrows known linked-successor slots. A foreign
//! include, missing root, final root, or exhausted successor stops the
//! borrowing and leaves the remaining slots empty `(behavioral-fidelity)`.
//!
//! All voicegroup sources are parsed and indexed, so malformed files and
//! duplicate labels fail extraction. [`closure`] resolves every distinct root
//! and its transitive key-split and rhythm children, rejecting dangling
//! labels, key-split tables, and closure edges; exactly those groups are
//! emitted, in label order, and the sample ids they reference are returned
//! for `super::audio_samples` to extract.

mod closure;
mod encode;
mod error;
pub(crate) mod parser;
mod resolve;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::{midi, read_text, ExtractError};
use pack_format::PackWriter;
use parser::RawVoiceGroup;

pub(crate) use error::VoiceGroupError;
pub(super) use resolve::{
    DIRECT_SOUND_ID_PREFIX, DIRECT_SOUND_SYMBOL_PREFIX, PROGRAMMABLE_WAVE_ID_PREFIX,
    PROGRAMMABLE_WAVE_SYMBOL_PREFIX,
};

/// Number of addressable slots in the asset-pack voicegroup schema, owned by
/// `crates/assets/src/audio/voicegroup.rs`.
pub(super) use assets::VOICE_SLOT_COUNT;

/// [`index_voicegroup_sources`]'s return shape: every parsed group keyed by
/// its declared label, alongside the path (relative to `sound/voicegroups/`,
/// forward-slashed) each one was parsed from, keyed the same way -- see that
/// function's own docs for why both maps are needed. `groups_by_label` is
/// crate-visible: `gen_rom_profile::audio` reuses this same index to plan
/// the ROM-side voicegroup layout from the identical parsed source of truth.
#[derive(Debug)]
pub(crate) struct VoiceGroupSourceIndex {
    pub(crate) groups_by_label: HashMap<String, RawVoiceGroup>,
    labels_by_relative_path: HashMap<String, String>,
}

/// One entry of `sound/voice_groups.inc`'s linked order: a parsed voicegroup
/// (by label) or a foreign include, which bounds successor borrowing and
/// predecessor aliasing alike.
pub(super) enum IndexedLinkOrderItem {
    VoiceGroup(String),
    ForeignInclude,
}

fn discover_voicegroup_sources(dir: &Path) -> Result<Vec<PathBuf>, ExtractError> {
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

/// Reads and fully parses every voicegroup `.inc` file under
/// `sound/voicegroups/`, keyed by declared label, alongside each file's path
/// relative to `sound/voicegroups/` keyed the same way (what
/// [`index_link_order`] needs to resolve `sound/voice_groups.inc`'s
/// path-based `.include` targets to labels).
pub(crate) fn index_voicegroup_sources(
    upstream: &Path,
) -> Result<VoiceGroupSourceIndex, ExtractError> {
    let source_root = upstream.join("sound/voicegroups");
    let mut groups_by_label = HashMap::new();
    let mut source_paths_by_label = HashMap::new();
    let mut labels_by_relative_path = HashMap::new();

    for path in discover_voicegroup_sources(&source_root)? {
        let text = read_text(&path)?;
        let raw = parser::parse_voice_group(&text)
            .map_err(|e| ExtractError::VoiceGroupFile(path.clone(), e))?;
        if let Some(first_path) = source_paths_by_label.insert(raw.label.clone(), path.clone()) {
            return Err(ExtractError::DuplicateVoiceGroupLabel {
                label: raw.label,
                first_path,
                second_path: path,
            });
        }
        let relative = path
            .strip_prefix(&source_root)
            .expect("discovered voicegroup source must be under its source root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        labels_by_relative_path.insert(relative, raw.label.clone());
        groups_by_label.insert(raw.label.clone(), raw);
    }
    Ok(VoiceGroupSourceIndex {
        groups_by_label,
        labels_by_relative_path,
    })
}

/// Resolves `sound/voice_groups.inc` into the linked order of known groups and
/// foreign-include barriers: successors fill an under-declared top-level group,
/// and a biased group aliases the records physically preceding it.
fn index_link_order(
    upstream: &Path,
    labels_by_relative_path: &HashMap<String, String>,
) -> Result<Vec<IndexedLinkOrderItem>, ExtractError> {
    let path = upstream.join("sound/voice_groups.inc");
    let text = read_text(&path)?;

    let mut link_order = Vec::new();
    for item in parser::parse_link_order(&text) {
        match item {
            parser::LinkOrderItem::VoiceGroup(relative) => {
                let label = labels_by_relative_path.get(&relative).ok_or_else(|| {
                    ExtractError::VoiceGroup(VoiceGroupError::UnindexedLinkOrderFile(
                        relative.clone(),
                    ))
                })?;
                link_order.push(IndexedLinkOrderItem::VoiceGroup(label.clone()));
            }
            parser::LinkOrderItem::Foreign => link_order.push(IndexedLinkOrderItem::ForeignInclude),
        }
    }
    Ok(link_order)
}

/// The labels linked directly after `root_label`, up to the first foreign
/// include; empty when the root is unlinked or last.
#[cfg(test)]
fn link_order_successors(
    upstream: &Path,
    root_label: &str,
    labels_by_relative_path: &HashMap<String, String>,
) -> Result<Vec<String>, ExtractError> {
    let link_order = index_link_order(upstream, labels_by_relative_path)?;
    let Some(index) = link_order.iter().position(
        |item| matches!(item, IndexedLinkOrderItem::VoiceGroup(label) if label == root_label),
    ) else {
        return Ok(Vec::new());
    };
    Ok(link_order
        .into_iter()
        .skip(index + 1)
        .map_while(|item| match item {
            IndexedLinkOrderItem::VoiceGroup(label) => Some(label),
            IndexedLinkOrderItem::ForeignInclude => None,
        })
        .collect())
}

/// Extracts the voicegroup closure of every manifest song and returns the
/// pack id of every sample those groups reference.
pub(super) fn extract_voicegroups(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<BTreeSet<String>, ExtractError> {
    let root_labels = midi::manifest_voicegroup_labels(upstream)?;
    extract_voicegroup_closure(upstream, &root_labels, writer)
}

fn extract_voicegroup_closure(
    upstream: &Path,
    root_labels: &[String],
    writer: &mut PackWriter,
) -> Result<BTreeSet<String>, ExtractError> {
    let source_index = index_voicegroup_sources(upstream)?;
    let link_order = index_link_order(upstream, &source_index.labels_by_relative_path)?;

    let keysplit_path = upstream.join("sound/keysplit_tables.inc");
    let keysplit_text = read_text(&keysplit_path)?;
    let keysplit_tables = parser::parse_keysplit_tables_with_adjacency(&keysplit_text)
        .map_err(|e| ExtractError::VoiceGroupFile(keysplit_path, e))?;

    let resolved_groups = closure::resolve_closure(
        root_labels,
        &source_index.groups_by_label,
        &keysplit_tables,
        &link_order,
    )
    .map_err(ExtractError::VoiceGroup)?;

    for group in &resolved_groups {
        writer.push(pack_format::raw_entry(
            resolve::voice_group_pack_id(&group.label),
            encode::encode_voice_group(group).map_err(ExtractError::VoiceGroup)?,
        ));
    }
    Ok(closure::referenced_sample_ids(&resolved_groups))
}

#[cfg(test)]
mod tests;
