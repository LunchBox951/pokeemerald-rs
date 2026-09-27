//! `BATTLE_TYPE_FIRST_BATTLE`: the Route 101 intro Zigzagoon fight's three
//! deltas from an ordinary wild encounter -- crit suppression, Run rejection
//! for non-Run-Away holders, and the wild opponent's AI-branch move choice --
//! pinned end to end through the public `battle` API. Unit-level draw-count
//! pins for each formula in isolation live alongside the formula itself
//! (`crate::critical`, `crate::hit`, `crate::battle`'s own doc-comment
//! derivations).
//!
//! Zigzagoon (species 288) is the actual scripted first-battle opponent
//! (`pokeemerald/src/battle_controllers.c:67`-`:72` creates it at level 2 --
//! species and level only). Tackle + Growl is the moveset that construction
//! implies: `CreateMon` (`pokeemerald/src/pokemon.c:2195`) delegates to
//! `CreateBoxMon` (`:2206`), which ends in `GiveBoxMonInitialMoveset`
//! (`:2302`, defined at `:2991`-`:3012`) -- and that walks
//! `sZigzagoonLevelUpLearnset` (`src/data/pokemon/level_up_learnsets.h:3765`)
//! up to the mon's level -- at level 2 exactly the two level-1 entries,
//! Tackle and Growl. Reused here rather than inventing a stand-in species.

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::{Battle, BattleError, BattleEvent, BattleOutcome, ChangedStat, Dex, PlayerAction};

const TACKLE: MoveId = MoveId(33);
const GROWL: MoveId = MoveId(45);
const SLASH: MoveId = MoveId(163);

#[test]
fn run_is_forbidden_before_any_draw_and_leaves_the_battle_usable() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 1, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, 288, 2, vec![TACKLE, GROWL]);

    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1);

    let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();
    assert_eq!(failure.error(), BattleError::RunForbidden);
    assert!(
        failure.events().is_empty(),
        "a pre-draw rejection reports no events"
    );
    assert_eq!(
        rng.draws(),
        1,
        "RunForbidden is checked before the turn-number draw -- the stream \
         must not move at all"
    );
    assert!(battle.outcome().is_none(), "the battle is still usable");

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "the battle must still be able to play a normal turn afterward"
    );
}

#[test]
fn first_battle_suppresses_the_crit_draw_and_never_crits() {
    let dex = Dex::new();
    // Obviously faster, and Slash one-shots the enemy, so only the
    // selection draws before it -- never the enemy's own move -- are spent.
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    let enemy = max_iv_mon(&dex, 288, 2, vec![TACKLE, GROWL]);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let is_critical = events
        .iter()
        .find_map(|e| match e {
            BattleEvent::Hit {
                by_player: true,
                is_critical,
                ..
            } => Some(*is_critical),
            _ => None,
        })
        .expect("the player's Slash must land");
    assert!(!is_critical, "first_battle must suppress the crit roll");
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerWon)),
        "a L50 Slash must one-shot a L2 Zigzagoon"
    );
    assert_eq!(
        rng.draws(),
        10,
        "no crit draw: the suppressed-crit hit spends only 3 draws, one \
         fewer than an ordinary hit's 4 -- a reintroduced crit draw would \
         exhaust this exact-sized script and panic"
    );
}

#[test]
fn first_battle_ai_tie_break_can_land_on_the_second_move() {
    let dex = Dex::new();
    // Enemy faster, so its chosen move is directly observable. Both sides
    // use only Growl, so neither action can faint the other, and the
    // player (second mover) is guaranteed to act too.
    let player = max_iv_mon(&dex, 19, 5, vec![GROWL]);
    let enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]);

    let tie_break_selects_second_slot = 1;
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, tie_break_selects_second_slot, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let enemy_move = events.iter().find_map(|e| match e {
        BattleEvent::StatFell {
            by_player: false,
            move_id,
            ..
        }
        | BattleEvent::StatWontGoLower {
            by_player: false,
            move_id,
            ..
        }
        | BattleEvent::Missed {
            by_player: false,
            move_id,
        } => Some(*move_id),
        _ => None,
    });
    assert_eq!(
        enemy_move,
        Some(GROWL),
        "tie-break draw 1 must select the second usable slot, not the first"
    );
}

