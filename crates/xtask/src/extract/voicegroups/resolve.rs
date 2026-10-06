//! Resolves parsed voicegroup references (cycle-safe, and rejecting a
//! second level of indirection -- see
//! [`resolve_voice_groups_with_link_order`]) into stable
//! [`pack_format`] ids and normalizes each emitted group to exactly
//! [`super::VOICE_SLOT_COUNT`] slots (see [`pad_to_128`]).
//!
//! # Sample id scheme
//!
//! `DirectSoundWaveData_<name>` maps to
//! `audio/sample/direct-sound/<name>`. `ProgrammableWaveData_<n>` maps to
//! `audio/sample/programmable-wave/<nn>`, with the numeric suffix padded to
//! two digits. A resolved group label maps to `audio/voicegroup/<label>`.
//!
//! # Link adjacency
//!
//! The top-level group's undeclared trailing slots are filled in order from
//! the linked successor groups in the link order from `super::index_link_order`.
//! Borrowed entries use their source group's label for diagnostics and may
//! resolve an indirection child. Indirection-target groups never borrow
//! trailing entries; any unfilled trailing position remains
//! [`VoiceSlot::Empty`].
//!
//! A nonzero `starting_note` aliases the group label that many ToneData
//! records before its first declared one, so slots `0..starting_note` are the
//! physically preceding linked records (see
//! `Resolver::collect_alias_predecessors`). A missing or insufficient
//! predecessor is an error, never silent empty slots.

use std::collections::HashMap;

use super::parser::{
    DirectSoundMode, Envelope, RawKeySplitTable, RawSlot, RawVoiceGroup, VoiceGroupError,
};
use super::{IndexedLinkOrderItem, VOICE_SLOT_COUNT};

const DIRECT_SOUND_SAMPLE_PREFIX: &str = "DirectSoundWaveData_";
const PROGRAMMABLE_WAVE_SAMPLE_PREFIX: &str = "ProgrammableWaveData_";

/// One fully-resolved voicegroup slot: every reference (sample, child
/// group) has been turned into a stable pack id. The shape mirrors
/// `crates/assets`'s `VoiceEntry`, duplicated rather than shared since this
/// crate never depends on `crates/assets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VoiceSlot {
    DirectSound {
        base_key: u8,
        pan: Option<u8>,
        sample_id: String,
        envelope: Envelope,
        mode: DirectSoundMode,
    },
    Square1 {
        base_key: u8,
        length: u8,
        sweep: u8,
        duty: u8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    Square2 {
        base_key: u8,
        length: u8,
        duty: u8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    ProgrammableWave {
        base_key: u8,
        length: u8,
        wave_id: String,
        envelope: Envelope,
        fixed_rate: bool,
    },
    Noise {
        base_key: u8,
        length: u8,
        period: u8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    KeySplit {
        starting_note: u8,
        table: Vec<u8>,
        children_id: String,
    },
    Rhythm {
        children_id: String,
    },
    /// Preserves an unused position in the normalized slot table.
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedVoiceGroup {
    pub label: String,
    pub slots: Vec<VoiceSlot>,
}

/// The asset pack stores every id behind a `u16` byte-length prefix, so an
/// over-long one is rejected here, while `group` still names its source.
fn checked_pack_id(id: String, group: &str) -> Result<String, VoiceGroupError> {
    if u16::try_from(id.len()).is_err() {
        return Err(VoiceGroupError::PackIdTooLong {
            group: group.to_owned(),
            id_len: id.len(),
        });
    }
    Ok(id)
}

fn direct_sound_sample_id(symbol: &str, group: &str) -> Result<String, VoiceGroupError> {
    let id = symbol
        .strip_prefix(DIRECT_SOUND_SAMPLE_PREFIX)
        .filter(|name| !name.is_empty())
        .map(|name| format!("audio/sample/direct-sound/{name}"))
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: symbol.to_owned(),
            expected_prefix: DIRECT_SOUND_SAMPLE_PREFIX,
        })?;
    checked_pack_id(id, group)
}

fn programmable_wave_sample_id(symbol: &str, group: &str) -> Result<String, VoiceGroupError> {
    let suffix = symbol
        .strip_prefix(PROGRAMMABLE_WAVE_SAMPLE_PREFIX)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: symbol.to_owned(),
            expected_prefix: PROGRAMMABLE_WAVE_SAMPLE_PREFIX,
        })?;
    let index: u32 =
        suffix
            .parse()
            .map_err(|_| VoiceGroupError::MalformedProgrammableWaveIndex {
                group: group.to_owned(),
                reference: symbol.to_owned(),
            })?;
    checked_pack_id(format!("audio/sample/programmable-wave/{index:02}"), group)
}

