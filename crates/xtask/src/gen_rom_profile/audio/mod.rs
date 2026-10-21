//! Locating `MUS_TITLE` and everything it plays through.
//!
//! Audio is the one domain the ROM cannot describe on its own. Samples and
//! tables are all *there*, but two facts a reader needs are not encoded
//! anywhere in the image: how many slots a voicegroup declares (the next
//! group is linked straight after it, with nothing in between saying where
//! one ends), and where a key-split table's data really begins (the ROM's
//! pointer is deliberately biased backwards by the table's first note). So
//! this locator reads those two facts from the upstream `sound/` sources,
//! through the same parsers `cargo xtask extract` already uses, and gets
//! every address from the ROM.
//!
//! The walk runs bottom-up, because the bottom is the only part that has a
//! signature:
//!
//! 1. Each `DirectSound` sample's PCM is thousands of unique bytes. Match
//!    it, step back over the 16-byte `WaveData` header, and check the
//!    header's frequency, loop point, and size against the pack entry. The
//!    pack's PCM buffer holds one more byte than `WaveData.size`: a retained
//!    interpolation guard wav2agb's payload writer always emits
//!    (`crates/xtask/src/extract/wav.rs`'s module docs).
//! 2. `voicegroup_title` is the run of 12-byte slots whose `DirectSound`
//!    and programmable-wave slots point at exactly those samples, in the
//!    order `title.inc` declares them.
//! 3. Its key-split and rhythm slots name the child voicegroups and tables.
//! 4. The song header is what points at `voicegroup_title`; the song table
//!    entry is what points at the header; `gSongTable` is that entry minus
//!    the song's own `MUS_*` index.
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

/// A `WaveData` header is a type, a status, and three `u32` fields.
const WAVE_HEADER_BYTES: u32 = 16;
/// `WaveData.status`'s loop-in-progress bit, mirroring `rom-import`'s reader.
const WAVE_STATUS_LOOP: u16 = 0x4000;
/// One `ToneData` slot is 12 bytes.
const SLOT_BYTES: u32 = 12;
/// Offset of a slot's data pointer.
const SLOT_POINTER: u32 = 4;
/// Offset of a slot's key-split table pointer, which overlays the envelope.
const SLOT_TABLE: u32 = 8;
/// A `voice_keysplit` slot's type byte.
const TYPE_KEYSPLIT: u8 = 0x40;
/// A `voice_keysplit_all` (rhythm) slot's type byte.
const TYPE_RHYTHM: u8 = 0x80;
/// One `gSongTable` entry is a header pointer and two `u16` player ids.
const SONG_TABLE_ENTRY_BYTES: u32 = 8;
/// `Song.ms`'s offset within a `gSongTable` entry.
const SONG_TABLE_MS_OFFSET: u32 = 4;
/// The most tracks any song header declares.
const MAX_TRACKS: u8 = 16;
/// A CGB programmable-wave table is 32 four-bit samples.
const WAVE_TABLE_BYTES: usize = 16;
/// Every voicegroup is addressed over 128 slots, whatever it declares:
/// the mixer's `voicegroup + voice * 12` fetch is unchecked.
const ADDRESSABLE_SLOTS: u16 = 128;

/// The pack-id prefix for `DirectSound` samples.
const DIRECT_SOUND_PREFIX: &str = "audio/sample/direct-sound/";
/// The pack-id prefix for programmable-wave tables.
const PROGRAMMABLE_WAVE_PREFIX: &str = "audio/sample/programmable-wave/";
/// The upstream symbol prefix a `DirectSound` slot names.
const DIRECT_SOUND_SYMBOL: &str = "DirectSoundWaveData_";
/// The upstream symbol prefix a programmable-wave slot names.
const PROGRAMMABLE_WAVE_SYMBOL: &str = "ProgrammableWaveData_";

/// Locate the audio domain.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack's `audio/song/` ids
/// are anything but `extract::midi`'s [`SUPPORTED_SONG_ID`],
/// [`GenRomProfileError::MissingUpstreamCheckout`] without a `pokeemerald/`
/// checkout, [`GenRomProfileError::UpstreamSource`] if one of its `sound/`
/// files does not parse, plus any locator failure.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<AudioPlan, GenRomProfileError> {
    // Checked first, and before any of the ROM searches below: a pack built
    // for a song this locator was never written for should fail on that
    // fact, not on a coincidental missing-checkout or sample-search error.
    // Every derivation past this point -- the title voicegroup, its
    // children, and `gSongTable` -- is written around this one song, never
    // a family of them.
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

    let label = "title";
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
