//! Typed access to the extracted species, move, and type-effectiveness data.

use assets::{BaseStats, Effectiveness, MoveData, MoveId, MoveTable, SpeciesId};
use assets::{SpeciesTable, Type, TypeChart};

use crate::error::BattleError;

/// Owned, read-only battle data tables.
#[derive(Debug, Clone)]
pub struct Dex {
    species_table: SpeciesTable,
    move_table: MoveTable,
    type_chart: TypeChart,
}

impl Dex {
    /// Creates a dex over the extracted data.
    #[must_use]
    pub fn new() -> Self {
        Self {
            species_table: SpeciesTable::new(),
            move_table: MoveTable::new(),
            type_chart: TypeChart::new(),
        }
    }

    /// Returns the base stats for `species`.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownSpecies`] if `species` is outside the
    /// extracted table.
    pub fn species(&self, species: SpeciesId) -> Result<&BaseStats, BattleError> {
        self.species_table
            .base_stats(species)
            .map_err(|_| BattleError::UnknownSpecies(species))
    }

    /// Returns the battle data for `move_id`.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] if `move_id` is outside the
    /// extracted table.
    pub fn move_data(&self, move_id: MoveId) -> Result<&MoveData, BattleError> {
        self.move_table
            .get(move_id)
            .map_err(|_| BattleError::UnknownMove(move_id))
    }

    /// Returns the effectiveness of an `attacker`-type move against a
    /// `defender` type.
    #[must_use]
    pub fn effectiveness(&self, attacker: Type, defender: Type) -> Effectiveness {
        self.type_chart.multiplier(attacker, defender)
    }
}

impl Default for Dex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Dex;
    use crate::error::BattleError;
    use assets::{Effectiveness, MoveId, SpeciesId, SpeciesTable, Type};

    const BULBASAUR: SpeciesId = SpeciesId(1);
    const POUND: MoveId = MoveId(1);
    const UNKNOWN_MOVE: MoveId = MoveId(60_000);

    #[test]
    fn species_returns_base_stats() {
        let dex = Dex::new();
        let bulbasaur = dex.species(BULBASAUR).unwrap();
        assert_eq!(bulbasaur.hp, 45);
        assert_eq!(bulbasaur.types, [Type::Grass, Type::Poison]);
    }

    #[test]
    fn species_reports_out_of_range_ids() {
        let dex = Dex::new();
        let bad = SpeciesId(SpeciesTable::LEN_U16);
        assert_eq!(dex.species(bad), Err(BattleError::UnknownSpecies(bad)));
    }

    #[test]
    fn move_data_returns_battle_properties() {
        let dex = Dex::new();
        let pound = dex.move_data(POUND).unwrap();
        assert_eq!(pound.power, 40);
        assert_eq!(pound.accuracy, 100);
        assert_eq!(pound.pp, 35);
    }

    #[test]
    fn move_data_reports_out_of_range_ids() {
        let dex = Dex::new();
        assert_eq!(
            dex.move_data(UNKNOWN_MOVE),
            Err(BattleError::UnknownMove(UNKNOWN_MOVE))
        );
    }

    #[test]
    fn effectiveness_forwards_to_the_type_chart() {
        let dex = Dex::new();
        assert_eq!(
            dex.effectiveness(Type::Fire, Type::Grass),
            Effectiveness::SuperEffective
        );
        assert_eq!(
            dex.effectiveness(Type::Electric, Type::Ground),
            Effectiveness::NoEffect
        );
    }

    #[test]
    fn default_constructs_the_same_data() {
        let default_dex = Dex::default();
        let new_dex = Dex::new();
        assert_eq!(default_dex.species(BULBASAUR), new_dex.species(BULBASAUR));
    }
}
