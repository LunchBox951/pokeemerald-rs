//! Reads one song's music-player assignment from `sound/song_table.inc`.
//!
//! Each `song <symbol>, <player>, <me>` row of `gSongTable` is a `struct Song` whose `ms` is the
//! `MUSIC_PLAYER_*` index that `m4aSongNumStart` uses to select `gMPlayTable[song->ms]`
//! (`src/m4a.c:107-114`). The `.equiv` lines at the top of the file define those indices.

use super::error::MidiError;

/// Returns the `ms` value of the `song <symbol>, ...` row, resolving a `MUSIC_PLAYER_*` name
/// through the file's `.equiv` lines and accepting a decimal literal directly.
///
/// # Errors
///
/// [`MidiError::SongTableEntryMissing`] when no row names `symbol`;
/// [`MidiError::SongTablePlayerMalformed`] when the row has no player field, or
/// its player is neither a defined `MUSIC_PLAYER_*` name nor a `u16` literal.
pub(super) fn player_for(text: &str, symbol: &str) -> Result<u16, MidiError> {
    // `Some(None)` is a row that names `symbol` but carries no player field:
    // malformed table data, which must not read as an absent row.
    let mut row_player = None;
    for line in text.lines() {
        let line = line.split('@').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("song ") {
            let mut fields = rest.split(',').map(str::trim);
            if fields.next() == Some(symbol) {
                row_player = Some(fields.next().filter(|field| !field.is_empty()));
                break;
            }
        }
    }
    let player = row_player
        .ok_or_else(|| MidiError::SongTableEntryMissing(symbol.to_owned()))?
        .ok_or_else(|| MidiError::SongTablePlayerMalformed(String::new()))?;
    if let Ok(value) = player.parse::<u16>() {
        return Ok(value);
    }
    for line in text.lines() {
        let line = line.split('@').next().unwrap_or("").trim();
        let Some(rest) = line.strip_prefix(".equiv ") else {
            continue;
        };
        let mut fields = rest.split(',').map(str::trim);
        if fields.next() == Some(player) {
            return fields
                .next()
                .and_then(|value| value.parse().ok())
                .ok_or_else(|| MidiError::SongTablePlayerMalformed(player.to_owned()));
        }
    }
    Err(MidiError::SongTablePlayerMalformed(player.to_owned()))
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
}
