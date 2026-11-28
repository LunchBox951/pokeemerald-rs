//! `EFFECT_SPECIAL_ATTACK_DOWN_HIT` (Mist Ball) driven through complete turns:
//! the secondary Special Attack drop lands after the hit damage and its chance
//! draw, never on a fainted target, and is silent when an ability or the stage
//! floor stops it (`SetMoveEffect` -> `ChangeStatBuffs` with `flags == 0`,
//! `pokeemerald/src/battle_script_commands.c:2666-2683,6987-7037`).
//!
//! Mist Ball's 50% `secondaryEffectChance` takes `Cmd_seteffectwithchance`'s
//! drawing branch (`:2908-2930`): a landed hit spends accuracy, critical,
//! damage and chance draws, and the roll `Random() % 100 < 50` separates 49
//! from 50.

use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, ChangedStat, Dex, PlayerAction,
    StatStage, StatStages,
};

const MIST_BALL: MoveId = MoveId::MIST_BALL;
const TACKLE: MoveId = MoveId::TACKLE;
const WATER_GUN: MoveId = MoveId::WATER_GUN;

/// `SPECIES_RATTATA`, outspeeding every level-5 enemy below.
const RATTATA: u16 = 19;
const ZIGZAGOON: u16 = 288;
/// A special attacker that Mist Ball does not resist or overkill.
const POLIWAG: u16 = 60;
/// Hyper Cutter in its only ability slot.
const PINSIR: u16 = SpeciesId::PINSIR.0;
/// Clear Body in its only ability slot.
const BELDUM: u16 = SpeciesId::BELDUM.0;
/// Shield Dust in its only ability slot.
const WURMPLE: u16 = 290;
/// White Smoke in its first ability slot.
const TORKOAL: u16 = SpeciesId::TORKOAL.0;
/// Serene Grace in its first ability slot.
const DUNSPARCE: u16 = 206;

/// The constructor, turn-start and enemy-selection draws.
const SETUP: [u16; 3] = [0, 0, 0];
/// Mist Ball's accuracy, no-crit and damage draws.
const LANDING: [u16; 3] = [0, 1, 0];
/// The enemy's attack: accuracy, no-crit, damage and the discarded chance draw.
const ENEMY_ATTACK: [u16; 4] = [0, 1, 0, 0];
/// The highest chance draw below Mist Ball's 50% threshold, and the lowest at it.
const LAST_LANDING_ROLL: u16 = 49;
const FIRST_FAILING_ROLL: u16 = 50;

/// A level-10 Rattata's Mist Ball on a level-5 Zigzagoon at the damage draw of
/// 0 (the full random factor): `((2 * 10 / 5 + 2) * 70 * 13 / 10) / 50 + 2` with
/// Special Attack 13 against Special Defense 10, no STAB and neutral Psychic
/// against Normal.
const MIST_BALL_DAMAGE: u32 = 12;

fn mist_ball_script(chance: u16) -> SequenceRng {
    SequenceRng::new(
        SETUP
            .into_iter()
            .chain(LANDING)
            .chain([chance])
            .chain(ENEMY_ATTACK),
    )
}

fn mist_ball_turn(enemy: BattlePokemon, chance: u16) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![MIST_BALL]);
    let mut rng = mist_ball_script(chance);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

fn enemy_of(species: u16) -> BattlePokemon {
    max_iv_mon(&Dex::new(), species, 5, vec![TACKLE])
}

fn sp_attack_loss(stage: i8) -> StatStages {
    StatStages {
        sp_attack: StatStage::new(stage).unwrap(),
        ..StatStages::default()
    }
}

fn stat_events(events: &[BattleEvent]) -> Vec<&BattleEvent> {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                BattleEvent::StatFell { .. }
                    | BattleEvent::StatWontGoLower { .. }
                    | BattleEvent::StatLossPrevented { .. }
            )
        })
        .collect()
}

