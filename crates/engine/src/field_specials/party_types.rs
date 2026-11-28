//! The Grass-type party check behind upstream's `IsGrassTypeInParty`
//! (`src/field_specials.c:1230-1249`).

use assets::{AssetError, SpeciesId, SpeciesTable, Type};

use crate::save::block::PARTY_SIZE;

/// One party slot as upstream's `IsGrassTypeInParty` sees it.
///
/// The caller classifies the slot, since upstream reads these facts through
/// `GetMonData` (`MON_DATA_SANITY_HAS_SPECIES`, `MON_DATA_IS_EGG` and
/// `MON_DATA_SPECIES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartySlot {
    /// The slot's species, read only when the slot is occupied and not an Egg.
    pub species: SpeciesId,
    /// Whether the slot holds a Pokemon.
    pub occupied: bool,
    /// Whether the slot's Pokemon is an Egg.
    pub is_egg: bool,
}

/// Returns whether any occupied, non-Egg slot has Grass as either of its two
/// types, scanning the slots in order and stopping at the first match.
///
/// Unoccupied slots and Eggs are skipped before their species is looked up, so
/// their species is never validated.
///
/// # Errors
///
/// Returns [`AssetError::UnknownSpecies`] when a slot that reaches the lookup
/// before any match holds a species outside `species_table`.
pub fn party_has_grass_type(
    party: &[PartySlot; PARTY_SIZE],
    species_table: &SpeciesTable,
) -> Result<bool, AssetError> {
    for slot in party {
        if !slot.occupied || slot.is_egg {
            continue;
        }
        let types = species_table.base_stats(slot.species)?.types;
        if types[0] == Type::Grass || types[1] == Type::Grass {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: PartySlot = PartySlot {
        species: SpeciesId::NONE,
        occupied: false,
        is_egg: false,
    };

    fn member(species: SpeciesId) -> PartySlot {
        PartySlot {
            species,
            occupied: true,
            is_egg: false,
        }
    }

    fn party_with(slot: usize, entry: PartySlot) -> [PartySlot; PARTY_SIZE] {
        let mut party = [EMPTY; PARTY_SIZE];
        party[slot] = entry;
        party
    }

    fn check(party: &[PartySlot; PARTY_SIZE]) -> Result<bool, AssetError> {
        party_has_grass_type(party, &SpeciesTable::new())
    }

    #[test]
    fn first_type_grass_is_found() {
        let base = SpeciesTable::new()
            .base_stats(SpeciesId::BULBASAUR)
            .unwrap();
        assert_eq!(base.types[0], Type::Grass);
        assert_ne!(base.types[1], Type::Grass);
        assert_eq!(
            check(&party_with(0, member(SpeciesId::BULBASAUR))),
            Ok(true)
        );
    }

    #[test]
    fn second_type_grass_is_found() {
        let base = SpeciesTable::new().base_stats(SpeciesId::LOTAD).unwrap();
        assert_ne!(base.types[0], Type::Grass);
        assert_eq!(base.types[1], Type::Grass);
        assert_eq!(check(&party_with(0, member(SpeciesId::LOTAD))), Ok(true));
    }

    #[test]
    fn a_party_without_grass_is_false() {
        let mut party = [EMPTY; PARTY_SIZE];
        party[0] = member(SpeciesId::TORCHIC);
        party[3] = member(SpeciesId::TORCHIC);
        assert_eq!(check(&party), Ok(false));
        assert_eq!(check(&[EMPTY; PARTY_SIZE]), Ok(false));
    }

    #[test]
    fn a_grass_egg_is_skipped() {
        let egg = PartySlot {
            is_egg: true,
            ..member(SpeciesId::BULBASAUR)
        };
        assert_eq!(check(&party_with(0, egg)), Ok(false));
    }

    #[test]
    fn an_unoccupied_slot_with_a_grass_species_is_skipped() {
        let vacant = PartySlot {
            occupied: false,
            ..member(SpeciesId::BULBASAUR)
        };
        assert_eq!(check(&party_with(2, vacant)), Ok(false));
    }

    #[test]
    fn the_last_slot_is_scanned() {
        let party = party_with(PARTY_SIZE - 1, member(SpeciesId::LOTAD));
        assert_eq!(check(&party), Ok(true));
    }

    #[test]
    fn a_grass_egg_does_not_hide_a_later_grass_member() {
        let mut party = party_with(
            0,
            PartySlot {
                is_egg: true,
                ..member(SpeciesId::BULBASAUR)
            },
        );
        party[1] = member(SpeciesId::LOTAD);
        assert_eq!(check(&party), Ok(true));
    }

    #[test]
    fn guards_run_before_the_species_lookup() {
        let invalid = SpeciesId(SpeciesTable::LEN_U16);
        let egg = PartySlot {
            is_egg: true,
            ..member(invalid)
        };
        let vacant = PartySlot {
            occupied: false,
            ..member(invalid)
        };
        assert_eq!(check(&party_with(0, egg)), Ok(false));
        assert_eq!(check(&party_with(0, vacant)), Ok(false));
    }

    #[test]
    fn an_occupied_invalid_species_returns_the_lookup_error() {
        let invalid = SpeciesId(SpeciesTable::LEN_U16);
        assert_eq!(
            check(&party_with(0, member(invalid))),
            Err(AssetError::UnknownSpecies(SpeciesTable::LEN_U16))
        );
    }

    #[test]
    fn a_match_before_an_invalid_species_stops_the_scan() {
        let mut party = party_with(0, member(SpeciesId::BULBASAUR));
        party[1] = member(SpeciesId(u16::MAX));
        assert_eq!(check(&party), Ok(true));
    }

    #[test]
    fn an_invalid_species_before_a_match_is_an_error() {
        let mut party = party_with(0, member(SpeciesId(u16::MAX)));
        party[1] = member(SpeciesId::BULBASAUR);
        assert_eq!(check(&party), Err(AssetError::UnknownSpecies(u16::MAX)));
    }
}
