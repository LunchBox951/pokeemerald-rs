//! Locates the audio plan for `MUS_TITLE` and everything it plays through.

mod samples;
mod songs;
mod validation;
mod voicegroups;

use crate::extract::midi::SONG_PACK_ID as SUPPORTED_SONG_ID;
use crate::extract::voicegroups::index_voicegroup_sources;

use super::error::GenRomProfileError;
use super::plan::{AudioPlan, ReportLine, Resolution};
use super::Context;

use samples::{locate_direct_sound, locate_programmable_wave};
use songs::locate_song;
use validation::check_voicegroups;
use voicegroups::{locate_voicegroup, plan_for, voicegroup_symbol, walk_children};

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
const ROOT_VOICEGROUP_LABEL: &str = "title";

/// Plans the audio for the one supported song, [`SUPPORTED_SONG_ID`].
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack's `audio/song/` ids
/// are anything but [`SUPPORTED_SONG_ID`],
/// [`GenRomProfileError::MissingUpstreamCheckout`] without a `pokeemerald/`
/// checkout, [`GenRomProfileError::UpstreamSource`] if one of its `sound/`
/// files does not parse, plus any locator failure.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<AudioPlan, GenRomProfileError> {
    // Every derivation below is written around this one song, never a family
    // of them, so a pack with other songs must fail here.
    let song_ids = ctx.pack.ids_with_prefix("audio/song/");
    if song_ids.len() != 1 || song_ids[0] != SUPPORTED_SONG_ID {
        return Err(GenRomProfileError::StructMismatch {
            id: "audio/song/*".to_owned(),
            reason: format!(
                "only [\"{SUPPORTED_SONG_ID}\"] is supported, the pack holds {song_ids:?}"
            ),
        });
    }

    if !ctx.upstream.join("sound").is_dir() {
        return Err(GenRomProfileError::MissingUpstreamCheckout(
            ctx.upstream.clone(),
        ));
    }
    let direct_sound = locate_direct_sound(ctx, report)?;
    let programmable_wave = locate_programmable_wave(ctx, report)?;

    let groups = index_voicegroup_sources(&ctx.upstream)
        .map_err(|err| GenRomProfileError::UpstreamSource {
            path: ctx.upstream.join("sound/voicegroups"),
            reason: err.to_string(),
        })?
        .groups_by_label;

    let label = ROOT_VOICEGROUP_LABEL;
    let group = groups
        .get(label)
        .ok_or_else(|| GenRomProfileError::UpstreamSource {
            path: ctx.upstream.join("sound/voicegroups"),
            reason: format!("no voicegroup declares the label `{label}`"),
        })?;
    let root = locate_voicegroup(ctx, label, group, &direct_sound, &programmable_wave)?;
    let mut line = ReportLine::unique(format!("audio/voicegroup/{label}"), root, 0)
        .with(Resolution::StructDerived)
        .note(format!("{} slots declared", group.slots.len()));
    line.symbol = voicegroup_symbol(label, group.starting_note);
    report.push(line);
    let mut voicegroups = vec![plan_for(label, root, group)];

    let (child_voicegroups, mut keysplits) = walk_children(ctx, root, group, &groups, report)?;
    voicegroups.extend(child_voicegroups);

    let (song, song_table) = locate_song(ctx, SUPPORTED_SONG_ID, label, root, report)?;

    keysplits.sort_by(|a, b| a.label.cmp(&b.label));
    voicegroups.sort_by(|a, b| a.label.cmp(&b.label));

    let plan = AudioPlan {
        song_table,
        songs: vec![song],
        voicegroups,
        keysplits,
        direct_sound,
        programmable_wave,
    };
    check_voicegroups(ctx, &plan)?;
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
