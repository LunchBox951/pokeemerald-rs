//! Resolves the voicegroup closure of a set of song roots.
//!
//! Every distinct root is resolved on its own with
//! [`resolve_voice_groups_with_link_order`], because a group is a top-level
//! root for one song and a key-split or rhythm child for another, and the
//! resolver treats the two roles differently (only a root borrows its linked
//! successors' slots). Upstream links one copy of each group, so the results
//! merge slot by slot: an empty slot takes the other result's record, and two
//! different records at one slot fail extraction rather than letting one root
//! win silently.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::parser::{RawKeySplitTable, RawVoiceGroup, VoiceGroupError};
use super::resolve::{
    resolve_voice_groups_with_link_order, voice_group_pack_id, ResolvedVoiceGroup, VoiceSlot,
};
use super::IndexedLinkOrderItem;

/// The groups every root in `root_labels` needs, once each, in label order.
///
/// # Errors
///
/// Returns the resolver's error for a root, [`VoiceGroupError::ConflictingClosureSlot`] when two
/// roots disagree about a shared group, or [`VoiceGroupError::UnresolvedClosureEdge`] when a
/// resolved group names a child the closure lacks.
pub(super) fn resolve_closure(
    root_labels: &[String],
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    link_order: &[IndexedLinkOrderItem],
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    let mut merged: BTreeMap<String, ResolvedVoiceGroup> = BTreeMap::new();
    let distinct_roots: BTreeSet<&String> = root_labels.iter().collect();
    for root in distinct_roots {
        let resolved =
            resolve_voice_groups_with_link_order(root, raw_groups, keysplit_tables, link_order)?;
        for group in resolved {
            match merged.get_mut(&group.label) {
                Some(existing) => merge_group(existing, group, root)?,
                None => {
                    merged.insert(group.label.clone(), group);
                }
            }
        }
    }
    let groups: Vec<ResolvedVoiceGroup> = merged.into_values().collect();
    check_closed(&groups)?;
    Ok(groups)
}

/// Fills `existing`'s empty slots from `incoming`, a resolution of the same
/// group reached from `root`.
fn merge_group(
    existing: &mut ResolvedVoiceGroup,
    incoming: ResolvedVoiceGroup,
    root: &str,
) -> Result<(), VoiceGroupError> {
    for (slot, (kept, new)) in existing.slots.iter_mut().zip(incoming.slots).enumerate() {
        if new == VoiceSlot::Empty || *kept == new {
            continue;
        }
        if *kept != VoiceSlot::Empty {
            return Err(VoiceGroupError::ConflictingClosureSlot {
                group: existing.label.clone(),
                slot,
                root: root.to_owned(),
            });
        }
        *kept = new;
    }
    Ok(())
}

fn check_closed(groups: &[ResolvedVoiceGroup]) -> Result<(), VoiceGroupError> {
    let held: BTreeSet<String> = groups
        .iter()
        .map(|group| voice_group_pack_id(&group.label))
        .collect();
    for group in groups {
        for slot in &group.slots {
            let (VoiceSlot::KeySplit { children_id, .. } | VoiceSlot::Rhythm { children_id }) =
                slot
            else {
                continue;
            };
            if !held.contains(children_id) {
                return Err(VoiceGroupError::UnresolvedClosureEdge {
                    group: group.label.clone(),
                    target: children_id.clone(),
                });
            }
        }
    }
    Ok(())
}

/// The pack id of every `DirectSound` and programmable-wave sample `groups` reference.
pub(super) fn referenced_sample_ids(groups: &[ResolvedVoiceGroup]) -> BTreeSet<String> {
    groups
        .iter()
        .flat_map(|group| &group.slots)
        .filter_map(|slot| match slot {
            VoiceSlot::DirectSound { sample_id, .. } => Some(sample_id.clone()),
            VoiceSlot::ProgrammableWave { wave_id, .. } => Some(wave_id.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests;
