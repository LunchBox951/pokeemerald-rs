//! Cute Charm's move-end contact infatuation.

use assets::{AbilityId, MoveId};

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::gender::Gender;

use super::{Battle, BattleEvent, BattlerSide};

/// Cute Charm triggers on a zero `Random() % 3` (`battle_util.c:2858`).
const CUTE_CHARM_CHANCE_DENOMINATOR: u16 = 3;

impl Battle {
    /// `MOVEEND_ON_DAMAGE_ABILITIES` for `ABILITY_CUTE_CHARM`
    /// (`battle_script_commands.c:4280`-`:4284`, `battle_util.c:2851`-`:2870`;
    /// `BattleScript_CuteCharmActivates`, `data/battle_scripts_1.s:4193`-`:4197`).
    ///
    /// Unlike Static and Poison Point, a fainted holder skips the roll
    /// (`:2853`-`:2858`); the eligibility guards follow it (`:2859`-`:2864`),
    /// so an ineligible pair still consumes the draw.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    pub(super) fn apply_cute_charm_contact(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let makes_contact = self.dex.move_data(move_id)?.flags.makes_contact();
        let (attacker, holder) = self.battlers(attacker_is_player);
        if holder.ability() != AbilityId::CUTE_CHARM
            || attacker.is_fainted()
            || holder.is_fainted()
            || !makes_contact
        {
            return Ok(());
        }
        if !rng.next_u16().is_multiple_of(CUTE_CHARM_CHANCE_DENOMINATOR) {
            return Ok(());
        }
        let attacker_gender = attacker.gender();
        let holder_gender = holder.gender();
        if attacker.ability() == AbilityId::OBLIVIOUS
            || attacker_gender == holder_gender
            || attacker_gender == Gender::Genderless
            || holder_gender == Gender::Genderless
            || attacker.volatiles().infatuated()
        {
            return Ok(());
        }
        self.battler_mut(attacker_is_player)
            .volatiles_mut()
            .set_infatuated_by(BattlerSide::from_is_player(!attacker_is_player));
        events.push(BattleEvent::InfatuatedByCuteCharm {
            by_player: attacker_is_player,
            move_id,
        });
        Ok(())
    }
}