pub(super) fn voice_group_pack_id(label: &str) -> String {
    format!("audio/voicegroup/{label}")
}

/// Appends the trailing [`VoiceSlot::Empty`] positions that complete a group
/// to [`VOICE_SLOT_COUNT`] slots. The caller has already placed the aliased
/// predecessor records and the declared slots.
fn pad_to_128(mut slots: Vec<VoiceSlot>) -> Vec<VoiceSlot> {
    slots.resize_with(VOICE_SLOT_COUNT, || VoiceSlot::Empty);
    slots
}

#[cfg(test)]
pub(super) fn resolve_voice_groups(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    resolve_voice_groups_with_link_successors(top_label, raw_groups, keysplit_tables, &[])
}

/// Resolves with `top_label` followed by exactly `link_successors` as the
/// linked data; no group precedes it.
#[cfg(test)]
pub(super) fn resolve_voice_groups_with_link_successors(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    link_successors: &[String],
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    let link_order: Vec<IndexedLinkOrderItem> =
        std::iter::once(IndexedLinkOrderItem::VoiceGroup(top_label.to_owned()))
            .chain(
                link_successors
                    .iter()
                    .cloned()
                    .map(IndexedLinkOrderItem::VoiceGroup),
            )
            .collect();
    resolve_voice_groups_with_link_order(top_label, raw_groups, keysplit_tables, &link_order)
}

pub(super) fn resolve_voice_groups_with_link_order(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    link_order: &[IndexedLinkOrderItem],
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    Resolver::new(raw_groups, keysplit_tables, link_order).resolve(top_label)
}

