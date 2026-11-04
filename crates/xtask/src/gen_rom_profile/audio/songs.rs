//! Resolves the song header and `gSongTable` for a planned voicegroup.

use std::path::Path;

use assets::audio::Song;

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{
    only_one_matching, to_offset, u16_at_addr, u32_at_addr, u8_at_addr,
};
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SongPlan};
use crate::gen_rom_profile::Context;

use super::validation::leak_str;
use super::{raw_payload, MAX_TRACKS, SLOT_POINTER, SONG_TABLE_ENTRY_BYTES, SONG_TABLE_MS_OFFSET};

const HEADER_TRACK_COUNT_OFFSET: u32 = 0;
const HEADER_PRIORITY_OFFSET: u32 = 2;
const HEADER_REVERB_OFFSET: u32 = 3;
const HEADER_FIXED_BYTES: u32 = 8;
const TRACK_POINTER_BYTES: u32 = 4;
const REVERB_ENABLED_BIT: u8 = 0x80;

/// A song's plan and the pack voicegroup its header's tone pointer names.
pub(super) struct LocatedSong {
    pub plan: SongPlan,
    pub voicegroup_id: String,
}

/// Finds `gSongTable` from the one song `song_id` that plays through the
/// already located voicegroup: the table starts at the header's entry minus
/// the song's `MUS_*` index in entries.
pub(super) fn locate_table(
    ctx: &Context<'_>,
    song_id: &str,
    voicegroup: u32,
) -> Result<u32, GenRomProfileError> {
    let header = only_one_matching(
        song_id,
        "the MP2K song-header layout",
        ctx.pointers
            .refs_to(voicegroup)
            .iter()
            .filter_map(|at| at.checked_sub(SLOT_POINTER)),
        |&base| is_playable_song_header(ctx, base),
    )?;
    let entry = only_one_matching(
        song_id,
        "a gSongTable entry",
        ctx.pointers.refs_to(header).iter().copied(),
        |_| true,
    )?;
    let index = song_index(&ctx.upstream, song_id)?;
    entry
        .checked_sub(u32::from(index) * SONG_TABLE_ENTRY_BYTES)
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: "gSongTable".to_owned(),
            reason: format!("entry {index} at {entry:08X} puts the table before the ROM"),
        })
}

/// Reads song `song_id`'s `gSongTable` entry and header, and requires them
/// and the tracks they point at to agree with the pack song.
pub(super) fn plan_song(
    ctx: &Context<'_>,
    table: u32,
    song_id: &str,
    report: &mut Vec<ReportLine>,
) -> Result<LocatedSong, GenRomProfileError> {
    let index = song_index(&ctx.upstream, song_id)?;
    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: song_id.to_owned(),
        reason,
    };
    let entry = table
        .checked_add(u32::from(index) * SONG_TABLE_ENTRY_BYTES)
        .ok_or_else(|| mismatch(format!("entry {index} is past the address space")))?;
    let header = u32_at_addr(ctx.rom, entry)
        .filter(|value| to_offset(*value).is_some())
        .ok_or_else(|| {
            mismatch(format!(
                "the song-table entry {index} at {entry:08X} holds no cartridge pointer"
            ))
        })?;
    if !is_song_header(ctx, header) {
        return Err(mismatch(format!(
            "{header:08X} is not the MP2K song-header layout"
        )));
    }
    let track_count = u8_at_addr(ctx.rom, header + HEADER_TRACK_COUNT_OFFSET).unwrap_or(0);
    let voicegroup = u32_at_addr(ctx.rom, header + SLOT_POINTER)
        .filter(|value| to_offset(*value).is_some())
        .ok_or_else(|| mismatch(format!("the header at {header:08X} has no tone pointer")))?;
    let voicegroup_id = check_song_header(ctx, song_id, header, entry)?;

    report.push(
        ReportLine::unique(
            song_id,
            header,
            HEADER_FIXED_BYTES + u32::from(track_count) * TRACK_POINTER_BYTES,
        )
        .with(Resolution::PointerWalk)
        .note(format!("{track_count} tracks; gSongTable entry {index}")),
    );
    Ok(LocatedSong {
        plan: SongPlan {
            id: song_id.to_owned(),
            index,
            header,
            track_count,
            voicegroup,
        },
        voicegroup_id,
    })
}

