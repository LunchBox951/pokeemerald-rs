//! Charge in the trainer AI's simulated damage: `AI_CalcDmg` doubles an
//! Electric move for a charged attacker (`src/battle_script_commands.c:1306-1316`),
//! so every `if_can_faint` read and move-power comparison sees the doubled
//! figure, and `Charge` itself is scoreable under `TRY_TO_FAINT`.

use super::{
    choose_trainer_action, compare_move_power, ensure_scoreable, estimated_damage, MoveScores,
    PowerComparison, FIRST_TURN, PERCENT_SCALE,
};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, Stats, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::{MoveId, SpeciesId};

const LEVEL: u8 = 5;
const PERSONALITY: u32 = 0;
const MAXIMUM_SIMULATED_DAMAGE_DRAW: u16 = 0;
const MAXIMUM_DAMAGE_ROLL: u16 = 0;
const SELECT_SECOND_TIED_MOVE: u16 = 1;

/// Fire with Blaze only: neutral to Electric and Normal, and a single ability
/// keeps the player's ability read draw-free.
const TORCHIC: SpeciesId = SpeciesId(280);
/// Water: the charged attacker, with no Electric STAB.
const MUDKIP: SpeciesId = SpeciesId(283);

const SHOCK_WAVE: MoveId = MoveId::SHOCK_WAVE;
const HYPER_VOICE: MoveId = MoveId::HYPER_VOICE;
const GROWL: MoveId = MoveId::GROWL;
const TACKLE: MoveId = MoveId::TACKLE;

fn pokemon(species: SpeciesId, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        LEVEL,
        Ivs::default(),
        PERSONALITY,
        moves,
    )
    .expect("test Pokemon must be dex-resident")
}

fn charged(mut pokemon: BattlePokemon) -> BattlePokemon {
    pokemon.volatiles_mut().set_charge();
    pokemon
}

fn estimate(move_id: MoveId, attacker: &BattlePokemon, defender: &BattlePokemon) -> u32 {
    estimated_damage(&Dex::new(), move_id, attacker, defender, PERCENT_SCALE).unwrap()
}

fn trainer_flags() -> AiFlags {
    AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT)
}

fn rng_with_maximum_simulated_damage(
    subsequent_draws: impl IntoIterator<Item = u16>,
) -> SequenceRng {
    SequenceRng::new(
        [MAXIMUM_SIMULATED_DAMAGE_DRAW; MAX_MON_MOVES]
            .into_iter()
            .chain(subsequent_draws),
    )
}

#[test]
fn a_charged_electric_estimate_matches_the_live_damage_step() {
    let dex = Dex::new();
    let attacker = charged(pokemon(MUDKIP, vec![SHOCK_WAVE]));
    let defender = pokemon(TORCHIC, vec![TACKLE]);

    let mut no_draws = SequenceRng::new([]);
    let before_roll =
        crate::hit::damage_before_roll(&dex, SHOCK_WAVE, &attacker, &defender, true, &mut no_draws)
            .unwrap();
    let mut best_roll = SequenceRng::new([MAXIMUM_DAMAGE_ROLL]);
    let live_damage = crate::damage::apply_damage_roll(before_roll.damage, &mut best_roll);

    assert_eq!(estimate(SHOCK_WAVE, &attacker, &defender), live_damage);
    let uncharged = pokemon(MUDKIP, vec![SHOCK_WAVE]);
    assert!(
        estimate(SHOCK_WAVE, &attacker, &defender) > estimate(SHOCK_WAVE, &uncharged, &defender)
    );
}

#[test]
fn charge_leaves_a_non_electric_estimate_unchanged() {
    let defender = pokemon(TORCHIC, vec![TACKLE]);
    let plain = pokemon(MUDKIP, vec![TACKLE, HYPER_VOICE]);
    let primed = charged(pokemon(MUDKIP, vec![TACKLE, HYPER_VOICE]));

    for move_id in [TACKLE, HYPER_VOICE, MoveId::STRUGGLE] {
        assert_eq!(
            estimate(move_id, &primed, &defender),
            estimate(move_id, &plain, &defender),
            "{move_id:?}"
        );
    }
}

#[test]
fn a_saturated_charged_estimate_does_not_overflow() {
    let dex = Dex::new();
    let planted = |stat| {
        BattlePokemon::new(
            &dex,
            MUDKIP,
            100,
            Ivs::default(),
            PERSONALITY,
            vec![SHOCK_WAVE],
        )
        .unwrap()
        .with_cached_stats(
            Stats {
                max_hp: 1000,
                attack: stat,
                defense: stat,
                speed: stat,
                sp_attack: stat,
                sp_defense: stat,
            },
            1000,
        )
    };
    let attacker = charged(planted(u32::MAX));
    let defender = planted(1);

    assert_eq!(
        estimated_damage(&dex, SHOCK_WAVE, &attacker, &defender, PERCENT_SCALE),
        Ok(u32::MAX)
    );
}

