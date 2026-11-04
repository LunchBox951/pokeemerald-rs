//! Locates the audio plan for every pack song and everything it plays
//! through, and the species cries.

mod closure;
mod cry;
mod samples;
mod songs;
mod validation;
mod voicegroups;

#[cfg(test)]
mod multi_song_tests;

use crate::extract::midi::SONG_PACK_ID as SUPPORTED_SONG_ID;
use std::collections::BTreeMap;

use crate::extract::voicegroups::index_voicegroup_sources;

use super::error::GenRomProfileError;
use super::plan::{AudioPlan, ReportLine, Resolution};
use super::Context;

use closure::{walk, Sources};
use cry::locate_cry;
use samples::{locate_direct_sound, locate_programmable_wave};
use songs::{locate_table, pack_song, plan_song};
use validation::check_plan;
use voicegroups::{locate_voicegroup, read_keysplit_tables};

const WAVE_HEADER_BYTES: u32 = 16;
const WAVE_STATUS_LOOP: u16 = 0x4000;
const SLOT_BYTES: u32 = 12;
const SLOT_POINTER: u32 = 4;
/// A key-split slot's table pointer overlays the envelope bytes.
const SLOT_TABLE: u32 = 8;
const TYPE_KEYSPLIT: u8 = 0x40;
const TYPE_RHYTHM: u8 = 0x80;
const SONG_TABLE_ENTRY_BYTES: u32 = 8;
const SONG_TABLE_MS_OFFSET: u32 = 4;
const MAX_TRACKS: u8 = 16;
const WAVE_TABLE_BYTES: usize = 16;
/// Every voicegroup is addressed over 128 slots, whatever it declares,
/// because the mixer's slot fetch is unchecked.
const ADDRESSABLE_SLOTS: u16 = 128;

const DIRECT_SOUND_PREFIX: &str = "audio/sample/direct-sound/";
const PROGRAMMABLE_WAVE_PREFIX: &str = "audio/sample/programmable-wave/";
const DIRECT_SOUND_SYMBOL: &str = "DirectSoundWaveData_";
const PROGRAMMABLE_WAVE_SYMBOL: &str = "ProgrammableWaveData_";

/// Plans the audio for every pack song: its table entry and header, the
/// voicegroups, key-split tables and samples it plays through, and the species
/// cries.
///
/// `gSongTable` is anchored once, from [`SUPPORTED_SONG_ID`]'s voicegroup;
/// every song then comes from its constant's table entry, and every other
/// group, table and sample from the pointers of the groups reached.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack's `audio/song/` ids
/// do not include [`SUPPORTED_SONG_ID`], or if the ROM disagrees with the
/// pack anywhere in the closure,
/// [`GenRomProfileError::MissingUpstreamCheckout`] without a `pokeemerald/`
/// checkout, [`GenRomProfileError::UpstreamSource`] if one of its `sound/`
/// files does not parse, plus any locator failure.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<AudioPlan, GenRomProfileError> {
    let song_ids = ctx.pack.ids_with_prefix("audio/song/");
    if !song_ids.iter().any(|id| id == SUPPORTED_SONG_ID) {
        return Err(GenRomProfileError::StructMismatch {
            id: "audio/song/*".to_owned(),
            reason: format!("[\"{SUPPORTED_SONG_ID}\"] is required, the pack holds {song_ids:?}"),
        });
    }

    if !ctx.upstream.join("sound").is_dir() {
        return Err(GenRomProfileError::MissingUpstreamCheckout(
            ctx.upstream.clone(),
        ));
    }
    let direct_sound = locate_direct_sound(ctx)?;
    let programmable_wave = locate_programmable_wave(ctx)?;

    let groups = index_voicegroup_sources(&ctx.upstream)
        .map_err(|err| GenRomProfileError::UpstreamSource {
            path: ctx.upstream.join("sound/voicegroups"),
            reason: err.to_string(),
        })?
        .groups_by_label;
    let tables = read_keysplit_tables(&ctx.upstream)?;

    let anchor_group = pack_song(ctx, SUPPORTED_SONG_ID)?.voicegroup().0.clone();
    let label = anchor_group
        .strip_prefix("audio/voicegroup/")
        .unwrap_or(&anchor_group);
    let group = groups
        .get(label)
        .ok_or_else(|| GenRomProfileError::UpstreamSource {
            path: ctx.upstream.join("sound/voicegroups"),
            reason: format!("no voicegroup declares the label `{label}`"),
        })?;
    let anchor = locate_voicegroup(ctx, label, group, &direct_sound, &programmable_wave)?;
    let song_table = locate_table(ctx, SUPPORTED_SONG_ID, anchor)?;
    report.push(
        ReportLine::unique("gSongTable", song_table, 0)
            .with(Resolution::PointerWalk)
            .symbol("gSongTable")
            .note(format!(
                "{SUPPORTED_SONG_ID}'s entry points at its header; {} songs planned",
                song_ids.len()
            )),
    );

    let mut songs = Vec::with_capacity(song_ids.len());
    let mut roots = BTreeMap::new();
    let mut indices = BTreeMap::new();
    for id in &song_ids {
        let located = plan_song(ctx, song_table, id, report)?;
        if let Some(other) = indices.insert(located.plan.index, id.clone()) {
            return Err(GenRomProfileError::StructMismatch {
                id: id.clone(),
                reason: format!(
                    "{other} has the same gSongTable index {}",
                    located.plan.index
                ),
            });
        }
        match roots.insert(located.voicegroup_id.clone(), located.plan.voicegroup) {
            Some(bound) if bound != located.plan.voicegroup => {
                return Err(GenRomProfileError::StructMismatch {
                    id: id.clone(),
                    reason: format!(
                        "{} is at {bound:08X} for an earlier song and {:08X} here",
                        located.voicegroup_id, located.plan.voicegroup
                    ),
                });
            }
            _ => {}
        }
        songs.push(located.plan);
    }

    let closure = walk(
        ctx,
        &Sources {
            groups: &groups,
            tables: &tables,
            direct_sound: &direct_sound,
            programmable_wave: &programmable_wave,
        },
        &roots,
        report,
    )?;
    let cry = locate_cry(ctx, report)?;

    let plan = AudioPlan {
        song_table,
        songs,
        voicegroups: closure.voicegroups,
        keysplits: closure.keysplits,
        direct_sound: closure.direct_sound,
        programmable_wave: closure.programmable_wave,
        cry,
    };
    check_plan(ctx, &plan)?;
    Ok(plan)
}

/// The payload of pack entry `id`, which the ROM importers always emit as a
/// `Raw` entry.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] when `id` is absent;
/// [`GenRomProfileError::WrongPackEntryKind`] naming `id` for any other kind.
pub(super) fn raw_payload<'a>(
    ctx: &'a Context<'_>,
    id: &str,
) -> Result<&'a [u8], GenRomProfileError> {
    ctx.pack.raw_payload(id)
}
