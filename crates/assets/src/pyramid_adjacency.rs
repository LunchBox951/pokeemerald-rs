//! The Battle Pyramid square adjacency table: for each of the sixteen squares
//! of a floor's four-by-four grid, the squares that border it.
//!
//! Source: `src/battle_pyramid.c`, the `sBorderedSquareIds` table
//! (`:812-830`). Squares are numbered `0` to `15` in row-major order. Each
//! square lists up to four neighbours in the order the upstream table gives
//! them, ascending, and upstream pads a short list with a byte sentinel. This
//! module reads a padded slot as [`None`], so an absent neighbour is never a
//! square number. Present neighbours always come before the absent slots.
//!
//! Data only: floor generation, connectivity decisions, map assets, trainer
//! placement and random draws are not ported here.

use std::error::Error;
use std::fmt;

/// The number of squares on a Battle Pyramid floor.
pub const PYRAMID_SQUARES: usize = 16;

/// The number of neighbour slots each square has.
pub const PYRAMID_SQUARE_NEIGHBOR_SLOTS: usize = 4;

/// A failed lookup in the Battle Pyramid square adjacency table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyramidSquareError {
    /// The square is not below [`PYRAMID_SQUARES`].
    InvalidSquare(usize),
}

impl fmt::Display for PyramidSquareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSquare(square) => write!(
                f,
                "Battle Pyramid square `{square}` is out of range (there are {PYRAMID_SQUARES})"
            ),
        }
    }
}

impl Error for PyramidSquareError {}

/// `sBorderedSquareIds` (`src/battle_pyramid.c:812-830`), one row per square.
#[rustfmt::skip]
const SQUARE_NEIGHBORS: [[Option<usize>; PYRAMID_SQUARE_NEIGHBOR_SLOTS]; PYRAMID_SQUARES] = [
    [Some(1),  Some(4),  None,     None],
    [Some(0),  Some(2),  Some(5),  None],
    [Some(1),  Some(3),  Some(6),  None],
    [Some(2),  Some(7),  None,     None],
    [Some(0),  Some(5),  Some(8),  None],
    [Some(1),  Some(4),  Some(6),  Some(9)],
    [Some(2),  Some(5),  Some(7),  Some(10)],
    [Some(3),  Some(6),  Some(11), None],
    [Some(4),  Some(9),  Some(12), None],
    [Some(5),  Some(8),  Some(10), Some(13)],
    [Some(6),  Some(9),  Some(11), Some(14)],
    [Some(7),  Some(10), Some(15), None],
    [Some(8),  Some(13), None,     None],
    [Some(9),  Some(12), Some(14), None],
    [Some(10), Some(13), Some(15), None],
    [Some(11), Some(14), None,     None],
];

/// Whether `neighbor` is a present neighbour of `square` in the table.
const fn lists(square: usize, neighbor: usize) -> bool {
    let mut slot = 0;
    while slot < PYRAMID_SQUARE_NEIGHBOR_SLOTS {
        if let Some(found) = SQUARE_NEIGHBORS[square][slot] {
            if found == neighbor {
                return true;
            }
        }
        slot += 1;
    }
    false
}

/// Whether the table keeps the invariants its users depend on: every present
/// neighbour is a square, present neighbours come first and ascend, and
/// adjacency is reciprocal.
const fn table_is_well_formed() -> bool {
    let mut square = 0;
    while square < PYRAMID_SQUARES {
        let mut previous: Option<usize> = None;
        let mut gap = false;
        let mut slot = 0;
        while slot < PYRAMID_SQUARE_NEIGHBOR_SLOTS {
            match SQUARE_NEIGHBORS[square][slot] {
                None => gap = true,
                Some(neighbor) => {
                    if gap || neighbor >= PYRAMID_SQUARES || neighbor == square {
                        return false;
                    }
                    if let Some(before) = previous {
                        if neighbor <= before {
                            return false;
                        }
                    }
                    if !lists(neighbor, square) {
                        return false;
                    }
                    previous = Some(neighbor);
                }
            }
            slot += 1;
        }
        square += 1;
    }
    true
}

const _: () = assert!(
    table_is_well_formed(),
    "Battle Pyramid square adjacency table is malformed"
);

