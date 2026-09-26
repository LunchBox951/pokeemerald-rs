//! Escape attempts, run counters, and escape-specific turn behavior.
//!
//! Scripted RNGs hold, in order, the battle-start draw, then per turn the
//! turn-number refresh, the opponent's move pick, the escape roll when the
//! run is not decided outright, and each resolved hit's own draws as
//! `battle::hit` pins them.

use crate::common::{
    max_iv_mon, max_iv_mon_with_personality, slow_runner_rattata, SequenceRng,
    SECONDARY_ABILITY_PERSONALITY,
};
use assets::{AbilityId, MoveId, Type};
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, Dex, PlayerAction, StatStage, STRUGGLE,
};

#[test]
fn a_successful_run_ends_the_battle_immediately_without_either_mon_acting() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let player_hp = player.current_hp();
    let enemy_hp = enemy.current_hp();

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
    assert_eq!(rng.draws(), 3, "no escape draw, but selection still drew");
    assert_eq!(
        battle.run_tries(),
        1,
        "upstream increments runTries outside the roll branch, so even \
         the no-roll fast-path success counts the attempt"
    );
    assert_eq!(battle.player().current_hp(), player_hp);
    assert_eq!(battle.enemy().current_hp(), enemy_hp);
}

#[test]
fn a_failed_run_burns_the_turn_and_the_enemy_still_acts() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);

    let escape_roll_that_fails: u16 = 65_000;
    let mut rng = SequenceRng::new([0, 0, 0, escape_roll_that_fails, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events[0],
        BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        }
    );
    assert!(events.iter().any(|e| matches!(
        e,
        BattleEvent::Hit {
            by_player: false,
            ..
        }
    )));
    assert_eq!(battle.run_tries(), 1);
    assert_eq!(rng.draws(), 8);
}

#[test]
fn a_failed_run_lets_the_enemys_forced_struggle_execute_afterward() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let player_max_hp = player.stats().max_hp;
    let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // The all-spent enemy's forced Struggle pick bypasses the move-selection
    // draw, so the script has no selection entry between the escape roll and
    // Struggle's own three draws.
    let escape_roll_that_fails: u16 = 65_000;
    let mut rng = SequenceRng::new([0, 0, escape_roll_that_fails, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: STRUGGLE,
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Recoil {
                by_player: false,
                move_id: STRUGGLE,
                damage: player_max_hp / 4,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "the failed run and the forced Struggle that follows it must both commit"
    );
    assert_eq!(battle.run_tries(), 1, "the attempt committed");
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "the forced pick spends no PP"
    );
    assert_eq!(rng.draws(), 6);
}

#[test]
fn an_all_spent_enemy_still_lets_a_successful_run_end_the_battle() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let mut enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // The all-spent enemy's forced pick skips the selection draw, and the
    // player's raw speed >= the enemy's raw speed skips the escape roll,
    // leaving only Battle::new's draw and this turn's turn-number draw.
    let mut rng = SequenceRng::new([0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
    assert_eq!(rng.draws(), 2);
}

#[test]
fn escape_uses_raw_speed_while_turn_order_uses_effective_speed() {
    // `TryRunFromBattle` reads raw speed (`battle_util.c:463`-`:466`); turn
    // order reads stage-modified effective speed instead.
    let dex = Dex::new();
    let stage_boosted = |dex: &Dex| {
        let mut mon = max_iv_mon(dex, 1, 10, vec![MoveId(33)]);
        mon.stages_mut().speed = StatStage::new(6).unwrap();
        mon
    };

    // Raw 17 < raw 40 forces the RNG branch, where a roll below the computed
    // threshold escapes. Effective 68 >= 40 would instead escape
    // unconditionally and leave this roll undrawn.
    let escape_roll_below_threshold: u16 = 10;
    let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 0, escape_roll_below_threshold]);
    let mut battle = Battle::new(dex.clone(), stage_boosted(&dex), enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
    );
    assert_eq!(
        rng.draws(),
        4,
        "1 (battle start) + 2 (turn number, pick) + 1 (the escape roll \
         a raw-speed comparison must make)"
    );

    // Effective 68 > 40 seats the boosted Bulbasaur first despite its raw
    // 17 < 40; reading raw speed for turn order would seat the enemy first
    // instead.
    let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex.clone(), stage_boosted(&dex), enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let bulbasaur_tackle_damage = 4;
    let rattata_tackle_damage = 22;
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: bulbasaur_tackle_damage,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: rattata_tackle_damage,
                is_critical: false,
            },
        ],
        "the +6-stage mon moves first only if turn order reads \
         effective speed"
    );
    assert_eq!(rng.draws(), 11);
}

