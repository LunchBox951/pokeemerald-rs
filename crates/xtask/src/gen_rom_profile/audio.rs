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
//!    header's frequency, loop point, and size against the pack entry.
//! 2. `voicegroup_title` is the run of 12-byte slots whose `DirectSound`
//!    and programmable-wave slots point at exactly those samples, in the
//!    order `title.inc` declares them.
//! 3. Its key-split and rhythm slots name the child voicegroups and tables.
//! 4. The song header is what points at `voicegroup_title`; the song table
//!    entry is what points at the header; `gSongTable` is that entry minus
//!    the song's own `MUS_*` index.

use std::path::Path;

use crate::extract::voicegroups::parser::{RawSlot, RawVoiceGroup};
use crate::extract::voicegroups::{index_voicegroup_sources, parser};

use super::error::GenRomProfileError;
use super::locate::{exactly_one, only_one_matching, slice_at_addr, to_offset, u32_at_addr};
use super::plan::{
    AudioPlan, KeysplitPlan, ReportLine, Resolution, SamplePlan, SongPlan, SymbolExpectation,
    VoicegroupPlan,
};
use super::Context;

/// A `WaveData` header is a type, a status, and three `u32` fields.
const WAVE_HEADER_BYTES: u32 = 16;
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
/// [`GenRomProfileError::MissingUpstreamCheckout`] without a `pokeemerald/`
/// checkout, [`GenRomProfileError::UpstreamSource`] if one of its `sound/`
/// files does not parse, plus any locator failure.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<AudioPlan, GenRomProfileError> {
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

    let songs_with_groups = ctx.pack.ids_with_prefix("audio/song/");
    let mut voicegroups = Vec::new();
    let mut keysplits = Vec::new();
    let mut songs = Vec::new();
    let mut song_table = None;

    for song_id in songs_with_groups {
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
        voicegroups.push(plan_for(label, root, group));

        let children = walk_children(ctx, root, group, &groups, report)?;
        voicegroups.extend(children.0);
        keysplits.extend(children.1);

        let (song, table) = locate_song(ctx, &song_id, root, report)?;
        songs.push(song);
        song_table = Some(table);
    }

    let song_table = song_table.ok_or_else(|| GenRomProfileError::StructMismatch {
        id: "gSongTable".to_owned(),
        reason: "the pack holds no song, so nothing locates the table".to_owned(),
    })?;
    keysplits.sort_by(|a, b| a.label.cmp(&b.label));
    voicegroups.sort_by(|a, b| a.label.cmp(&b.label));

    Ok(AudioPlan {
        song_table,
        songs,
        voicegroups,
        keysplits,
        direct_sound,
        programmable_wave,
    })
}

/// What a linker map should say at a voicegroup's address.
///
/// An unbiased group declares a real global. A group with a
/// `starting_note` bias is a `.set` to an address before its first slot,
/// which need not appear as a defined symbol at all.
fn voicegroup_symbol(label: &str, starting_note: u8) -> SymbolExpectation {
    if starting_note == 0 {
        SymbolExpectation::Exact(format!("voicegroup_{label}"))
    } else {
        SymbolExpectation::Unnamed
    }
}

/// Describe one voicegroup, biasing its address the way the ROM does.
fn plan_for(label: &str, addr: u32, group: &RawVoiceGroup) -> VoicegroupPlan {
    VoicegroupPlan {
        id: format!("audio/voicegroup/{label}"),
        label: label.to_owned(),
        addr,
        starting_note: group.starting_note,
        declared_slots: u16::try_from(group.slots.len()).unwrap_or(ADDRESSABLE_SLOTS),
    }
}

