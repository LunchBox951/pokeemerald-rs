//! Nature-to-Pokéblock flavor compatibility matrix.
//!
//! Transcribes `gPokeblockFlavorCompatibilityTable`
//! (`pokeemerald/src/pokeblock.c:136-164`): one signed value per nature and
//! flavor. A value of `1` marks a liked flavor, `-1` a disliked one, and `0` a
//! neutral one; `PokeblockGetGain` (`src/pokeblock.c:1407-1419`) multiplies
//! each positive flavor strength by the matching signed value.
//!
//! Natures are addressed by their Gen III numeric discriminant
//! (`NATURE_HARDY` = 0 through `NATURE_QUIRKY` = 24,
//! `include/constants/pokemon.h:72`), not by `battle::Nature`: this crate stays
//! independent of the battle crate, and callers pass `Nature::id()`.
//!
//! The flavor order is Spicy, Dry, Sweet, Bitter, Sour
//! (`include/constants/berry.h:13-18`).

use core::fmt;

/// A Pokéblock/berry flavor, in upstream column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Flavor {
    /// `FLAVOR_SPICY`.
    Spicy = 0,
    /// `FLAVOR_DRY`.
    Dry = 1,
    /// `FLAVOR_SWEET`.
    Sweet = 2,
    /// `FLAVOR_BITTER`.
    Bitter = 3,
    /// `FLAVOR_SOUR`.
    Sour = 4,
}

impl Flavor {
    /// Every flavor in upstream column order.
    pub const ALL: [Flavor; FLAVOR_COUNT] = [
        Flavor::Spicy,
        Flavor::Dry,
        Flavor::Sweet,
        Flavor::Bitter,
        Flavor::Sour,
    ];

    /// Returns the zero-based column index (`FLAVOR_*`).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// The number of flavors (`FLAVOR_COUNT`).
pub const FLAVOR_COUNT: usize = 5;

/// The number of natures (`NUM_NATURES`).
pub const NATURE_COUNT: usize = 25;

/// A nature discriminant outside `0..NATURE_COUNT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownNature(pub u8);

impl fmt::Display for UnknownNature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown nature id `{}`", self.0)
    }
}

impl std::error::Error for UnknownNature {}

/// `gPokeblockFlavorCompatibilityTable`: rows are natures in numeric order,
/// columns are Spicy, Dry, Sweet, Bitter, Sour.
#[rustfmt::skip]
const COMPATIBILITY: [[i8; FLAVOR_COUNT]; NATURE_COUNT] = [
    [ 0,  0,  0,  0,  0], // Hardy
    [ 1,  0,  0,  0, -1], // Lonely
    [ 1,  0, -1,  0,  0], // Brave
    [ 1, -1,  0,  0,  0], // Adamant
    [ 1,  0,  0, -1,  0], // Naughty
    [-1,  0,  0,  0,  1], // Bold
    [ 0,  0,  0,  0,  0], // Docile
    [ 0,  0, -1,  0,  1], // Relaxed
    [ 0, -1,  0,  0,  1], // Impish
    [ 0,  0,  0, -1,  1], // Lax
    [-1,  0,  1,  0,  0], // Timid
    [ 0,  0,  1,  0, -1], // Hasty
    [ 0,  0,  0,  0,  0], // Serious
    [ 0, -1,  1,  0,  0], // Jolly
    [ 0,  0,  1, -1,  0], // Naive
    [-1,  1,  0,  0,  0], // Modest
    [ 0,  1,  0,  0, -1], // Mild
    [ 0,  1, -1,  0,  0], // Quiet
    [ 0,  0,  0,  0,  0], // Bashful
    [ 0,  1,  0, -1,  0], // Rash
    [-1,  0,  0,  1,  0], // Calm
    [ 0,  0,  0,  1, -1], // Gentle
    [ 0,  0, -1,  1,  0], // Sassy
    [ 0, -1,  0,  1,  0], // Careful
    [ 0,  0,  0,  0,  0], // Quirky
];

