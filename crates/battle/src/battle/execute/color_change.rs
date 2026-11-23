//! Color Change's move-end type change.

use assets::{AbilityId, MoveId};

use crate::damage::STRUGGLE;
use crate::error::BattleError;

use super::{Battle, BattleEvent};

impl Battle {
    /// `MOVEEND_ON_DAMAGE_ABILITIES` for `ABILITY_COLOR_CHANGE`
    /// (`battle_script_commands.c:4280`-`:4284`, `battle_util.c:2749`-`:2766`;
    /// `BattleScript_ColorChangeActivates`, `data/battle_scripts_1.s:4179`-
    /// `:4182`).
    ///
    /// A surviving holder takes the damaging move's type in both type slots
    /// when the move is not Struggle, has nonzero base power, and the holder
    /// lacks that type. Contact is not required, the attacker may be fainted,
    /// and no `Random()` call is made. A missed or immune move never reaches
    /// the seam, which its callers enter only after the target lost HP. The
    /// new typing lasts until the holder leaves the field or faints, where
    /// the battle scratch cleanup restores the species typing.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    pub(super) fn apply_color_change(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let (_, holder) = self.battlers(attacker_is_player);
        if holder.ability() != AbilityId::COLOR_CHANGE || holder.is_fainted() {
            return Ok(());
        }
        let move_data = self.dex.move_data(move_id)?;
        if move_id == STRUGGLE || move_data.power == 0 {
            return Ok(());
        }
        let Some(new_type) = move_data.move_type.battle_type() else {
            return Ok(());
        };
        if holder.types().contains(&new_type) {
            return Ok(());
        }
        self.battler_mut(!attacker_is_player)
            .set_temporary_type(new_type);
        events.push(BattleEvent::TypeChanged {
            by_player: attacker_is_player,
            move_id,
            new_type,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assets::{AbilityId, MoveId, SpeciesId, Type};

    use super::*;
    use crate::battle::trainer::fixed_ivs;
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::pokemon::BattlePokemon;

    /// Answers the constructor's seed and tie draws; the seam under test
    /// takes no RNG at all.
    struct Zeroes;

    impl BattleRng for Zeroes {
        fn next_u16(&mut self) -> u16 {
            0
        }
    }

    fn battle(enemy_species: SpeciesId) -> Battle {
        let dex = Dex::new();
        let build = |species| {
            BattlePokemon::new(&dex, species, 50, fixed_ivs(31), 0, vec![MoveId::TACKLE]).unwrap()
        };
        Battle::new(
            Dex::new(),
            build(SpeciesId::RATTATA),
            build(enemy_species),
            false,
            &mut Zeroes,
        )
        .unwrap()
    }

    fn run(battle: &mut Battle, move_id: MoveId) -> Vec<BattleEvent> {
        let mut events = Vec::new();
        battle
            .apply_color_change(true, move_id, &mut events)
            .unwrap();
        events
    }

    #[test]
    fn a_zero_power_move_changes_nothing_whatever_its_type() {
        let mut battle = battle(SpeciesId::KECLEON);
        assert_eq!(battle.enemy().ability(), AbilityId::COLOR_CHANGE);
        let dex = Dex::new();
        let will_o_wisp = dex.move_data(MoveId::WILL_O_WISP).unwrap();
        assert_eq!(will_o_wisp.power, 0);
        assert_eq!(will_o_wisp.move_type.battle_type(), Some(Type::Fire));

        assert_eq!(run(&mut battle, MoveId::WILL_O_WISP), vec![]);
        assert_eq!(battle.enemy().types(), [Type::Normal, Type::Normal]);
    }

    #[test]
    fn a_powered_move_changes_a_living_holder_and_not_a_fainted_one() {
        let mut living = battle(SpeciesId::KECLEON);
        assert_eq!(
            run(&mut living, MoveId::WATER_GUN),
            vec![BattleEvent::TypeChanged {
                by_player: true,
                move_id: MoveId::WATER_GUN,
                new_type: Type::Water,
            }]
        );
        assert_eq!(living.enemy().types(), [Type::Water, Type::Water]);

        let mut fainted = battle(SpeciesId::KECLEON);
        let max_hp = fainted.enemy().stats().max_hp;
        fainted.enemy.apply_damage(max_hp);
        assert_eq!(run(&mut fainted, MoveId::WATER_GUN), vec![]);
        assert_eq!(fainted.enemy().types(), [Type::Normal, Type::Normal]);
    }

    #[test]
    fn a_holder_without_color_change_is_left_alone() {
        let mut battle = battle(SpeciesId::ZIGZAGOON);
        assert_eq!(run(&mut battle, MoveId::WATER_GUN), vec![]);
        assert_eq!(battle.enemy().types(), [Type::Normal, Type::Normal]);
    }
}