fn convert_leaf_slot(raw_slot: &RawSlot, group_label: &str) -> Result<VoiceSlot, VoiceGroupError> {
    Ok(match raw_slot {
        RawSlot::DirectSound {
            base_key,
            pan,
            sample_symbol,
            envelope,
            mode,
        } => VoiceSlot::DirectSound {
            base_key: *base_key,
            pan: *pan,
            sample_id: direct_sound_sample_id(sample_symbol, group_label)?,
            envelope: *envelope,
            mode: *mode,
        },
        RawSlot::Square1 {
            base_key,
            length,
            sweep,
            duty,
            envelope,
            fixed_rate,
        } => VoiceSlot::Square1 {
            base_key: *base_key,
            length: *length,
            sweep: *sweep,
            duty: *duty,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::Square2 {
            base_key,
            length,
            duty,
            envelope,
            fixed_rate,
        } => VoiceSlot::Square2 {
            base_key: *base_key,
            length: *length,
            duty: *duty,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::ProgrammableWave {
            base_key,
            length,
            wave_symbol,
            envelope,
            fixed_rate,
        } => VoiceSlot::ProgrammableWave {
            base_key: *base_key,
            length: *length,
            wave_id: programmable_wave_sample_id(wave_symbol, group_label)?,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::Noise {
            base_key,
            length,
            period,
            envelope,
            fixed_rate,
        } => VoiceSlot::Noise {
            base_key: *base_key,
            length: *length,
            period: *period,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::KeySplit { .. } | RawSlot::Rhythm { .. } => {
            unreachable!("Resolver::resolve_slot handles indirection before leaf conversion")
        }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupRole {
    TopLevel,
    IndirectionTarget,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotOrigin {
    TopLevelGroup,
    BorrowedLinkSuccessor,
    IndirectionTarget,
}

struct Resolver<'a> {
    raw_groups: &'a HashMap<String, RawVoiceGroup>,
    key_split_tables: &'a HashMap<String, RawKeySplitTable>,
    link_order: &'a [IndexedLinkOrderItem],
    resolution_path: Vec<String>,
    emission_order: Vec<String>,
    resolved_groups: HashMap<String, ResolvedVoiceGroup>,
}

impl<'a> Resolver<'a> {
    fn new(
        raw_groups: &'a HashMap<String, RawVoiceGroup>,
        key_split_tables: &'a HashMap<String, RawKeySplitTable>,
        link_order: &'a [IndexedLinkOrderItem],
    ) -> Self {
        Self {
            raw_groups,
            key_split_tables,
            link_order,
            resolution_path: Vec::new(),
            emission_order: Vec::new(),
            resolved_groups: HashMap::new(),
        }
    }

    fn resolve(mut self, top_label: &str) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
        self.resolve_group(top_label, GroupRole::TopLevel)?;
        Ok(self
            .emission_order
            .into_iter()
            .map(|label| {
                self.resolved_groups
                    .remove(&label)
                    .expect("emission order only contains resolved group labels")
            })
            .collect())
    }

    fn resolve_group(&mut self, label: &str, role: GroupRole) -> Result<(), VoiceGroupError> {
        if self.resolved_groups.contains_key(label) {
            return Ok(());
        }
        if self.resolution_path.iter().any(|seen| seen == label) {
            let mut cycle = self.resolution_path.clone();
            cycle.push(label.to_owned());
            return Err(VoiceGroupError::Cycle(cycle));
        }

        let raw_group = self.raw_groups.get(label).cloned().ok_or_else(|| {
            VoiceGroupError::DanglingVoiceGroupReference {
                referrer: self
                    .resolution_path
                    .last()
                    .cloned()
                    .unwrap_or_else(|| label.to_owned()),
                target: label.to_owned(),
            }
        })?;

        self.resolution_path.push(label.to_owned());
        let declared_slot_origin = match role {
            GroupRole::TopLevel => SlotOrigin::TopLevelGroup,
            GroupRole::IndirectionTarget => SlotOrigin::IndirectionTarget,
        };
        let declared_end = usize::from(raw_group.starting_note) + raw_group.slots.len();
        if declared_end > VOICE_SLOT_COUNT {
            return Err(VoiceGroupError::TooManySlots {
                group: raw_group.label.clone(),
                starting_note: raw_group.starting_note,
                slot_count: raw_group.slots.len(),
            });
        }
        let mut slots = self.collect_alias_predecessors(&raw_group, declared_slot_origin)?;
        for raw_slot in &raw_group.slots {
            slots.push(self.resolve_slot(&raw_group.label, declared_slot_origin, raw_slot)?);
        }

        if role == GroupRole::TopLevel {
            let missing_trailing_slot_count = VOICE_SLOT_COUNT - declared_end;
            slots.extend(
                self.collect_link_adjacency_overflow(
                    &raw_group.label,
                    missing_trailing_slot_count,
                )?,
            );
        }

        let normalized_slots = pad_to_128(slots);
        self.resolution_path.pop();
        self.resolved_groups.insert(
            raw_group.label.clone(),
            ResolvedVoiceGroup {
                label: raw_group.label.clone(),
                slots: normalized_slots,
            },
        );
        self.emission_order.push(raw_group.label);
        Ok(())
    }

    fn resolve_slot(
        &mut self,
        group_label: &str,
        slot_origin: SlotOrigin,
        raw_slot: &RawSlot,
    ) -> Result<VoiceSlot, VoiceGroupError> {
        match raw_slot {
            RawSlot::KeySplit {
                child_label,
                table_label,
            } => self.resolve_indirection_slot(
                group_label,
                slot_origin,
                child_label,
                Some(table_label),
            ),
            RawSlot::Rhythm { child_label } => {
                self.resolve_indirection_slot(group_label, slot_origin, child_label, None)
            }
            leaf => convert_leaf_slot(leaf, group_label),
        }
    }

    fn resolve_indirection_slot(
        &mut self,
        parent_label: &str,
        parent_slot_origin: SlotOrigin,
        child_label: &str,
        table_label: Option<&str>,
    ) -> Result<VoiceSlot, VoiceGroupError> {
        if parent_slot_origin == SlotOrigin::IndirectionTarget {
            return Err(VoiceGroupError::NestedIndirection {
                parent: parent_label.to_owned(),
                child: child_label.to_owned(),
            });
        }

        let key_split_table = table_label
            .map(|table_label| {
                self.key_split_tables
                    .get(table_label)
                    .cloned()
                    .ok_or_else(|| VoiceGroupError::DanglingKeySplitTableReference {
                        referrer: parent_label.to_owned(),
                        target: table_label.to_owned(),
                    })
            })
            .transpose()?;

        self.resolve_group(child_label, GroupRole::IndirectionTarget)?;
        let children_id = checked_pack_id(voice_group_pack_id(child_label), parent_label)?;
        Ok(match key_split_table {
            Some(table) => VoiceSlot::KeySplit {
                starting_note: table.starting_note,
                table: table.table,
                children_id,
            },
            None => VoiceSlot::Rhythm { children_id },
        })
    }

    /// Upstream's `voice_group label, N` sets the label `N` ToneData records
    /// before the group's first declared record (`asm/macros/m4a.inc`), so
    /// slots `0..N` are the `N` physically preceding linked records, never
    /// empty positions. Predecessor groups contribute their raw declared
    /// records; borrowed ones keep their source group's label for diagnostics
    /// and the borrower's origin for the nested-indirection rule.
    fn collect_alias_predecessors(
        &mut self,
        group: &RawVoiceGroup,
        slot_origin: SlotOrigin,
    ) -> Result<Vec<VoiceSlot>, VoiceGroupError> {
        let needed = usize::from(group.starting_note);
        if needed == 0 {
            return Ok(Vec::new());
        }
        let insufficient = |available: usize| VoiceGroupError::InsufficientAliasPredecessors {
            group: group.label.clone(),
            starting_note: group.starting_note,
            available,
        };
        let raw_groups = self.raw_groups;
        let position = self
            .link_order
            .iter()
            .position(
                |item| matches!(item, IndexedLinkOrderItem::VoiceGroup(label) if *label == group.label),
            )
            .ok_or_else(|| insufficient(0))?;

        let mut records: Vec<(&str, &RawSlot)> = Vec::with_capacity(needed);
        for item in self.link_order[..position].iter().rev() {
            let IndexedLinkOrderItem::VoiceGroup(label) = item else {
                break;
            };
            let predecessor = raw_groups.get(label).ok_or_else(|| {
                VoiceGroupError::DanglingVoiceGroupReference {
                    referrer: group.label.clone(),
                    target: label.clone(),
                }
            })?;
            records.extend(
                predecessor
                    .slots
                    .iter()
                    .rev()
                    .map(|slot| (predecessor.label.as_str(), slot)),
            );
            if records.len() >= needed {
                break;
            }
        }
        if records.len() < needed {
            return Err(insufficient(records.len()));
        }
        records.truncate(needed);
        records.reverse();
        records
            .into_iter()
            .map(|(label, slot)| self.resolve_slot(label, slot_origin, slot))
            .collect()
    }

    fn top_level_link_successors(&self, top_label: &str) -> Vec<String> {
        let Some(position) = self.link_order.iter().position(
            |item| matches!(item, IndexedLinkOrderItem::VoiceGroup(label) if label == top_label),
        ) else {
            return Vec::new();
        };
        self.link_order[position + 1..]
            .iter()
            .map_while(|item| match item {
                IndexedLinkOrderItem::VoiceGroup(label) => Some(label.clone()),
                IndexedLinkOrderItem::ForeignInclude => None,
            })
            .collect()
    }

    fn collect_link_adjacency_overflow(
        &mut self,
        borrower_label: &str,
        missing_slot_count: usize,
    ) -> Result<Vec<VoiceSlot>, VoiceGroupError> {
        let mut borrowed_slots = Vec::with_capacity(missing_slot_count);
        for successor_label in self.top_level_link_successors(borrower_label) {
            if borrowed_slots.len() == missing_slot_count {
                break;
            }
            let successor = self
                .raw_groups
                .get(&successor_label)
                .cloned()
                .ok_or_else(|| VoiceGroupError::DanglingVoiceGroupReference {
                    referrer: borrower_label.to_owned(),
                    target: successor_label,
                })?;
            for raw_slot in &successor.slots {
                if borrowed_slots.len() == missing_slot_count {
                    break;
                }
                borrowed_slots.push(self.resolve_slot(
                    &successor.label,
                    SlotOrigin::BorrowedLinkSuccessor,
                    raw_slot,
                )?);
            }
        }
        Ok(borrowed_slots)
    }
}

#[cfg(test)]
mod tests;