/// Returns the five signed flavor values of `nature`, in
/// [`Flavor::ALL`] order.
///
/// # Errors
///
/// Returns [`UnknownNature`] if `nature` is not below [`NATURE_COUNT`].
pub fn nature_row(nature: u8) -> Result<[i8; FLAVOR_COUNT], UnknownNature> {
    COMPATIBILITY
        .get(usize::from(nature))
        .copied()
        .ok_or(UnknownNature(nature))
}

/// Returns the signed compatibility of `nature` with `flavor`: `1` liked,
/// `-1` disliked, `0` neutral.
///
/// # Errors
///
/// Returns [`UnknownNature`] if `nature` is not below [`NATURE_COUNT`].
pub fn compatibility(nature: u8, flavor: Flavor) -> Result<i8, UnknownNature> {
    nature_row(nature).map(|row| row[flavor.index()])
}

/// Returns the flavor `nature` likes, or `None` for a neutral nature.
///
/// # Errors
///
/// Returns [`UnknownNature`] if `nature` is not below [`NATURE_COUNT`].
pub fn liked_flavor(nature: u8) -> Result<Option<Flavor>, UnknownNature> {
    flavor_with_sign(nature, 1)
}

/// Returns the flavor `nature` dislikes, or `None` for a neutral nature.
///
/// # Errors
///
/// Returns [`UnknownNature`] if `nature` is not below [`NATURE_COUNT`].
pub fn disliked_flavor(nature: u8) -> Result<Option<Flavor>, UnknownNature> {
    flavor_with_sign(nature, -1)
}

fn flavor_with_sign(nature: u8, sign: i8) -> Result<Option<Flavor>, UnknownNature> {
    let row = nature_row(nature)?;
    Ok(Flavor::ALL
        .into_iter()
        .find(|flavor| row[flavor.index()] == sign))
}

#[cfg(test)]
mod tests {
    use super::*;

    use Flavor::{Bitter, Dry, Sour, Spicy, Sweet};

    /// Upstream nature names in numeric order (`include/constants/pokemon.h`).
    const NAMES: [&str; NATURE_COUNT] = [
        "Hardy", "Lonely", "Brave", "Adamant", "Naughty", "Bold", "Docile", "Relaxed", "Impish",
        "Lax", "Timid", "Hasty", "Serious", "Jolly", "Naive", "Modest", "Mild", "Quiet", "Bashful",
        "Rash", "Calm", "Gentle", "Sassy", "Careful", "Quirky",
    ];

    fn id(name: &str) -> u8 {
        let index = NAMES
            .iter()
            .position(|n| *n == name)
            .unwrap_or_else(|| panic!("no nature {name}"));
        u8::try_from(index).unwrap()
    }

    /// Independent fixture: (nature, liked, disliked) for the 20 non-neutral
    /// natures, as written in the upstream row comments
    /// (`src/pokeblock.c:138-163`).
    const PAIRS: [(&str, Flavor, Flavor); 20] = [
        ("Lonely", Spicy, Sour),
        ("Brave", Spicy, Sweet),
        ("Adamant", Spicy, Dry),
        ("Naughty", Spicy, Bitter),
        ("Bold", Sour, Spicy),
        ("Relaxed", Sour, Sweet),
        ("Impish", Sour, Dry),
        ("Lax", Sour, Bitter),
        ("Timid", Sweet, Spicy),
        ("Hasty", Sweet, Sour),
        ("Jolly", Sweet, Dry),
        ("Naive", Sweet, Bitter),
        ("Modest", Dry, Spicy),
        ("Mild", Dry, Sour),
        ("Quiet", Dry, Sweet),
        ("Rash", Dry, Bitter),
        ("Calm", Bitter, Spicy),
        ("Gentle", Bitter, Sour),
        ("Sassy", Bitter, Sweet),
        ("Careful", Bitter, Dry),
    ];

