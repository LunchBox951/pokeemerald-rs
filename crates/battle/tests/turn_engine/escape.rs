//! Escape attempts, run counters, and escape-specific turn behavior.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, Dex, PlayerAction, StatStage, STRUGGLE,
};

#[test]
fn a_successful_run_ends_the_battle_immediately_without_either_mon_acting() {
    let dex = Dex::new();
    // Player far faster than the enemy: try_run_from_battle succeeds
    // unconditionally (player_speed >= enemy_speed), no escape draw.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let player_hp = player.current_hp();
    let enemy_hp = enemy.current_hp();

    // Battle-start turn number, the turn's turn number, and the wild
    // mon's move pick -- which happens even though it never gets to act,
    // because action selection completes for both battlers before the
    // run is resolved.
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
    // Neither mon took any action/damage.
    assert_eq!(battle.player().current_hp(), player_hp);
    assert_eq!(battle.enemy().current_hp(), enemy_hp);
}

#[test]
fn a_failed_run_burns_the_turn_and_the_enemy_still_acts() {
    let dex = Dex::new();
    // Player slower than the enemy: forces the RNG-driven branch, fed a
    // roll that fails (see crate::escape's own tests for the formula).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow Rattata
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast Charmander

    // draws: battle-start turn number, turn number, opponent's move pick,
    // escape roll (65000 & 0xFF = 232 >= speedVar 19 -> failure), then
    // the enemy's hit (accuracy / no crit / best roll / effect chance).
    let mut rng = SequenceRng::new([0, 0, 0, 65000, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events[0],
        BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        }
    );
    // The enemy's move resolved afterward (by_player: false).
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
fn a_failed_run_reports_the_attempt_even_when_the_enemy_cannot_act() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
    let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // battle start, turn number, escape roll (fails) -- no selection
    // draw, the all-spent enemy's forced-Struggle pick bypasses the
    // rejection loop. The fallback then has to act, which stops the turn.
    let mut rng = SequenceRng::new([0, 0, 65000]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();

    assert_eq!(
        failure.error(),
        BattleError::UnsupportedMoveEffect(STRUGGLE)
    );
    assert_eq!(
        failure.events(),
        [BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        }],
        "the run was attempted and burned the turn; that must be reported"
    );
    assert_eq!(battle.run_tries(), 1, "the attempt committed");
    assert_eq!(rng.draws(), 3);
}

#[test]
fn an_all_spent_enemy_still_lets_a_successful_run_end_the_battle() {
    let dex = Dex::new();
    // Upstream fidelity for the same all-spent enemy when the fallback
    // never has to act: the player's run resolves first and succeeds, so
    // the battle ends PlayerRan -- upstream's forced Struggle never
    // executes either, and no error is reported.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: run succeeds
    let mut enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // battle start + turn number only: no selection draw (forced pick),
    // no escape draw (raw speed >= raw speed succeeds unconditionally).
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
    // The same +6 Speed stage must change turn order but NOT escape
    // odds: TryRunFromBattle reads raw gBattleMons speed
    // (battle_util.c:463-:465) while GetWhoStrikesFirst reads the
    // stage-modified effective Speed. Bulbasaur L10 (raw 17, +6 stage ->
    // effective 68) vs Rattata L20 (raw 40, neutral) puts the two on
    // opposite sides of the comparison, so each leg pins its accessor.
    let dex = Dex::new();
    let stage_boosted = |dex: &Dex| {
        let mut mon = max_iv_mon(dex, 1, 10, vec![MoveId(33)]); // Bulbasaur
        mon.stages_mut().speed = StatStage::new(6).unwrap();
        mon
    };

    // Leg 1 -- escape: raw 17 < raw 40, so the run takes the RNG branch
    // and draws (speedVar = 17*128/40 = 54; roll 10 < 54 -> success).
    // Were escape fed the effective 68 >= 40, it would succeed
    // *unconditionally*, consume no escape draw, and leave the script's
    // last value unread.
    let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]); // Rattata L20
    let mut rng = SequenceRng::new([0, 0, 0, 10]);
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

    // Leg 2 -- turn order: effective 68 > 40, so the boosted Bulbasaur
    // moves first despite its raw 17 < 40. Were turn order fed raw
    // speeds, the enemy's hit would come first.
    //
    // Damage pins, hand computed: Bulbasaur L10 Tackle (atk 17) into
    // Rattata L20 (def 25): 17*35 = 595, *(2*10/5+2 = 6) = 3570, /25 =
    // 142, /50 = 2, +2 = 4 (no STAB, neutral). Rattata L20 Tackle (atk
    // 33) into Bulbasaur L10 (def 17): 33*35 = 1155, *(2*20/5+2 = 10) =
    // 11550, /17 = 679, /50 = 13, +2 = 15, STAB -> 22. Both survive
    // (48-hp Rattata, 32-hp Bulbasaur).
    let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex.clone(), stage_boosted(&dex), enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 4,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 22,
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
    // The +30-per-previous-attempt term (TryRunFromBattle's
    // `gBattleStruct->runTries * 30`): one roll value fails on turn 1
    // and succeeds on turn 2 *only* because the counter fed the formula.
    // Rattata L5 (speed 13) vs Charmander L10 (speed 21): speedVar =
    // 13*128/21 = 79 on the first try, 109 on the second. Roll 90 sits
    // between them: 90 >= 79 fails, 90 < 109 escapes. An engine that
    // tracked run_tries but fed the formula 0 would fail turn 2 as well
    // and panic this script by drawing for the enemy's move.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]);

    let mut rng = SequenceRng::new([
        0, // battle start
        0, 0, 90, // turn 1: turn number, pick, escape roll -> fail
        0, 1, 0, 0, // ...so the enemy acts: its 4-draw hit (9 damage)
        0, 0, 90, // turn 2: same roll now beats 79+30 -> escape
    ]);
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
    // The same mirror match as the test above (Rattata L5 both sides,
    // effective Speed 13), but the player runs. A chosen Run makes
    // SetActionsAndBattlersTurnOrder short-circuit to `turnOrderId = 5`
    // (`battle_main.c:4784`-`:4813`), seating the runner first without
    // ever reaching GetWhoStrikesFirst -- so even with tied speeds the
    // turn must not consume the mid-turn tie draw. The escape roll is
    // skipped too (player_speed >= enemy_speed succeeds
    // unconditionally), leaving exactly the turn number and the wild
    // mon's selection pick.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([
        0, 0, // Battle::new: battle-start turn number + initial-seeding tie
        0, // the turn's own turn number
        0, // selection: 0 % 4 -> slot 0, the wild mon's only move
    ]);
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
    // gBattleStruct->runTries is a byte: the 256th failed attempt wraps
    // it to 0, resetting the +30 escape bonus. 256 failed runs are
    // reachable: the enemy's Tackle slot is drained up front, so every
    // turn its pick (draw 0 -> slot 0) fails via FailedNoPp -- no
    // damage, no PP change -- while Scratch in slot 1 keeps the moveset
    // only *partially* spent (an all-spent moveset would divert to the
    // forced-Struggle fallback instead).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: runs can fail
    let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // fast
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // Escape roll 255 always fails: speedVar is a byte, so
    // `speed_var > 255` is false for every possible speedVar.
    // Per turn: turn number, pick, escape roll -- 3 draws, no move draws.
    let script = std::iter::once(0u16)
        .chain((0..256).flat_map(|_| [0u16, 0, 255]))
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
