//! Walks the pack's voicegroup closure through the ROM's own pointers.
//!
//! Every song's header names its root voicegroup. From each root the walk reads
//! all 128 slots of the pack's decoded group: a key-split or rhythm slot
//! points at its child group, a key-split slot also at its table, and a sample
//! slot at the sample it plays. Each pointer is bound to the pack id the slot
//! names, so a group, table or sample reached twice must be at one address,
//! and a sample stored at several ROM addresses is the one a slot points at.
//! Slots the pack leaves empty are not followed: the engine never resolves a
//! second level of indirection (`pokeemerald/src/m4a_1.s:1603-1607`).

use std::collections::{BTreeMap, HashMap, VecDeque};

use assets::audio::{VoiceEntry, VoiceGroup};

use crate::extract::voicegroups::parser::{RawKeySplitTable, RawVoiceGroup};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{
    only_one_matching, slice_at_addr, to_offset, u32_at_addr, u8_at_addr,
};
use crate::gen_rom_profile::plan::{
    KeysplitPlan, ReportLine, Resolution, SamplePlan, VoicegroupPlan,
};
use crate::gen_rom_profile::Context;

use super::samples::{direct_sound_line, programmable_wave_line, SampleCandidates};
use super::voicegroups::{keysplit_symbol, plan_for, voicegroup_symbol};
use super::{SLOT_BYTES, SLOT_POINTER, SLOT_TABLE, TYPE_KEYSPLIT, TYPE_RHYTHM};

const VOICEGROUP_PREFIX: &str = "audio/voicegroup/";

/// What the closure holds, each list sorted by id or label.
pub(super) struct Closure {
    pub voicegroups: Vec<VoicegroupPlan>,
    pub keysplits: Vec<KeysplitPlan>,
    pub direct_sound: Vec<SamplePlan>,
    pub programmable_wave: Vec<SamplePlan>,
}

/// The source the walk checks the ROM against.
pub(super) struct Sources<'a> {
    pub groups: &'a HashMap<String, RawVoiceGroup>,
    pub tables: &'a HashMap<String, RawKeySplitTable>,
    pub direct_sound: &'a [SampleCandidates],
    pub programmable_wave: &'a [SampleCandidates],
}

/// Walks every group reachable from `roots` (pack id to ROM address).
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] when a slot's type or pointer
/// disagrees with the pack, a group, table or sample is bound to two
/// addresses, a table matches no source table or several, or a pack sample is
/// pointed at by no slot; [`GenRomProfileError::UpstreamSource`] for a group
/// the source does not declare; plus the pack-entry errors for a group or
/// sample the pack lacks or malforms.
pub(super) fn walk(
    ctx: &Context<'_>,
    sources: &Sources<'_>,
    roots: &BTreeMap<String, u32>,
    report: &mut Vec<ReportLine>,
) -> Result<Closure, GenRomProfileError> {
    let mut walk = Walk {
        ctx,
        sources,
        groups: BTreeMap::new(),
        queue: VecDeque::new(),
        keysplits: BTreeMap::new(),
        samples: BTreeMap::new(),
    };
    for (id, &addr) in roots {
        walk.bind_group(id, addr)?;
    }
    while let Some(id) = walk.queue.pop_front() {
        walk.visit(&id)?;
    }
    walk.finish(report)
}

struct Walk<'a, 'c> {
    ctx: &'a Context<'c>,
    sources: &'a Sources<'a>,
    /// Every bound group's address by pack id.
    groups: BTreeMap<String, u32>,
    queue: VecDeque<String>,
    keysplits: BTreeMap<u32, KeysplitPlan>,
    /// Every sample slot's chosen address by pack id.
    samples: BTreeMap<String, u32>,
}

impl Walk<'_, '_> {
    fn bind_group(&mut self, id: &str, addr: u32) -> Result<(), GenRomProfileError> {
        match self.groups.get(id) {
            Some(&bound) if bound != addr => Err(GenRomProfileError::StructMismatch {
                id: id.to_owned(),
                reason: format!("pointers put the group at {bound:08X} and at {addr:08X}"),
            }),
            Some(_) => Ok(()),
            None => {
                self.groups.insert(id.to_owned(), addr);
                self.queue.push_back(id.to_owned());
                Ok(())
            }
        }
    }