    const NEUTRAL: [&str; 5] = ["Hardy", "Docile", "Serious", "Bashful", "Quirky"];

    #[test]
    fn flavor_order_is_spicy_dry_sweet_bitter_sour() {
        assert_eq!(Flavor::ALL, [Spicy, Dry, Sweet, Bitter, Sour]);
        for (i, flavor) in Flavor::ALL.into_iter().enumerate() {
            assert_eq!(flavor.index(), i);
        }
        assert_eq!(FLAVOR_COUNT, 5);
        assert_eq!(NATURE_COUNT, 25);
    }

    #[test]
    fn neutral_natures_are_all_zero() {
        for name in NEUTRAL {
            assert_eq!(nature_row(id(name)), Ok([0; 5]), "{name}");
            assert_eq!(liked_flavor(id(name)), Ok(None), "{name}");
            assert_eq!(disliked_flavor(id(name)), Ok(None), "{name}");
        }
        // Neutral natures are exactly the all-zero rows.
        let zero_rows = COMPATIBILITY.iter().filter(|row| **row == [0; 5]).count();
        assert_eq!(zero_rows, NEUTRAL.len());
    }

    #[test]
    fn every_liked_and_disliked_pair_matches() {
        assert_eq!(PAIRS.len() + NEUTRAL.len(), NATURE_COUNT);
        for (name, liked, disliked) in PAIRS {
            let nature = id(name);
            assert_eq!(liked_flavor(nature), Ok(Some(liked)), "{name} liked");
            assert_eq!(
                disliked_flavor(nature),
                Ok(Some(disliked)),
                "{name} disliked"
            );
            assert_eq!(compatibility(nature, liked), Ok(1), "{name}");
            assert_eq!(compatibility(nature, disliked), Ok(-1), "{name}");
            let nonzero = nature_row(nature)
                .unwrap()
                .iter()
                .filter(|v| **v != 0)
                .count();
            assert_eq!(nonzero, 2, "{name} has exactly one like and one dislike");
        }
    }

    #[test]
    fn rows_match_hand_pinned_upstream_samples() {
        assert_eq!(nature_row(1), Ok([1, 0, 0, 0, -1])); // Lonely
        assert_eq!(nature_row(10), Ok([-1, 0, 1, 0, 0])); // Timid
        assert_eq!(nature_row(19), Ok([0, 1, 0, -1, 0])); // Rash
        assert_eq!(nature_row(23), Ok([0, -1, 0, 1, 0])); // Careful
    }

    #[test]
    fn invalid_natures_are_rejected() {
        for bad in [25_u8, 26, 100, 255] {
            assert_eq!(nature_row(bad), Err(UnknownNature(bad)));
            assert_eq!(compatibility(bad, Spicy), Err(UnknownNature(bad)));
            assert_eq!(liked_flavor(bad), Err(UnknownNature(bad)));
            assert_eq!(disliked_flavor(bad), Err(UnknownNature(bad)));
        }
        assert_eq!(UnknownNature(25).to_string(), "unknown nature id `25`");
    }

    /// FNV-1a 64-bit hash, matching the change locator in `xtask::record_snapshot`.
    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// Digest of all 125 signed entries of `gPokeblockFlavorCompatibilityTable`
    /// (`src/pokeblock.c:136-164`), each entry rendered as its two's-complement
    /// byte in nature-major, flavor-minor order. Computed independently by
    /// parsing the upstream C source.
    #[test]
    fn all_125_entries_match_the_canonical_digest() {
        let bytes: Vec<u8> = COMPATIBILITY
            .iter()
            .flatten()
            .map(|v| v.to_le_bytes()[0])
            .collect();
        assert_eq!(bytes.len(), 125);
        assert_eq!(
            format!("{:016x}", fnv1a64(&bytes)),
            "109cbbd73f8cab13",
            "flavor matrix diverges from the canonical upstream table",
        );
    }
}
