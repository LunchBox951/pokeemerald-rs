//! Poison infliction events and the end-of-turn residual: damage, order,
//! lethal ticks, and wild-knockout suppression.

use crate::common::slow_runner_rattata;
use crate::poison_support::*;
use battle::status1::poison_residual_damage;
use battle::{Battle, BattleEvent, BattleOutcome, Dex, PlayerAction, Status1};

/// `SPECIES_ABRA`: faster than [`slow_runner_rattata`]'s Rattata, so a run
/// is never automatic, and too weak to end the battle before its residuals.
const ABRA: u16 = 63;

/// An escape roll that fails for [`slow_runner_rattata`] against [`ABRA`]
/// (`battle::escape`'s own tests pin the threshold).
const ESCAPE_ROLL_FAILS: u16 = 65000;

/// Both battlers use a damaging move, every roll on its default branch.
const BOTH_BATTLERS_ATTACK: [u16; 11] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];

/// The player's move fells the enemy before it acts.
const PLAYER_ACTS_ALONE: [u16; 7] = [0, 0, 0, 0, 1, 0, 0];
/// A refused run, then the enemy's damaging move.
const FAILED_RUN_THEN_ENEMY_ATTACK: [u16; 8] = [0, 0, 0, ESCAPE_ROLL_FAILS, 0, 1, 0, 0];

#[test]
fn a_landed_poison_sting_reports_poisoned_immediately_after_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![POISON_STING]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let hit_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    move_id: POISON_STING,
                    ..
                }
            )
        })
        .expect("Poison Sting must land: {events:?}");
    assert_eq!(
        events[hit_index + 1],
        BattleEvent::Poisoned {
            by_player: true,
            move_id: POISON_STING,
        },
        "Poisoned follows Hit immediately, matching seteffectwithchance \
         preceding tryfaintmon: {events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
}

#[test]
fn a_poison_type_target_is_never_poisoned_even_on_a_successful_roll() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 10, vec![POISON_STING]);
    let enemy = max_iv_mon(&dex, EKANS, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(POISON_STING_LANDS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::Poisoned { .. })),
        "a Poison-type target must never carry Poisoned: {events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

#[test]
fn end_of_turn_poison_damage_matches_the_pinned_formula_and_settles_no_faint() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_max_hp = player.stats().max_hp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let expected_damage = poison_residual_damage(player_max_hp);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: true,
                    ..
                }
            ))
            .count(),
        1,
        "exactly one residual tick for the one poisoned battler: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: true,
            damage: expected_damage,
        }),
        "residual damage must match poison_residual_damage(max_hp): {events:?}"
    );
    // The enemy's own Tackle lands this same turn, so only a lower bound holds.
    assert!(battle.player().current_hp() <= player_max_hp - expected_damage);
    assert!(battle.outcome().is_none());
}

#[test]
fn both_battlers_poisoned_take_residual_damage_in_the_rebuilt_end_turn_order() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let player_tick = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: true,
                    ..
                }
            )
        })
        .expect("the player must take a residual tick: {events:?}");
    let enemy_tick = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: false,
                    ..
                }
            )
        })
        .expect("the enemy must take a residual tick: {events:?}");
    assert!(
        player_tick < enemy_tick,
        "the faster Rattata's residual tick must precede the slower \
         Zigzagoon's, exactly like the end-turn order rebuilt from current Speed: {events:?}"
    );
}

#[test]
fn a_lethal_residual_tick_faints_and_ends_the_battle_like_a_lethal_hit() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);

    // Park the enemy exactly one residual tick above Tackle's damage.
    let tackle_damage = {
        let mut probe = SequenceRng::new([1, 0]);
        match battle::damage_core(&dex, TACKLE, &player, &enemy, false, &mut probe).unwrap() {
            battle::HitOutcome::Hit { damage, .. } => damage,
            other => panic!("Tackle must deal damage against Zigzagoon: {other:?}"),
        }
    };
    let lethal_damage = poison_residual_damage(enemy.stats().max_hp);
    enemy.apply_damage(enemy.stats().max_hp - (tackle_damage + lethal_damage));
    enemy.set_status1(Status1::Poisoned);
    assert_eq!(
        enemy.current_hp(),
        tackle_damage + lethal_damage,
        "fixture sanity"
    );

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Hit {
            by_player: true,
            move_id: TACKLE,
            damage: tackle_damage,
            is_critical: false,
        }),
        "fixture sanity -- the direct hit must not itself faint the enemy: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: false,
            damage: lethal_damage,
        }),
        "{events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the residual tick alone must faint the enemy: {events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.enemy().status1(),
        Status1::Healthy,
        "faint settlement must clear the corpse's primary status"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// Once a residual script's own `checkteamslost`
