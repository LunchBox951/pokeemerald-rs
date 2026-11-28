//! Height of each enemy battler above its usual battle position.
//!
//! Only the table is modelled here: nothing positions a battler or its shadow
//! from these values yet.

use crate::error::AssetError;
use crate::species::{SpeciesId, SpeciesTable};

macro_rules! define_enemy_elevations {
    ($($species:ident = $elevation:literal),+ $(,)?) => {
        const ENEMY_ELEVATIONS: [u8; SpeciesTable::LEN] = {
            let mut elevations = [0; SpeciesTable::LEN];
            $(elevations[SpeciesId::$species.0 as usize] = $elevation;)+
            elevations
        };

        /// Number of species whose enemy battler floats above the ground.
        pub const ENEMY_ELEVATED_SPECIES_COUNT: usize = [$(SpeciesId::$species),+].len();
    };
}

#[rustfmt::skip]
define_enemy_elevations! {
    BUTTERFREE = 8,
    BEEDRILL = 8,
    PIDGEY = 16,
    PIDGEOT = 4,
    FEAROW = 6,
    ZUBAT = 8,
    GOLBAT = 8,
    VENOMOTH = 8,
    GEODUDE = 16,
    MAGNEMITE = 16,
    MAGNETON = 8,
    GASTLY = 4,
    HAUNTER = 4,
    VOLTORB = 10,
    ELECTRODE = 12,
    KOFFING = 8,
    WEEZING = 6,
    AERODACTYL = 7,
    ARTICUNO = 6,
    ZAPDOS = 8,
    MOLTRES = 5,
    DRAGONITE = 6,
    MEW = 8,
    LEDIAN = 8,
    CROBAT = 6,
    HOPPIP = 11,
    SKIPLOOM = 12,
    JUMPLUFF = 9,
    YANMA = 8,
    MISDREAVUS = 8,
    UNOWN = 8,
    GLIGAR = 6,
    LUGIA = 6,
    HO_OH = 6,
    CELEBI = 15,
    BEAUTIFLY = 8,
    DUSTOX = 10,
    NINJASK = 10,
    SHEDINJA = 8,
    WINGULL = 16,
    PELIPPER = 8,
    MASQUERAIN = 10,
    BALTOY = 4,
    CLAYDOL = 10,
    FLYGON = 7,
    GLALIE = 12,
    LUNATONE = 13,
    SOLROCK = 4,
    SWABLU = 12,
    ALTARIA = 8,
    DUSKULL = 9,
    SHUPPET = 12,
    BANETTE = 8,
    CASTFORM = 16,
    BELDUM = 8,
    RAYQUAZA = 6,
    LATIAS = 6,
    LATIOS = 6,
    JIRACHI = 12,
    DEOXYS = 8,
    CHIMECHO = 12,
}

/// Returns how far above its usual position `species` is drawn as the
/// opposing battler, zero for every species that stays on the ground.
///
/// # Errors
///
/// Returns [`AssetError::UnknownSpecies`] when `species` is outside the
/// species table.
pub fn enemy_elevation(species: SpeciesId) -> Result<u8, AssetError> {
    ENEMY_ELEVATIONS
        .get(usize::from(species.index()))
        .copied()
        .ok_or(AssetError::UnknownSpecies(species.index()))
}

#[cfg(test)]
mod tests {
    use super::{enemy_elevation, ENEMY_ELEVATED_SPECIES_COUNT};
    use crate::error::AssetError;
    use crate::species::{SpeciesId, SpeciesTable};