/// Returns the four neighbour slots of one square, in upstream order
/// (`sBorderedSquareIds`). A slot with no neighbour is [`None`]; the present
/// neighbours come first.
///
/// # Errors
///
/// Returns [`PyramidSquareError::InvalidSquare`] when `square` is not below
/// [`PYRAMID_SQUARES`].
pub fn pyramid_square_neighbors(
    square: usize,
) -> Result<&'static [Option<usize>; PYRAMID_SQUARE_NEIGHBOR_SLOTS], PyramidSquareError> {
    SQUARE_NEIGHBORS
        .get(square)
        .ok_or(PyramidSquareError::InvalidSquare(square))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sBorderedSquareIds`, with `-1` standing for an absent neighbour.
    #[rustfmt::skip]
    const EXPECTED: [[i8; PYRAMID_SQUARE_NEIGHBOR_SLOTS]; PYRAMID_SQUARES] = [
        [1, 4, -1, -1],
        [0, 2, 5, -1],
        [1, 3, 6, -1],
        [2, 7, -1, -1],
        [0, 5, 8, -1],
        [1, 4, 6, 9],
        [2, 5, 7, 10],
        [3, 6, 11, -1],
        [4, 9, 12, -1],
        [5, 8, 10, 13],
        [6, 9, 11, 14],
        [7, 10, 15, -1],
        [8, 13, -1, -1],
        [9, 12, 14, -1],
        [10, 13, 15, -1],
        [11, 14, -1, -1],
    ];

    fn slots(row: [i8; PYRAMID_SQUARE_NEIGHBOR_SLOTS]) -> [Option<usize>; 4] {
        row.map(|value| usize::try_from(value).ok())
    }

    #[test]
    fn every_row_is_pinned() {
        for (square, row) in EXPECTED.iter().enumerate() {
            assert_eq!(
                pyramid_square_neighbors(square),
                Ok(&slots(*row)),
                "square {square}"
            );
        }
    }

    #[test]
    fn spot_checked_rows_read_as_upstream() {
        assert_eq!(
            pyramid_square_neighbors(0),
            Ok(&[Some(1), Some(4), None, None])
        );
        assert_eq!(
            pyramid_square_neighbors(5),
            Ok(&[Some(1), Some(4), Some(6), Some(9)])
        );
        assert_eq!(
            pyramid_square_neighbors(15),
            Ok(&[Some(11), Some(14), None, None])
        );
    }

    #[test]
    fn the_table_has_the_upstream_dimensions() {
        assert_eq!(PYRAMID_SQUARES, 16);
        assert_eq!(PYRAMID_SQUARE_NEIGHBOR_SLOTS, 4);
        assert!(pyramid_square_neighbors(PYRAMID_SQUARES - 1).is_ok());
    }

    #[test]
    fn an_absent_neighbor_is_none_and_never_a_square_number() {
        let mut absent = 0;
        for square in 0..PYRAMID_SQUARES {
            for slot in pyramid_square_neighbors(square).unwrap() {
                match slot {
                    None => absent += 1,
                    Some(neighbor) => assert!(*neighbor < PYRAMID_SQUARES, "square {square}"),
                }
            }
        }
        assert_eq!(absent, 16);
    }

    #[test]
    fn adjacency_is_reciprocal() {
        for square in 0..PYRAMID_SQUARES {
            for neighbor in pyramid_square_neighbors(square)
                .unwrap()
                .iter()
                .flatten()
                .copied()
            {
                assert!(
                    pyramid_square_neighbors(neighbor)
                        .unwrap()
                        .contains(&Some(square)),
                    "{square} lists {neighbor} but not the reverse"
                );
            }
        }
    }

    #[test]
    fn invalid_squares_are_refused() {
        assert_eq!(
            pyramid_square_neighbors(PYRAMID_SQUARES),
            Err(PyramidSquareError::InvalidSquare(PYRAMID_SQUARES))
        );
        assert_eq!(
            pyramid_square_neighbors(255),
            Err(PyramidSquareError::InvalidSquare(255))
        );
        assert_eq!(
            pyramid_square_neighbors(usize::MAX),
            Err(PyramidSquareError::InvalidSquare(usize::MAX))
        );
    }

    #[test]
    fn errors_name_the_offending_square() {
        assert_eq!(
            PyramidSquareError::InvalidSquare(16).to_string(),
            "Battle Pyramid square `16` is out of range (there are 16)"
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

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_table() {
        let source = upstream_file("src/battle_pyramid.c");
        let start = source
            .find("sBorderedSquareIds[][4]")
            .expect("declaration present");
        let body = &source[start..];
        let open = body.find("=\n{").expect("initializer opens") + 3;
        let close = body.find("\n};").expect("initializer closes");
        let rows: Vec<[i8; PYRAMID_SQUARE_NEIGHBOR_SLOTS]> = body[open..close]
            .trim()
            .lines()
            .map(|line| {
                let inner = line
                    .trim()
                    .trim_end_matches(',')
                    .strip_prefix('{')
                    .and_then(|rest| rest.strip_suffix('}'))
                    .unwrap_or_else(|| panic!("braced row expected: `{line}`"));
                let values: Vec<i8> = inner
                    .split(',')
                    .map(|value| value.trim().parse().expect("small integer"))
                    .collect();
                values.try_into().expect("four values per row")
            })
            .collect();
        assert_eq!(rows, EXPECTED);
    }
}