/// (`data/battle_scripts_1.s:3746`) has decided the battle, `BattleTurnPassed`
/// never re-enters `DoBattlerEndTurnEffects` (`battle_main.c:3960-3966`).
#[test]
fn the_first_battlers_lethal_residual_tick_stops_the_second_battlers_from_running() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 20, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 20, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    enemy.set_status1(Status1::Poisoned);

    // Park the player, processed first, exactly one residual tick above the
    // enemy's Tackle damage.
    let tackle_damage = {
        let mut probe = SequenceRng::new([1, 0]);
        match battle::damage_core(&dex, TACKLE, &enemy, &player, false, &mut probe).unwrap() {
            battle::HitOutcome::Hit { damage, .. } => damage,
            other => panic!("Tackle must deal damage against Rattata: {other:?}"),
        }
    };
    let player_lethal_damage = poison_residual_damage(player.stats().max_hp);
    player.apply_damage(player.stats().max_hp - (tackle_damage + player_lethal_damage));

    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: true,
            damage: player_lethal_damage,
        }),
        "the player, processed first, must take its own residual tick: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "that tick alone must faint the player: {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::HurtByPoison {
                by_player: false,
                ..
            }
        )),
        "the enemy's own residual tick must never run once the player's \
         already ended the battle this same pass: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
}

#[test]
fn a_failed_run_still_ticks_the_poison_residual() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, ABRA, 5, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);
    let enemy_max_hp = enemy.stats().max_hp;

    let mut rng = SequenceRng::new(FAILED_RUN_THEN_ENEMY_ATTACK);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();

    let expected_damage = poison_residual_damage(enemy_max_hp);
    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: false,
            damage: expected_damage,
        }),
        "a failed run does not skip end-of-turn residuals: {events:?}"
    );
}

/// A won battle routes through `HandleEndTurn_BattleWon`, never
/// `HandleEndTurn_ContinueBattle` and its `BattleTurnPassed`
/// (`src/battle_main.c:4937`-`:4952`), so `DoBattlerEndTurnEffects` never
/// runs for the turn the knockout ended.
#[test]
fn a_direct_hit_wild_ko_ends_the_battle_before_any_residual_can_tick() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_hp_before = player.current_hp();
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    let mut rng = SequenceRng::new(PLAYER_ACTS_ALONE);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the fixture must knock the wild mon out with a direct hit: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::HurtByPoison { .. })),
        "a knockout that already won the battle leaves no turn for a \
         residual tick to run in: {events:?}"
    );
    assert_eq!(battle.player().current_hp(), player_hp_before);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, BattleEvent::ExpGained(_))),
        "the knockout still pays out: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// Projects the sides of every `Hit` and `HurtByPoison` event, in order.
fn hit_and_tick_sides(events: &[BattleEvent]) -> (Vec<bool>, Vec<bool>) {
    let hits = events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::Hit { by_player, .. } => Some(*by_player),
            _ => None,
        })
        .collect();
    let ticks = events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::HurtByPoison { by_player, .. } => Some(*by_player),
            _ => None,
        })
        .collect();
    (hits, ticks)
}

