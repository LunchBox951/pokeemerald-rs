//! Locating the song header and table entry, and reading the song's `MUS_*` constant, with the song-constant tests.

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

/// Find the song header that plays through `voicegroup`, and its table.
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
    let track_count = u8_at_addr(ctx.rom, header).unwrap_or(0);
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

/// Decode the pack's song through the canonical wire schema and check the
/// located `SongHeader` against it: track count, priority, and reverb, and
/// the `gSongTable` entry at `entry` against its `Song.ms` player index.
///
/// It then decodes the ROM's track byte-code with `rom-import` and requires
/// the canonical bytes to equal the pack payload exactly.
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
    let track_count = byte(0);
    if track_count.map(usize::from) != Some(song.tracks().len()) {
        return Err(mismatch(format!(
            "the header at {header:08X} holds {track_count:?} tracks, the pack song {}",
            song.tracks().len()
        )));
    }
    if byte(2) != Some(song.priority()) {
        return Err(mismatch(format!(
            "the header priority is {:?}, the pack song's is {}",
            byte(2),
            song.priority()
        )));
    }
    let reverb = byte(3).and_then(|value| (value & 0x80 != 0).then_some(value & !0x80));
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

/// Whether the bytes at `base` read as an MP2K song header.
fn is_song_header(ctx: &Context<'_>, base: u32) -> bool {
    let Some(track_count) = u8_at_addr(ctx.rom, base) else {
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
