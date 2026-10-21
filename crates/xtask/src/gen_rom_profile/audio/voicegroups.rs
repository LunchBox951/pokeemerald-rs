//! Locating voicegroups, key-split tables and their children, with the symbol-expectation tests.

use std::path::Path;

use crate::extract::voicegroups::parser;
use crate::extract::voicegroups::parser::{RawSlot, RawVoiceGroup};

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{
    only_one_matching, slice_at_addr, to_offset, u32_at_addr, u8_at_addr,
};
use crate::gen_rom_profile::plan::{
    KeysplitPlan, ReportLine, Resolution, SamplePlan, SymbolExpectation, VoicegroupPlan,
};
use crate::gen_rom_profile::Context;

use super::{
    ADDRESSABLE_SLOTS, DIRECT_SOUND_PREFIX, DIRECT_SOUND_SYMBOL, PROGRAMMABLE_WAVE_PREFIX,
    PROGRAMMABLE_WAVE_SYMBOL, SLOT_BYTES, SLOT_POINTER, SLOT_TABLE, TYPE_KEYSPLIT, TYPE_RHYTHM,
};

/// What a linker map should say at a voicegroup's address.
///
/// Upstream's `voice_group` macro declares a `.global` only in its unbiased
/// spelling; a `starting_note` bias makes the label a `.set` naming an
/// address *before* the first slot, which no map lists and which is
/// interior to whatever data precedes the group.
pub(super) fn voicegroup_symbol(label: &str, starting_note: u8) -> SymbolExpectation {
    if starting_note == 0 {
        SymbolExpectation::Exact(format!("voicegroup_{label}"))
    } else {
        SymbolExpectation::Interior
    }
}

/// What a linker map should say at a key-split table's address.
///
/// Upstream's `keysplit` macro splits the same way `voice_group` does, and
/// a biased table's address is interior for the same reason.
fn keysplit_symbol(label: &str, starting_note: u8) -> SymbolExpectation {
    if starting_note == 0 {
        SymbolExpectation::Exact(format!("keysplit_{label}"))
    } else {
        SymbolExpectation::Interior
    }
}

/// Describe one voicegroup, biasing its address the way the ROM does.
pub(super) fn plan_for(label: &str, addr: u32, group: &RawVoiceGroup) -> VoicegroupPlan {
    VoicegroupPlan {
        id: format!("audio/voicegroup/{label}"),
        label: label.to_owned(),
        addr,
        starting_note: group.starting_note,
        declared_slots: u16::try_from(group.slots.len()).unwrap_or(ADDRESSABLE_SLOTS),
    }
}

