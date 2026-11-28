//! The Name Rater's original-trainer check (`MonOTNameNotPlayer`,
//! `src/field_specials.c:1572-1583`).
//!
//! The predicate only. Choosing the party slot, dispatching the special and
//! the Name Rater's UI belong to callers.

use crate::save::block::PLAYER_NAME_BUF_LEN;
use crate::save::BOX_OT_NAME_LEN;
use crate::text::EOS;

/// `GAME_LANGUAGE` (`include/constants/global.h:30`): `LANGUAGE_ENGLISH`
/// (`include/constants/global.h:21`).
pub const GAME_LANGUAGE: u8 = 2;

/// Reports whether a party Pokemon was not originally the player's, as
/// upstream's `MonOTNameNotPlayer` does after it has read the Pokemon's data.
///
/// A language other than [`GAME_LANGUAGE`] is not the player's. Otherwise the
/// names compare byte by byte as `StringCompare` does
/// (`src/string_util.c:124-135`): equal when every byte matches up to a shared
/// `EOS`, ignoring what follows it. `GetMonData(MON_DATA_OT_NAME)` copies the
/// seven stored bytes and appends `EOS` (`src/pokemon.c:3808-3820`), so a
/// seven-glyph OT name is terminated one byte past its field.
#[must_use]
pub fn mon_ot_name_not_player(
    mon_language: u8,
    player_name: &[u8; PLAYER_NAME_BUF_LEN],
    mon_ot_name: &[u8; BOX_OT_NAME_LEN],
) -> bool {
    if mon_language != GAME_LANGUAGE {
        return true;
    }

    let mut ot_name = [EOS; PLAYER_NAME_BUF_LEN];
    ot_name[..BOX_OT_NAME_LEN].copy_from_slice(mon_ot_name);

    for (player_byte, ot_byte) in player_name.iter().zip(ot_name) {
        if *player_byte != ot_byte {
            return true;
        }
        if ot_byte == EOS {
            return false;
        }
    }
    // The appended `EOS` ends the comparison before the buffers run out.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: [u8; PLAYER_NAME_BUF_LEN] = [0xBB, 0xC0, 0xC5, 0xC8, 0xFF, 0x00, 0x00, 0x00];
    const SAME_OT: [u8; BOX_OT_NAME_LEN] = [0xBB, 0xC0, 0xC5, 0xC8, 0xFF, 0x00, 0x00];

    #[test]
    fn game_language_is_english() {
        assert_eq!(GAME_LANGUAGE, 2);
    }

    #[test]
    fn identical_names_in_the_game_language_are_the_player() {
        assert!(!mon_ot_name_not_player(GAME_LANGUAGE, &PLAYER, &SAME_OT));
    }

    #[test]
    fn every_other_language_is_not_the_player() {
        for language in (0..=u8::MAX).filter(|&l| l != GAME_LANGUAGE) {
            assert!(
                mon_ot_name_not_player(language, &PLAYER, &SAME_OT),
                "language {language}"
            );
        }
    }

    #[test]
    fn a_differing_byte_before_the_terminator_is_not_the_player() {
        for position in 0..4 {
            let mut ot = SAME_OT;
            ot[position] ^= 1;
            assert!(
                mon_ot_name_not_player(GAME_LANGUAGE, &PLAYER, &ot),
                "position {position}"
            );
        }
    }

    #[test]
    fn bytes_after_the_first_terminator_are_ignored() {
        let mut player = PLAYER;
        player[5..].copy_from_slice(&[0x12, 0x34, 0x56]);
        let mut ot = SAME_OT;
        ot[5..].copy_from_slice(&[0xA1, 0xB2]);
        assert!(!mon_ot_name_not_player(GAME_LANGUAGE, &player, &ot));
    }

    #[test]
    fn a_name_that_is_a_prefix_of_the_other_differs() {
        let longer_ot = [0xBB, 0xC0, 0xC5, 0xC8, 0xC9, 0xFF, 0x00];
        assert!(mon_ot_name_not_player(GAME_LANGUAGE, &PLAYER, &longer_ot));

        let mut longer_player = PLAYER;
        longer_player[4] = 0xC9;
        longer_player[5] = EOS;
        assert!(mon_ot_name_not_player(
            GAME_LANGUAGE,
            &longer_player,
            &SAME_OT
        ));
    }

    #[test]
    fn an_empty_name_matches_only_an_empty_name() {
        let empty_player = [EOS; PLAYER_NAME_BUF_LEN];
        let empty_ot = [EOS; BOX_OT_NAME_LEN];
        assert!(!mon_ot_name_not_player(
            GAME_LANGUAGE,
            &empty_player,
            &empty_ot
        ));
        assert!(mon_ot_name_not_player(GAME_LANGUAGE, &PLAYER, &empty_ot));
        assert!(mon_ot_name_not_player(
            GAME_LANGUAGE,
            &empty_player,
            &SAME_OT
        ));
    }

    #[test]
    fn a_full_width_ot_name_ends_at_the_terminator_the_read_appends() {
        let full_ot = [0xBB, 0xC0, 0xC5, 0xC8, 0xC9, 0xCA, 0xCB];
        let mut full_player = [0; PLAYER_NAME_BUF_LEN];
        full_player[..7].copy_from_slice(&full_ot);
        full_player[7] = EOS;
        assert!(!mon_ot_name_not_player(
            GAME_LANGUAGE,
            &full_player,
            &full_ot
        ));

        let mut other_last = full_ot;
        other_last[6] ^= 1;
        assert!(mon_ot_name_not_player(
            GAME_LANGUAGE,
            &full_player,
            &other_last
        ));
    }

    #[test]
    fn a_player_name_with_no_terminator_never_matches_a_seven_byte_ot_name() {
        let full_ot = [0xBB, 0xC0, 0xC5, 0xC8, 0xC9, 0xCA, 0xCB];
        let mut unterminated = [0; PLAYER_NAME_BUF_LEN];
        unterminated[..7].copy_from_slice(&full_ot);
        unterminated[7] = 0xCC;
        assert!(mon_ot_name_not_player(
            GAME_LANGUAGE,
            &unterminated,
            &full_ot
        ));
    }

    #[test]
    fn a_full_width_name_is_not_equal_to_a_shorter_one_sharing_its_prefix() {
        let full_ot = [0xBB, 0xC0, 0xC5, 0xC8, 0xC9, 0xCA, 0xCB];
        assert!(mon_ot_name_not_player(GAME_LANGUAGE, &PLAYER, &full_ot));
    }
}
