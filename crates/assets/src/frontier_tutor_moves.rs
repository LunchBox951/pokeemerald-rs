//! The two Battle Frontier tutor menus: the move each row teaches
//! (`src/field_specials.c:3059-3093`). Rows start at `0`; out-of-range rows are refused.

use std::error::Error;
use std::fmt;

use crate::battle_moves::MoveId;

/// The row count of each tutor menu (`src/field_specials.c:3059-3085`).
pub const TUTOR_MENU_LEN: usize = 10;

/// Which of the two tutor menus a script selects with `gSpecialVar_0x8005`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrontierTutorMenu {
    /// `sBattleFrontier_TutorMoves1`, chosen when the selector is `0` (`src/field_specials.c:3089-3092`).
    First,
    /// `sBattleFrontier_TutorMoves2`, chosen for any other selector.
    Second,
}

/// A refused tutor menu lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierTutorError {
    /// The row is not below [`TUTOR_MENU_LEN`].
    InvalidRow(usize),
}

impl fmt::Display for FrontierTutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRow(row) => write!(
                f,
                "Battle Frontier tutor menu row `{row}` is out of range (there are \
                 {TUTOR_MENU_LEN})"
            ),
        }
    }
}

impl Error for FrontierTutorError {}

/// `sBattleFrontier_TutorMoves1` (`src/field_specials.c:3059-3071`).
const TUTOR_MOVES_1: [MoveId; TUTOR_MENU_LEN] = [
    MoveId::SOFT_BOILED,
    MoveId::SEISMIC_TOSS,
    MoveId::DREAM_EATER,
    MoveId::MEGA_PUNCH,
    MoveId::MEGA_KICK,
    MoveId::BODY_SLAM,
    MoveId::ROCK_SLIDE,
    MoveId::COUNTER,
    MoveId::THUNDER_WAVE,
    MoveId::SWORDS_DANCE,
];

/// `sBattleFrontier_TutorMoves2` (`src/field_specials.c:3073-3085`).
const TUTOR_MOVES_2: [MoveId; TUTOR_MENU_LEN] = [
    MoveId::DEFENSE_CURL,
    MoveId::SNORE,
    MoveId::MUD_SLAP,
    MoveId::SWIFT,
    MoveId::ICY_WIND,
    MoveId::ENDURE,
    MoveId::PSYCH_UP,
    MoveId::ICE_PUNCH,
    MoveId::THUNDER_PUNCH,
    MoveId::FIRE_PUNCH,
];

/// Returns the ten moves of a menu, in menu order.
#[must_use]
pub const fn frontier_tutor_moves(menu: FrontierTutorMenu) -> &'static [MoveId; TUTOR_MENU_LEN] {
    match menu {
        FrontierTutorMenu::First => &TUTOR_MOVES_1,
        FrontierTutorMenu::Second => &TUTOR_MOVES_2,
    }
}

