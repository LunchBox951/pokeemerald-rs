//! `GetMostSuitableMonToSwitchInto`'s typing and most-damage passes: the
//! forced post-faint replacement selector [`TrainerContext::send_out_next`]
//! tries before falling back to party order.

use assets::{AbilityId, Effectiveness, MoveId, Type, TypeChart};

use crate::ability::{huge_power_attack, pinch_boosts_power, thick_fat_special_attack};
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

/// Power value of the OHKO moves, the only one the most-damage pass skips
/// (`pokeemerald/src/battle_ai_switch_items.c:776`).
const OHKO_POWER_SENTINEL: u8 = 1;

impl TrainerContext {
    /// The healthy bench member with the best typing score against `player`
    /// that also knows a super-effective move, else the next-best typing.
    /// Only the super-effective check honours `player`'s Levitate: the typing
    /// score reads no ability (`pokeemerald/src/battle_ai_switch_items.c:709`-`:733`).
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

    /// The healthy bench member whose move scores highest against `player`,
    /// layered onto `resolving_move`'s stale base damage from `fainted`.
    /// The running best is a `u8`, reproducing upstream's `bestDmg` wraparound
    /// (ledger entry `GetMostSuitableMonToSwitchInto`).
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

/// Scores `candidate`'s typing against `player`'s. Both of `player`'s type
/// slots apply even when equal, squaring the effect for a single-typed
/// `player` (`pokeemerald/src/battle_ai_switch_items.c:709`-`:710`). Unlike
/// [`apply_dual_type_effectiveness`], no minimum-one floor applies (`:605`-`:623`).
fn typing_suitability_score(player: &BattlePokemon, candidate: &BattlePokemon) -> u32 {
    let candidate_types = candidate.types();
    let player_types = player.types();
    let score = apply_raw_type_multiplier(NEUTRAL_TYPE_SCORE, player_types[0], candidate_types);
    apply_raw_type_multiplier(score, player_types[1], candidate_types)
}

/// Multiplies `score` by every [`TypeChart`] row of `attacking_type` hitting
/// a distinct defending slot, truncating each step
/// (`pokeemerald/src/battle_ai_switch_items.c:605`-`:627`).
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

/// Whether `defender`'s Levitate refuses `move_type`. Upstream's `TypeCalc`
/// skips the type chart after STAB and leaves the damage untouched, so the
/// most-damage pass keeps the STAB-multiplied base and the super-effective
/// flag is never set (`pokeemerald/src/battle_script_commands.c:1547`-`:1556`).
fn levitate_refuses(defender: &BattlePokemon, move_type: Type) -> bool {
    move_type == Type::Ground && defender.ability() == AbilityId::LEVITATE
}

/// Whether `candidate` knows a damaging move super effective against
/// `player`. An immunity row is terminal here, unlike in
/// [`most_suitable_type_effectiveness`]: a later super-effective row cannot
/// set the flag (`pokeemerald/src/battle_script_commands.c:1525`).
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

/// The most-damage pass's shared base damage from `resolving_move`, with
/// `fainted` attacking `player`, or `None` for a `???`-typed move.
/// `resolving_move`'s own power is never excluded, matching upstream's
/// unconditional `AI_CalcDmg`.
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
    // Thick Fat halves raw Special Attack before stage scaling
    // (`pokemon.c:3202-3204`), after Huge/Pure Power as in the trainer AI.
    let attack_stat = thick_fat_special_attack(player.ability(), move_type, attack_stat);
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

/// Layers `move_id`'s STAB (from `fainted`'s types, as upstream's `TypeCalc`
/// does) and type effectiveness against `defender` onto the shared `base`
/// from [`stale_base_damage`]; `None` for an OHKO move.
/// Struggle, `???`-typed moves and Levitate-refused moves skip the chart.
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
    // `battle_script_commands.c:1541`: Struggle returns before STAB.
    if move_id == MoveId::STRUGGLE {
        return Ok(Some(base));
    }
    let Some(move_type) = move_data.move_type.battle_type() else {
        return Ok(Some(base));
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

/// Applies each matching chart row to the running `damage`. Unlike
/// [`apply_dual_type_effectiveness`], an immune row is not terminal: a later
/// effective row floors the zeroed damage back to one
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

    /// Stock (non-`BUGFIX`) `u8 bestDmg` wraparound
    /// (`battle_ai_switch_items.c:781`-`:784`): Water Gun's over-255 score loses to Tackle's.
    #[test]
    fn the_most_damage_pass_narrows_its_running_winner_to_a_byte() {
        const METAGROSS: SpeciesId = SpeciesId(400);
        const SANDSHREW: SpeciesId = SpeciesId(27);
        const MUDKIP: SpeciesId = SpeciesId(283);
        const PICHU: SpeciesId = SpeciesId(172);
        const MEGA_KICK: MoveId = MoveId::MEGA_KICK;
        const TACKLE: MoveId = MoveId::TACKLE;
        const WATER_GUN: MoveId = MoveId::WATER_GUN;

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

    /// Against Gligar (Ground/Flying), Flying's row floors the zero Ground's
    /// immunity left to one (`battle_script_commands.c:1504`-`:1506`,
    /// `:1570`-`:1580`), so Pichu's Thunder Shock scores and the pass does not
    /// fall to party order. Pinsir's Guillotine is the OHKO sentinel and never scores.
    #[test]
    fn the_most_damage_pass_floors_an_immunity_a_later_row_reopens() {
        const METAGROSS: SpeciesId = SpeciesId(400);
        const GLIGAR: SpeciesId = SpeciesId(207);
        const PINSIR: SpeciesId = SpeciesId(127);
        const PICHU: SpeciesId = SpeciesId(172);
        const MEGA_KICK: MoveId = MoveId::MEGA_KICK;
        const GUILLOTINE: MoveId = MoveId::GUILLOTINE;
        const THUNDER_SHOCK: MoveId = MoveId::THUNDER_SHOCK;

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

    /// Against a Levitate Koffing the Grass candidate wins the ability-blind
    /// typing score, but its Earthquake is refused
    /// (`battle_script_commands.c:1554`-`:1556`), so the pass takes the Pichu
    /// whose Confusion is super effective against Poison.
    #[test]
    fn the_typing_pass_rejects_a_ground_move_the_players_levitate_refuses() {
        const KOFFING: SpeciesId = SpeciesId(109);
        const PICHU: SpeciesId = SpeciesId(172);
        const EARTHQUAKE: MoveId = MoveId::EARTHQUAKE;
        const CONFUSION: MoveId = MoveId::CONFUSION;
        const TACKLE: MoveId = MoveId::TACKLE;

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

    /// The most-damage pass's shared base figure must inherit Hustle from
    /// `fainted.attacking_stat`, the same accessor the real hit and
    /// trainer-AI estimate read, so all three agree.
    #[test]
    fn stale_base_damage_carries_a_hustle_fainted_attackers_boost() {
        // Delibird: Vital Spirit in slot 0, Hustle in slot 1.
        const DELIBIRD: SpeciesId = SpeciesId(225);
        const SQUIRTLE: SpeciesId = SpeciesId(7);
        const TACKLE: MoveId = MoveId::TACKLE;

        let dex = Dex::new();
        let hustle_fainted =
            BattlePokemon::new(&dex, DELIBIRD, 20, fixed_ivs(255), 0, vec![TACKLE])
                .expect("dex-resident")
                .with_ability_slot(1);
        assert_eq!(hustle_fainted.ability(), AbilityId::HUSTLE);
        let control_fainted =
            BattlePokemon::new(&dex, DELIBIRD, 20, fixed_ivs(255), 0, vec![TACKLE])
                .expect("dex-resident")
                .with_ability_slot(0);
        assert_ne!(control_fainted.ability(), AbilityId::HUSTLE);
        let player = BattlePokemon::new(&dex, SQUIRTLE, 20, fixed_ivs(255), 0, vec![TACKLE])
            .expect("dex-resident");

        let hustle_base = super::stale_base_damage(&dex, TACKLE, &hustle_fainted, &player)
            .unwrap()
            .unwrap();
        let control_base = super::stale_base_damage(&dex, TACKLE, &control_fainted, &player)
            .unwrap()
            .unwrap();

        assert!(
            hustle_base > control_base,
            "a Hustle fainted attacker's stale base damage must carry the boost: \
             {hustle_base} vs {control_base}"
        );
    }

    /// Upstream's `CalculateBaseDamage` halves raw Special Attack for a Fire or
    /// Ice move against Thick Fat (`pokemon.c:3202-3204`), so the stale base
    /// damage must carry it; a non-Fire/Ice move must not.
    #[test]
    fn stale_base_damage_carries_the_defenders_thick_fat() {
        // Makuhita: Thick Fat in slot 0, Guts in slot 1.
        const JYNX: SpeciesId = SpeciesId(124);
        const MAKUHITA: SpeciesId = SpeciesId(335);

        let dex = Dex::new();
        let fainted = BattlePokemon::new(
            &dex,
            JYNX,
            20,
            fixed_ivs(255),
            0,
            vec![MoveId::ICICLE_SPEAR],
        )
        .expect("dex-resident");
        let player = |slot| {
            BattlePokemon::new(&dex, MAKUHITA, 20, fixed_ivs(255), 0, vec![MoveId::TACKLE])
                .expect("dex-resident")
                .with_ability_slot(slot)
        };
        let thick = player(0);
        let control = player(1);
        assert_eq!(thick.ability(), AbilityId::THICK_FAT);
        assert_eq!(control.ability(), AbilityId::GUTS);

        let base = |mv, p: &BattlePokemon| {
            super::stale_base_damage(&dex, mv, &fainted, p)
                .unwrap()
                .unwrap()
        };
        assert!(
            base(MoveId::ICICLE_SPEAR, &thick) < base(MoveId::ICICLE_SPEAR, &control),
            "Thick Fat must lower an Ice move's stale base damage"
        );
        assert_eq!(
            base(MoveId::PSYCHIC, &thick),
            base(MoveId::PSYCHIC, &control),
            "Thick Fat must not touch a Psychic move"
        );
    }

    /// Against Gligar (Ground/Flying) the Electric->Ground immunity row
    /// (`battle_main.c:356`) blocks the following super-effective Flying row
    /// (`battle_script_commands.c:1525`), so Thunder Shock cannot qualify the
    /// Torchic and the pass takes the Mudkip.
    #[test]
    fn the_typing_pass_rejects_a_super_effective_row_behind_an_immunity_row() {
        const GLIGAR: SpeciesId = SpeciesId(207);
        const TORCHIC: SpeciesId = SpeciesId(280);
        const MUDKIP: SpeciesId = SpeciesId(283);
        const THUNDER_SHOCK: MoveId = MoveId::THUNDER_SHOCK;
        const WATER_GUN: MoveId = MoveId::WATER_GUN;
        const TACKLE: MoveId = MoveId::TACKLE;

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

    /// Levitate leaves Earthquake on the plain base (`:1547`-`:1556`), tied
    /// with Tackle under the strict `bestDmg < gBattleMoveDamage` comparison
    /// (`battle_ai_switch_items.c:781`), so the earlier member wins.
    #[test]
    fn the_most_damage_pass_scores_a_levitate_refused_ground_move_without_the_chart() {
        const METAGROSS: SpeciesId = SpeciesId(400);
        const KOFFING: SpeciesId = SpeciesId(109);
        const PICHU: SpeciesId = SpeciesId(172);
        const MEGA_KICK: MoveId = MoveId::MEGA_KICK;
        const EARTHQUAKE: MoveId = MoveId::EARTHQUAKE;
        const TACKLE: MoveId = MoveId::TACKLE;

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

    /// Struggle returns before any chart row (`battle_script_commands.c:1536`-`:1552`).
    #[test]
    fn struggle_keeps_its_untyped_base_against_every_defender() {
        let dex = Dex::new();
        let mon = |species, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, 5, fixed_ivs(255), 0, moves).expect("dex-resident")
        };
        let fainted = mon(SpeciesId(400), vec![MoveId::MEGA_KICK]);
        // Gastly, Onix (Rock/Ground), Magnemite (Electric/Steel), Aron (Steel/Rock).
        for species in [92, 95, 81, 382] {
            let defender = mon(SpeciesId(species), vec![MoveId::TACKLE]);
            for base in [0, 100] {
                assert_eq!(
                    super::candidate_move_damage(&dex, MoveId::STRUGGLE, base, &fainted, &defender),
                    Ok(Some(base)),
                    "species {species} base {base}"
                );
            }
        }
    }

    /// Against a Ghost with no scoring bench move, upstream's damage pass
    /// picks the member whose depleted Struggle keeps its base while Tackle
    /// is immune; the port used to zero both and fall to party order.
    #[test]
    fn a_depleted_struggle_wins_the_damage_pass_against_a_ghost() {
        let dex = Dex::new();
        let mon = |species, level, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, level, fixed_ivs(255), 0, moves)
                .expect("dex-resident")
        };
        let fainted = mon(SpeciesId(400), 70, vec![MoveId::MEGA_KICK]);
        let player = mon(SpeciesId(92), 50, vec![MoveId::TACKLE]);
        let mut treecko = mon(TREECKO, 5, vec![MoveId::TACKLE, MoveId::STRUGGLE]);
        while treecko.moves()[1].pp > 0 {
            treecko.deduct_pp(1).unwrap();
        }
        let bench = vec![mon(SpeciesId(172), 5, vec![MoveId::TACKLE]), treecko];
        let context = TrainerContext::new(
            MAY_ROUTE_103_MUDKIP,
            trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real trainer"),
            bench,
        );

        assert_eq!(
            context.most_suitable_by_damage(&dex, &fainted, MoveId::MEGA_KICK, &player),
            Ok(Some(1))
        );
    }

    /// Curse is `???`-typed and takes neither STAB nor a chart row.
    #[test]
    fn curse_keeps_its_untyped_base_against_every_defender() {
        let dex = Dex::new();
        let mon = |species, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, 5, fixed_ivs(255), 0, moves).expect("dex-resident")
        };
        let fainted = mon(SpeciesId(400), vec![MoveId::MEGA_KICK]);
        for species in [92, 95, 81, 382] {
            let defender = mon(SpeciesId(species), vec![MoveId::TACKLE]);
            for base in [0, 100] {
                assert_eq!(
                    super::candidate_move_damage(&dex, MoveId::CURSE, base, &fainted, &defender),
                    Ok(Some(base)),
                    "species {species} base {base}"
                );
            }
        }
    }

    /// Upstream's loop skips only `MOVE_NONE` and power 1, so a depleted Curse still scores.
    #[test]
    fn a_depleted_curse_wins_the_damage_pass_against_a_ghost() {
        let dex = Dex::new();
        let mon = |species, level, moves: Vec<MoveId>| {
            BattlePokemon::new(&dex, species, level, fixed_ivs(255), 0, moves)
                .expect("dex-resident")
        };
        let fainted = mon(SpeciesId(400), 70, vec![MoveId::MEGA_KICK]);
        let player = mon(SpeciesId(92), 50, vec![MoveId::TACKLE]);
        let mut treecko = mon(TREECKO, 5, vec![MoveId::TACKLE, MoveId::CURSE]);
        while treecko.moves()[1].pp > 0 {
            treecko.deduct_pp(1).unwrap();
        }
        let bench = vec![mon(SpeciesId(172), 5, vec![MoveId::TACKLE]), treecko];
        let context = TrainerContext::new(
            MAY_ROUTE_103_MUDKIP,
            trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real trainer"),
            bench,
        );

        assert_eq!(
            context.most_suitable_by_damage(&dex, &fainted, MoveId::MEGA_KICK, &player),
            Ok(Some(1))
        );
    }
}