    /// Every floating species as (species index, elevation), ordered by index.
    const EXPECTED: [(u16, u8); 61] = [
        (12, 8),   // BUTTERFREE
        (15, 8),   // BEEDRILL
        (16, 16),  // PIDGEY
        (18, 4),   // PIDGEOT
        (22, 6),   // FEAROW
        (41, 8),   // ZUBAT
        (42, 8),   // GOLBAT
        (49, 8),   // VENOMOTH
        (74, 16),  // GEODUDE
        (81, 16),  // MAGNEMITE
        (82, 8),   // MAGNETON
        (92, 4),   // GASTLY
        (93, 4),   // HAUNTER
        (100, 10), // VOLTORB
        (101, 12), // ELECTRODE
        (109, 8),  // KOFFING
        (110, 6),  // WEEZING
        (142, 7),  // AERODACTYL
        (144, 6),  // ARTICUNO
        (145, 8),  // ZAPDOS
        (146, 5),  // MOLTRES
        (149, 6),  // DRAGONITE
        (151, 8),  // MEW
        (166, 8),  // LEDIAN
        (169, 6),  // CROBAT
        (187, 11), // HOPPIP
        (188, 12), // SKIPLOOM
        (189, 9),  // JUMPLUFF
        (193, 8),  // YANMA
        (200, 8),  // MISDREAVUS
        (201, 8),  // UNOWN
        (207, 6),  // GLIGAR
        (249, 6),  // LUGIA
        (250, 6),  // HO_OH
        (251, 15), // CELEBI
        (292, 8),  // BEAUTIFLY
        (294, 10), // DUSTOX
        (302, 10), // NINJASK
        (303, 8),  // SHEDINJA
        (309, 16), // WINGULL
        (310, 8),  // PELIPPER
        (312, 10), // MASQUERAIN
        (318, 4),  // BALTOY
        (319, 10), // CLAYDOL
        (334, 7),  // FLYGON
        (347, 12), // GLALIE
        (348, 13), // LUNATONE
        (349, 4),  // SOLROCK
        (358, 12), // SWABLU
        (359, 8),  // ALTARIA
        (361, 9),  // DUSKULL
        (377, 12), // SHUPPET
        (378, 8),  // BANETTE
        (385, 16), // CASTFORM
        (398, 8),  // BELDUM
        (406, 6),  // RAYQUAZA
        (407, 6),  // LATIAS
        (408, 6),  // LATIOS
        (409, 12), // JIRACHI
        (410, 8),  // DEOXYS
        (411, 12), // CHIMECHO
    ];

    #[test]
    fn sixty_one_species_are_elevated() {
        assert_eq!(ENEMY_ELEVATED_SPECIES_COUNT, 61);
        assert_eq!(EXPECTED.len(), 61);
    }

    #[test]
    fn every_listed_species_returns_its_elevation() {
        for (species, elevation) in EXPECTED {
            assert_eq!(
                enemy_elevation(SpeciesId(species)),
                Ok(elevation),
                "species {species}"
            );
        }
    }

    #[test]
    fn every_other_in_range_species_is_grounded() {
        for species in 0..SpeciesTable::LEN_U16 {
            if EXPECTED.iter().all(|&(listed, _)| listed != species) {
                assert_eq!(
                    enemy_elevation(SpeciesId(species)),
                    Ok(0),
                    "species {species}"
                );
            }
        }
    }

    #[test]
    fn spot_checks_match_upstream_values() {
        assert_eq!(enemy_elevation(SpeciesId::PIDGEY), Ok(16));
        assert_eq!(enemy_elevation(SpeciesId::PIDGEOT), Ok(4));
        assert_eq!(enemy_elevation(SpeciesId::CHIMECHO), Ok(12));
        assert_eq!(enemy_elevation(SpeciesId::NONE), Ok(0));
        assert_eq!(enemy_elevation(SpeciesId::BULBASAUR), Ok(0));
    }

    #[test]
    fn species_past_the_table_is_refused() {
        let first_invalid = SpeciesTable::LEN_U16;
        assert_eq!(
            enemy_elevation(SpeciesId(first_invalid)),
            Err(AssetError::UnknownSpecies(first_invalid))
        );
        assert_eq!(
            enemy_elevation(SpeciesId(u16::MAX)),
            Err(AssetError::UnknownSpecies(u16::MAX))
        );
    }
}
