//! `EFFECT_ACCURACY_DOWN_HIT` (Mud-Slap) driven through complete turns: the
//! secondary Accuracy drop lands after the hit and its chance draw, never on a
//! fainted target, and is silent when an ability or the stage floor stops it
//! (`SetMoveEffect` -> `ChangeStatBuffs` with `flags == 0`,
//! `pokeemerald/src/battle_script_commands.c:2666-2683,6987-7037`).
//!
//! Mud-Slap's 100% `secondaryEffectChance` still takes `Cmd_seteffectwithchance`'s
//! drawing branch: only a prepared `MOVE_EFFECT_CERTAIN` flag skips the draw
//! (`:2908-2930`), so a landed Mud-Slap spends four draws.

use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{
    ensure_wild_startable, initial_moveset, Battle, BattleEvent, BattleOutcome, BattlePokemon,
    ChangedStat, Dex, PlayerAction, StatStage, StatStages,
};

const MUD_SLAP: MoveId = MoveId::MUD_SLAP;
const TACKLE: MoveId = MoveId::TACKLE;
const GROWL: MoveId = MoveId::GROWL;

/// `SPECIES_RATTATA`, outspeeding every level-5 enemy below.
const RATTATA: u16 = 19;
const ZIGZAGOON: u16 = 288;
/// Keen Eye in its only ability slot, and Ground-vulnerable (Spearow's Flying
/// type would stop Mud-Slap before it reached the ability guard).
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
/// Mud-Slap's accuracy, no-crit and damage draws.
const LANDING: [u16; 3] = [0, 1, 0];
/// A draw giving an accuracy roll of 81, which Tackle's 100 accuracy passes but
/// the 75 left by one lowered stage fails.
const ROLL_BETWEEN_STAGES: u16 = 80;
/// The enemy's Tackle: accuracy, no-crit, damage and the discarded chance draw.
const ENEMY_TACKLE: [u16; 4] = [0, 1, 0, 0];

fn mud_slap_script(chance: u16) -> SequenceRng {
    SequenceRng::new(
        SETUP
            .into_iter()
            .chain(LANDING)
            .chain([chance])
            .chain(ENEMY_TACKLE),
    )
}

fn mud_slap_turn(enemy: BattlePokemon, chance: u16) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![MUD_SLAP]);
    let mut rng = mud_slap_script(chance);
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

fn accuracy_loss(stage: i8) -> StatStages {
    StatStages {
        accuracy: StatStage::new(stage).unwrap(),
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

#[test]
fn a_landed_mud_slap_lowers_accuracy_one_stage_and_spends_four_draws() {
    for chance in [0, 99] {
        let (battle, events) = mud_slap_turn(enemy_of(ZIGZAGOON), chance);

        assert!(
            matches!(
                events[1],
                BattleEvent::Hit {
                    by_player: true,
                    move_id: MUD_SLAP,
                    ..
                }
            ),
            "{events:?}"
        );
        assert_eq!(
            events[2],
            BattleEvent::StatFell {
                by_player: true,
                move_id: MUD_SLAP,
                stat: ChangedStat::Accuracy,
                new_stage: StatStage::new(-1).unwrap(),
                magnitude: 1,
            },
            "chance draw {chance}: the 100% chance passes every roll: {events:?}"
        );
        assert_eq!(battle.enemy().stages(), accuracy_loss(-1));
    }
}

/// Serene Grace doubles 100 to 200, which clears the same comparison in the
/// same drawing branch (`battle_script_commands.c:2912-2930`).
#[test]
fn a_serene_grace_mud_slap_is_the_same_four_draw_landing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, DUNSPARCE, 20, vec![MUD_SLAP]);
    assert_eq!(player.ability(), AbilityId::SERENE_GRACE);
    let mut rng = mud_slap_script(99);
    let mut battle = Battle::new(dex, player, enemy_of(ZIGZAGOON), false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert_eq!(stat_events(&events).len(), 1, "{events:?}");
    assert_eq!(battle.enemy().stages(), accuracy_loss(-1));
}

/// A forced accuracy miss ends the script before the chance draw, leaving the
/// accuracy draw as Mud-Slap's only one.
#[test]
fn a_missed_mud_slap_spends_exactly_one_draw_and_lowers_nothing() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 10, vec![MUD_SLAP]);
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
            move_id: MUD_SLAP
        }),
        "fixture sanity -- the accuracy check must fail: {events:?}"
    );
    assert!(stat_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages(), StatStages::default());
}