/// Locate every `DirectSound` sample by its PCM, then check its header.
fn locate_direct_sound(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<SamplePlan>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix(DIRECT_SOUND_PREFIX);
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(pcm_of(ctx, id)?.to_vec());
    }
    let hits = ctx.raw.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let pcm_addr = exactly_one(id, &hits[index])?;
        let addr = pcm_addr.checked_sub(WAVE_HEADER_BYTES).ok_or_else(|| {
            GenRomProfileError::StructMismatch {
                id: id.clone(),
                reason: "the PCM starts before a header could fit".to_owned(),
            }
        })?;
        check_wave_header(ctx, id, addr, &needles[index])?;
        let data_len = u32::try_from(needles[index].len()).expect("a sample fits in u32");
        let symbol = format!(
            "{DIRECT_SOUND_SYMBOL}{}",
            id.trim_start_matches(DIRECT_SOUND_PREFIX)
        );
        report.push(
            ReportLine::unique(id, addr, WAVE_HEADER_BYTES + data_len)
                .symbol(symbol)
                .note("PCM matched; WaveData header checked"),
        );
        plans.push(SamplePlan {
            id: id.clone(),
            addr,
            header_len: WAVE_HEADER_BYTES,
            data_len,
        });
    }
    Ok(plans)
}

/// The PCM bytes of a `DirectSound` pack entry.
fn pcm_of<'a>(ctx: &'a Context<'_>, id: &str) -> Result<&'a [u8], GenRomProfileError> {
    let payload = &ctx.pack.get(id)?.payload;
    payload
        .get(14..)
        .ok_or_else(|| GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: "a DirectSound sample payload is shorter than its own header".to_owned(),
        })
}

/// Check a `WaveData` header against the pack entry's own fields.
fn check_wave_header(
    ctx: &Context<'_>,
    id: &str,
    addr: u32,
    pcm: &[u8],
) -> Result<(), GenRomProfileError> {
    let payload = &ctx.pack.get(id)?.payload;
    let word = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().expect("four bytes"));
    let expected_frequency = word(1);
    let expected_loop = if payload[5] == 1 { word(6) } else { 0 };
    let expected_size = u32::try_from(pcm.len()).expect("a sample fits in u32");

    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason,
    };
    let frequency = u32_at_addr(ctx.rom, addr + 4).ok_or_else(|| mismatch("truncated".into()))?;
    let loop_start = u32_at_addr(ctx.rom, addr + 8).ok_or_else(|| mismatch("truncated".into()))?;
    let size = u32_at_addr(ctx.rom, addr + 12).ok_or_else(|| mismatch("truncated".into()))?;
    if frequency != expected_frequency || loop_start != expected_loop || size != expected_size {
        return Err(mismatch(format!(
            "WaveData at {addr:08X} reads freq {frequency}, loop {loop_start}, size {size}; \
             the pack says {expected_frequency}, {expected_loop}, {expected_size}"
        )));
    }
    Ok(())
}

/// A programmable-wave table's length, as the `u32` the profile records.
fn wave_table_len() -> u32 {
    u32::try_from(WAVE_TABLE_BYTES).expect("16 fits in u32")
}

/// Locate every programmable-wave table by its 16 bytes.
fn locate_programmable_wave(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<SamplePlan>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix(PROGRAMMABLE_WAVE_PREFIX);
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        let payload = &ctx.pack.get(id)?.payload;
        let table =
            payload
                .get(1..1 + WAVE_TABLE_BYTES)
                .ok_or_else(|| GenRomProfileError::EntryShape {
                    id: id.clone(),
                    reason: "a programmable-wave payload is not a 16-byte table".to_owned(),
                })?;
        needles.push(table.to_vec());
    }
    let hits = ctx.raw.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let addr = exactly_one(id, &hits[index])?;
        // The pack pads the index to two digits; upstream does not.
        let number = id
            .trim_start_matches(PROGRAMMABLE_WAVE_PREFIX)
            .trim_start_matches('0');
        report.push(
            ReportLine::unique(id, addr, wave_table_len())
                .symbol(format!("{PROGRAMMABLE_WAVE_SYMBOL}{number}")),
        );
        plans.push(SamplePlan {
            id: id.clone(),
            addr,
            header_len: 0,
            data_len: wave_table_len(),
        });
    }
    Ok(plans)
}