/// Find the run of slots that is this voicegroup.
///
/// The signature is every slot that names a sample: the ROM's pointer at
/// that slot must be the address already found for that sample. A group
/// with a `starting_note` bias is addressed before its first slot, exactly
/// as the mixer indexes it, so the returned address is biased too.
pub(super) fn locate_voicegroup(
    ctx: &Context<'_>,
    label: &str,
    group: &RawVoiceGroup,
    direct_sound: &[SamplePlan],
    programmable_wave: &[SamplePlan],
) -> Result<u32, GenRomProfileError> {
    let find = |id: &str, table: &[SamplePlan]| {
        table
            .iter()
            .find(|plan| plan.id == id)
            .map(|plan| plan.addr)
    };
    // Each slot that names a sample constrains the run; collect them as
    // (slot index, required pointer).
    let mut constraints: Vec<(usize, u32)> = Vec::new();
    for (index, slot) in group.slots.iter().enumerate() {
        let required = match slot {
            RawSlot::DirectSound { sample_symbol, .. } => sample_symbol
                .strip_prefix(DIRECT_SOUND_SYMBOL)
                .and_then(|name| find(&format!("{DIRECT_SOUND_PREFIX}{name}"), direct_sound)),
            RawSlot::ProgrammableWave { wave_symbol, .. } => wave_symbol
                .strip_prefix(PROGRAMMABLE_WAVE_SYMBOL)
                .and_then(|index| index.parse::<u32>().ok())
                .and_then(|index| {
                    find(
                        &format!("{PROGRAMMABLE_WAVE_PREFIX}{index:02}"),
                        programmable_wave,
                    )
                }),
            _ => None,
        };
        if let Some(addr) = required {
            constraints.push((index, addr));
        }
    }
    let (anchor_index, anchor_addr) =
        *constraints
            .first()
            .ok_or_else(|| GenRomProfileError::StructMismatch {
                id: format!("voicegroup_{label}"),
                reason: "no slot names a sample, so nothing anchors the run".to_owned(),
            })?;

    let bias = u32::from(group.starting_note) * SLOT_BYTES;
    let anchor_slot = u32::try_from(anchor_index).expect("a slot index fits in u32");
    only_one_matching(
        &format!("voicegroup_{label}"),
        "every sample slot in the group",
        ctx.pointers
            .refs_to(anchor_addr)
            .iter()
            .filter_map(|at| at.checked_sub(SLOT_POINTER + anchor_slot * SLOT_BYTES)),
        |&first_slot| {
            constraints.iter().all(|&(index, addr)| {
                let slot = u32::try_from(index).expect("a slot index fits in u32");
                u32_at_addr(ctx.rom, first_slot + slot * SLOT_BYTES + SLOT_POINTER) == Some(addr)
            })
        },
    )
    .and_then(|first_slot| {
        first_slot
            .checked_sub(bias)
            .ok_or_else(|| GenRomProfileError::StructMismatch {
                id: format!("voicegroup_{label}"),
                reason: "the starting-note bias runs off the start of the ROM".to_owned(),
            })
    })
}

/// Follow a group's key-split and rhythm slots to its children.
type Children = (Vec<VoicegroupPlan>, Vec<KeysplitPlan>);

pub(super) fn walk_children(
    ctx: &Context<'_>,
    parent: u32,
    group: &RawVoiceGroup,
    groups: &std::collections::HashMap<String, RawVoiceGroup>,
    report: &mut Vec<ReportLine>,
) -> Result<Children, GenRomProfileError> {
    let tables = read_keysplit_tables(&ctx.upstream)?;
    let mut voicegroups = Vec::new();
    let mut keysplits = Vec::new();

    for (index, slot) in group.slots.iter().enumerate() {
        let slot_addr = parent
            + u32::from(group.starting_note) * SLOT_BYTES
            + u32::try_from(index).expect("a slot index fits in u32") * SLOT_BYTES;
        let (child_label, table_label, expected_type) = match slot {
            RawSlot::KeySplit {
                child_label,
                table_label,
            } => (child_label, Some(table_label), TYPE_KEYSPLIT),
            RawSlot::Rhythm { child_label } => (child_label, None, TYPE_RHYTHM),
            _ => continue,
        };
        let kind = u8_at_addr(ctx.rom, slot_addr);
        if kind != Some(expected_type) {
            return Err(GenRomProfileError::StructMismatch {
                id: format!("voicegroup_{child_label}"),
                reason: format!(
                    "slot {index} reads type {kind:?}, not the {expected_type:#04X} its source declares"
                ),
            });
        }
        let child_addr = u32_at_addr(ctx.rom, slot_addr + SLOT_POINTER)
            .filter(|value| to_offset(*value).is_some())
            .ok_or_else(|| GenRomProfileError::StructMismatch {
                id: format!("voicegroup_{child_label}"),
                reason: format!("slot {index} does not point into the cartridge"),
            })?;
        let child =
            groups
                .get(child_label.as_str())
                .ok_or_else(|| GenRomProfileError::UpstreamSource {
                    path: ctx.upstream.join("sound/voicegroups"),
                    reason: format!("no voicegroup declares the label `{child_label}`"),
                })?;
        let mut line = ReportLine::unique(format!("audio/voicegroup/{child_label}"), child_addr, 0)
            .with(Resolution::PointerWalk)
            .note(format!("{} slots declared", child.slots.len()));
        line.symbol = voicegroup_symbol(child_label, child.starting_note);
        report.push(line);
        voicegroups.push(plan_for(child_label, child_addr, child));

        if let Some(table_label) = table_label {
            keysplits.push(locate_keysplit(
                ctx,
                slot_addr,
                table_label,
                &tables,
                report,
            )?);
        }
    }
    Ok((voicegroups, keysplits))
}