/// The pack song `song_id`, decoded.
///
/// # Errors
///
/// The pack-entry errors, or [`GenRomProfileError::EntryShape`] naming
/// `song_id` when the payload does not decode as a song.
pub(super) fn pack_song(ctx: &Context<'_>, song_id: &str) -> Result<Song, GenRomProfileError> {
    Song::decode(raw_payload(ctx, song_id)?).map_err(|err| GenRomProfileError::EntryShape {
        id: song_id.to_owned(),
        reason: err.to_string(),
    })
}

/// Checks the located header and `gSongTable` entry against the pack's
/// canonical [`Song`], and requires the ROM's decoded tracks to equal the
/// pack payload byte for byte. Returns the id of the voicegroup the song
/// plays through.
///
/// # Errors
///
/// [`GenRomProfileError::EntryShape`] naming `song_id` when the pack payload
/// does not decode as a song;
/// [`GenRomProfileError::StructMismatch`] when the header disagrees;
/// [`GenRomProfileError::SongBytecodeMismatch`] when the tracks disagree.
fn check_song_header(
    ctx: &Context<'_>,
    song_id: &str,
    header: u32,
    entry: u32,
) -> Result<String, GenRomProfileError> {
    let song = pack_song(ctx, song_id)?;

    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: song_id.to_owned(),
        reason,
    };
    let byte = |offset: u32| u8_at_addr(ctx.rom, header + offset);
    let track_count = byte(HEADER_TRACK_COUNT_OFFSET);
    if track_count.map(usize::from) != Some(song.tracks().len()) {
        return Err(mismatch(format!(
            "the header at {header:08X} holds {track_count:?} tracks, the pack song {}",
            song.tracks().len()
        )));
    }
    if byte(HEADER_PRIORITY_OFFSET) != Some(song.priority()) {
        return Err(mismatch(format!(
            "the header priority is {:?}, the pack song's is {}",
            byte(HEADER_PRIORITY_OFFSET),
            song.priority()
        )));
    }
    let reverb = byte(HEADER_REVERB_OFFSET)
        .and_then(|value| (value & REVERB_ENABLED_BIT != 0).then_some(value & !REVERB_ENABLED_BIT));
    if reverb != song.reverb() {
        return Err(mismatch(format!(
            "the header reverb is {reverb:?}, the pack song's is {:?}",
            song.reverb()
        )));
    }

    let player = u16_at_addr(ctx.rom, entry + SONG_TABLE_MS_OFFSET);
    if player != Some(song.player().0) {
        return Err(mismatch(format!(
            "the song-table `ms` is {player:?}, the pack song's is {}",
            song.player().0
        )));
    }

    let bytecode = |reason: String| GenRomProfileError::SongBytecodeMismatch {
        id: song_id.to_owned(),
        reason,
    };
    let ptr = rom_import::GbaPtr::new(header)
        .ok_or_else(|| bytecode(format!("the header {header:08X} is outside the ROM")))?;
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded = rom_import::decode_song_payload(
        &reader,
        leak_str(song_id),
        ptr,
        &song.voicegroup().0,
        song.player().0,
    )
    .map_err(|err| bytecode(format!("the ROM tracks do not decode: {err}")))?;
    if decoded != ctx.pack.get(song_id)?.payload {
        return Err(bytecode(
            "the ROM's decoded tracks differ from the pack payload".to_owned(),
        ));
    }
    Ok(song.voicegroup().0.clone())
}

/// A header with a playable track, which anchors the table search: a
/// voicegroup's references include no silent header's.
fn is_playable_song_header(ctx: &Context<'_>, base: u32) -> bool {
    u8_at_addr(ctx.rom, base + HEADER_TRACK_COUNT_OFFSET).is_some_and(|count| count != 0)
        && is_song_header(ctx, base)
}

fn is_song_header(ctx: &Context<'_>, base: u32) -> bool {
    let Some(track_count) = u8_at_addr(ctx.rom, base + HEADER_TRACK_COUNT_OFFSET) else {
        return false;
    };
    if track_count > MAX_TRACKS {
        return false;
    }
    (0..u32::from(track_count)).all(|track| {
        u32_at_addr(
            ctx.rom,
            base + HEADER_FIXED_BYTES + track * TRACK_POINTER_BYTES,
        )
        .is_some_and(|value| to_offset(value).is_some())
    })
}

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