    fn visit(&mut self, id: &str) -> Result<(), GenRomProfileError> {
        let group = VoiceGroup::decode(self.ctx.pack.raw_payload(id)?).map_err(|err| {
            GenRomProfileError::EntryShape {
                id: id.to_owned(),
                reason: err.to_string(),
            }
        })?;
        let base = self.groups[id];
        for (index, entry) in group.slots().iter().enumerate() {
            let slot = base + u32::try_from(index).expect("a slot index fits in u32") * SLOT_BYTES;
            let at = Slot {
                group: id,
                index,
                addr: slot,
            };
            match entry {
                VoiceEntry::KeySplit(voice) => {
                    self.indirection(&at, TYPE_KEYSPLIT, &voice.children.0)?;
                    self.keysplit(&at)?;
                }
                VoiceEntry::Rhythm(voice) => {
                    self.indirection(&at, TYPE_RHYTHM, &voice.children.0)?;
                }
                VoiceEntry::DirectSound(voice) => {
                    self.sample(&at, &voice.sample.0, self.sources.direct_sound)?;
                }
                VoiceEntry::ProgrammableWave(voice) => {
                    self.sample(&at, &voice.wave.0, self.sources.programmable_wave)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Binds the child a key-split or rhythm slot points at.
    fn indirection(
        &mut self,
        at: &Slot<'_>,
        expected: u8,
        child: &str,
    ) -> Result<(), GenRomProfileError> {
        let kind = u8_at_addr(self.ctx.rom, at.addr);
        if kind != Some(expected) {
            return Err(at.mismatch(&format!("reads type {kind:?}, not {expected:#04X}")));
        }
        let addr = at
            .pointer(self.ctx, SLOT_POINTER)
            .ok_or_else(|| at.mismatch("does not point into the cartridge"))?;
        self.bind_group(child, addr)
    }

    /// Identifies the key-split table a slot's table pointer names by its
    /// bytes in the source tables.
    fn keysplit(&mut self, at: &Slot<'_>) -> Result<(), GenRomProfileError> {
        let addr = at
            .pointer(self.ctx, SLOT_TABLE)
            .ok_or_else(|| at.mismatch("table pointer is not a cartridge address"))?;
        if self.keysplits.contains_key(&addr) {
            return Ok(());
        }
        let mut tables: Vec<(&String, &RawKeySplitTable)> = self.sources.tables.iter().collect();
        tables.sort_by_key(|(label, _)| *label);
        let (label, table) = only_one_matching(
            &format!("keysplit@{addr:08X}"),
            "a source key-split table",
            tables,
            |(_, table)| {
                // The pointer is biased back by `starting_note`; the bytes start after it.
                let data = addr + u32::from(table.starting_note);
                !table.table.is_empty()
                    && slice_at_addr(self.ctx.rom, data, table.table.len())
                        == Some(table.table.as_slice())
            },
        )?;
        let len = u16::try_from(table.table.len()).expect("a key-split table fits in u16");
        self.keysplits.insert(
            addr,
            KeysplitPlan {
                label: label.clone(),
                addr,
                starting_note: table.starting_note,
                len,
            },
        );
        Ok(())
    }

    /// Binds a sample slot's pointer to one of the sample's ROM copies.
    fn sample(
        &mut self,
        at: &Slot<'_>,
        sample: &str,
        located: &[SampleCandidates],
    ) -> Result<(), GenRomProfileError> {
        let candidates = located
            .iter()
            .find(|candidates| candidates.id == sample)
            .ok_or_else(|| GenRomProfileError::MissingPackEntry(sample.to_owned()))?;
        let mismatch = |reason: String| GenRomProfileError::StructMismatch {
            id: sample.to_owned(),
            reason,
        };
        let addr = at.pointer(self.ctx, SLOT_POINTER).ok_or_else(|| {
            mismatch(format!(
                "slot {} of {} does not point into the cartridge",
                at.index, at.group
            ))
        })?;
        if !candidates.addrs.contains(&addr) {
            return Err(mismatch(format!(
                "slot {} of {} points at {addr:08X}, not at a copy of the sample ({:08X?})",
                at.index, at.group, candidates.addrs
            )));
        }
        match self.samples.insert(sample.to_owned(), addr) {
            Some(bound) if bound != addr => Err(mismatch(format!(
                "voicegroup slots point at two copies, {bound:08X} and {addr:08X}"
            ))),
            _ => Ok(()),
        }
    }

    fn finish(self, report: &mut Vec<ReportLine>) -> Result<Closure, GenRomProfileError> {
        let mut voicegroups = Vec::with_capacity(self.groups.len());
        for (id, &addr) in &self.groups {
            let label = id.strip_prefix(VOICEGROUP_PREFIX).unwrap_or(id);
            let group = self.sources.groups.get(label).ok_or_else(|| {
                GenRomProfileError::UpstreamSource {
                    path: self.ctx.upstream.join("sound/voicegroups"),
                    reason: format!("no voicegroup declares the label `{label}`"),
                }
            })?;
            let mut line = ReportLine::unique(id, addr, 0)
                .with(Resolution::PointerWalk)
                .note(format!("{} slots declared", group.slots.len()));
            line.symbol = voicegroup_symbol(label, group.starting_note);
            report.push(line);
            voicegroups.push(plan_for(label, addr, group));
        }

        let mut keysplits: Vec<KeysplitPlan> = self.keysplits.values().cloned().collect();
        keysplits.sort_by(|a, b| a.label.cmp(&b.label));
        for table in &keysplits {
            let mut line = ReportLine::unique(
                format!("keysplit_{}", table.label),
                table.addr,
                u32::from(table.len),
            )
            .with(Resolution::PointerWalk);
            line.symbol = keysplit_symbol(&table.label, table.starting_note);
            report.push(line.note(format!(
                "biased back {} notes; {} notes mapped",
                table.starting_note, table.len
            )));
        }

        let direct_sound =
            self.sample_plans(self.sources.direct_sound, report, direct_sound_line)?;
        let programmable_wave = self.sample_plans(
            self.sources.programmable_wave,
            report,
            programmable_wave_line,
        )?;
        Ok(Closure {
            voicegroups,
            keysplits,
            direct_sound,
            programmable_wave,
        })
    }

    /// The plans of `located`, which every one of must have been pointed at.
    fn sample_plans(
        &self,
        located: &[SampleCandidates],
        report: &mut Vec<ReportLine>,
        line: fn(&SamplePlan, usize) -> ReportLine,
    ) -> Result<Vec<SamplePlan>, GenRomProfileError> {
        located
            .iter()
            .map(|candidates| {
                let addr = self.samples.get(&candidates.id).ok_or_else(|| {
                    GenRomProfileError::StructMismatch {
                        id: candidates.id.clone(),
                        reason: "no voicegroup slot in the closure points at this sample"
                            .to_owned(),
                    }
                })?;
                let plan = candidates.at(*addr);
                report.push(line(&plan, candidates.addrs.len()));
                Ok(plan)
            })
            .collect()
    }
}

/// One `ToneData` slot of a located group.
struct Slot<'a> {
    group: &'a str,
    index: usize,
    addr: u32,
}

impl Slot<'_> {
    fn mismatch(&self, reason: &str) -> GenRomProfileError {
        GenRomProfileError::StructMismatch {
            id: self.group.to_owned(),
            reason: format!("slot {} {reason}", self.index),
        }
    }

    /// The cartridge pointer `offset` bytes into the slot.
    fn pointer(&self, ctx: &Context<'_>, offset: u32) -> Option<u32> {
        u32_at_addr(ctx.rom, self.addr + offset).filter(|value| to_offset(*value).is_some())
    }
}
