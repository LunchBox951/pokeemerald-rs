//! Reads one song's music-player assignment from `sound/song_table.inc`.
//!
//! Each `song <symbol>, <player>, <me>` row of `gSongTable` is a `struct Song` whose `ms` is the
//! `MUSIC_PLAYER_*` index that `m4aSongNumStart` uses to select `gMPlayTable[song->ms]`
//! (`src/m4a.c:107-114`). The `.equiv` lines at the top of the file define those indices.

use super::error::MidiError;

/// `label`, `music_player`, and `unknown` of the `song` macro (`asm/macros/m4a.inc:1-5`).
const SONG_MACRO_OPERANDS: usize = 3;

/// The comment-free, trimmed text of `line`.
fn code_of(line: &str) -> &str {
    line.split('@').next().unwrap_or("").trim()
}

/// Returns the `ms` value of the one `song <symbol>, ...` row, resolving a `MUSIC_PLAYER_*` name
/// through the file's `.equiv` lines and accepting a decimal literal directly. Rows for other
/// symbols are not inspected, so upstream's repeated reserved rows stay legal.
///
/// # Errors
///
/// [`MidiError::SongTableEntryMissing`] when no row names `symbol`;
/// [`MidiError::SongTableEntryDuplicate`] when several do;
/// [`MidiError::SongTablePlayerMalformed`] when the row has no player field, or
/// its player is neither a defined `MUSIC_PLAYER_*` name nor a `u16` literal;
/// [`MidiError::SongTableRowMalformed`] when the row lacks the `me` operand, carries extra
/// operands, or has a `me` operand that is not a `u16` literal.
pub(super) fn player_for(text: &str, symbol: &str) -> Result<u16, MidiError> {
    let mut rows = text
        .lines()
        .filter_map(|line| code_of(line).strip_prefix("song "))
        .map(|rest| rest.split(',').map(str::trim).collect::<Vec<_>>())
        .filter(|fields| fields[0] == symbol);
    let fields = rows
        .next()
        .ok_or_else(|| MidiError::SongTableEntryMissing(symbol.to_owned()))?;
    if rows.next().is_some() {
        return Err(MidiError::SongTableEntryDuplicate(symbol.to_owned()));
    }
    let player = fields
        .get(1)
        .copied()
        .filter(|field| !field.is_empty())
        .ok_or_else(|| MidiError::SongTablePlayerMalformed(String::new()))?;
    let me_is_literal = fields.get(2).is_some_and(|me| me.parse::<u16>().is_ok());
    if fields.len() != SONG_MACRO_OPERANDS || !me_is_literal {
        return Err(MidiError::SongTableRowMalformed(symbol.to_owned()));
    }
    if let Ok(value) = player.parse::<u16>() {
        return Ok(value);
    }
    let equiv_value = text
        .lines()
        .filter_map(|line| code_of(line).strip_prefix(".equiv "))
        .map(|rest| rest.split(',').map(str::trim))
        .find_map(|mut fields| (fields.next() == Some(player)).then(|| fields.next()));
    equiv_value
        .flatten()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| MidiError::SongTablePlayerMalformed(player.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\t.equiv MUSIC_PLAYER_BGM,0\n\t.equiv MUSIC_PLAYER_SE1,1\n\
        gSongTable::\n\tsong mus_dummy, MUSIC_PLAYER_BGM, 0\n\
        \tsong se_use_item, MUSIC_PLAYER_SE1, 1\n\tsong se_raw, 7, 1\n\
        \tsong se_bad, MUSIC_PLAYER_NOPE, 1\n";

    #[test]
    fn a_named_player_resolves_through_its_equiv() {
        assert_eq!(player_for(TABLE, "mus_dummy"), Ok(0));
        assert_eq!(player_for(TABLE, "se_use_item"), Ok(1));
    }

    #[test]
    fn a_literal_player_is_taken_as_written() {
        assert_eq!(player_for(TABLE, "se_raw"), Ok(7));
    }

    #[test]
    fn an_absent_row_is_reported() {
        assert_eq!(
            player_for(TABLE, "mus_title"),
            Err(MidiError::SongTableEntryMissing("mus_title".to_owned()))
        );
    }

    #[test]
    fn a_row_without_a_player_field_is_malformed() {
        assert_eq!(
            player_for("\tsong mus_title\n", "mus_title"),
            Err(MidiError::SongTablePlayerMalformed(String::new()))
        );
        assert_eq!(
            player_for("\tsong mus_title,\n", "mus_title"),
            Err(MidiError::SongTablePlayerMalformed(String::new()))
        );
    }

    #[test]
    fn an_undefined_player_name_is_reported() {
        assert_eq!(
            player_for(TABLE, "se_bad"),
            Err(MidiError::SongTablePlayerMalformed(
                "MUSIC_PLAYER_NOPE".to_owned()
            ))
        );
    }

    #[test]
    fn a_selected_row_listed_twice_is_ambiguous() {
        let table = "\tsong se_select, 2, 2\n\tsong se_select, 2, 2\n";
        assert_eq!(
            player_for(table, "se_select"),
            Err(MidiError::SongTableEntryDuplicate("se_select".to_owned()))
        );
    }

    #[test]
    fn repeated_unselected_rows_stay_legal() {
        let table = "\tsong dummy_song_header, 0, 0\n\tsong dummy_song_header, 0, 0\n\
            \tsong se_select, 2, 2\n";
        assert_eq!(player_for(table, "se_select"), Ok(2));
    }

    #[test]
    fn a_row_missing_its_me_operand_or_carrying_extras_is_malformed() {
        for row in [
            "song se_select, 2",
            "song se_select, 2,",
            "song se_select, 2, x",
            "song se_select, 2, 2, 2",
        ] {
            assert_eq!(
                player_for(row, "se_select"),
                Err(MidiError::SongTableRowMalformed("se_select".to_owned())),
                "{row}"
            );
        }
    }

    #[test]
    fn a_trailing_comment_does_not_change_a_row() {
        assert_eq!(
            player_for("\tsong se_select,2,2 @ comment\n", "se_select"),
            Ok(2)
        );
    }
}
