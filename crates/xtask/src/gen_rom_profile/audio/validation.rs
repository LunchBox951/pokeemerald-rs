//! Checks the planned audio against the pack and against itself.

use assets::audio::VoiceGroup;

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{AudioPlan, CryPlan, SamplePlan};
use crate::gen_rom_profile::Context;

use super::ADDRESSABLE_SLOTS;

/// Requires the plan's roots to be distinct places, and the pack's voicegroups
/// to be exactly the planned ones, each equal
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
pub(super) fn check_plan(ctx: &Context<'_>, plan: &AudioPlan) -> Result<(), GenRomProfileError> {
    check_distinct_addresses(plan)?;
    check_voicegroups(ctx, plan)
}

fn check_voicegroups(ctx: &Context<'_>, plan: &AudioPlan) -> Result<(), GenRomProfileError> {
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

/// Requires every song header, voicegroup, key-split table and sample to be
/// its own place: two ids at one address, or two samples or tables whose bytes
/// overlap, mean one of the derivations picked the wrong copy.
///
/// Voicegroups are exempt from the overlap rule: a biased group's address is an
/// alias before its first declared slot, and a child may be the tail of a
/// linked neighbour, so only their addresses must differ.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] naming the second of the two roots.
fn check_distinct_addresses(plan: &AudioPlan) -> Result<(), GenRomProfileError> {
    distinct(
        "song header",
        plan.songs
            .iter()
            .map(|song| (song.id.clone(), song.header, 1)),
    )?;
    distinct(
        "voicegroup",
        plan.voicegroups
            .iter()
            .map(|group| (group.id.clone(), group.addr, 1)),
    )?;
    distinct(
        "key-split table",
        plan.keysplits.iter().map(|table| {
            (
                format!("keysplit_{}", table.label),
                table.addr + u32::from(table.starting_note),
                u32::from(table.len),
            )
        }),
    )?;
    let samples = plan
        .direct_sound
        .iter()
        .chain(&plan.programmable_wave)
        .map(|sample| {
            // The PCM is followed by wav2agb's interpolation-guard byte.
            let guard = u32::from(sample.header_len != 0);
            (
                sample.id.clone(),
                sample.addr,
                sample.header_len + sample.data_len + guard,
            )
        });
    distinct("sample", samples)
}

/// Requires `roots` (id, address, byte length) to occupy disjoint ranges.
fn distinct(
    what: &str,
    roots: impl Iterator<Item = (String, u32, u32)>,
) -> Result<(), GenRomProfileError> {
    let mut sorted: Vec<(String, u32, u32)> = roots.collect();
    sorted.sort_by_key(|(_, addr, _)| *addr);
    for pair in sorted.windows(2) {
        let [(first, start, len), (second, next, _)] = pair else {
            continue;
        };
        if next < &(start + len.max(&1)) {
            return Err(GenRomProfileError::StructMismatch {
                id: second.clone(),
                reason: format!(
                    "the {what} at {next:08X} overlaps {first}, which holds {start:08X}..{:08X}",
                    start + len
                ),
            });
        }
    }
    Ok(())
}

pub(super) fn leak_str(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

fn leak_slice<T>(items: Vec<T>) -> &'static [T] {
    Box::leak(items.into_boxed_slice())
}

fn bus_ptr(id: &str, addr: u32) -> Result<rom_import::GbaPtr, GenRomProfileError> {
    rom_import::GbaPtr::new(addr).ok_or_else(|| GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason: format!("{addr:08X} is outside the ROM"),
    })
}

fn importer_samples(
    plans: &[SamplePlan],
) -> Result<Vec<rom_import::SampleRoot>, GenRomProfileError> {
    plans
        .iter()
        .map(|sample| {
            Ok(rom_import::SampleRoot {
                id: leak_str(&sample.id),
                addr: bus_ptr(&sample.id, sample.addr)?,
                header_len: sample.header_len,
                data_len: sample.data_len,
            })
        })
        .collect()
}

/// `plan` as the importer's cry roots.
///
/// The roots borrow `'static` data, so the strings and slices are leaked; the
/// generator builds them once per run.
pub(super) fn importer_cry_roots(
    plan: &CryPlan,
) -> Result<rom_import::CryRoots, GenRomProfileError> {
    Ok(rom_import::CryRoots {
        normal: bus_ptr("gCryTable", plan.normal)?,
        reverse: bus_ptr("gCryTable_Reverse", plan.reverse)?,
        samples: leak_slice(importer_samples(&plan.samples)?),
    })
}

/// `plan` as the importer's roots, voicegroups in plan order.
///
/// The roots borrow `'static` data, so the strings and slices are leaked; the
/// generator builds them once per run.
fn importer_roots(plan: &AudioPlan) -> Result<rom_import::AudioRoots, GenRomProfileError> {
    let songs = plan
        .songs
        .iter()
        .map(|song| {
            Ok(rom_import::SongRoot {
                id: leak_str(&song.id),
                index: song.index,
                header: bus_ptr(&song.id, song.header)?,
                track_count: song.track_count,
                voicegroup: bus_ptr(&song.id, song.voicegroup)?,
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
                addr: bus_ptr(&group.id, group.addr)?,
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
                addr: bus_ptr(&table.label, table.addr)?,
                starting_note: table.starting_note,
                len: table.len,
            })
        })
        .collect::<Result<Vec<_>, GenRomProfileError>>()?;
    Ok(rom_import::AudioRoots {
        song_table: bus_ptr("gSongTable", plan.song_table)?,
        songs: leak_slice(songs),
        voicegroups: leak_slice(voicegroups),
        keysplits: leak_slice(keysplits),
        direct_sound: leak_slice(importer_samples(&plan.direct_sound)?),
        programmable_wave: leak_slice(importer_samples(&plan.programmable_wave)?),
        cry: plan.cry.as_ref().map(importer_cry_roots).transpose()?,
    })
}
