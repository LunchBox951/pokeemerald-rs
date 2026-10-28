//! Resolves the song header and `gSongTable` for a planned voicegroup.

use std::path::Path;

use assets::audio::Song;

use crate::extract::midi::SONG_PACK_ID as SUPPORTED_SONG_ID;

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{
    only_one_matching, to_offset, u16_at_addr, u32_at_addr, u8_at_addr,
};
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SongPlan};
use crate::gen_rom_profile::Context;

use super::{raw_payload, MAX_TRACKS, SLOT_POINTER, SONG_TABLE_ENTRY_BYTES, SONG_TABLE_MS_OFFSET};

const HEADER_TRACK_COUNT_OFFSET: u32 = 0;
const HEADER_PRIORITY_OFFSET: u32 = 2;
const HEADER_REVERB_OFFSET: u32 = 3;
const HEADER_FIXED_BYTES: u32 = 8;
const TRACK_POINTER_BYTES: u32 = 4;
const REVERB_ENABLED_BIT: u8 = 0x80;

/// Finds the header of the song that plays through `voicegroup`, and the
/// `gSongTable` holding it: the table starts at the header's entry minus
/// the song's `MUS_*` index in entries.
pub(super) fn locate_song(
    ctx: &Context<'_>,
    song_id: &str,
    voicegroup_label: &str,
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
    let track_count = u8_at_addr(ctx.rom, header + HEADER_TRACK_COUNT_OFFSET).unwrap_or(0);
    let entry = only_one_matching(
        song_id,
        "a gSongTable entry",
        ctx.pointers.refs_to(header).iter().copied(),
        |_| true,
    )?;
    check_song_header(ctx, song_id, voicegroup_label, header, entry)?;
    let index = song_index(&ctx.upstream, song_id)?;
    let table = entry
        .checked_sub(u32::from(index) * SONG_TABLE_ENTRY_BYTES)
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: "gSongTable".to_owned(),
            reason: format!("entry {index} at {entry:08X} puts the table before the ROM"),
        })?;

    report.push(
        ReportLine::unique(
            song_id,
            header,
            HEADER_FIXED_BYTES + u32::from(track_count) * TRACK_POINTER_BYTES,
        )
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

/// Checks the located header and `gSongTable` entry against the pack's
/// canonical [`Song`], and requires the ROM's decoded tracks to equal the
/// pack payload byte for byte.
///
/// # Errors
///
/// [`GenRomProfileError::EntryShape`] naming `song_id` when the pack payload
/// does not decode as a song, or whose voicegroup is not
/// `audio/voicegroup/{voicegroup_label}`;
/// [`GenRomProfileError::StructMismatch`] when the header disagrees;
/// [`GenRomProfileError::SongBytecodeMismatch`] when the tracks disagree.
fn check_song_header(
    ctx: &Context<'_>,
    song_id: &str,
    voicegroup_label: &str,
    header: u32,
    entry: u32,
) -> Result<(), GenRomProfileError> {
    let shape = |reason: String| GenRomProfileError::EntryShape {
        id: song_id.to_owned(),
        reason,
    };
    let song = Song::decode(raw_payload(ctx, song_id)?).map_err(|err| shape(err.to_string()))?;
    let expected_group = format!("audio/voicegroup/{voicegroup_label}");
    if song.voicegroup().0 != expected_group {
        return Err(shape(format!(
            "plays through `{}`, not `{expected_group}`",
            song.voicegroup().0
        )));
    }

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
        SUPPORTED_SONG_ID,
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
    Ok(())
}

fn is_song_header(ctx: &Context<'_>, base: u32) -> bool {
    let Some(track_count) = u8_at_addr(ctx.rom, base + HEADER_TRACK_COUNT_OFFSET) else {
        return false;
    };
    if track_count == 0 || track_count > MAX_TRACKS {
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