/// Find the run of slots that is this voicegroup.
///
/// The signature is every slot that names a sample: the ROM's pointer at
/// that slot must be the address already found for that sample. A group
/// with a `starting_note` bias is addressed before its first slot, exactly
/// as the mixer indexes it, so the returned address is biased too.
fn locate_voicegroup(
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

fn walk_children(
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
        let kind = super::locate::u8_at_addr(ctx.rom, slot_addr);
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
    line.symbol = if table.starting_note == 0 {
        SymbolExpectation::Exact(format!("keysplit_{label}"))
    } else {
        // A biased table's label is a `.set`, not a defined symbol, and
        // sits before the data. A map need not name it.
        SymbolExpectation::Unnamed
    };
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

/// Find the song header that plays through `voicegroup`, and its table.
fn locate_song(
    ctx: &Context<'_>,
    song_id: &str,
    voicegroup: u32,
    report: &mut Vec<ReportLine>,
) -> Result<(SongPlan, u32), GenRomProfileError> {
    let header = only_one_matching(
        song_id,
        "the MP2K song-header layout",
        ctx.pointers
            .refs_to(voicegroup)
            .iter()
            .filter_map(|at| at.checked_sub(SLOT_POINTER)),
        |&base| is_song_header(ctx, base),
    )?;
    let track_count = super::locate::u8_at_addr(ctx.rom, header).unwrap_or(0);

    let entry = only_one_matching(
        song_id,
        "a gSongTable entry",
        ctx.pointers.refs_to(header).iter().copied(),
        |_| true,
    )?;
    let index = song_index(&ctx.upstream, song_id)?;
    let table = entry
        .checked_sub(u32::from(index) * SONG_TABLE_ENTRY_BYTES)
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: "gSongTable".to_owned(),
            reason: format!("entry {index} at {entry:08X} puts the table before the ROM"),
        })?;

    report.push(
        ReportLine::unique(song_id, header, 8 + u32::from(track_count) * 4)
            .with(Resolution::StructDerived)
            .note(format!("{track_count} tracks")),
    );
    report.push(
        ReportLine::unique("gSongTable", table, 0)
            .with(Resolution::PointerWalk)
            .symbol("gSongTable")
            .note(format!("entry {index} points at the song header")),
    );

    Ok((
        SongPlan {
            id: song_id.to_owned(),
            index,
            header,
            track_count,
            voicegroup,
        },
        table,
    ))
}

/// Whether the bytes at `base` read as an MP2K song header.
fn is_song_header(ctx: &Context<'_>, base: u32) -> bool {
    let Some(track_count) = super::locate::u8_at_addr(ctx.rom, base) else {
        return false;
    };
    if track_count == 0 || track_count > MAX_TRACKS {
        return false;
    }
    (0..u32::from(track_count)).all(|track| {
        u32_at_addr(ctx.rom, base + 8 + track * 4).is_some_and(|value| to_offset(value).is_some())
    })
}

/// The `MUS_*` index a song pack id names, from `constants/songs.h`.
fn song_index(upstream: &Path, song_id: &str) -> Result<u16, GenRomProfileError> {
    let stem = song_id.rsplit('/').next().unwrap_or(song_id);
    let symbol = stem.to_ascii_uppercase();
    let path = upstream.join("include/constants/songs.h");
    let text =
        std::fs::read_to_string(&path).map_err(|err| GenRomProfileError::UpstreamSource {
            path: path.clone(),
            reason: err.to_string(),
        })?;
    parse_song_constant(&text, &symbol).ok_or(GenRomProfileError::UpstreamSource {
        path,
        reason: format!("no `#define {symbol}` with a numeric value"),
    })
}

/// Find `#define <symbol> <number>` in a C header.
fn parse_song_constant(text: &str, symbol: &str) -> Option<u16> {
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        if tokens.next() != Some("#define") || tokens.next() != Some(symbol) {
            continue;
        }
        if let Some(value) = tokens.next().and_then(|token| token.parse().ok()) {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_song_constant;

    #[test]
    fn a_song_constant_is_read_past_its_trailing_comment() {
        let text =
            "#define MUS_TITLE3 411\n#define MUS_TITLE                   413 // MUS_TITLE3\n";
        assert_eq!(parse_song_constant(text, "MUS_TITLE"), Some(413));
        assert_eq!(parse_song_constant(text, "MUS_TITLE3"), Some(411));
        assert_eq!(parse_song_constant(text, "MUS_NOPE"), None);
    }

    #[test]
    fn a_non_numeric_define_is_not_a_song_index() {
        assert_eq!(
            parse_song_constant("#define MUS_ALIAS MUS_TITLE\n", "MUS_ALIAS"),
            None
        );
    }
}
