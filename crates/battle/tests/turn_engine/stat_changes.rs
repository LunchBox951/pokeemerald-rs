//! Stat-stage move effects and their downstream battle behavior.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{
    Battle, BattleEvent, BattleOutcome, ChangedStat, Dex, PlayerAction, StatStage, StatStages,
    Volatiles,
};

#[test]
fn wild_zigzagoon_growl_executes_when_the_rejection_loop_lands_on_it() {
    let dex = Dex::new();
    // The enemy knows two moves so the rejection loop can land on either
    // slot; this run pins it landing on Growl. Rattata is faster than
    // Zigzagoon, so Rattata's Tackle resolves first every turn.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 288, 3, vec![MoveId(33), MoveId(45)]); // Zigzagoon: Tackle, Growl
    let enemy_hp_before = enemy.current_hp();

    // battle start (no tie, speeds differ), turn number, the rejection
    // loop landing on slot 1 (Growl: draw 1 % 4 == 1), the player's
    // 4-draw Tackle, then Growl's 1-draw accuracy check (100 accuracy:
    // cannot miss).
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1);
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
            BattleEvent::StatFell {
                by_player: false,
                move_id: MoveId(45),
                stat: ChangedStat::Attack,
                new_stage: StatStage::new(-1).unwrap(),
                magnitude: 1,
            },
        ]
    );
    assert_eq!(battle.player().stages().attack, StatStage::new(-1).unwrap());
    assert_eq!(battle.enemy().current_hp(), enemy_hp_before - 9);
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (rejection loop) + 4 (Tackle) + 1 (Growl)"
    );
}

#[test]
fn wild_wurmple_string_shot_misses_when_the_rejection_loop_lands_on_it() {
    let dex = Dex::new();
    // The enemy knows two moves so the rejection loop can land on either
    // slot; this run pins it landing on String Shot. Rattata is faster
    // than Wurmple, so Rattata's Tackle resolves first.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 290, 3, vec![MoveId(33), MoveId(81)]); // Wurmple: Tackle, String Shot
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, the rejection loop landing on slot 1
    // (String Shot: draw 1 % 4 == 1), the player's 4-draw Tackle, then
    // String Shot's accuracy roll: draw 95 -> roll 96 > 95 (95
    // accuracy) -> miss.
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 95]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
            BattleEvent::Missed {
                by_player: false,
                move_id: MoveId(81),
            },
        ]
    );
    assert_eq!(
        battle.player().stages().speed,
        StatStage::NEUTRAL,
        "a miss must not change any stage"
    );
    assert_eq!(battle.enemy().current_hp(), enemy_hp_before - 9);
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (rejection loop) + 4 (Tackle) + 1 (String Shot)"
    );
}

#[test]
fn a_stat_already_at_the_floor_reports_wont_go_lower_and_stays_put() {
    let dex = Dex::new();
    // Rattata is faster, so its Growl reaches a Bulbasaur whose Attack
    // stage is already at the floor. Upstream's ChangeStatBuffs checks the
    // floor after the accuracy check passes, so the move still connects
    // even though the stage cannot move (battle_script_commands.c:7056).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(45)]); // Rattata/Growl
    let mut enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle
    enemy.stages_mut().attack = StatStage::MIN;

    // battle start, turn number, enemy pick (its only move), Growl's
    // 1-draw accuracy check (100 accuracy: cannot miss), then the
    // enemy's ordinary 4-draw Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Bulbasaur's own Tackle reads its already-floored Attack stage, since
    // it is the same mon and stat Growl just found at the floor.
    assert_eq!(
        events,
        vec![
            BattleEvent::StatWontGoLower {
                by_player: true,
                move_id: MoveId(45),
                stat: ChangedStat::Attack,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ]
    );
    assert_eq!(
        battle.enemy().stages().attack,
        StatStage::MIN,
        "already at the floor: the stage must not move"
    );
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (pick) + 1 (Growl) + 4 (Tackle)"
    );
}

#[test]
fn growl_lowers_the_players_subsequent_tackle_damage() {
    let dex = Dex::new();
    // The faster enemy's Growl resolves before the player's own Tackle in
    // the same turn, so the Tackle's damage must read the already-lowered
    // Attack stage rather than the player's neutral baseline.
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(45)]); // Rattata/Growl
    let player = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    // battle start, turn number, enemy pick (its only move), Growl's
    // 1-draw accuracy check (100 accuracy: cannot miss), then the
    // player's ordinary 4-draw Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::StatFell {
                by_player: false,
                move_id: MoveId(45),
                stat: ChangedStat::Attack,
                new_stage: StatStage::new(-1).unwrap(),
                magnitude: 1,
            },
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ],
        "the player's Tackle must reflect the -1 Attack stage Growl \
         just applied, not the neutral baseline of 5"
    );
    assert_eq!(battle.player().stages().attack, StatStage::new(-1).unwrap());
}

