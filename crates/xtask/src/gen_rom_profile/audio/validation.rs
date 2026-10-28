//! Checks the planned voicegroups against the pack.

use assets::audio::VoiceGroup;

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{AudioPlan, SamplePlan};
use crate::gen_rom_profile::Context;

use super::ADDRESSABLE_SLOTS;

/// Requires the pack's voicegroups to be exactly the planned ones, each equal
/// to what `rom_import::decode_voicegroup_payload` decodes at its planned root.
///
/// A pack voicegroup the plan does not reach is rejected: the importer would
/// never emit it.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] naming `audio/voicegroup/*` for an
/// unreachable pack voicegroup, or for a ROM group that cannot be decoded or
/// differs from the pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`] or
/// [`GenRomProfileError::EntryShape`] when a planned group's pack entry is
/// absent, not raw, or not a voicegroup.
pub(super) fn check_voicegroups(
    ctx: &Context<'_>,
    plan: &AudioPlan,
) -> Result<(), GenRomProfileError> {
    let unplanned: Vec<String> = ctx
        .pack
        .ids_with_prefix("audio/voicegroup/")
        .into_iter()
        .filter(|id| !plan.voicegroups.iter().any(|group| &group.id == id))
        .collect();
    if !unplanned.is_empty() {
        let planned: Vec<&str> = plan.voicegroups.iter().map(|g| g.id.as_str()).collect();
        return Err(GenRomProfileError::StructMismatch {
            id: "audio/voicegroup/*".to_owned(),
            reason: format!(
                "the pack holds {unplanned:?}, which no planned root reaches; \
                 the plan holds {planned:?}"
            ),
        });
    }
    let roots = importer_roots(plan)?;
    let reader = rom_import::RomReader::new(ctx.rom);
    for (voicegroup, root) in plan.voicegroups.iter().zip(roots.voicegroups) {
        let id = &voicegroup.id;
        let mismatch = |reason: String| GenRomProfileError::StructMismatch {
            id: id.clone(),
            reason,
        };
        let payload = ctx.pack.raw_payload(id)?;
        let expected =
            VoiceGroup::decode(payload).map_err(|err| GenRomProfileError::EntryShape {
                id: id.clone(),
                reason: err.to_string(),
            })?;
        let decoded = rom_import::decode_voicegroup_payload(&reader, &roots, root)
            .map_err(|err| mismatch(format!("the ROM voicegroup does not decode: {err}")))?;
        if decoded != expected.encode() {
            return Err(mismatch(
                "the ROM's decoded voicegroup differs from the pack entry".to_owned(),
            ));
        }
    }
    Ok(())
}

/// `plan` as the importer's roots, voicegroups in plan order.
///
/// The roots borrow `'static` data, so the strings and slices are leaked; the
/// generator builds them once per run.
fn importer_roots(plan: &AudioPlan) -> Result<rom_import::AudioRoots, GenRomProfileError> {
    fn leak_str(text: &str) -> &'static str {
        Box::leak(text.to_owned().into_boxed_str())
    }
    fn leak_slice<T>(items: Vec<T>) -> &'static [T] {
        Box::leak(items.into_boxed_slice())
    }
    let ptr = |id: &str, addr: u32| {
        rom_import::GbaPtr::new(addr).ok_or_else(|| GenRomProfileError::StructMismatch {
            id: id.to_owned(),
            reason: format!("{addr:08X} is outside the ROM"),
        })
    };
    let samples = |plans: &[SamplePlan]| {
        plans
            .iter()
            .map(|sample| {
                Ok(rom_import::SampleRoot {
                    id: leak_str(&sample.id),
                    addr: ptr(&sample.id, sample.addr)?,
                    header_len: sample.header_len,
                    data_len: sample.data_len,
                })
            })
            .collect::<Result<Vec<_>, GenRomProfileError>>()
    };
    let songs = plan
        .songs
        .iter()
        .map(|song| {
            Ok(rom_import::SongRoot {
                id: leak_str(&song.id),
                index: song.index,
                header: ptr(&song.id, song.header)?,
                track_count: song.track_count,
                voicegroup: ptr(&song.id, song.voicegroup)?,
            })
        })
        .collect::<Result<Vec<_>, GenRomProfileError>>()?;
    let voicegroups = plan
        .voicegroups
        .iter()
        .map(|group| {
            Ok(rom_import::VoicegroupRoot {
                id: leak_str(&group.id),
                label: leak_str(&group.label),
                addr: ptr(&group.id, group.addr)?,
                starting_note: group.starting_note,
                declared_slots: group.declared_slots,
                addressable_slots: ADDRESSABLE_SLOTS,
            })
        })
        .collect::<Result<Vec<_>, GenRomProfileError>>()?;
    let keysplits = plan
        .keysplits
        .iter()
        .map(|table| {
            Ok(rom_import::KeysplitRoot {
                label: leak_str(&table.label),
                addr: ptr(&table.label, table.addr)?,
                starting_note: table.starting_note,
                len: table.len,
            })
        })
        .collect::<Result<Vec<_>, GenRomProfileError>>()?;
    Ok(rom_import::AudioRoots {
        song_table: ptr("gSongTable", plan.song_table)?,
        songs: leak_slice(songs),
        voicegroups: leak_slice(voicegroups),
        keysplits: leak_slice(keysplits),
        direct_sound: leak_slice(samples(&plan.direct_sound)?),
        programmable_wave: leak_slice(samples(&plan.programmable_wave)?),
    })
}
