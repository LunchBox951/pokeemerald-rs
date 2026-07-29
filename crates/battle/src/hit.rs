//! Single-hit resolution (S-6): assembles [`crate::accuracy`],
//! [`crate::critical`], and [`crate::damage`] into upstream's exact
//! battle-script step order for one damaging move used against one target.
//!
//! Upstream spreads this across several `Cmd_*` battle-script commands run
//! back-to-back for `BattleScript_EffectHit`-shaped moves
//! (`pokeemerald/src/battle_script_commands.c`): `Cmd_accuracycheck`
//! (`:1099`), `Cmd_critcalc` (`:1253`), `Cmd_damagecalc` (`:1290`, which
//! calls `CalculateBaseDamage`, `src/pokemon.c:3106`, then applies
//! `gCritMultiplier` at `:1296`), `Cmd_typecalc` (`:1355` — STAB + type
//! effectiveness), and finally `ApplyRandomDmgMultiplier`. [`resolve_hit`]
//! runs the same sequence and draws the RNG at exactly the same points:
//!
//! 1. accuracy check (0 or 1 draw — see [`crate::accuracy`]); miss ends here.
//! 2. crit roll (1 draw, even if the hit will turn out type-immune — but see
//!    the caveat below).
//! 3. damage core + crit's stat-stage override ([`crate::critical`]) and
//!    `x2` multiply (no draw).
//! 4. STAB (no draw) — skipped for `MOVE_STRUGGLE`.
//! 5. dual-type effectiveness (no draw); a `NoEffect` row is terminal —
//!    also skipped entirely for `MOVE_STRUGGLE`.
//! 6. the `85..=100%` random roll (always 1 draw, even at `0` damage).
//!
//! So, **in the world this slice models**, a hit that clears accuracy draws
//! exactly 3 times and a miss draws exactly once. That "3" is not a universal
//! upstream property: `Cmd_critcalc`'s `Random()` is the last operand of a
//! short-circuiting `&&` chain (`battle_script_commands.c:1279`-`:1283`), so
//! a defender with Battle Armor / Shell Armor, an attacker under
//! `STATUS3_CANT_SCORE_A_CRIT`, or a `BATTLE_TYPE_WALLY_TUTORIAL` /
//! `BATTLE_TYPE_FIRST_BATTLE` battle makes step 2 draw **nothing**, for a
//! 2-draw hit. None of those three exist here (no abilities, no status3,
//! ordinary post-first-battle wild encounter) — see [`crate::critical`]'s
//! module docs — so the count is 3 for every hit [`resolve_hit`] can
//! currently produce.
//!
//! `MOVE_STRUGGLE` is the one shape-changing special case: `Cmd_typecalc`
//! returns immediately for it (`battle_script_commands.c:1360`-`:1364`),
//! *before* the STAB multiply and before every `ModulateDmgByType` call, so
//! Struggle gets neither STAB nor type effectiveness — it damages a Ghost
//! defender that Normal-type moves cannot touch. Its `EFFECT_RECOIL` half is
//! a separate battle-script step and is not modelled here. Only the damaging
//! (`EFFECT_HIT`-shaped) move path is modelled — see
//! [`crate::error::BattleError::NonDamagingMove`].

use assets::{MoveId, Type};

use crate::accuracy::accuracy_check;
use crate::critical::{crit_adjusted_stages, crit_roll, crit_stage_for_effect};
use crate::damage::{
    apply_damage_roll, apply_dual_type_effectiveness, apply_stab, base_damage, has_stab, BattleRng,
    DamageInput, MoveCategory, Weather, STRUGGLE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

/// The result of resolving one move against one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitOutcome {
    /// The accuracy check failed (`MOVE_RESULT_MISSED`).
    Miss,
    /// The move connected but the target's typing made it deal no damage
    /// (`MOVE_RESULT_DOESNT_AFFECT_FOE`).
    NoEffect,
    /// The move connected and dealt `damage` HP (already floored to at
    /// least `1` by the pipeline).
    Hit {
        /// HP of damage dealt.
        damage: u32,
        /// Whether this was a critical hit.
        is_critical: bool,
    },
}