/// A surviving Speed tie is decided again when the end-turn order is rebuilt:
/// one more ordering draw, independent of the action-phase tie
/// (`pokeemerald/src/battle_util.c:1199`-`:1210`; `GetWhoStrikesFirst`
/// short-circuits its draw to an exact priority and Speed tie,
/// `pokeemerald/src/battle_main.c:4595`).
#[test]
fn a_surviving_speed_tie_redraws_the_end_turn_order_independently_of_the_action_tie() {
    for (action_tie, end_turn_tie, hits, ticks) in [
        (0, 1, [true, false], [false, true]),
        (1, 0, [false, true], [true, false]),
    ] {
        let dex = Dex::new();
        let mut player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
        player.set_status1(Status1::Poisoned);
        let mut enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
        enemy.set_status1(Status1::Poisoned);

        let mut rng = SequenceRng::new([
            0,
            0, // Battle::new: turn number, initial-seeding tie
            0,
            0, // turn number, enemy pick
            action_tie,
            0,
            1,
            0,
            0, // first Tackle
            0,
            1,
            0,
            0, // second Tackle
            end_turn_tie,
        ]);
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        let (hit_sides, tick_sides) = hit_and_tick_sides(&events);
        assert_eq!(hit_sides, hits, "{events:?}");
        assert_eq!(tick_sides, ticks, "{events:?}");
        assert_eq!(rng.draws(), 14, "2 + 2 + action tie + 4 + 4 + end-turn tie");
    }
}

/// A Speed-stage change made during the action phase reorders the same turn's
/// residuals: the order is rebuilt from current Speed, not inherited.
#[test]
fn a_speed_drop_during_the_action_phase_flips_the_same_turns_poison_order() {
    let dex = Dex::new();
    // Poochyena (Speed 10) beats Wurmple (Speed 8) in the action phase; the
    // String Shot it eats drops it to 6 before the residual pass.
    let mut player = max_iv_mon(&dex, 290, 5, vec![assets::MoveId::STRING_SHOT]);
    player.set_status1(Status1::Poisoned);
    let mut enemy = max_iv_mon(&dex, 286, 5, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);

    let mut rng = SequenceRng::new([
        0, // Battle::new: turn number (8 vs 10, no tie)
        0, 0, // turn number, enemy pick
        0, 1, 0, 0, // Poochyena's Tackle
        0, // Wurmple's String Shot accuracy
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let action_sides: Vec<bool> = events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::Hit { by_player, .. } | BattleEvent::StatFell { by_player, .. } => {
                Some(*by_player)
            }
            _ => None,
        })
        .collect();
    assert_eq!(action_sides, [false, true], "{events:?}");
    let (_, tick_sides) = hit_and_tick_sides(&events);
    assert_eq!(
        tick_sides,
        [true, false],
        "the slowed enemy now ticks after the player: {events:?}"
    );
    assert_eq!(rng.draws(), 8);
}

/// The end-turn comparison reads the move at each battler's selected position
/// as the battlers stand then: a replacement without the lead's +1 priority
/// move ties the enemy again and draws (`pokeemerald/src/battle_main.c:4697`-
/// `:4714`, `:4743`-`:4746`).
#[test]
fn a_faint_replacement_is_compared_by_its_own_moves_at_the_end_turn() {
    let dex = Dex::new();
    let lead = max_iv_mon(&dex, RATTATA, 5, vec![assets::MoveId::QUICK_ATTACK]);
    let reserve = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);

    let mut rng = SequenceRng::new([
        0, // Battle::new_with_player_reserves: turn number (5 vs 50, no tie)
        0, 0, // turn number, enemy pick
        0, 1, 0, 0, // the lead's Quick Attack (+1 priority: no action tie)
        0, 1, 0, 0, // the enemy's Tackle fells the lead
        0, // the end-turn tie between the reserve and the enemy
    ]);
    let mut battle =
        Battle::new_with_player_reserves(dex, lead, vec![reserve], enemy, false, &mut rng).unwrap();
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(rng.draws(), 12, "the replacement's Tackle ties the enemy's");
}

/// Equal Speed with unequal chosen priorities needs no end-turn tie draw.
#[test]
fn an_unequal_priority_selection_consumes_no_end_turn_tie_draw() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 5, vec![assets::MoveId::QUICK_ATTACK]);
    player.set_status1(Status1::Poisoned);
    let mut enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);

    let mut rng = SequenceRng::new([
        0, 0, // Battle::new: turn number, seeding tie
        0, 0, // turn number, enemy pick
        0, 1, 0, 0, // Quick Attack
        0, 1, 0, 0, // Tackle
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let (_, ticks) = hit_and_tick_sides(&events);
    assert_eq!(ticks, [true, false], "{events:?}");
    assert_eq!(rng.draws(), 12);
}
