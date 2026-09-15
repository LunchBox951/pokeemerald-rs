//! `GetMostSuitableMonToSwitchInto`'s typing and most-damage passes: the
//! forced post-faint replacement selector [`TrainerContext::send_out_next`]
//! tries before falling back to party order.

use assets::{AbilityId, Effectiveness, MoveId, Type, TypeChart};

use crate::ability::{huge_power_attack, pinch_boosts_power};
use crate::damage::{
    apply_dual_type_effectiveness, apply_stab, apply_type_effectiveness, base_damage, has_stab,
    DamageInput, MoveCategory, Weather,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

use super::TrainerContext;

/// The neutral type-effectiveness baseline (`TYPE_MUL_NORMAL`).
const NEUTRAL_TYPE_SCORE: u32 = 10;

/// `gBattleMoves[move].power`'s OHKO sentinel (`GUILLOTINE`, `HORN_DRILL`,
/// `FISSURE`, `SHEER_COLD`), the one power value upstream's most-damage pass
/// excludes (`pokeemerald/src/battle_ai_switch_items.c:776`).
const OHKO_POWER_SENTINEL: u8 = 1;

impl TrainerContext {
    /// The healthy bench member whose typing takes the most damage from
    /// `player` and knows a super-effective move back, retrying the
    /// next-worst typing otherwise. The typing score itself runs
    /// `ModulateByTypeEffectiveness`, which reads no ability
    /// (`pokeemerald/src/battle_ai_switch_items.c:709`-`:710`); only the
    /// super-effective check runs `TypeCalc` and so honours `player`'s
    /// Levitate (`:733`, see [`levitate_refuses`]).
    pub(super) fn most_suitable_by_type(
        &self,
        dex: &Dex,
        player: &BattlePokemon,
    ) -> Result<Option<usize>, BattleError> {
        let mut invalid = vec![false; self.bench.len()];
        loop {
            let mut best_index = None;
            let mut best_score = 0;
            for (index, candidate) in self.bench.iter().enumerate() {
                if invalid[index] || candidate.is_fainted() {
                    continue;
                }
                let score = typing_suitability_score(player, candidate);
                if best_score < score {
                    best_score = score;
                    best_index = Some(index);
                }
            }
            let Some(index) = best_index else {
                return Ok(None);
            };
            if has_super_effective_move(dex, &self.bench[index], player)? {
                return Ok(Some(index));
            }
            invalid[index] = true;
        }
    }

    /// The healthy bench member whose own move scores highest against
    /// `player` once layered onto `resolving_move`'s stale shared base
    /// damage from `fainted`. See the ledger's `GetMostSuitableMonToSwitchInto`
    /// entry (`src/battle_ai_switch_items.c`) for the full upstream mapping,
    /// the `u8 bestDmg` wraparound this reproduces, and the Levitate/OHKO
    /// edge cases [`levitate_refuses`] and [`OHKO_POWER_SENTINEL`] cover.
    pub(super) fn most_suitable_by_damage(
        &self,
        dex: &Dex,
        fainted: &BattlePokemon,
        resolving_move: MoveId,
        player: &BattlePokemon,
    ) -> Result<Option<usize>, BattleError> {
        let Some(base) = stale_base_damage(dex, resolving_move, fainted, player)? else {
            return Ok(None);
        };
        let mut best_index = None;
        let mut best_damage: u8 = 0;
        for (index, candidate) in self.bench.iter().enumerate() {
            if candidate.is_fainted() {
                continue;
            }
            for slot in candidate.moves() {
                let Some(damage) = candidate_move_damage(dex, slot.move_id, base, fainted, player)?
                else {
                    continue;
                };
                if u32::from(best_damage) < damage {
                    best_damage = u8::try_from(damage % 256).expect("modulo 256 fits a byte");
                    best_index = Some(index);
                }
            }
        }
        Ok(best_index)
    }
}

/// Scores `candidate`'s typing against `player`'s, applying `player`'s two
/// type slots even when they repeat (`ModulateByTypeEffectiveness`'s two
/// unguarded calls in `pokeemerald/src/battle_ai_switch_items.c:709`-`:710`
/// square the effect for a single-typed `player`). Plain truncating
/// multiplication with no floor, unlike [`apply_dual_type_effectiveness`]:
/// upstream's suitability score (unlike real damage) has no minimum-one rule
/// (`pokeemerald/src/battle_ai_switch_items.c:605`-`:623`).
fn typing_suitability_score(player: &BattlePokemon, candidate: &BattlePokemon) -> u32 {
    let candidate_types = candidate.types();
    let player_types = player.types();
    let score = apply_raw_type_multiplier(NEUTRAL_TYPE_SCORE, player_types[0], candidate_types);
    apply_raw_type_multiplier(score, player_types[1], candidate_types)
}

/// Applies every [`TypeChart`] row matching `attacking_type` against each of
/// `defending_types`' distinct slots, truncating after each row with no
/// floor (`ModulateByTypeEffectiveness`, `pokeemerald/src/battle_ai_switch_items.c:605`-`:627`).
fn apply_raw_type_multiplier(score: u32, attacking_type: Type, defending_types: [Type; 2]) -> u32 {
    let mut score = score;
    let distinct = defending_types[1] != defending_types[0];
    for &(atk, def, effectiveness) in TypeChart::rows() {
        if atk != attacking_type {
            continue;
        }
        if def == defending_types[0] {
            score = raw_multiply(score, effectiveness);
        }
        if distinct && def == defending_types[1] {
            score = raw_multiply(score, effectiveness);
        }
    }
    score
}

fn raw_multiply(score: u32, effectiveness: Effectiveness) -> u32 {
    score * u32::from(effectiveness.multiplier_x10()) / 10
}

/// Whether `defender`'s Levitate refuses `move_type`, `TypeCalc`'s one
/// ability short-circuit: a Ground move against a Levitate holder skips the
/// whole type-chart loop and sets only `MOVE_RESULT_MISSED |
/// MOVE_RESULT_DOESNT_AFFECT_FOE`
/// (`pokeemerald/src/battle_script_commands.c:1554`-`:1556`). The branch
/// sits after the STAB multiply (`:1547`-`:1551`) and touches
/// `gBattleMoveDamage` not at all, so the most-damage pass keeps the
/// STAB-multiplied base rather than zeroing it; only the super-effective
/// pass's flags are decided, and `MOVE_RESULT_SUPER_EFFECTIVE` is never
/// among them.
fn levitate_refuses(defender: &BattlePokemon, move_type: Type) -> bool {
    move_type == Type::Ground && defender.ability() == AbilityId::LEVITATE
}

/// Whether `candidate` knows a damaging move super effective against
/// `player`. An immunity row is terminal for this flag, unlike for
/// [`most_suitable_type_effectiveness`]'s running damage: it leaves
/// `MOVE_RESULT_DOESNT_AFFECT_FOE` set, so a later super-effective row fails
/// its `MOVE_RESULT_NO_EFFECT` guard and never sets the bit the selector
/// tests (`pokeemerald/src/battle_script_commands.c:1525`;
/// `pokeemerald/include/constants/battle.h:228`).
fn has_super_effective_move(
    dex: &Dex,
    candidate: &BattlePokemon,
    player: &BattlePokemon,
) -> Result<bool, BattleError> {
    for slot in candidate.moves() {
        let move_data = dex.move_data(slot.move_id)?;
        if move_data.power == 0 {
            continue;
        }
        let Some(move_type) = move_data.move_type.battle_type() else {
            continue;
        };
        if levitate_refuses(player, move_type) {
            continue;
        }
        let effectiveness =
            apply_dual_type_effectiveness(NEUTRAL_TYPE_SCORE, move_type, player.types());
        if effectiveness > NEUTRAL_TYPE_SCORE {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Computes the most-damage pass's one shared base-damage figure from
/// `resolving_move`, with `fainted` as attacker and `player` as defender, or
/// `None` for a `???`-typed `resolving_move` this crate cannot run type math
/// on. Unlike a real hit, `resolving_move`'s own power is never excluded
/// here (only each candidate's own move can be the
/// [`OHKO_POWER_SENTINEL`]) -- matching upstream, whose `AI_CalcDmg` runs
/// unconditionally before the candidate move's own guard is even checked.
fn stale_base_damage(
    dex: &Dex,
    resolving_move: MoveId,
    fainted: &BattlePokemon,
    player: &BattlePokemon,
) -> Result<Option<u32>, BattleError> {
    let move_data = dex.move_data(resolving_move)?;
    let Some(move_type) = move_data.move_type.battle_type() else {
        return Ok(None);
    };
    let category = MoveCategory::for_type(move_type);
    let (attack_stat, attack_stage) = fainted.attacking_stat(category);
    let attack_stat = huge_power_attack(fainted.ability(), category, attack_stat);
    let (defense_stat, defense_stage) = player.defending_stat(category);
    let input = DamageInput {
        attacker_level: fainted.level(),
        power: u32::from(move_data.power),
        move_type,
        attack_stat,
        attack_stage,
        defense_stat,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
        attacker_pinch_boost: pinch_boosts_power(
            fainted.ability(),
            move_type,
            fainted.current_hp(),
            fainted.stats().max_hp,
        ),
    };
    Ok(Some(base_damage(&input)))
}

/// Applies `move_id`'s STAB (against `fainted`'s types, matching upstream's
/// use of the fainted battler for `TypeCalc`'s STAB check) and type
/// effectiveness (against `defender`'s types) on top of the shared `base`
/// from [`stale_base_damage`], or `None` for the [`OHKO_POWER_SENTINEL`] or
/// a `???`-typed move. A move [`levitate_refuses`] keeps its STAB-multiplied
/// `base` and takes no type-chart row at all.
fn candidate_move_damage(
    dex: &Dex,
    move_id: MoveId,
    base: u32,
    fainted: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<Option<u32>, BattleError> {
    let move_data = dex.move_data(move_id)?;
    if move_data.power == OHKO_POWER_SENTINEL {
        return Ok(None);
    }
    let Some(move_type) = move_data.move_type.battle_type() else {
        return Ok(None);
    };
    let damage = apply_stab(base, has_stab(fainted.types(), move_id, move_type));
    if levitate_refuses(defender, move_type) {
        return Ok(Some(damage));
    }
    Ok(Some(most_suitable_type_effectiveness(
        damage,
        move_type,
        defender.types(),
    )))
}

/// `TypeCalc`'s per-row modulation for the most-damage pass's candidate
/// contribution has no cross-row terminal-immunity override, unlike
/// [`apply_dual_type_effectiveness`]: a later effective row still floors a
/// running damage already zeroed by an earlier immune row back up to one
/// (`battle_script_commands.c:1504`-`:1506`, `:1570`-`:1580`).
fn most_suitable_type_effectiveness(
    damage: u32,
    attacking_type: Type,
    defender_types: [Type; 2],
) -> u32 {
    let mut damage = damage;
    let distinct = defender_types[1] != defender_types[0];
    for &(atk, def, effectiveness) in TypeChart::rows() {
        if atk != attacking_type {
            continue;
        }
        if def == defender_types[0] {
            damage = apply_type_effectiveness(damage, effectiveness);
        }
        if distinct && def == defender_types[1] {
            damage = apply_type_effectiveness(damage, effectiveness);
        }
    }
    damage
}

#[cfg(test)]
mod tests {
    use super::TrainerContext;
    use crate::battle::trainer::{fixed_ivs, trainer_data};
    use crate::dex::Dex;
    use crate::pokemon::BattlePokemon;
    use assets::trainers::TrainerId;
    use assets::{AbilityId, MoveId, SpeciesId};

    const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);
    const TREECKO: SpeciesId = SpeciesId(277);

    /// Pins the stock (non-`BUGFIX`) `u8 bestDmg` wraparound: see the
    /// ledger's `GetMostSuitableMonToSwitchInto` entry
    /// (`src/battle_ai_switch_items.c:781`-`:784`) for why Water Gun's
    /// over-255 true score loses to Tackle's untouched one.
    #[test]
    fn the_most_damage_pass_narrows_its_running_winner_to_a_byte() {
        const METAGROSS: SpeciesId = SpeciesId(400);
        const SANDSHREW: SpeciesId = SpeciesId(27);
        const MUDKIP: SpeciesId = SpeciesId(283);
        const PICHU: SpeciesId = SpeciesId(172);
        const MEGA_KICK: MoveId = MoveId(25);
        const TACKLE: MoveId = MoveId(33);
        const WATER_GUN: MoveId = MoveId(55);

        let dex = Dex::new();
        let mon = |species, level, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, level, fixed_ivs(255), 0, moves)
                .expect("dex-resident")
        };
        let fainted = mon(METAGROSS, 70, vec![MEGA_KICK]);
        let player = mon(SANDSHREW, 50, vec![TACKLE]);
        let bench = vec![mon(MUDKIP, 5, vec![WATER_GUN]), mon(PICHU, 5, vec![TACKLE])];
        let context = TrainerContext::new(
            MAY_ROUTE_103_MUDKIP,
            trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real trainer"),
            bench,
        );

        assert_eq!(
            context.most_suitable_by_damage(&dex, &fainted, MEGA_KICK, &player),
            Ok(Some(1)),
            "Water Gun's narrowed score loses to Tackle's unnarrowed one"
        );
    }

    /// `TypeCalc`'s per-row modulation for the most-damage pass's candidate
    /// contribution has no cross-row terminal-immunity override
    /// (`battle_script_commands.c:1504`-`:1506`, `:1570`-`:1580`), unlike
    /// [`crate::damage::apply_dual_type_effectiveness`]: a later
    /// nonzero-effectiveness row still floors a running damage already
    /// zeroed by an earlier immune row back up to one. A level-70 Metagross's
    /// stale Mega Kick against a level-50 Gligar (Ground/Flying) gives
    /// Pichu's Thunder Shock a score of one, not zero, once Flying's super
    /// effectiveness floors the zero Ground's immunity left behind -- reopening
    /// the most-damage pass instead of falling through to party order.
    /// Pinsir's Guillotine is the [`super::OHKO_POWER_SENTINEL`] and never
    /// scores.
    #[test]
    fn the_most_damage_pass_floors_an_immunity_a_later_row_reopens() {
        const METAGROSS: SpeciesId = SpeciesId(400);
        const GLIGAR: SpeciesId = SpeciesId(207);
        const PINSIR: SpeciesId = SpeciesId(127);
        const PICHU: SpeciesId = SpeciesId(172);
        const MEGA_KICK: MoveId = MoveId(25);
        const GUILLOTINE: MoveId = MoveId(12);
        const THUNDER_SHOCK: MoveId = MoveId(84);

        let dex = Dex::new();
        let mon = |species, level, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, level, fixed_ivs(255), 0, moves)
                .expect("dex-resident")
        };
        let fainted = mon(METAGROSS, 70, vec![MEGA_KICK]);
        let player = mon(GLIGAR, 50, vec![THUNDER_SHOCK]);
        let bench = vec![
            mon(PINSIR, 5, vec![GUILLOTINE]),
            mon(PICHU, 5, vec![THUNDER_SHOCK]),
        ];
        let context = TrainerContext::new(
            MAY_ROUTE_103_MUDKIP,
            trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real trainer"),
            bench,
        );

        assert_eq!(
            context.most_suitable_by_damage(&dex, &fainted, MEGA_KICK, &player),
            Ok(Some(1)),
            "Ground's immunity is floored back to one by Flying's super effectiveness"
        );
    }

    /// The super-effective pass runs `TypeCalc`, whose Levitate branch
    /// short-circuits the whole type chart for a Ground move
    /// (`pokeemerald/src/battle_script_commands.c:1554`-`:1556`), so such a
    /// move never carries `MOVE_RESULT_SUPER_EFFECTIVE` back to the check at
    /// `battle_ai_switch_items.c:733`. Against a Levitate Koffing the Grass
    /// candidate still wins the ability-blind typing score
    /// (`ModulateByTypeEffectiveness`, `:709`-`:710`, doubles Poison against
    /// Grass twice), but its Earthquake cannot qualify it, so the pass
    /// invalidates it and takes the Pichu whose Confusion is genuinely super
    /// effective against Poison.
    #[test]
    fn the_typing_pass_rejects_a_ground_move_the_players_levitate_refuses() {
        const KOFFING: SpeciesId = SpeciesId(109);
        const PICHU: SpeciesId = SpeciesId(172);
        const EARTHQUAKE: MoveId = MoveId(89);
        const CONFUSION: MoveId = MoveId(93);
        const TACKLE: MoveId = MoveId(33);

        let dex = Dex::new();
        let mon = |species, level, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, level, fixed_ivs(255), 0, moves)
                .expect("dex-resident")
        };
        let player = mon(KOFFING, 50, vec![TACKLE]);
        assert_eq!(player.ability(), AbilityId::LEVITATE, "the fixture holds");
        let bench = vec![
            mon(TREECKO, 5, vec![EARTHQUAKE]),
            mon(PICHU, 5, vec![CONFUSION]),
        ];
        let context = TrainerContext::new(
            MAY_ROUTE_103_MUDKIP,
            trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real trainer"),
            bench,
        );

        assert_eq!(
            context.most_suitable_by_type(&dex, &player),
            Ok(Some(1)),
            "Levitate disqualifies Earthquake, so the best typing is invalidated"
        );
    }

    /// The super-effective check reads `TypeCalc`'s flags, where an immunity
    /// row is terminal: against Gligar (Ground/Flying) the table's
    /// Electric->Ground no-effect row (`pokeemerald/src/battle_main.c:356`)
    /// sets `MOVE_RESULT_DOESNT_AFFECT_FOE`, so the Electric->Flying
    /// super-effective row that follows (`:357`) refloors the damage to one
    /// but fails its `MOVE_RESULT_NO_EFFECT` guard
    /// (`battle_script_commands.c:1525`;
    /// `pokeemerald/include/constants/battle.h:228`) and leaves
    /// `MOVE_RESULT_SUPER_EFFECTIVE` clear. Thunder Shock therefore cannot
    /// qualify the Torchic that wins the typing score, and the pass
    /// invalidates it for the Mudkip whose Water Gun doubles against Ground.
    #[test]
    fn the_typing_pass_rejects_a_super_effective_row_behind_an_immunity_row() {
        const GLIGAR: SpeciesId = SpeciesId(207);
        const TORCHIC: SpeciesId = SpeciesId(280);
        const MUDKIP: SpeciesId = SpeciesId(283);
        const THUNDER_SHOCK: MoveId = MoveId(84);
        const WATER_GUN: MoveId = MoveId(55);
        const TACKLE: MoveId = MoveId(33);

        let dex = Dex::new();
        let mon = |species, level, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, level, fixed_ivs(255), 0, moves)
                .expect("dex-resident")
        };
        let player = mon(GLIGAR, 50, vec![TACKLE]);
        let bench = vec![
            mon(TORCHIC, 5, vec![THUNDER_SHOCK]),
            mon(MUDKIP, 5, vec![WATER_GUN]),
        ];
        let context = TrainerContext::new(
            MAY_ROUTE_103_MUDKIP,
            trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real trainer"),
            bench,
        );

        assert_eq!(
            context.most_suitable_by_type(&dex, &player),
            Ok(Some(1)),
            "Ground's immunity keeps Thunder Shock from ever reading as super \
             effective"
        );
    }

    /// `TypeCalc`'s Levitate branch sits after the STAB multiply and leaves
    /// `gBattleMoveDamage` untouched (`:1547`-`:1556`), so a Ground move
    /// against a Levitate holder scores the shared base as-is rather than
    /// taking Poison's doubling row. Neither move gets STAB off the fainted
    /// Metagross's Steel/Psychic typing, so Earthquake would otherwise
    /// double the base and overtake the earlier Tackle under the pass's
    /// strict `bestDmg < gBattleMoveDamage` comparison
    /// (`battle_ai_switch_items.c:781`); honouring Levitate leaves the two
    /// tied on the plain base, and the tie keeps the earlier member.
    #[test]
    fn the_most_damage_pass_scores_a_levitate_refused_ground_move_without_the_chart() {
        const METAGROSS: SpeciesId = SpeciesId(400);
        const KOFFING: SpeciesId = SpeciesId(109);
        const PICHU: SpeciesId = SpeciesId(172);
        const MEGA_KICK: MoveId = MoveId(25);
        const EARTHQUAKE: MoveId = MoveId(89);
        const TACKLE: MoveId = MoveId(33);

        let dex = Dex::new();
        let mon = |species, level, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, level, fixed_ivs(255), 0, moves)
                .expect("dex-resident")
        };
        let fainted = mon(METAGROSS, 5, vec![MEGA_KICK]);
        let player = mon(KOFFING, 50, vec![TACKLE]);
        let bench = vec![
            mon(PICHU, 5, vec![TACKLE]),
            mon(TREECKO, 5, vec![EARTHQUAKE]),
        ];
        let context = TrainerContext::new(
            MAY_ROUTE_103_MUDKIP,
            trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real trainer"),
            bench,
        );

        assert_eq!(
            context.most_suitable_by_damage(&dex, &fainted, MEGA_KICK, &player),
            Ok(Some(0)),
            "Earthquake takes no chart row against Levitate, so it cannot \
             overtake Tackle"
        );
    }
}