fn hit_damage(events: &[BattleEvent], move_id: MoveId) -> u32 {
    events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit {
                move_id: hit_move,
                damage,
                ..
            } if *hit_move == move_id => Some(*damage),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {move_id:?} hit in {events:?}"))
}

#[test]
fn the_roll_just_below_the_threshold_lowers_special_attack_and_the_roll_at_it_does_not() {
    let (landed, landed_events) = mist_ball_turn(enemy_of(ZIGZAGOON), LAST_LANDING_ROLL);
    assert_eq!(
        landed_events[1],
        BattleEvent::Hit {
            by_player: true,
            move_id: MIST_BALL,
            damage: MIST_BALL_DAMAGE,
            is_critical: false,
        },
        "{landed_events:?}"
    );
    assert_eq!(
        landed_events[2],
        BattleEvent::StatFell {
            by_player: true,
            move_id: MIST_BALL,
            stat: ChangedStat::SpAttack,
            new_stage: StatStage::new(-1).unwrap(),
            magnitude: 1,
        },
        "{landed_events:?}"
    );
    assert_eq!(landed.enemy().stages(), sp_attack_loss(-1));

    let (missed_roll, missed_events) = mist_ball_turn(enemy_of(ZIGZAGOON), FIRST_FAILING_ROLL);
    assert_eq!(hit_damage(&missed_events, MIST_BALL), MIST_BALL_DAMAGE);
    assert!(stat_events(&missed_events).is_empty(), "{missed_events:?}");
    assert_eq!(missed_roll.enemy().stages(), StatStages::default());
}

#[test]
fn the_lowered_special_attack_weakens_the_targets_next_special_hit() {
    let poliwag = || max_iv_mon(&Dex::new(), POLIWAG, 5, vec![WATER_GUN]);
    let (_, landed_events) = mist_ball_turn(poliwag(), LAST_LANDING_ROLL);
    let (_, control_events) = mist_ball_turn(poliwag(), FIRST_FAILING_ROLL);

    assert_eq!(
        hit_damage(&landed_events, MIST_BALL),
        hit_damage(&control_events, MIST_BALL),
        "the drop never changes the damage already dealt"
    );
    assert!(
        hit_damage(&landed_events, WATER_GUN) < hit_damage(&control_events, WATER_GUN),
        "{landed_events:?} versus {control_events:?}"
    );
}

/// Serene Grace doubles the chance, which is unported where the drop can land,
/// so both screens refuse it rather than silently ignoring the doubling: the
/// enemy's moveset at construction and the player's selection before any draw.
#[test]
fn a_serene_grace_mist_ball_is_refused_before_any_draw() {
    let dex = Dex::new();
    let serene_grace_enemy = max_iv_mon(&dex, DUNSPARCE, 20, vec![MIST_BALL]);
    assert_eq!(serene_grace_enemy.ability(), AbilityId::SERENE_GRACE);
    let refusal = Some(BattleError::UnportedAbilityInteraction(
        AbilityId::SERENE_GRACE,
    ));
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(
            Dex::new(),
            enemy_of(ZIGZAGOON),
            serene_grace_enemy,
            false,
            &mut rng
        )
        .err(),
        refusal
    );
    rng.assert_exhausted();

    let player = max_iv_mon(&dex, DUNSPARCE, 20, vec![MIST_BALL]);
    // Only the constructor's turn-number seed draw; the refused selection spends none.
    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(Dex::new(), player, enemy_of(ZIGZAGOON), false, &mut rng).unwrap();
    let turn_error = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(Some(turn_error.error()), refusal);
    rng.assert_exhausted();
}

/// A forced accuracy miss ends the script before the chance draw, leaving the
/// accuracy draw as Mist Ball's only one.
#[test]
fn a_missed_mist_ball_spends_exactly_one_draw_and_lowers_nothing() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 10, vec![MIST_BALL]);
    player.stages_mut().accuracy = StatStage::MIN;
    let mut rng = SequenceRng::new(SETUP.into_iter().chain([99]).chain(ENEMY_ATTACK));
    let mut battle = Battle::new(dex, player, enemy_of(ZIGZAGOON), false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: true,
            move_id: MIST_BALL
        }),
        "fixture sanity -- the accuracy check must fail: {events:?}"
    );
    assert!(stat_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages(), StatStages::default());
}

