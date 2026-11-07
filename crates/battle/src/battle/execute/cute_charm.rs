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
    /// Only a living attacker's damaging contact hit on a holder that survived
    /// it spends the `Random() % 3` roll (`:2853`-`:2858`), unlike Static and
    /// Poison Point, which still roll for a holder that fainted. The caller
    /// reaches this seam only for a hit that damaged the target, which covers
    /// the no-effect result, and a confusion self-hit never reaches it, so
    /// neither can infatuate. The Oblivious, gender and existing-infatuation
    /// guards follow the roll (`:2859`-`:2864`), so a zero roll spent on an
    /// ineligible pair still consumed its draw. A pair that passes records
    /// the holder's slot as the attacker's infatuation, the state Attract's
    /// `Cmd_tryinfatuating` writes (`:2866`).
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
