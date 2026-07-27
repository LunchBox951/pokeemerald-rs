//! The battle crate's typed data facade (S-6): species base stats, move
//! data, and the type-effectiveness chart.
//!
//! All three tables are already extracted upstream data living in `assets`
//! ([`assets::SpeciesTable`], [`assets::MoveTable`], [`assets::TypeChart`])
//! `(no-verbatim)` — this slice does not re-port them. [`Dex`] is a thin
//! owned bundle over the three so battle-formula callers have one type to
//! construct and pass around instead of three `(oop-boundaries)`. It adds no
//! new data of its own; for the data this slice *does* add — nature
//! modifiers and stat-stage multipliers, which had no upstream home in
//! `assets` yet — see [`crate::nature`] and [`crate::stat_stage`].
//!
//! Errors from the underlying tables are returned as `assets::AssetError`
//! directly rather than wrapped into [`crate::error::BattleError`]: they are
//! exactly the errors those tables already define and documented, and
//! wrapping them would just be a relabelling exercise for callers.

use assets::{AssetError, BaseStats, Effectiveness, MoveData, MoveId, MoveTable, SpeciesId};
use assets::{SpeciesTable, Type, TypeChart};

/// Owned, read-only typed access to species base stats, move data, and the
/// type-effectiveness chart `(oop-boundaries)`. Holds no mutable state;
/// construct once with [`Dex::new`] and share it.
#[derive(Debug, Clone)]
pub struct Dex {
    species_table: SpeciesTable,
    move_table: MoveTable,
    type_chart: TypeChart,
}

impl Dex {
    /// Build the dex over the extracted upstream data.
    #[must_use]
    pub fn new() -> Self {
        Self {
            species_table: SpeciesTable::new(),
            move_table: MoveTable::new(),
            type_chart: TypeChart::new(),
        }
    }

    /// The base stats for `species`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside the
    /// extracted range. See [`SpeciesTable::base_stats`].
    pub fn species(&self, species: SpeciesId) -> Result<&BaseStats, AssetError> {
        self.species_table.base_stats(species)
    }

    /// The battle data for `mv`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMove`] if `mv` is outside `gBattleMoves`.
    /// See [`MoveTable::get`].
    pub fn move_data(&self, mv: MoveId) -> Result<&MoveData, AssetError> {
        self.move_table.get(mv)
    }

    /// The effectiveness of an `attacker`-type move against a `defender`
    /// type.
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
    use assets::{AssetError, Effectiveness, MoveId, SpeciesId, SpeciesTable, Type};

    #[test]
    fn species_forwards_to_the_species_table() {
        let dex = Dex::new();
        let bulbasaur = dex.species(SpeciesId(1)).unwrap();
        assert_eq!(bulbasaur.hp, 45);
        assert_eq!(bulbasaur.types, [Type::Grass, Type::Poison]);
    }

    #[test]
    fn species_reports_out_of_range_ids() {
        let dex = Dex::new();
        let bad = SpeciesId(SpeciesTable::LEN_U16);
        assert_eq!(dex.species(bad), Err(AssetError::UnknownSpecies(bad.0)));
    }

    #[test]
    fn move_data_forwards_to_the_move_table() {
        let dex = Dex::new();
        // MOVE_POUND (1): 40 power, Normal type, 100 accuracy, 35 PP.
        let pound = dex.move_data(MoveId(1)).unwrap();
        assert_eq!(pound.power, 40);
        assert_eq!(pound.accuracy, 100);
        assert_eq!(pound.pp, 35);
    }

    #[test]
    fn move_data_reports_out_of_range_ids() {
        let dex = Dex::new();
        let bad = MoveId(60_000);
        assert_eq!(dex.move_data(bad), Err(AssetError::UnknownMove(bad.0)));
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
    fn default_matches_new() {
        // Both build over the same static data; spot-check they agree.
        let a = Dex::default();
        let b = Dex::new();
        assert_eq!(a.species(SpeciesId(1)), b.species(SpeciesId(1)));
    }
}
