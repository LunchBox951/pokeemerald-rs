//! `estimated_damage` and `compare_move_power` against the real damage step.

use crate::battle::opponent_ai::EnemyAction;
use crate::battle::trainer_ai::choose_trainer_action;
use crate::battle::trainer_ai::damage_estimate::compare_move_power;
use crate::battle::trainer_ai::damage_estimate::estimated_damage;
use crate::battle::trainer_ai::damage_estimate::PowerComparison;
use crate::battle::trainer_ai::scores::MoveScores;
use crate::battle::trainer_ai::scores::PERCENT_SCALE;
use crate::battle::trainer_ai::setup_first_turn::FIRST_TURN;
use crate::battle::trainer_ai::test_support::pokemon;
use crate::battle::trainer_ai::test_support::rng_with_maximum_simulated_damage;
use crate::battle::trainer_ai::test_support::spend_move;
use crate::battle::trainer_ai::test_support::DEFAULT_PERSONALITY;
use crate::battle::trainer_ai::test_support::DELIBIRD;
use crate::battle::trainer_ai::test_support::EARTHQUAKE;
use crate::battle::trainer_ai::test_support::GASTLY;
use crate::battle::trainer_ai::test_support::GROWL;
use crate::battle::trainer_ai::test_support::GUTS_ABILITY_SLOT;
use crate::battle::trainer_ai::test_support::HUGE_POWER_ABILITY_SLOT;
use crate::battle::trainer_ai::test_support::HUSTLE_ABILITY_SLOT;
use crate::battle::trainer_ai::test_support::MAKUHITA;
use crate::battle::trainer_ai::test_support::MARILL;
use crate::battle::trainer_ai::test_support::MAXIMUM_DAMAGE_ROLL;
use crate::battle::trainer_ai::test_support::MILOTIC;
use crate::battle::trainer_ai::test_support::POUND;
use crate::battle::trainer_ai::test_support::ROUTE_103_LEVEL;
use crate::battle::trainer_ai::test_support::SELECT_FIRST_TIED_MOVE;
use crate::battle::trainer_ai::test_support::SQUIRTLE;
use crate::battle::trainer_ai::test_support::TACKLE;
use crate::battle::trainer_ai::test_support::TRAPINCH;
use crate::battle::trainer_ai::test_support::WING_ATTACK;
use crate::battle::trainer_ai::try_to_faint::score_try_to_faint;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::pokemon::Ivs;
use crate::pokemon::MAX_MON_MOVES;
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::MoveId;
use assets::SpeciesId;

/// `SPECIES_JYNX`: an Ice type whose Special Attack far exceeds its
/// Attack, so a weak Ice move can rival a resisted physical one.
const JYNX: SpeciesId = SpeciesId(124);

/// Upstream's `AI_CalcDmg` runs `CalculateBaseDamage` for every nonempty
/// slot, depleted or not (`battle_ai_script_commands.c:1204-1213`), and it
/// halves raw Special Attack for Fire/Ice against Thick Fat
/// (`pokemon.c:3202-3204`). Icicle Spear is the one Fire/Ice move that
/// trainer admission accepts, so the depleted slot is reachable.
#[test]
fn depleted_ice_move_respects_thick_fat_in_power_comparison() {
    let dex = Dex::new();
    let moves = vec![MoveId::ROCK_THROW, GROWL, MoveId::ICICLE_SPEAR];
    for move_id in &moves {
        assert_eq!(
            crate::battle::trainer::ensure_move_playable(&dex, *move_id, AiFlags::TRY_TO_FAINT),
            Ok(()),
            "{move_id:?}"
        );
    }
    let build = |species, moves| {
        BattlePokemon::new(
            &dex,
            species,
            20,
            Ivs {
                hp: 31,
                attack: 31,
                defense: 31,
                speed: 31,
                sp_attack: 31,
                sp_defense: 31,
            },
            DEFAULT_PERSONALITY,
            moves,
        )
        .unwrap()
    };
    let mut enemy = build(JYNX, moves);
    spend_move(&mut enemy, 2);
    let player = build(MAKUHITA, vec![TACKLE]);
    assert_eq!(player.ability(), crate::ability::THICK_FAT);

    assert_eq!(
        estimated_damage(&dex, MoveId::ROCK_THROW, &enemy, &player, PERCENT_SCALE).unwrap(),
        7
    );
    assert_eq!(
        estimated_damage(&dex, MoveId::ICICLE_SPEAR, &enemy, &player, PERCENT_SCALE).unwrap(),
        6
    );

    let mut rng = rng_with_maximum_simulated_damage([]);
    let mut scores = MoveScores::initialize(&enemy, &mut rng);
    assert_eq!(
        compare_move_power(&dex, &scores, 0, MoveId::ROCK_THROW, &enemy, &player),
        Ok(PowerComparison::Strongest)
    );
    for slot in 0..2 {
        let move_id = enemy.moves()[slot].move_id;
        score_try_to_faint(&dex, &mut scores, slot, move_id, &enemy, &player, &mut rng).unwrap();
    }
    assert_eq!(scores.values, [100, 100, 0, 0]);

    let mut rng = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
    assert_eq!(
        choose_trainer_action(
            &dex,
            &enemy,
            &player,
            AiFlags::TRY_TO_FAINT,
            FIRST_TURN,
            &mut rng,
        )
        .unwrap(),
        EnemyAction::Move(0)
    );
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
    rng.assert_exhausted();
}

