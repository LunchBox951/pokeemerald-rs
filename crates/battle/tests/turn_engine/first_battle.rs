//! `BATTLE_TYPE_FIRST_BATTLE` (issue #187): the Route 101 intro Zigzagoon
//! fight's three deltas from an ordinary wild encounter -- crit suppression,
//! running forbidden, and the wild opponent's AI-branch move choice -- pinned
//! end to end through the public `battle` API, the same way every other
//! `turn_engine/` module pins its family of behavior. Unit-level draw-count
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
use assets::MoveId;
use battle::{Battle, BattleError, BattleEvent, BattleOutcome, Dex, LoweredStat, PlayerAction};

const TACKLE: MoveId = MoveId(33);
const GROWL: MoveId = MoveId(45);
const SLASH: MoveId = MoveId(163);

#[test]
fn run_is_forbidden_before_any_draw_and_leaves_the_battle_usable() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 1, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, 288, 2, vec![TACKLE, GROWL]);

    let mut rng = SequenceRng::new([0]); // Battle::new's battle-start draw only
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

    // The battle is left exactly as it was: a real action still works.
    let mut rng = SequenceRng::new([
        0, 0, 0, 0, 0, 0, // turn number + 4 discarded simulatedRNG + tie-break
        0, 0, 0, // player's suppressed-crit Tackle: accuracy, damage roll, effect chance
    ]);
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
    // L50 Rattata vs L2 Zigzagoon: obviously faster, and Slash one-shots it,
    // so the enemy's chosen action never gets to execute -- only the
    // selection draws before it, plus the player's own hit, are spent.
    let player = max_iv_mon(&dex, 19, 50, vec![SLASH]);
    let enemy = max_iv_mon(&dex, 288, 2, vec![TACKLE, GROWL]);

    // draws: 0 battle-start turn number (unequal speed, no tie draw).
    // take_turn: 1 turn number, 2-5 discarded simulatedRNG, 6 tie-break
    // (0 % 2 -> Tackle, though it never gets to act), 7 accuracy (hit),
    // 8 damage roll (best case), 9 effect chance (discarded). If crit
    // suppression were broken, an extra crit draw here would shift every
    // later index by one and this 10-value script would panic on
    // exhaustion rather than silently passing.
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
    assert_eq!(rng.draws(), 10);
}

#[test]
fn first_battle_ai_tie_break_can_land_on_the_second_move() {
    let dex = Dex::new();
    // Enemy faster this time, so its own chosen move is directly observable.
    // Both sides use only Growl, so neither side's action can faint the
    // other -- the second mover (the player) is guaranteed to also act,
    // regardless of this test's own concern (the tie-break index).
    let player = max_iv_mon(&dex, 19, 5, vec![GROWL]);
    let enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]);

    // draws: battle start (0), turn number (0), 4 discarded simulatedRNG,
    // tie-break 1 -> 1 % 2 = 1 -> the second usable slot (Growl), the
    // enemy's Growl (1 draw), then the player's own Growl (1 draw).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 1, 0, 0]);
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

    // The tie-break draw is deliberately an *even* value: with the PP
    // screen in place there is one usable slot and `998 % 1 == 0` selects
    // Growl, but if the screen were dropped both slots would be candidates
    // and `998 % 2 == 0` would land on the spent Tackle slot -- so this
    // draw value actually distinguishes the PP-aware pre-draw filter from
    // a plain wild mon's PP-ignoring redraw loop (an odd value like 999
    // would pick Growl either way and pin nothing).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 998, 0, 0]);
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
    let mut enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]); // faster
    for slot in 0..enemy.moves().len() {
        for _ in 0..enemy.moves()[slot].pp {
            enemy.deduct_pp(slot).unwrap();
        }
    }

    // battle start + turn number only: `AreAllMovesUnusable` forces
    // Struggle before `BattleAI_SetupAIData` ever runs, so neither the four
    // simulatedRNG draws nor a tie-break draw happen.
    let mut rng = SequenceRng::new([0, 0]);
    let mut battle = Battle::new(dex, player, enemy, true, &mut rng).unwrap();
    let failure = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();

    assert_eq!(
        failure.error(),
        BattleError::UnsupportedMoveEffect(battle::STRUGGLE)
    );
    assert!(
        failure.events().is_empty(),
        "the faster, all-spent enemy is the first mover, so nothing acted yet"
    );
    assert_eq!(rng.draws(), 2, "no simulatedRNG, no tie-break draw");
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

    // battle start (0, unequal speed), turn number (0), 4 discarded
    // simulatedRNG -- the flee check itself draws nothing, and the battle
    // ends before the player's action or a tie-break draw is ever reached.
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
    assert_eq!(rng.draws(), 6);
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

    // battle start (0), turn number (0), 4 discarded simulatedRNG, then the
    // player's Growl (1 draw: accuracy only, stat_change's pipeline shape)
    // -- no tie-break draw, since the flee check short-circuits before it.
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
                stat: LoweredStat::Attack,
                new_stage: battle::StatStage::new(-1).unwrap(),
            },
            BattleEvent::WildFled,
            BattleEvent::Ended(BattleOutcome::WildFled),
        ],
        "the first mover's events must survive; the enemy flees only after"
    );
    assert_eq!(rng.draws(), 7);
}

