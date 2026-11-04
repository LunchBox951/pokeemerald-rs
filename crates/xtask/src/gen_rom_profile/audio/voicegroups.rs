//! Locates voicegroups and key-split tables from source-declared slots.

use std::path::Path;

use crate::extract::voicegroups::parser;
use crate::extract::voicegroups::parser::{RawSlot, RawVoiceGroup};

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{only_one_matching, u32_at_addr};
use crate::gen_rom_profile::plan::{SymbolExpectation, VoicegroupPlan};
use crate::gen_rom_profile::Context;

use super::samples::SampleCandidates;
use super::{
    ADDRESSABLE_SLOTS, DIRECT_SOUND_PREFIX, DIRECT_SOUND_SYMBOL, PROGRAMMABLE_WAVE_PREFIX,
    PROGRAMMABLE_WAVE_SYMBOL, SLOT_BYTES, SLOT_POINTER,
};

/// The map symbol expected at a voicegroup's address.
///
/// A biased group's label is a `.set`, which no map lists.
pub(super) fn voicegroup_symbol(label: &str, starting_note: u8) -> SymbolExpectation {
    if starting_note == 0 {
        SymbolExpectation::Exact(format!("voicegroup_{label}"))
    } else {
        SymbolExpectation::Interior
    }
}

/// The map symbol expected at a key-split table's address; biased tables
/// have none, as with [`voicegroup_symbol`].
pub(super) fn keysplit_symbol(label: &str, starting_note: u8) -> SymbolExpectation {
    if starting_note == 0 {
        SymbolExpectation::Exact(format!("keysplit_{label}"))
    } else {
        SymbolExpectation::Interior
    }
}

/// The plan entry for the voicegroup `label` at `addr`.
pub(super) fn plan_for(label: &str, addr: u32, group: &RawVoiceGroup) -> VoicegroupPlan {
    VoicegroupPlan {
        id: format!("audio/voicegroup/{label}"),
        label: label.to_owned(),
        addr,
        starting_note: group.starting_note,
        declared_slots: u16::try_from(group.slots.len()).unwrap_or(ADDRESSABLE_SLOTS),
    }
}

/// The ROM address of `group`: the single run of slots whose sample pointers
/// each name one of the located copies of the sample the source declares.
///
/// `starting_note` biases the returned address back from the first slot. This
/// only anchors the song table from the title; every other group is reached
/// through pointers (`closure`).
pub(super) fn locate_voicegroup(
    ctx: &Context<'_>,
    label: &str,
    group: &RawVoiceGroup,
    direct_sound: &[SampleCandidates],
    programmable_wave: &[SampleCandidates],
) -> Result<u32, GenRomProfileError> {
    fn find<'a>(id: &str, table: &'a [SampleCandidates]) -> Option<&'a [u32]> {
        table
            .iter()
            .find(|candidates| candidates.id == id)
            .map(|candidates| candidates.addrs.as_slice())
    }
    let mut constraints: Vec<(usize, &[u32])> = Vec::new();
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
        if let Some(addrs) = required {
            constraints.push((index, addrs));
        }
    }
    let &(anchor_index, anchor_addrs) = constraints
        .iter()
        .min_by_key(|(_, addrs)| addrs.len())
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: format!("voicegroup_{label}"),
            reason: "no slot names a sample, so nothing anchors the run".to_owned(),
        })?;

    let bias = u32::from(group.starting_note) * SLOT_BYTES;
    let anchor_slot = u32::try_from(anchor_index).expect("a slot index fits in u32");
    only_one_matching(
        &format!("voicegroup_{label}"),
        "every sample slot in the group",
        anchor_addrs.iter().flat_map(|&anchor_addr| {
            ctx.pointers
                .refs_to(anchor_addr)
                .iter()
                .filter_map(move |at| at.checked_sub(SLOT_POINTER + anchor_slot * SLOT_BYTES))
        }),
        |&first_slot| {
            constraints.iter().all(|&(index, addrs)| {
                let slot = u32::try_from(index).expect("a slot index fits in u32");
                u32_at_addr(ctx.rom, first_slot + slot * SLOT_BYTES + SLOT_POINTER)
                    .is_some_and(|value| addrs.contains(&value))
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

/// The key-split tables of `sound/keysplit_tables.inc`.
pub(super) fn read_keysplit_tables(
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
        // Biased roots are `.set` labels, so demanding a map symbol would fail
        // `--map` on a correct ROM.
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