#[test]
fn clear_body_white_smoke_and_shield_dust_keep_the_draw_and_stay_silent() {
    for species in [BELDUM, WURMPLE, TORKOAL] {
        let (battle, events) = mist_ball_turn(enemy_of(species), 0);

        assert!(
            events.iter().any(|event| matches!(
                event,
                BattleEvent::Hit {
                    move_id: MIST_BALL,
                    ..
                }
            )),
            "species {species}: the hit must land for the guard to be reached: {events:?}"
        );
        assert!(!battle.enemy().is_fainted(), "species {species}");
        assert!(
            stat_events(&events).is_empty(),
            "species {species}: a secondary is silent where a stat-lowering move would announce: {events:?}"
        );
        assert_eq!(battle.enemy().stages(), StatStages::default());
    }
}

/// Hyper Cutter guards Attack only, so it leaves the Special Attack drop to land.
#[test]
fn hyper_cutter_does_not_stop_the_special_attack_drop() {
    let (battle, events) = mist_ball_turn(enemy_of(PINSIR), 0);
    assert_eq!(stat_events(&events).len(), 1, "{events:?}");
    assert_eq!(battle.enemy().stages(), sp_attack_loss(-1));
}

#[test]
fn the_guarding_species_carry_the_abilities_the_matrix_names() {
    let dex = Dex::new();
    for (species, ability) in [
        (BELDUM, AbilityId::CLEAR_BODY),
        (WURMPLE, AbilityId::SHIELD_DUST),
        (TORKOAL, AbilityId::WHITE_SMOKE),
        (PINSIR, AbilityId::HYPER_CUTTER),
    ] {
        assert_eq!(
            max_iv_mon_with_personality(&dex, species, 5, vec![TACKLE], 0).ability(),
            ability,
            "species {species}"
        );
    }
}

#[test]
fn a_target_already_at_minus_six_is_left_alone_without_a_message() {
    let mut enemy = enemy_of(ZIGZAGOON);
    enemy.stages_mut().sp_attack = StatStage::MIN;
    let (battle, events) = mist_ball_turn(enemy, 0);

    assert!(stat_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages().sp_attack, StatStage::MIN);
}

#[test]
fn a_target_at_minus_five_is_lowered_to_the_floor() {
    let mut enemy = enemy_of(ZIGZAGOON);
    enemy.stages_mut().sp_attack = StatStage::new(-5).unwrap();
    let (battle, events) = mist_ball_turn(enemy, 0);

    assert!(
        events.contains(&BattleEvent::StatFell {
            by_player: true,
            move_id: MIST_BALL,
            stat: ChangedStat::SpAttack,
            new_stage: StatStage::MIN,
            magnitude: 1,
        }),
        "{events:?}"
    );
    assert_eq!(battle.enemy().stages().sp_attack, StatStage::MIN);
}

/// The chance draw precedes the faint guard, so a lethal landed roll still
/// spends it but applies nothing (`battle_script_commands.c:2261-2264`,
/// `:2908-2930`).
#[test]
fn a_lethal_mist_ball_spends_four_draws_but_never_lowers_a_fainted_target() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![MIST_BALL]);
    let mut enemy = enemy_of(ZIGZAGOON);
    enemy.apply_damage(enemy.stats().max_hp - 1);
    let mut rng = SequenceRng::new(SETUP.into_iter().chain(LANDING).chain([0]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "fixture sanity -- the hit must be lethal: {events:?}"
    );
    assert!(stat_events(&events).is_empty(), "{events:?}");
    rng.assert_exhausted();
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(battle.enemy().stages(), StatStages::default());
}