#[test]
fn each_failed_run_raises_the_next_attempts_odds_through_run_tries() {
    const BATTLE_START: [u16; 1] = [0];
    /// Turn number, then the enemy's move pick.
    const TURN_PREAMBLE: [u16; 2] = [0, 0];
    /// Accuracy, no crit, best damage roll, effect chance.
    const ENEMY_HIT: [u16; 4] = [0, 1, 0, 0];

    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]);

    // The same roll fails turn 1's escape threshold and clears turn 2's
    // higher one, so turn 2 can only succeed if run_tries fed the formula.
    let roll_between_unboosted_and_boosted_thresholds: u16 = 90;
    let mut rng = SequenceRng::new(
        BATTLE_START
            .into_iter()
            .chain(TURN_PREAMBLE)
            .chain([roll_between_unboosted_and_boosted_thresholds])
            .chain(ENEMY_HIT)
            .chain(TURN_PREAMBLE)
            .chain([roll_between_unboosted_and_boosted_thresholds]),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let turn1 = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        turn1[0],
        BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        }
    );
    assert!(battle.outcome().is_none());

    let turn2 = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        turn2,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ],
        "the identical roll escapes only via the run_tries bonus"
    );
    assert_eq!(battle.run_tries(), 2);
    assert_eq!(rng.draws(), 11);
}

#[test]
fn an_equal_speed_run_turn_never_consumes_the_tie_draw() {
    let dex = Dex::new();
    // A chosen Run makes `SetActionsAndBattlersTurnOrder` short-circuit to
    // `turnOrderId = 5` (`battle_main.c:4784`-`:4813`), seating the runner
    // first without ever reaching `GetWhoStrikesFirst`'s speed-tie draw --
    // so an equal-speed Run must not consume that draw either.
    let player = slow_runner_rattata(&dex);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(
        rng.draws(),
        2,
        "mirror match: Battle::new takes the seeding tie draw"
    );
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(
        rng.draws(),
        4,
        "2 (battle start) + 2 (turn number, pick): no tie draw, no escape roll"
    );
}

#[test]
fn run_tries_wraps_at_256_like_upstreams_byte_counter() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    // Draining only the first move keeps the enemy's moveset partially
    // spent, so its pick always fails via FailedNoPp instead of diverting
    // to the forced-Struggle fallback exercised above.
    let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // A byte-valued escape threshold can never exceed 255, so this roll
    // fails every one of the 256 attempts regardless of the run_tries bonus.
    let escape_roll_that_always_fails: u16 = 255;
    let script = std::iter::once(0u16)
        .chain((0..256).flat_map(|_| [0u16, 0, escape_roll_that_always_fails]))
        .collect::<Vec<_>>();
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    for turn in 0u16..256 {
        assert_eq!(
            battle.run_tries(),
            u8::try_from(turn % 256).unwrap(),
            "before failed attempt {turn}"
        );
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events[0],
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            }
        );
    }
    assert_eq!(
        battle.run_tries(),
        0,
        "the 256th failed attempt wraps the byte counter to 0"
    );
    assert!(battle.outcome().is_none());
    assert_eq!(rng.draws(), 1 + 256 * 3);
}

#[test]
fn the_slow_runner_fixture_does_not_carry_run_away() {
    let dex = Dex::new();
    let runner = slow_runner_rattata(&dex);
    assert_ne!(
        runner.ability(),
        AbilityId::RUN_AWAY,
        "the runner escapes unconditionally upstream, so its failed runs are unreachable"
    );
}

#[test]
fn run_away_escapes_without_an_escape_roll_or_a_run_try() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    assert_eq!(player.ability(), AbilityId::RUN_AWAY);
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
    assert_eq!(rng.draws(), 3, "the Run Away branch draws nothing");
    assert_eq!(
        battle.run_tries(),
        0,
        "`runTries++` lives in the ordinary branch only (`battle_util.c:475`)"
    );
}