#[test]
fn first_battle_ai_does_not_flee_above_the_hp_threshold() {
    let dex = Dex::new();
    // Growl on both sides again: this test only cares whether the flee
    // branch fires, not whether either mon survives a damaging hit.
    let player = max_iv_mon(&dex, 1, 5, vec![GROWL]); // slow, full HP
    let enemy = max_iv_mon(&dex, 288, 50, vec![GROWL]); // fast

    // Same shape as the flee test, but the player is at full HP, so the
    // flee check must fail and fall through to the ordinary tie-break --
    // one more draw than the flee path (the tie-break itself), plus both
    // sides' Growl, and no WildFled outcome.
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
    // Same shape as the full-HP no-flee test above, but the player sits at
    // the *smallest* HP whose truncating percentage still exceeds the
    // threshold -- one integer percent above 20. The opponent_ai unit test
    // pins the inclusive edge (exactly 20% flees); this pins the exclusive
    // one from the other side, so a threshold drifting upward (which a
    // full-HP player can never notice) flees here and fails.
    let mut player = max_iv_mon(&dex, 1, 5, vec![GROWL]); // slow
    let enemy = max_iv_mon(&dex, 288, 50, vec![GROWL]); // fast

    let max_hp = player.stats().max_hp;
    let just_above = (21 * max_hp).div_ceil(100);
    player.apply_damage(max_hp - just_above);
    assert!(
        100 * player.current_hp() / max_hp > 20,
        "test setup must sit strictly above AI_FirstBattle's flee threshold"
    );

    // battle start, turn number, 4 simulatedRNG, tie-break, both Growls'
    // accuracy rolls -- a flee would end the turn 3 draws short of this
    // script and fail the draw-count pin even before the event assert.
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
    assert_eq!(rng.draws(), 9);
}

#[test]
fn first_battle_suppresses_the_wild_opponents_crit_draw_too() {
    let dex = Dex::new();
    // Wild-side mirror of `first_battle_suppresses_the_crit_draw_and_never_crits`:
    // this time the *enemy* is faster and its Tackle actually lands on a
    // surviving player -- the single most-executed path once #221 wires the
    // scripted Route 101 encounter, whose L2 Zigzagoon spams Tackle.
    let player = max_iv_mon(&dex, 1, 50, vec![GROWL]); // slower
    let enemy = max_iv_mon(&dex, 288, 50, vec![TACKLE, GROWL]); // faster

    // draws: battle start (0, unequal speed), turn number (0), 4 discarded
    // simulatedRNG, tie-break (0 % 2 -> Tackle), the enemy's
    // suppressed-crit Tackle (accuracy, damage roll, effect chance -- 3
    // draws, not the ordinary hit's 4), then the player's own Growl
    // (accuracy). If the wild side's crit suppression regressed, the
    // reintroduced crit draw would exhaust this exact-sized 11-value
    // script and panic -- and its 0 value would also *be* a crit, failing
    // the assertion below first.
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
    assert_eq!(rng.draws(), 11);
}