#[test]
fn exhausted_ground_slots_keep_prechart_damage_against_levitate() {
    let dex = Dex::new();
    let target = pokemon(GASTLY, vec![POUND]);
    assert_eq!(target.ability(), assets::AbilityId::LEVITATE);
    let cases = [
        (vec![WING_ATTACK, GROWL], EnemyAction::Move(0)),
        (
            vec![WING_ATTACK, GROWL, MoveId::MUD_SLAP],
            EnemyAction::Move(0),
        ),
        (vec![WING_ATTACK, GROWL, EARTHQUAKE], EnemyAction::Move(1)),
    ];
    for (moves, expected) in cases {
        let mut enemy = pokemon(TRAPINCH, moves);
        if enemy.moves().len() == 3 {
            spend_move(&mut enemy, 2);
        }
        let mut rng = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
        let action = choose_trainer_action(
            &dex,
            &enemy,
            &target,
            AiFlags::TRY_TO_FAINT,
            FIRST_TURN,
            &mut rng,
        )
        .unwrap();
        assert_eq!(action, expected);
        assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
    }
}

#[test]
fn huge_power_estimated_damage_matches_the_real_damage_step() {
    let dex = Dex::new();
    let attacker = BattlePokemon::new(
        &dex,
        MARILL,
        ROUTE_103_LEVEL,
        Ivs::default(),
        DEFAULT_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
    .with_ability_slot(HUGE_POWER_ABILITY_SLOT);
    assert_eq!(attacker.ability(), crate::ability::HUGE_POWER);
    let defender = pokemon(SQUIRTLE, vec![TACKLE]);

    let critical_hits_suppressed = true;
    let mut no_draws = SequenceRng::new([]);
    let damage_before_roll = crate::hit::damage_before_roll(
        &dex,
        TACKLE,
        &attacker,
        &defender,
        critical_hits_suppressed,
        &mut no_draws,
    )
    .unwrap();
    let mut best_damage_roll = SequenceRng::new([MAXIMUM_DAMAGE_ROLL]);
    let real_damage =
        crate::damage::apply_damage_roll(damage_before_roll.damage, &mut best_damage_roll);
    let estimated_damage =
        estimated_damage(&dex, TACKLE, &attacker, &defender, PERCENT_SCALE).unwrap();

    assert_eq!(estimated_damage, real_damage);
}

#[test]
fn hustle_estimated_damage_matches_the_real_damage_step() {
    let dex = Dex::new();
    let attacker = BattlePokemon::new(
        &dex,
        DELIBIRD,
        ROUTE_103_LEVEL,
        Ivs::default(),
        DEFAULT_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
    .with_ability_slot(HUSTLE_ABILITY_SLOT);
    assert_eq!(attacker.ability(), crate::ability::HUSTLE);
    let defender = pokemon(SQUIRTLE, vec![TACKLE]);

    let control = BattlePokemon::new(
        &dex,
        DELIBIRD,
        ROUTE_103_LEVEL,
        Ivs::default(),
        DEFAULT_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
    .with_ability_slot(0);
    assert_ne!(control.ability(), crate::ability::HUSTLE);
    let control_estimate =
        estimated_damage(&dex, TACKLE, &control, &defender, PERCENT_SCALE).unwrap();

    let critical_hits_suppressed = true;
    let mut no_draws = SequenceRng::new([]);
    let damage_before_roll = crate::hit::damage_before_roll(
        &dex,
        TACKLE,
        &attacker,
        &defender,
        critical_hits_suppressed,
        &mut no_draws,
    )
    .unwrap();
    let mut best_damage_roll = SequenceRng::new([MAXIMUM_DAMAGE_ROLL]);
    let real_damage =
        crate::damage::apply_damage_roll(damage_before_roll.damage, &mut best_damage_roll);
    let estimated_damage =
        estimated_damage(&dex, TACKLE, &attacker, &defender, PERCENT_SCALE).unwrap();

    assert_eq!(estimated_damage, real_damage);
    assert!(
        estimated_damage > control_estimate,
        "the Hustle attacker's estimate must exceed the unboosted control's"
    );
}

#[test]
fn a_statused_guts_attackers_estimated_damage_matches_the_real_damage_step() {
    let dex = Dex::new();
    let mut attacker = BattlePokemon::new(
        &dex,
        MAKUHITA,
        ROUTE_103_LEVEL,
        Ivs::default(),
        DEFAULT_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
    .with_ability_slot(GUTS_ABILITY_SLOT);
    assert_eq!(attacker.ability(), crate::ability::GUTS);
    attacker.set_status1(crate::status1::Status1::Paralysed);
    let defender = pokemon(MILOTIC, vec![TACKLE]);

    let critical_hits_suppressed = true;
    let mut no_draws = SequenceRng::new([]);
    let damage_before_roll = crate::hit::damage_before_roll(
        &dex,
        TACKLE,
        &attacker,
        &defender,
        critical_hits_suppressed,
        &mut no_draws,
    )
    .unwrap();
    let mut best_damage_roll = SequenceRng::new([MAXIMUM_DAMAGE_ROLL]);
    let real_damage =
        crate::damage::apply_damage_roll(damage_before_roll.damage, &mut best_damage_roll);
    let estimated_damage =
        estimated_damage(&dex, TACKLE, &attacker, &defender, PERCENT_SCALE).unwrap();

    assert_eq!(estimated_damage, real_damage);
    assert!(
        estimated_damage
            > estimated_damage_of_a_healthy_attacker(&dex, &defender)
                .expect("healthy control must resolve"),
        "the statused Guts attacker's estimate must exceed the healthy control's"
    );
}

fn estimated_damage_of_a_healthy_attacker(
    dex: &Dex,
    defender: &BattlePokemon,
) -> Result<u32, BattleError> {
    let healthy_attacker = BattlePokemon::new(
        dex,
        MAKUHITA,
        ROUTE_103_LEVEL,
        Ivs::default(),
        DEFAULT_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
    .with_ability_slot(GUTS_ABILITY_SLOT);
    estimated_damage(dex, TACKLE, &healthy_attacker, defender, PERCENT_SCALE)
}

#[test]
fn a_statused_marvel_scale_defenders_estimated_damage_matches_the_real_damage_step() {
    let dex = Dex::new();
    let attacker = pokemon(MAKUHITA, vec![TACKLE]);
    let mut defender = pokemon(MILOTIC, vec![TACKLE]);
    assert_eq!(defender.ability(), crate::ability::MARVEL_SCALE);
    defender.set_status1(crate::status1::Status1::Poisoned);

    let critical_hits_suppressed = true;
    let mut no_draws = SequenceRng::new([]);
    let damage_before_roll = crate::hit::damage_before_roll(
        &dex,
        TACKLE,
        &attacker,
        &defender,
        critical_hits_suppressed,
        &mut no_draws,
    )
    .unwrap();
    let mut best_damage_roll = SequenceRng::new([MAXIMUM_DAMAGE_ROLL]);
    let real_damage =
        crate::damage::apply_damage_roll(damage_before_roll.damage, &mut best_damage_roll);
    let statused_estimate =
        estimated_damage(&dex, TACKLE, &attacker, &defender, PERCENT_SCALE).unwrap();

    assert_eq!(statused_estimate, real_damage);
    let healthy_defender = pokemon(MILOTIC, vec![TACKLE]);
    let healthy_estimate =
        estimated_damage(&dex, TACKLE, &attacker, &healthy_defender, PERCENT_SCALE).unwrap();
    assert!(
        statused_estimate < healthy_estimate,
        "the statused Marvel Scale defender's estimate must be lower than the healthy control's"
    );
}

/// `TypeCalc` returns for Struggle before the chart, so a depleted
/// Struggle outranks an immune or resisted Take Down (power 90, resisted ~45 against 50) in the `TryToFaint`
/// power comparison instead of flooring to one alongside it.
#[test]
fn a_depleted_struggle_keeps_untyped_damage_in_the_power_comparison() {
    let dex = Dex::new();
    let attacker_with = |moves: Vec<MoveId>| {
        let mut attacker = pokemon(TRAPINCH, moves);
        spend_move(&mut attacker, 1);
        attacker
    };
    let attacker = attacker_with(vec![MoveId::TAKE_DOWN, MoveId::STRUGGLE]);
    // Gastly (Ghost), Onix (Rock/Ground), Magnemite (Electric/Steel), Aron.
    for species in [GASTLY, SpeciesId(95), SpeciesId(81), SpeciesId(382)] {
        let defender = pokemon(species, vec![POUND]);
        let mut rng = rng_with_maximum_simulated_damage([]);
        let scores = MoveScores::initialize(&attacker, &mut rng);
        let struggle = estimated_damage(
            &dex,
            MoveId::STRUGGLE,
            &attacker,
            &defender,
            scores.simulated_damage_percent[1],
        )
        .unwrap();
        assert!(struggle > 1, "species {species:?}: {struggle}");
        assert_eq!(
            compare_move_power(&dex, &scores, 0, MoveId::TAKE_DOWN, &attacker, &defender),
            Ok(PowerComparison::WeakerThanAnotherMove),
            "species {species:?}"
        );
    }
}

#[test]
fn plus_minus_estimated_damage_matches_the_real_damage_step() {
    let dex = Dex::new();
    let plusle = pokemon(SpeciesId(353), vec![MoveId::WATER_GUN]);
    let minun = pokemon(SpeciesId(354), vec![MoveId::WATER_GUN]);
    let squirtle = pokemon(SQUIRTLE, vec![MoveId::WATER_GUN]);
    assert_eq!(plusle.ability(), assets::AbilityId::PLUS);
    assert_eq!(minun.ability(), assets::AbilityId::MINUS);

    for (attacker, defender) in [(&plusle, &minun), (&minun, &plusle), (&plusle, &squirtle)] {
        let mut no_draws = SequenceRng::new([]);
        let before_roll = crate::hit::damage_before_roll(
            &dex,
            MoveId::WATER_GUN,
            attacker,
            defender,
            true,
            &mut no_draws,
        )
        .unwrap();
        let mut best_roll = SequenceRng::new([MAXIMUM_DAMAGE_ROLL]);
        let real = crate::damage::apply_damage_roll(before_roll.damage, &mut best_roll);
        let estimate =
            estimated_damage(&dex, MoveId::WATER_GUN, attacker, defender, PERCENT_SCALE).unwrap();
        assert_eq!(estimate, real);
    }
    // The boost itself must show: the complement out-damages a same-ability bystander.
    let boosted =
        estimated_damage(&dex, MoveId::WATER_GUN, &plusle, &minun, PERCENT_SCALE).unwrap();
    let control =
        estimated_damage(&dex, MoveId::WATER_GUN, &plusle, &plusle, PERCENT_SCALE).unwrap();
    assert!(boosted > control, "{boosted} vs {control}");
}