/// Returns the move a menu row teaches.
///
/// # Errors
///
/// Returns [`FrontierTutorError::InvalidRow`] when `row` is not below [`TUTOR_MENU_LEN`].
pub fn frontier_tutor_move(
    menu: FrontierTutorMenu,
    row: usize,
) -> Result<MoveId, FrontierTutorError> {
    frontier_tutor_moves(menu)
        .get(row)
        .copied()
        .ok_or(FrontierTutorError::InvalidRow(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MENUS: [FrontierTutorMenu; 2] = [FrontierTutorMenu::First, FrontierTutorMenu::Second];

    /// Raw `MOVE_*` values from `include/constants/moves.h`, read independently of the tables.
    const EXPECTED: [[u16; TUTOR_MENU_LEN]; 2] = [
        [135, 69, 138, 5, 25, 34, 157, 68, 86, 14],
        [111, 173, 189, 129, 196, 203, 244, 8, 9, 7],
    ];

    #[test]
    fn every_row_is_pinned_in_menu_order() {
        for (menu, expected) in MENUS.into_iter().zip(EXPECTED) {
            for (row, raw) in expected.into_iter().enumerate() {
                assert_eq!(
                    frontier_tutor_move(menu, row),
                    Ok(MoveId(raw)),
                    "{menu:?} row {row}"
                );
            }
            assert_eq!(frontier_tutor_moves(menu).map(MoveId::index), expected);
        }
    }

    #[test]
    fn the_first_and_last_rows_are_named() {
        let at = |menu, row| frontier_tutor_move(menu, row).unwrap();
        assert_eq!(at(FrontierTutorMenu::First, 0), MoveId::SOFT_BOILED);
        assert_eq!(at(FrontierTutorMenu::First, 9), MoveId::SWORDS_DANCE);
        assert_eq!(at(FrontierTutorMenu::Second, 0), MoveId::DEFENSE_CURL);
        assert_eq!(at(FrontierTutorMenu::Second, 9), MoveId::FIRE_PUNCH);
    }

    #[test]
    fn each_menu_has_ten_entries_and_the_menus_are_distinct() {
        for menu in MENUS {
            assert_eq!(frontier_tutor_moves(menu).len(), 10);
        }
        assert_ne!(
            frontier_tutor_moves(FrontierTutorMenu::First),
            frontier_tutor_moves(FrontierTutorMenu::Second)
        );
        for first in frontier_tutor_moves(FrontierTutorMenu::First) {
            assert!(!frontier_tutor_moves(FrontierTutorMenu::Second).contains(first));
        }
    }

    #[test]
    fn rows_outside_the_menu_are_refused() {
        for menu in MENUS {
            assert_eq!(
                frontier_tutor_move(menu, 10),
                Err(FrontierTutorError::InvalidRow(10))
            );
            assert_eq!(
                frontier_tutor_move(menu, usize::MAX),
                Err(FrontierTutorError::InvalidRow(usize::MAX))
            );
        }
    }

    #[test]
    fn the_error_names_the_offending_row() {
        assert_eq!(
            FrontierTutorError::InvalidRow(12).to_string(),
            "Battle Frontier tutor menu row `12` is out of range (there are 10)"
        );
    }

    /// Reads a file of the pinned upstream checkout.
    fn upstream_file(relative: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate sits two levels below the workspace root")
            .join("pokeemerald");
        std::fs::read_to_string(root.join(relative)).unwrap_or_else(|error| {
            panic!("cannot read pokeemerald/{relative}: {error}; run ./init.sh")
        })
    }

    /// The move names listed in the initializer of `declaration`.
    fn upstream_names<'a>(source: &'a str, declaration: &str) -> Vec<&'a str> {
        let start = source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"));
        let body = &source[start..];
        let open = body.find('{').expect("initializer opens") + 1;
        let close = body.find('}').expect("initializer closes");
        body[open..close]
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_tables() {
        let moves: std::collections::HashMap<String, u16> =
            upstream_file("include/constants/moves.h")
                .lines()
                .filter_map(|line| {
                    let mut words = line.split_whitespace();
                    (words.next()? == "#define").then_some(())?;
                    let name = words.next()?.to_owned();
                    let value = words.next()?.parse().ok()?;
                    Some((name, value))
                })
                .collect();
        let source = upstream_file("src/field_specials.c");

        for (declaration, menu) in [
            ("sBattleFrontier_TutorMoves1[] =", FrontierTutorMenu::First),
            ("sBattleFrontier_TutorMoves2[] =", FrontierTutorMenu::Second),
        ] {
            let names = upstream_names(&source, declaration);
            assert_eq!(names.len(), TUTOR_MENU_LEN, "{declaration}");
            for (row, name) in names.into_iter().enumerate() {
                assert_eq!(
                    frontier_tutor_move(menu, row),
                    Ok(MoveId(moves[name])),
                    "{declaration} row {row} {name}"
                );
            }
        }
    }
}