/// Read a key-split slot's table pointer and check the bytes behind it.
fn locate_keysplit(
    ctx: &Context<'_>,
    slot_addr: u32,
    label: &str,
    tables: &std::collections::HashMap<String, parser::RawKeySplitTable>,
    report: &mut Vec<ReportLine>,
) -> Result<KeysplitPlan, GenRomProfileError> {
    let table = tables
        .get(label)
        .ok_or_else(|| GenRomProfileError::UpstreamSource {
            path: ctx.upstream.join("sound/keysplit_tables.inc"),
            reason: format!("no key-split table named `{label}`"),
        })?;
    let addr = u32_at_addr(ctx.rom, slot_addr + SLOT_TABLE)
        .filter(|value| to_offset(*value).is_some())
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: format!("keysplit_{label}"),
            reason: "the slot's table pointer is not a cartridge address".to_owned(),
        })?;
    // The ROM's pointer is biased back by the first note the table maps, so
    // `table[note]` indexes directly. The real bytes start at the bias.
    let data = addr + u32::from(table.starting_note);
    if slice_at_addr(ctx.rom, data, table.table.len()) != Some(table.table.as_slice()) {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("keysplit_{label}"),
            reason: format!(
                "the {} bytes at {data:08X} are not the table",
                table.table.len()
            ),
        });
    }
    let len = u16::try_from(table.table.len()).expect("a key-split table fits in u16");
    let mut line = ReportLine::unique(format!("keysplit_{label}"), addr, u32::from(len))
        .with(Resolution::PointerWalk);
    line.symbol = keysplit_symbol(label, table.starting_note);
    report.push(line.note(format!(
        "biased back {} notes; {len} notes mapped",
        table.starting_note
    )));
    Ok(KeysplitPlan {
        label: label.to_owned(),
        addr,
        starting_note: table.starting_note,
        len,
    })
}

/// Parse `sound/keysplit_tables.inc`.
fn read_keysplit_tables(
    upstream: &Path,
) -> Result<std::collections::HashMap<String, parser::RawKeySplitTable>, GenRomProfileError> {
    let path = upstream.join("sound/keysplit_tables.inc");
    let text =
        std::fs::read_to_string(&path).map_err(|err| GenRomProfileError::UpstreamSource {
            path: path.clone(),
            reason: err.to_string(),
        })?;
    parser::parse_keysplit_tables(&text).map_err(|err| GenRomProfileError::UpstreamSource {
        path,
        reason: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{keysplit_symbol, voicegroup_symbol};
    use crate::gen_rom_profile::plan::SymbolExpectation;

    #[test]
    fn a_biased_audio_root_asks_a_map_for_nothing() {
        // Upstream's biased `voice_group`/`keysplit` spellings emit a `.set`
        // and no `.global`, so the biased address is in the middle of the
        // preceding data and no map names it. Demanding *some* symbol there
        // would fail `--map` on a correct ROM and a correct map: every
        // key-split table upstream ships is biased, as is `rs_drumset`.
        assert_eq!(
            voicegroup_symbol("rs_drumset", 36),
            SymbolExpectation::Interior
        );
        assert_eq!(keysplit_symbol("piano", 36), SymbolExpectation::Interior);
        assert_eq!(keysplit_symbol("tuba", 24), SymbolExpectation::Interior);
    }

    #[test]
    fn an_unbiased_audio_root_is_checked_by_name() {
        assert_eq!(
            voicegroup_symbol("title", 0),
            SymbolExpectation::Exact("voicegroup_title".to_owned())
        );
        assert_eq!(
            keysplit_symbol("piano", 0),
            SymbolExpectation::Exact("keysplit_piano".to_owned())
        );
    }
}
