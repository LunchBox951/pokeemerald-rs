//! `EFFECT_SPEED_DOWN_HIT` (Constrict) driven through complete turns: the
//! secondary Speed drop lands after the hit and its chance draw, never on a
//! fainted target, and is silent when an ability or the stage floor stops it
//! (`SetMoveEffect` -> `ChangeStatBuffs` with `flags == 0`,
//! `pokeemerald/src/battle_script_commands.c:2666-2683,6987-7037`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::{MoveId, SpeciesId};
use battle::{
    Battle, BattleEvent, BattleOutcome, ChangedStat, Dex, PlayerAction, StatStage, StatStages,
};

const CONSTRICT: MoveId = MoveId::CONSTRICT;
const TACKLE: MoveId = MoveId::TACKLE;

/// `SPECIES_RATTATA`, outspeeding every level-5 enemy below.
const RATTATA: SpeciesId = SpeciesId::RATTATA;
const ZIGZAGOON: SpeciesId = SpeciesId::ZIGZAGOON;
/// Clear Body in its first ability slot.
const TENTACOOL: SpeciesId = SpeciesId::TENTACOOL;
/// Shield Dust in its only ability slot.
const WURMPLE: SpeciesId = SpeciesId::WURMPLE;

/// The setup draws, then Constrict's accuracy, no-crit and damage draws.
const BEFORE_CHANCE: [u16; 6] = [0, 0, 0, 0, 1, 0];
/// A chance draw under Constrict's 10%, then the enemy's Tackle.
const CHANCE_HITS: u16 = 9;
const CHANCE_MISSES: u16 = 10;
const ENEMY_TACKLE: [u16; 4] = [0, 1, 0, 0];

fn script_with(chance: u16) -> SequenceRng {
    SequenceRng::new(
        BEFORE_CHANCE
            .into_iter()
            .chain([chance])
            .chain(ENEMY_TACKLE),
    )
}

fn constrict_turn(enemy_species: SpeciesId, chance: u16) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![CONSTRICT]);
    let enemy = max_iv_mon(&dex, enemy_species, 5, vec![TACKLE]);
    let mut rng = script_with(chance);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

fn stat_fell_events(events: &[BattleEvent]) -> Vec<&BattleEvent> {
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
fn a_landed_constrict_lowers_speed_one_stage_right_after_its_hit() {
    let (battle, events) = constrict_turn(ZIGZAGOON, CHANCE_HITS);

    assert!(
        matches!(
            events[0],
            BattleEvent::Hit {
                by_player: true,
                move_id: CONSTRICT,
                ..
            }
        ),
        "{events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::StatFell {
            by_player: true,
            move_id: CONSTRICT,
            stat: ChangedStat::Speed,
            new_stage: StatStage::new(-1).unwrap(),
            magnitude: 1,
        },
        "{events:?}"
    );
    assert_eq!(
        battle.enemy().stages(),
        StatStages {
            speed: StatStage::new(-1).unwrap(),
            ..StatStages::default()
        }
    );
}

#[test]
fn a_failed_chance_roll_spends_its_draw_and_changes_nothing() {
    let (battle, events) = constrict_turn(ZIGZAGOON, CHANCE_MISSES);

    assert!(stat_fell_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages(), StatStages::default());
}

#[test]
fn clear_body_and_shield_dust_stop_the_drop_without_a_message() {
    for species in [TENTACOOL, WURMPLE] {
        let (battle, events) = constrict_turn(species, CHANCE_HITS);

        assert!(
            stat_fell_events(&events).is_empty(),
            "species {species:?}: a secondary is silent where a stat-lowering move would announce: {events:?}"
        );
        assert_eq!(battle.enemy().stages(), StatStages::default());
    }
}

#[test]
fn a_target_already_at_minus_six_is_left_alone_without_a_message() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![CONSTRICT]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    enemy.stages_mut().speed = StatStage::MIN;
    let mut rng = script_with(CHANCE_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(stat_fell_events(&events).is_empty(), "{events:?}");
    assert_eq!(battle.enemy().stages().speed, StatStage::MIN);
}

/// The chance draw precedes the faint guard, so a lethal landed roll still
/// spends it but applies nothing (`battle_script_commands.c:2261-2264`,
/// `:2908-2930`).
#[test]
fn a_lethal_constrict_spends_the_chance_draw_but_never_lowers_a_fainted_target() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![CONSTRICT]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    enemy.apply_damage(enemy.stats().max_hp - 1);
    let mut rng = SequenceRng::new(BEFORE_CHANCE.into_iter().chain([CHANCE_HITS]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "fixture sanity -- the hit must be lethal: {events:?}"
    );
    assert!(stat_fell_events(&events).is_empty(), "{events:?}");
    rng.assert_exhausted();
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}