#[test]
fn charge_changes_the_move_power_comparison_between_electric_and_normal_moves() {
    let dex = Dex::new();
    let defender = pokemon(TORCHIC, vec![TACKLE]);
    let plain = pokemon(MUDKIP, vec![SHOCK_WAVE, HYPER_VOICE]);
    let primed = charged(pokemon(MUDKIP, vec![SHOCK_WAVE, HYPER_VOICE]));
    let scores = MoveScores::initialize(&plain, &mut rng_with_maximum_simulated_damage([]));

    let compare = |attacker: &BattlePokemon, slot, move_id| {
        compare_move_power(&dex, &scores, slot, move_id, attacker, &defender).unwrap()
    };
    assert_eq!(
        compare(&plain, 0, SHOCK_WAVE),
        PowerComparison::WeakerThanAnotherMove
    );
    assert_eq!(compare(&primed, 0, SHOCK_WAVE), PowerComparison::Strongest);
    assert_eq!(
        compare(&primed, 1, HYPER_VOICE),
        PowerComparison::WeakerThanAnotherMove
    );
}

#[test]
fn charge_flips_the_chosen_move_through_the_power_comparison_without_a_new_draw() {
    let dex = Dex::new();
    let moves = vec![SHOCK_WAVE, HYPER_VOICE];
    let player = pokemon(TORCHIC, vec![TACKLE]);

    let choose = |enemy: &BattlePokemon| {
        // Four simulated-damage draws, then the one tie-break draw; the
        // quadruple-effectiveness roll is never reached against Torchic.
        let mut rng = rng_with_maximum_simulated_damage([SELECT_SECOND_TIED_MOVE]);
        let action =
            choose_trainer_action(&dex, enemy, &player, trainer_flags(), FIRST_TURN, &mut rng)
                .unwrap();
        (action, rng.draws())
    };

    let plain = pokemon(MUDKIP, moves.clone());
    let primed = charged(pokemon(MUDKIP, moves));
    assert_eq!(choose(&plain), (EnemyAction::Move(1), MAX_MON_MOVES + 1));
    assert_eq!(choose(&primed), (EnemyAction::Move(0), MAX_MON_MOVES + 1));
}

#[test]
fn charge_moves_the_knockout_threshold_without_a_new_draw() {
    let dex = Dex::new();
    let moves = vec![SHOCK_WAVE, GROWL];
    let plain = pokemon(MUDKIP, moves.clone());
    let primed = charged(pokemon(MUDKIP, moves));
    let mut player = pokemon(TORCHIC, vec![TACKLE]);
    let doubled = estimate(SHOCK_WAVE, &primed, &player);
    assert!(estimate(SHOCK_WAVE, &plain, &player) < doubled);
    player.apply_damage(player.stats().max_hp - doubled);
    assert_eq!(player.current_hp(), doubled);

    let choose = |enemy: &BattlePokemon, draws: Vec<u16>| {
        let mut rng = rng_with_maximum_simulated_damage(draws);
        let action =
            choose_trainer_action(&dex, enemy, &player, trainer_flags(), FIRST_TURN, &mut rng)
                .unwrap();
        (action, rng.draws())
    };

    // Uncharged, neither move is boosted: the two tie and the tie-break draw
    // picks Growl. Charged, Shock Wave alone reaches the knockout bonus, so
    // the one tie-break draw is spent on a single candidate.
    assert_eq!(
        choose(&plain, vec![SELECT_SECOND_TIED_MOVE]),
        (EnemyAction::Move(1), MAX_MON_MOVES + 1)
    );
    assert_eq!(
        choose(&primed, vec![SELECT_SECOND_TIED_MOVE]),
        (EnemyAction::Move(0), MAX_MON_MOVES + 1)
    );
}

#[test]
fn charge_is_scoreable_under_try_to_faint_and_quick_attack_is_not() {
    let dex = Dex::new();
    for flags in [
        AiFlags::CHECK_BAD_MOVE,
        trainer_flags(),
        AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY),
    ] {
        assert_eq!(ensure_scoreable(&dex, MoveId::CHARGE, flags), Ok(()));
    }
    assert_eq!(
        ensure_scoreable(&dex, MoveId::QUICK_ATTACK, trainer_flags()),
        Err(BattleError::UnscoreableMoveEffect(MoveId::QUICK_ATTACK))
    );
}