#[test]
fn shadow_tag_refuses_a_nominally_successful_run() {
    let dex = Dex::new();
    // Faster than the enemy, so only Shadow Tag explains the refusal.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let wobbuffet_species_id = 202;
    let enemy = max_iv_mon(&dex, wobbuffet_species_id, 5, vec![MoveId(33)]);
    assert_eq!(enemy.ability(), AbilityId::SHADOW_TAG);

    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(
        rng.draws(),
        1,
        "construction still takes its battle-start draw"
    );
    let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();
    assert_eq!(failure.error(), BattleError::RunForbidden);
    assert_eq!(
        failure.events(),
        [],
        "`IsRunningFromBattleImpossible` refuses the selection before any \
         event or draw (`battle_main.c:4043`-`:4052`)"
    );
    assert_eq!(rng.draws(), 1, "the refusal itself draws nothing");
    assert_eq!(battle.run_tries(), 0);
    assert!(battle.outcome().is_none());
}

#[test]
fn arena_trap_refuses_a_grounded_nominally_successful_run() {
    let dex = Dex::new();
    // Faster than the enemy and grounded, so only Arena Trap explains the
    // refusal.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let trapinch_species_id = 332;
    let enemy = max_iv_mon_with_personality(
        &dex,
        trapinch_species_id,
        5,
        vec![MoveId(33)],
        SECONDARY_ABILITY_PERSONALITY,
    );
    assert_eq!(enemy.ability(), AbilityId::ARENA_TRAP);

    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();
    assert_eq!(failure.error(), BattleError::RunForbidden);
    assert_eq!(
        failure.events(),
        [],
        "`IsRunningFromBattleImpossible` refuses the selection before any \
         event or draw (`battle_main.c:4053`-`:4062`)"
    );
    assert_eq!(battle.run_tries(), 0);
    assert!(battle.outcome().is_none());
}

#[test]
fn arena_trap_exempts_a_levitate_runner() {
    let dex = Dex::new();
    let haunter_species_id = 93;
    let player = max_iv_mon(&dex, haunter_species_id, 5, vec![MoveId(33)]);
    assert_eq!(player.ability(), AbilityId::LEVITATE);
    let trapinch_species_id = 332;
    let enemy = max_iv_mon_with_personality(
        &dex,
        trapinch_species_id,
        5,
        vec![MoveId(33)],
        SECONDARY_ABILITY_PERSONALITY,
    );
    assert_eq!(enemy.ability(), AbilityId::ARENA_TRAP);

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ],
        "Arena Trap does not apply to a Levitate holder (`battle_main.c:4054`)"
    );
}

#[test]
fn arena_trap_exempts_a_flying_runner() {
    let dex = Dex::new();
    let pidgey_species_id = 16;
    let player = max_iv_mon(&dex, pidgey_species_id, 5, vec![MoveId(33)]);
    assert!(player.types().contains(&Type::Flying));
    let trapinch_species_id = 332;
    let enemy = max_iv_mon_with_personality(
        &dex,
        trapinch_species_id,
        5,
        vec![MoveId(33)],
        SECONDARY_ABILITY_PERSONALITY,
    );
    assert_eq!(enemy.ability(), AbilityId::ARENA_TRAP);

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ],
        "Arena Trap does not apply to a Flying-type runner (`battle_main.c:4055`)"
    );
}

#[test]
fn magnet_pull_refuses_a_steel_type_nominally_successful_run() {
    let dex = Dex::new();
    // Faster than the enemy, so only Magnet Pull explains the refusal.
    let aron_species_id = 382;
    let player = max_iv_mon(&dex, aron_species_id, 50, vec![MoveId(33)]);
    assert!(player.types().contains(&Type::Steel));
    let magnemite_species_id = 81;
    let enemy = max_iv_mon(&dex, magnemite_species_id, 5, vec![MoveId(33)]);
    assert_eq!(enemy.ability(), AbilityId::MAGNET_PULL);

    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();
    assert_eq!(failure.error(), BattleError::RunForbidden);
    assert_eq!(
        failure.events(),
        [],
        "`IsRunningFromBattleImpossible` refuses the selection before any \
         event or draw (`battle_main.c:4064`-`:4070`)"
    );
    assert_eq!(battle.run_tries(), 0);
    assert!(battle.outcome().is_none());
}

#[test]
fn magnet_pull_does_not_refuse_a_non_steel_runner() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    assert!(!player.types().contains(&Type::Steel));
    let magnemite_species_id = 81;
    let enemy = max_iv_mon(&dex, magnemite_species_id, 5, vec![MoveId(33)]);
    assert_eq!(enemy.ability(), AbilityId::MAGNET_PULL);

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ],
        "Magnet Pull does not apply to a non-Steel-type runner (`battle_main.c:4064`)"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
    assert_eq!(battle.run_tries(), 1);
}