#[test]
fn string_shot_flips_turn_order_once_the_targets_effective_speed_drops_below_the_threshold() {
    let dex = Dex::new();
    // Poochyena is faster than Wurmple, so Poochyena moves first in turn
    // 1. Wurmple's String Shot that same turn lowers Poochyena's Speed
    // stage to -1. Upstream's turn-order speed check multiplies before it
    // divides (`GetWhoStrikesFirst`, `battle_main.c:4624`-`:4626`), so
    // Poochyena's effective Speed truncates down to a value below
    // Wurmple's untouched Speed from turn 2 on, flipping who moves first
    // despite Wurmple being the raw-slower mon.
    let player = max_iv_mon(&dex, 290, 5, vec![MoveId(81), MoveId(33)]); // Wurmple: String Shot, Tackle
    let enemy = max_iv_mon(&dex, 286, 5, vec![MoveId(33)]); // Poochyena: Tackle
    let player_hp_before = player.current_hp();
    let enemy_hp_before = enemy.current_hp();

    // battle start (no tie, 8 vs 10), then:
    // turn 1: turn number, enemy pick (its only move), Poochyena's
    //   4-draw Tackle (it is still faster this turn), Wurmple's 1-draw
    //   String Shot accuracy check (draw 0 -> roll 1 <= 95 -> hit) --
    //   7 draws in all.
    // turn 2: turn number, enemy pick, Wurmple's 4-draw Tackle (now
    //   first -- the flip), Poochyena's 4-draw Tackle -- 10 draws.
    let mut rng = SequenceRng::new([
        0, // battle start
        0, 0, // turn 1: turn number, enemy pick
        0, 1, 0, 0, // turn 1: Poochyena's Tackle
        0, // turn 1: Wurmple's String Shot (hits)
        0, 0, // turn 2: turn number, enemy pick
        0, 1, 0, 0, // turn 2: Wurmple's Tackle (now first)
        0, 1, 0, 0, // turn 2: Poochyena's Tackle
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "no speed-tie draw: 8 and 10 differ");

    // Turn 1: Poochyena moves first (its raw Speed is still higher).
    let turn1 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        turn1,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
            BattleEvent::StatFell {
                by_player: true,
                move_id: MoveId(81),
                stat: ChangedStat::Speed,
                new_stage: StatStage::new(-1).unwrap(),
                magnitude: 1,
            },
        ],
        "turn 1: Poochyena (faster) moves first"
    );
    assert_eq!(battle.enemy().stages().speed, StatStage::new(-1).unwrap());
    assert_eq!(rng.draws(), 8);

    // Turn 2: Wurmple's untouched Speed now beats Poochyena's debuffed
    // effective Speed -- the order has flipped from turn 1, purely from
    // the stage change String Shot committed.
    let turn2 = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert_eq!(
        turn2,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
        ],
        "turn 2: the player now moves FIRST despite being the \
         raw-slower mon -- only explicable by the Speed debuff \
         turn 1 committed"
    );
    assert_eq!(battle.player().current_hp(), player_hp_before - 5 - 5);
    assert_eq!(battle.enemy().current_hp(), enemy_hp_before - 5);
    assert_eq!(rng.draws(), 18);
}

/// Upstream `FaintClearSetData` clears a fainted battler's accumulated
/// stages and volatiles (`src/battle_main.c:3264`-`:3273`).
#[test]
fn a_fainting_battler_drops_its_accumulated_stages() {
    let dex = Dex::new();
    // The enemy Hardens on itself first, then the player finishes it off
    // in the same turn: Harden does not damage or delay the enemy's own
    // faint.
    let mut enemy = max_iv_mon(&dex, 19, 50, vec![MoveId(106)]); // Rattata/Harden
    enemy.apply_damage(enemy.current_hp() - 1);
    enemy.volatiles_mut().set_focus_energy();
    enemy.volatiles_mut().set_charge();
    let player = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    // battle start, turn number, the enemy's 1-draw rejection-loop pick
    // (only slot 0 is known, so draw 0 -> 0 % 4 == 0 lands immediately),
    // Harden (0 draws -- BattleScript_EffectStatUp has no accuracycheck),
    // then Tackle's ordinary 4-draw hit (accuracy, no crit, worst damage
    // roll, discarded effect chance) -- any positive damage faints a 1-HP
    // target, so the exact roll does not matter here.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let rose = events.iter().any(|e| {
        matches!(
            e,
            BattleEvent::StatRose {
                by_player: false,
                stat: ChangedStat::Defense,
                new_stage,
                ..
            } if *new_stage == StatStage::new(1).unwrap()
        )
    });
    assert!(
        rose,
        "Harden must have raised the enemy's Defense to +1 before it fainted: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, BattleEvent::Fainted { by_player: false })),
        "the enemy must faint to Tackle: {events:?}"
    );
    assert_eq!(battle.enemy().stages(), StatStages::default());
    assert_eq!(battle.enemy().volatiles(), Volatiles::default());
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}