/// Resolve `attacker` using `move_id` against `defender`.
///
/// Status conditions (burn), Reflect/Light Screen, and weather are not
/// modelled this slice: [`crate::damage::DamageInput`]'s corresponding
/// fields are always the "no effect" value here (`false`/[`Weather::None`]).
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] if `move_id` is not in `dex`,
/// [`BattleError::UnsupportedMoveType`] if the move is the sole `???`-typed
/// move (`MOVE_CURSE`), or [`BattleError::NonDamagingMove`] if the move has
/// `0` base power (status moves — see the module docs).
pub fn resolve_hit(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    let mv = dex.move_data(move_id)?;
    if mv.power == 0 {
        return Err(BattleError::NonDamagingMove(move_id));
    }
    let move_type: Type = mv
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;
    let category = MoveCategory::for_type(move_type);

    if !accuracy_check(
        mv.accuracy,
        mv.effect,
        attacker.stages.accuracy,
        defender.stages.evasion,
        rng,
    ) {
        return Ok(HitOutcome::Miss);
    }

    let crit_stage = crit_stage_for_effect(mv.effect);
    let is_critical = crit_roll(crit_stage, rng);

    let (attack_stat, attack_stage) = attacker.attacking_stat(category);
    let (defense_stat, defense_stage) = defender.defending_stat(category);
    let (attack_stage, defense_stage) =
        crit_adjusted_stages(attack_stage, defense_stage, is_critical);

    let input = DamageInput {
        attacker_level: attacker.level,
        power: u32::from(mv.power),
        move_type,
        attack_stat,
        attack_stage,
        defense_stat,
        defense_stage,
        // Status conditions, side statuses, and weather are not modelled
        // this slice; a crit also forces reflect/light_screen off upstream
        // (`pokemon.c:3264`/`:3316`), which the constant `false` already
        // gives us regardless of `is_critical`.
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
    };

    let mut damage = base_damage(&input);
    if is_critical {
        // Cmd_damagecalc: `gBattleMoveDamage *= gCritMultiplier` (2 on a
        // crit), applied to the whole base-damage result before STAB.
        damage *= 2;
    }
    let stab = has_stab(attacker.types, move_id, move_type);
    let damage = apply_stab(damage, stab);
    // `Cmd_typecalc` returns at `battle_script_commands.c:1360`-`:1364` for
    // `MOVE_STRUGGLE`, ahead of both the STAB multiply ([`has_stab`] already
    // encodes that half) and every `ModulateDmgByType` call — so Struggle
    // ignores type effectiveness outright, immunities included. That is the
    // caller contract [`crate::damage::has_stab`] documents; honour it by
    // skipping the type step rather than passing a neutral multiplier, which
    // would still zero the damage against a Ghost defender.
    let damage = if move_id == STRUGGLE {
        damage
    } else {
        apply_dual_type_effectiveness(damage, move_type, defender.types)
    };
    let damage = apply_damage_roll(damage, rng);

    if damage == 0 {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage,
            is_critical,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_hit, HitOutcome};
    use crate::damage::{BattleRng, STRUGGLE};
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::nature::Nature;
    use crate::pokemon::{BattlePokemon, Ivs};
    use assets::{MoveId, SpeciesId};

    /// A `BattleRng` fed from a fixed sequence, for pinning exact draw
    /// order/count in a multi-draw pipeline.
    struct SequenceRng {
        values: Vec<u16>,
        index: usize,
    }
    impl SequenceRng {
        fn new(values: impl IntoIterator<Item = u16>) -> Self {
            Self {
                values: values.into_iter().collect(),
                index: 0,
            }
        }
        fn draws(&self) -> usize {
            self.index
        }
    }
    impl BattleRng for SequenceRng {
        fn next_u16(&mut self) -> u16 {
            let v = self
                .values
                .get(self.index)
                .copied()
                .expect("SequenceRng exhausted");
            self.index += 1;
            v
        }
    }

    fn mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
        BattlePokemon::new(
            dex,
            SpeciesId(species),
            level,
            Nature::Hardy,
            Ivs {
                hp: 31,
                attack: 31,
                defense: 31,
                speed: 31,
                sp_attack: 31,
                sp_defense: 31,
            },
            0,
            moves,
        )
        .unwrap()
    }

    #[test]
    fn a_miss_draws_exactly_once() {
        let dex = Dex::new();
        let attacker = mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle
        let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle
                                                          // Tackle accuracy 95: roll = draw%100+1. draw=95 -> roll=96 > 95 -> miss.
        let mut rng = SequenceRng::new([95]);
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(outcome, HitOutcome::Miss);
        assert_eq!(rng.draws(), 1);
    }

    #[test]
    fn a_hit_draws_exactly_three_times() {
        let dex = Dex::new();
        let attacker = mon(&dex, 1, 5, vec![MoveId(33)]);
        let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
        // draw0: accuracy roll 0 -> roll=1 <= 95 -> hit.
        // draw1: crit roll 1 -> 1%16 != 0 -> no crit.
        // draw2: damage roll 0 -> best (100%) roll.
        let mut rng = SequenceRng::new([0, 1, 0]);
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
        assert!(matches!(outcome, HitOutcome::Hit { .. }));
        assert_eq!(rng.draws(), 3);
    }

    /// The reference scenario both damage pins below are derived from, hand
    /// computed from upstream's arithmetic rather than from this crate's own
    /// output:
    ///
    /// Bulbasaur (species 1) level 5, max IVs, Hardy, using Tackle (move 33,
    /// power 35, accuracy 95, Normal/physical) against Squirtle (species 7)
    /// level 5, max IVs, Hardy.
    ///
    /// - attack  = `CALC_STAT(base 49, iv 31, lvl 5)` = `(2*49+31)*5/100 + 5`
    ///   = `645/100 = 6`, `+5` = **11** (Hardy is neutral, no scaling).
    /// - defense = `CALC_STAT(base 65, iv 31, lvl 5)` = `(2*65+31)*5/100 + 5`
    ///   = `805/100 = 8`, `+5` = **13**.
    /// - `CalculateBaseDamage`: `11 * 35` = 385; `* (2*5/5 + 2 = 4)` = 1540;
    ///   `/ 13` = 118 (`13*118 = 1534`); `/ 50` = 2; `+ 2` = **4**.
    /// - Bulbasaur is Grass/Poison, so a Normal move gets no STAB; Squirtle
    ///   is pure Water, so Normal is neutral against it — both steps are
    ///   identity multiplies.
    /// - A best-case damage roll (`draw % 16 == 0` -> 100%) leaves it at 4.
    ///
    /// A crit doubles `CalculateBaseDamage`'s result *before* STAB
    /// (`Cmd_damagecalc`, `battle_script_commands.c:1296`), and every stat
    /// stage here is neutral so the crit stage override is a no-op: 4 -> 8.
    const PINNED_NON_CRIT_DAMAGE: u32 = 4;
    const PINNED_CRIT_DAMAGE: u32 = 8;

    #[test]
    fn best_roll_non_critical_damage_matches_the_hand_computed_pin() {
        let dex = Dex::new();
        let attacker = mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle
        let defender = mon(&dex, 7, 5, vec![MoveId(33)]); // Squirtle

        // draw0: accuracy roll 1 <= 95 -> hit.
        // draw1: crit roll 1 -> 1%16 != 0 -> no crit.
        // draw2: damage roll 0 -> 100%.
        let mut rng = SequenceRng::new([0, 1, 0]);
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: PINNED_NON_CRIT_DAMAGE,
                is_critical: false,
            }
        );
        assert_eq!(rng.draws(), 3);
    }

    #[test]
    fn a_confirmed_crit_doubles_the_pinned_damage_and_is_reported() {
        let dex = Dex::new();
        let attacker = mon(&dex, 1, 5, vec![MoveId(33)]);
        let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
        // Same scenario and same draws as the test above, except draw1: crit
        // roll 0 -> 0%16 == 0 -> crit.
        let mut rng = SequenceRng::new([0, 0, 0]);
        let outcome = resolve_hit(&dex, MoveId(33), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: PINNED_CRIT_DAMAGE,
                is_critical: true,
            }
        );
        assert_eq!(PINNED_CRIT_DAMAGE, 2 * PINNED_NON_CRIT_DAMAGE);
        assert_eq!(rng.draws(), 3);
    }

    #[test]
    fn struggle_ignores_type_immunity_and_damages_a_ghost() {
        let dex = Dex::new();
        // Normal-type moves cannot touch a Ghost, but `Cmd_typecalc` returns
        // before every `ModulateDmgByType` call for MOVE_STRUGGLE
        // (battle_script_commands.c:1360-1364), so Struggle still connects.
        let attacker = mon(&dex, 1, 5, vec![STRUGGLE]); // Bulbasaur, attack 11
        let defender = mon(&dex, 92, 5, vec![MoveId(33)]); // Gastly, Ghost/Poison

        // Struggle: power 50, accuracy 100. Gastly defense =
        // (2*30+31)*5/100 + 5 = 455/100 = 4, +5 = 9.
        // 11*50 = 550; *4 = 2200; /9 = 244; /50 = 4; +2 = 6.
        // No STAB (Struggle is exempt), no type step, 100% roll -> 6.
        let mut rng = SequenceRng::new([0, 1, 0]);
        let outcome = resolve_hit(&dex, STRUGGLE, &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            outcome,
            HitOutcome::Hit {
                damage: 6,
                is_critical: false,
            }
        );

        // The control: an ordinary Normal move from the same attacker against
        // the same Ghost defender *is* nullified.
        let tackler = mon(&dex, 1, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0, 1, 0]);
        let outcome = resolve_hit(&dex, MoveId(33), &tackler, &defender, &mut rng).unwrap();
        assert_eq!(outcome, HitOutcome::NoEffect);
    }

    #[test]
    fn type_immunity_reports_no_effect_and_still_draws_the_full_sequence() {
        let dex = Dex::new();
        // Onix (Rock/Ground, id 95) is immune to Electric.
        let attacker = mon(&dex, 25, 20, vec![MoveId(84)]); // Pikachu/Thundershock
        let defender = mon(&dex, 95, 20, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0, 1, 0]);
        let outcome = resolve_hit(&dex, MoveId(84), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(outcome, HitOutcome::NoEffect);
        assert_eq!(
            rng.draws(),
            3,
            "immunity still draws crit + damage-roll RNG"
        );
    }

    #[test]
    fn zero_power_moves_are_reported_as_unsupported() {
        let dex = Dex::new();
        let attacker = mon(&dex, 1, 5, vec![MoveId(45)]); // Growl (status move)
        let defender = mon(&dex, 7, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            resolve_hit(&dex, MoveId(45), &attacker, &defender, &mut rng),
            Err(BattleError::NonDamagingMove(MoveId(45)))
        );
    }
}
