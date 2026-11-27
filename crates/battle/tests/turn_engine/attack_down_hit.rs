//! `EFFECT_ATTACK_DOWN_HIT` (Aurora Beam) driven through complete turns: the
//! secondary Attack drop lands after the hit damage and its chance draw, never
//! on a fainted target, and is silent when an ability or the stage floor stops
//! it (`SetMoveEffect` -> `ChangeStatBuffs` with `flags == 0`,
//! `pokeemerald/src/battle_script_commands.c:2666-2683,6987-7037`).
//!
//! Aurora Beam's 10% `secondaryEffectChance` takes `Cmd_seteffectwithchance`'s
//! drawing branch (`:2908-2930`): a landed hit spends accuracy, critical,
//! damage and chance draws, and the roll `Random() % 100 < 10` separates 9 from
//! 10.

use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, ChangedStat, Dex, PlayerAction,
    StatStage, StatStages,
};

const AURORA_BEAM: MoveId = MoveId::AURORA_BEAM;
const TACKLE: MoveId = MoveId::TACKLE;
const STRENGTH: MoveId = MoveId::STRENGTH;

/// `SPECIES_RATTATA`, outspeeding every level-5 enemy below.
const RATTATA: u16 = 19;
const ZIGZAGOON: u16 = 288;
/// A physical attacker that Aurora Beam does not resist or overkill.
const MACHOP: u16 = 66;
/// Hyper Cutter in its only ability slot.
const PINSIR: u16 = SpeciesId::PINSIR.0;
/// Keen Eye in its only ability slot.
const HITMONCHAN: u16 = SpeciesId::HITMONCHAN.0;
/// Clear Body in its first ability slot.
const TENTACOOL: u16 = 72;
/// Shield Dust in its only ability slot.
const WURMPLE: u16 = 290;
/// White Smoke in its first ability slot.
const TORKOAL: u16 = SpeciesId::TORKOAL.0;
/// Serene Grace in its first ability slot.
const DUNSPARCE: u16 = 206;

/// The constructor, turn-start and enemy-selection draws.
const SETUP: [u16; 3] = [0, 0, 0];
/// Aurora Beam's accuracy, no-crit and damage draws.
const LANDING: [u16; 3] = [0, 1, 0];
/// The enemy's Tackle: accuracy, no-crit, damage and the discarded chance draw.
const ENEMY_TACKLE: [u16; 4] = [0, 1, 0, 0];
/// The highest chance draw below Aurora Beam's 10% threshold, and the lowest at it.
const LAST_LANDING_ROLL: u16 = 9;
const FIRST_FAILING_ROLL: u16 = 10;

/// A level-10 Rattata's Aurora Beam on a level-5 Zigzagoon at the damage draw of
/// 0 (the full random factor): `((2 * 10 / 5 + 2) * 65 * 13 / 10) / 50 + 2` with
/// Special Attack 13 against Special Defense 10, no STAB and neutral Ice against
/// Normal.
const AURORA_BEAM_DAMAGE: u32 = 12;

fn aurora_beam_script(chance: u16) -> SequenceRng {
    SequenceRng::new(
        SETUP
            .into_iter()
            .chain(LANDING)
            .chain([chance])
            .chain(ENEMY_TACKLE),
    )
}

fn aurora_beam_turn(enemy: BattlePokemon, chance: u16) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![AURORA_BEAM]);
    let mut rng = aurora_beam_script(chance);
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