#[test]
fn a_failed_accuracy_secondary_ability_keeps_the_draw_and_stays_silent() {
    for species in [HITMONCHAN, TENTACOOL, WURMPLE, TORKOAL] {
        let enemy = enemy_of(species);
        let (battle, events) = mud_slap_turn(enemy, 0);

        assert!(
            events.iter().any(|event| matches!(
                event,
                BattleEvent::Hit {
                    move_id: MUD_SLAP,
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

#[test]
fn the_guarding_species_carry_the_abilities_the_matrix_names() {
    let dex = Dex::new();
    for (species, ability) in [
        (HITMONCHAN, AbilityId::KEEN_EYE),
        (TENTACOOL, AbilityId::CLEAR_BODY),
        (WURMPLE, AbilityId::SHIELD_DUST),
        (TORKOAL, AbilityId::WHITE_SMOKE),
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
    enemy.stages_mut().accuracy = StatStage::MIN;
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![MUD_SLAP]);
    let mut rng = SequenceRng::new(SETUP.into_iter().chain(LANDING).chain([0]).chain([99]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // The enemy's Tackle misses at 33% accuracy on a roll of 100.
    rng.assert_exhausted();
    assert!(stat_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages().accuracy, StatStage::MIN);
}

/// The chance draw precedes the faint guard, so a lethal landed roll still
/// spends it but applies nothing (`battle_script_commands.c:2261-2264`,
/// `:2908-2930`).
#[test]
fn a_lethal_mud_slap_spends_four_draws_but_never_lowers_a_fainted_target() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![MUD_SLAP]);
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
}

/// Mudkip learns Mud-Slap at level 6 (`level_up_learnsets.h:3676-3680`). The
/// wild gate admits the real moveset, and the drop lowers the enemy's accuracy
/// before its own Tackle's check in the same turn.
#[test]
fn a_level_6_mudkip_chooses_mud_slap_and_the_drop_changes_the_next_accuracy_check() {
    let dex = Dex::new();
    let moves = initial_moveset(SpeciesId::MUDKIP, 6);
    assert_eq!(moves, vec![TACKLE, GROWL, MUD_SLAP]);
    assert_eq!(ensure_wild_startable(&dex, SpeciesId::MUDKIP, 6), Ok(()));

    let mud_slap_slot = moves
        .iter()
        .position(|move_id| *move_id == MUD_SLAP)
        .unwrap();
    let new_battle = |rng: &mut SequenceRng| {
        let player = max_iv_mon(&dex, SpeciesId::MUDKIP.0, 6, moves.clone());
        let enemy = max_iv_mon(&dex, ZIGZAGOON, 2, vec![TACKLE]);
        Battle::new(Dex::new(), player, enemy, false, rng).unwrap()
    };

    // Mud-Slap's drop makes the same roll of 81 fail Tackle's accuracy.
    let mut rng = SequenceRng::new(
        SETUP
            .into_iter()
            .chain(LANDING)
            .chain([0])
            .chain([ROLL_BETWEEN_STAGES]),
    );
    let mut battle = new_battle(&mut rng);
    let events = battle
        .take_turn(PlayerAction::UseMove(mud_slap_slot), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    assert_eq!(battle.enemy().stages(), accuracy_loss(-1));
    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: false,
            move_id: TACKLE
        }),
        "{events:?}"
    );

    // Control: Growl leaves the stage alone, and the same roll lets Tackle hit.
    let growl_slot = moves.iter().position(|move_id| *move_id == GROWL).unwrap();
    let mut rng =
        SequenceRng::new(
            SETUP
                .into_iter()
                .chain([0])
                .chain([ROLL_BETWEEN_STAGES, 1, 0, 0]),
        );
    let mut battle = new_battle(&mut rng);
    let events = battle
        .take_turn(PlayerAction::UseMove(growl_slot), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    assert_eq!(battle.enemy().stages().accuracy, StatStage::default());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::Missed { .. })),
        "{events:?}"
    );
}