#[test]
fn first_battle_ai_never_picks_a_spent_move_slot() {
    let dex = Dex::new();
    // Enemy faster, so its move is directly observable; Tackle's PP is
    // fully spent first, leaving Growl as the only usable slot. Both sides
    // use Growl so the second mover (the player) always gets to act too.
    let player = max_iv_mon(&dex, 19, 5, vec![GROWL]);
    let mut enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // Even: `998 % 1` (the one PP-positive slot) selects Growl, but
    // `998 % 2` over both known slots would select the spent Tackle --
    // an odd value would select Growl either way and pin nothing.
    let tie_break_would_hit_spent_tackle_if_unfiltered = 998;
    let mut rng = SequenceRng::new([
        0,
        0,
        0,
        0,
        0,
        0,
        tie_break_would_hit_spent_tackle_if_unfiltered,
        0,
        0,
    ]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::FailedNoPp { .. })),
        "a PP-aware selection can never produce upstream's no-PP abort"
    );
    let enemy_move = events.iter().find_map(|e| match e {
        BattleEvent::StatFell {
            by_player: false,
            move_id,
            ..
        } => Some(*move_id),
        _ => None,
    });
    assert_eq!(enemy_move, Some(GROWL));
}

#[test]
fn first_battle_forces_struggle_with_no_selection_draw_when_every_slot_is_spent() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![TACKLE]);
    let player_max_hp = max_iv_mon(&dex, 19, 5, vec![TACKLE]).stats().max_hp;
    let mut enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]); // faster
    for slot in 0..enemy.moves().len() {
        for _ in 0..enemy.moves()[slot].pp {
            enemy.deduct_pp(slot).unwrap();
        }
    }

    // `AreAllMovesUnusable` forces Struggle before the first-battle AI's
    // setup draws run, so neither the four simulatedRNG draws nor a
    // tie-break draw happen.
    let mut rng = SequenceRng::new([0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: battle::STRUGGLE,
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Recoil {
                by_player: false,
                move_id: battle::STRUGGLE,
                damage: player_max_hp / 4,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "the faster, all-spent enemy is the first mover, and its forced \
         Struggle one-shots this fixture's level-5 Rattata: {events:?}"
    );
    assert_eq!(
        rng.draws(),
        4,
        "no simulatedRNG or tie-break draw for the forced Struggle, and no \
         crit draw: first_battle suppresses it entirely"
    );
}

#[test]
fn wild_flees_before_the_player_can_act_once_its_hp_drops_at_or_below_twenty_percent() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, 1, 5, vec![TACKLE]); // slow
    let enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]); // fast

    let max_hp = player.stats().max_hp;
    player.apply_damage(max_hp - max_hp * 20 / 100);
    assert!(
        100 * player.current_hp() / max_hp <= 20,
        "test setup must actually cross AI_FirstBattle's flee threshold"
    );
    let player_pp = player.moves()[0].pp;

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::WildFled,
            BattleEvent::Ended(BattleOutcome::WildFled),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::WildFled));
    assert_eq!(
        battle.player().moves()[0].pp,
        player_pp,
        "the player never got to act, so its PP must be untouched"
    );
    assert_eq!(
        rng.draws(),
        6,
        "the flee check itself draws nothing: the battle ends before the \
         player's action or a tie-break draw is ever reached"
    );
}

#[test]
fn wild_flees_after_the_players_move_already_resolved() {
    let dex = Dex::new();
    // Player faster this time, using Growl so the enemy's HP (and thus
    // whether it faints) never enters into it -- only the flee mechanic is
    // under test here.
    let mut player = max_iv_mon(&dex, 1, 10, vec![GROWL]);
    let enemy = max_iv_mon(&dex, 288, 5, vec![TACKLE, GROWL]); // slower

    let max_hp = player.stats().max_hp;
    player.apply_damage(max_hp - max_hp * 20 / 100);
    assert!(100 * player.current_hp() / max_hp <= 20);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::StatFell {
                by_player: true,
                move_id: GROWL,
                stat: ChangedStat::Attack,
                new_stage: battle::StatStage::new(-1).unwrap(),
                magnitude: 1,
            },
            BattleEvent::WildFled,
            BattleEvent::Ended(BattleOutcome::WildFled),
        ],
        "the first mover's events must survive; the enemy flees only after"
    );
    assert_eq!(
        rng.draws(),
        7,
        "the player's Growl spends its one accuracy draw, then the flee \
         check short-circuits before any tie-break draw"
    );
}