fn attack_loss(stage: i8) -> StatStages {
    StatStages {
        attack: StatStage::new(stage).unwrap(),
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
fn the_roll_just_below_the_threshold_lowers_attack_and_the_roll_at_it_does_not() {
    let (landed, landed_events) = aurora_beam_turn(enemy_of(ZIGZAGOON), LAST_LANDING_ROLL);
    assert_eq!(
        landed_events[1],
        BattleEvent::Hit {
            by_player: true,
            move_id: AURORA_BEAM,
            damage: AURORA_BEAM_DAMAGE,
            is_critical: false,
        },
        "{landed_events:?}"
    );
    assert_eq!(
        landed_events[2],
        BattleEvent::StatFell {
            by_player: true,
            move_id: AURORA_BEAM,
            stat: ChangedStat::Attack,
            new_stage: StatStage::new(-1).unwrap(),
            magnitude: 1,
        },
        "{landed_events:?}"
    );
    assert_eq!(landed.enemy().stages(), attack_loss(-1));

    let (missed_roll, missed_events) = aurora_beam_turn(enemy_of(ZIGZAGOON), FIRST_FAILING_ROLL);
    assert_eq!(hit_damage(&missed_events, AURORA_BEAM), AURORA_BEAM_DAMAGE);
    assert!(stat_events(&missed_events).is_empty(), "{missed_events:?}");
    assert_eq!(missed_roll.enemy().stages(), StatStages::default());
}

#[test]
fn the_lowered_attack_weakens_the_targets_next_physical_hit() {
    let machop = || max_iv_mon(&Dex::new(), MACHOP, 5, vec![STRENGTH]);
    let (_, landed_events) = aurora_beam_turn(machop(), LAST_LANDING_ROLL);
    let (_, control_events) = aurora_beam_turn(machop(), FIRST_FAILING_ROLL);

    assert_eq!(
        hit_damage(&landed_events, AURORA_BEAM),
        hit_damage(&control_events, AURORA_BEAM),
        "the drop never changes the damage already dealt"
    );
    assert!(
        hit_damage(&landed_events, STRENGTH) < hit_damage(&control_events, STRENGTH),
        "{landed_events:?} versus {control_events:?}"
    );
}

/// Serene Grace doubles the chance, which is unported where the drop can land,
/// so both screens refuse it rather than silently ignoring the doubling: the
/// enemy's moveset at construction and the player's selection before any draw.
#[test]
fn a_serene_grace_aurora_beam_is_refused_before_any_draw() {
    let dex = Dex::new();
    let serene_grace_enemy = max_iv_mon(&dex, DUNSPARCE, 20, vec![AURORA_BEAM]);
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

    let player = max_iv_mon(&dex, DUNSPARCE, 20, vec![AURORA_BEAM]);
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
/// accuracy draw as Aurora Beam's only one.
#[test]
fn a_missed_aurora_beam_spends_exactly_one_draw_and_lowers_nothing() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 10, vec![AURORA_BEAM]);
    player.stages_mut().accuracy = StatStage::MIN;
    let mut rng = SequenceRng::new(SETUP.into_iter().chain([99]).chain(ENEMY_TACKLE));
    let mut battle = Battle::new(dex, player, enemy_of(ZIGZAGOON), false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: true,
            move_id: AURORA_BEAM
        }),
        "fixture sanity -- the accuracy check must fail: {events:?}"
    );
    assert!(stat_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages(), StatStages::default());
}

#[test]
fn a_failed_attack_secondary_ability_keeps_the_draw_and_stays_silent() {
    for species in [TENTACOOL, WURMPLE, TORKOAL, PINSIR] {
        let (battle, events) = aurora_beam_turn(enemy_of(species), 0);

        assert!(
            events.iter().any(|event| matches!(
                event,
                BattleEvent::Hit {
                    move_id: AURORA_BEAM,
                    ..
                }
            )),
            "species {species}: the hit must land for the guard to be reached: {events:?}"
        );
        assert!(
            stat_events(&events).is_empty(),
            "species {species}: a secondary is silent where a stat-lowering move would announce: {events:?}"
        );
        assert_eq!(battle.enemy().stages(), StatStages::default());
    }
}

/// Keen Eye guards Accuracy only, so it leaves the Attack drop to land.
#[test]
fn keen_eye_does_not_stop_the_attack_drop() {
    let (battle, events) = aurora_beam_turn(enemy_of(HITMONCHAN), 0);
    assert_eq!(stat_events(&events).len(), 1, "{events:?}");
    assert_eq!(battle.enemy().stages(), attack_loss(-1));
}

#[test]
fn the_guarding_species_carry_the_abilities_the_matrix_names() {
    let dex = Dex::new();
    for (species, ability) in [
        (TENTACOOL, AbilityId::CLEAR_BODY),
        (WURMPLE, AbilityId::SHIELD_DUST),
        (TORKOAL, AbilityId::WHITE_SMOKE),
        (PINSIR, AbilityId::HYPER_CUTTER),
        (HITMONCHAN, AbilityId::KEEN_EYE),
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
    enemy.stages_mut().attack = StatStage::MIN;
    let (battle, events) = aurora_beam_turn(enemy, 0);

    assert!(stat_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages().attack, StatStage::MIN);
}

/// The chance draw precedes the faint guard, so a lethal landed roll still
/// spends it but applies nothing (`battle_script_commands.c:2261-2264`,
/// `:2908-2930`).
#[test]
fn a_lethal_aurora_beam_spends_four_draws_but_never_lowers_a_fainted_target() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![AURORA_BEAM]);
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
