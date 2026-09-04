//! Resolves parsed voicegroup references and normalizes each emitted group to
//! [`super::VOICE_SLOT_COUNT`] slots.
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
//! the linked successor groups supplied by `super::link_order_successors`.
//! Borrowed entries use their source group's label for diagnostics and may
//! resolve an indirection child. Indirection-target groups never borrow
//! adjacent entries; any unfilled position remains [`VoiceSlot::Empty`].

use std::collections::HashMap;

use super::parser::{
    DirectSoundMode, Envelope, RawKeySplitTable, RawSlot, RawVoiceGroup, VoiceGroupError,
};
use super::VOICE_SLOT_COUNT;

const DIRECT_SOUND_SAMPLE_PREFIX: &str = "DirectSoundWaveData_";
const PROGRAMMABLE_WAVE_SAMPLE_PREFIX: &str = "ProgrammableWaveData_";

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

fn pad_to_128(
    group: &str,
    starting_note: u8,
    mut slots: Vec<VoiceSlot>,
) -> Result<Vec<VoiceSlot>, VoiceGroupError> {
    let leading_empty_slot_count = usize::from(starting_note);
    let occupied_slot_count = leading_empty_slot_count
        .checked_add(slots.len())
        .filter(|&count| count <= VOICE_SLOT_COUNT);
    if occupied_slot_count.is_none() {
        return Err(VoiceGroupError::TooManySlots {
            group: group.to_owned(),
            starting_note,
            slot_count: slots.len(),
        });
    }
    let mut out = Vec::with_capacity(VOICE_SLOT_COUNT);
    out.extend(std::iter::repeat_n(
        VoiceSlot::Empty,
        leading_empty_slot_count,
    ));
    out.append(&mut slots);
    out.resize_with(VOICE_SLOT_COUNT, || VoiceSlot::Empty);
    Ok(out)
}

#[cfg(test)]
pub(super) fn resolve_voice_groups(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    resolve_voice_groups_with_link_successors(top_label, raw_groups, keysplit_tables, &[])
}

pub(super) fn resolve_voice_groups_with_link_successors(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    link_successors: &[String],
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    Resolver::new(raw_groups, keysplit_tables, link_successors).resolve(top_label)
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
    top_level_link_successors: &'a [String],
    resolution_path: Vec<String>,
    emission_order: Vec<String>,
    resolved_groups: HashMap<String, ResolvedVoiceGroup>,
}

impl<'a> Resolver<'a> {
    fn new(
        raw_groups: &'a HashMap<String, RawVoiceGroup>,
        key_split_tables: &'a HashMap<String, RawKeySplitTable>,
        top_level_link_successors: &'a [String],
    ) -> Self {
        Self {
            raw_groups,
            key_split_tables,
            top_level_link_successors,
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
        let mut slots = Vec::with_capacity(raw_group.slots.len());
        for raw_slot in &raw_group.slots {
            slots.push(self.resolve_slot(&raw_group.label, declared_slot_origin, raw_slot)?);
        }

        if role == GroupRole::TopLevel {
            let declared_slot_end = usize::from(raw_group.starting_note) + slots.len();
            let missing_trailing_slot_count = VOICE_SLOT_COUNT.saturating_sub(declared_slot_end);
            slots.extend(
                self.collect_link_adjacency_overflow(
                    &raw_group.label,
                    missing_trailing_slot_count,
                )?,
            );
        }

        let normalized_slots = pad_to_128(&raw_group.label, raw_group.starting_note, slots)?;
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

    fn collect_link_adjacency_overflow(
        &mut self,
        borrower_label: &str,
        missing_slot_count: usize,
    ) -> Result<Vec<VoiceSlot>, VoiceGroupError> {
        let mut borrowed_slots = Vec::with_capacity(missing_slot_count);
        for successor_label in self.top_level_link_successors.iter().cloned() {
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