#[test]
fn first_battle_ai_does_not_flee_above_the_hp_threshold() {
    let dex = Dex::new();
    // Growl on both sides again: this test only cares whether the flee
    // branch fires, not whether either mon survives a damaging hit.
    let player = max_iv_mon(&dex, 1, 5, vec![GROWL]); // slow, full HP
    let enemy = max_iv_mon(&dex, 288, 50, vec![GROWL]); // fast

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events.contains(&BattleEvent::WildFled),
        "a healthy player must not trigger the flee branch"
    );
    assert_ne!(battle.outcome(), Some(BattleOutcome::WildFled));
    assert_eq!(
        rng.draws(),
        9,
        "battle-start + turn number + 4 simulatedRNG + the tie-break the \
         flee path skips + both Growls' accuracy rolls -- an extra draw \
         already panics on the exhausted sequence, this pins a lost one"
    );
}

#[test]
fn first_battle_ai_does_not_flee_just_above_the_hp_threshold() {
    let dex = Dex::new();
    // The player sits at the *smallest* HP whose truncating percentage
    // still exceeds the threshold: `first_battle_choice_flees_at_or_below_
    // twenty_percent_after_setup_draws` (crates/battle/src/battle/
    // opponent_ai.rs) pins the inclusive edge from the other side.
    let mut player = max_iv_mon(&dex, 1, 5, vec![GROWL]); // slow
    let enemy = max_iv_mon(&dex, 288, 50, vec![GROWL]); // fast

    let max_hp = player.stats().max_hp;
    let just_above = (21 * max_hp).div_ceil(100);
    player.apply_damage(max_hp - just_above);
    assert!(
        100 * player.current_hp() / max_hp > 20,
        "test setup must sit strictly above AI_FirstBattle's flee threshold"
    );

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events.contains(&BattleEvent::WildFled),
        "just above the threshold must not trigger the flee branch"
    );
    assert_ne!(battle.outcome(), Some(BattleOutcome::WildFled));
    assert_eq!(
        rng.draws(),
        9,
        "a flee would leave 3 draws of this script unconsumed -- the event \
         assertion above already catches that before this count is checked"
    );
}

#[test]
fn first_battle_suppresses_the_wild_opponents_crit_draw_too() {
    let dex = Dex::new();
    // Wild-side mirror of `first_battle_suppresses_the_crit_draw_and_never_crits`:
    // here the *enemy* is faster, and its Tackle lands on a surviving player.
    let player = max_iv_mon(&dex, 1, 50, vec![GROWL]); // slower
    let enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]); // faster

    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let is_critical = events
        .iter()
        .find_map(|e| match e {
            BattleEvent::Hit {
                by_player: false,
                is_critical,
                ..
            } => Some(*is_critical),
            _ => None,
        })
        .expect("the wild Tackle must land on the surviving player");
    assert!(
        !is_critical,
        "first_battle must suppress the wild side's crit roll too"
    );
    assert!(
        battle.outcome().is_none(),
        "both sides survive this exchange"
    );
    assert_eq!(
        rng.draws(),
        11,
        "the enemy's suppressed-crit Tackle spends only 3 draws, one fewer \
         than an ordinary hit's 4 -- a reintroduced crit draw would exhaust \
         this exact-sized script and panic inside take_turn, before either \
         assertion above ever runs"
    );
}

#[test]
fn run_away_holder_escapes_the_first_battle_unconditionally() {
    // Upstream's `IsRunningFromBattleImpossible` answers `BATTLE_RUN_SUCCESS`
    // for a Run Away holder (`pokeemerald/src/battle_main.c:4038`-`:4039`)
    // before it ever reaches the `BATTLE_TYPE_FIRST_BATTLE` refusal
    // (`:4078`-`:4082`), so the selection is admitted and
    // `TryRunFromBattle`'s Run Away branch escapes with no draw and no
    // `runTries` increment (`pokeemerald/src/battle_util.c:426`-`:446`).
    let dex = Dex::new();
    // Rattata's primary ability is Run Away; an even personality selects it.
    let player = max_iv_mon(&dex, 19, 5, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::RUN_AWAY);
    let enemy = max_iv_mon(&dex, 288, 2, vec![TACKLE, GROWL]);

    let mut rng = SequenceRng::new([0; 7]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1);

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
    assert_eq!(
        battle.run_tries(),
        0,
        "Run Away never advances run_tries (`battle_util.c:427`-`:447`)"
    );
    assert_eq!(
        rng.draws(),
        7,
        "the turn draws the turn number and the enemy's first-battle AI \
         setup, but Run Away adds no escape-specific draw"
    );
}
