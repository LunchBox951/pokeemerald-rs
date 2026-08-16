//! Non-volatile status and confusion, through the whole turn engine (issue
//! #293): the pre-move canceller, the end-of-turn residual, and the two
//! moves that inflict them.
//!
//! The unit tests under `crates/battle/src/status`,
//! `.../primary_status` and `.../secondary` pin each piece's own arithmetic
//! and draw count in isolation. What this file pins is the *wiring*: that
//! the canceller runs where `Cmd_attackcanceler` runs (before the no-PP
//! test and before `ppreduce`), that the residual runs where
//! `DoBattlerEndTurnEffects` runs (after both actions, before the
//! fainted-mon pass), and that a status a move inflicts is still there next
//! turn.
//!
//! Every draw-count assertion is made **against a control run of the same
//! scenario without the status**, rather than against a hand-counted
//! absolute. The absolute totals are already pinned by the per-module tests;
//! what matters here is the *delta*, because that is what a caller sharing
//! the stream with the overworld actually has to budget for.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, BattleOutcome, ChangedStat, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId(33);
const THUNDER_WAVE: MoveId = MoveId(86);
const SUPERSONIC: MoveId = MoveId(48);
const POISON_STING: MoveId = MoveId(40);
const CONSTRICT: MoveId = MoveId(132);

const TREECKO: u16 = 277;
const RATTATA: u16 = 19;
const MARILL: u16 = 183;
const PLUSLE: u16 = 353;
const TENTACOOL: u16 = 72;

/// A long, uniform script: every draw is `0`, which lands on the "always
/// hits / never crits at stage 0 / best damage roll / effect fires" corner
/// of every roll in the engine. Long enough that no test under-provisions,
/// and `SequenceRng::draws` is what each assertion actually reads.
fn zeros() -> SequenceRng {
    SequenceRng::new([0u16; 256])
}

/// A full-paralysis turn: the move is cancelled, **no PP is spent** (neither
/// `BattleScript_MoveUsedIsParalyzed` nor the confusion script contains a
/// `ppreduce`), and the turn costs exactly one draw more than the same turn
/// without the status — `CANCELER_PARALYZED`'s `Random() % 4`.
#[test]
fn a_fully_paralysed_player_loses_the_turn_without_spending_pp() {
    let dex = Dex::new();

    // Control: the same battle, the same all-zero stream, no status.
    // Level 20 on both sides: neither Tackle is a knockout, so **both**
    // battlers act in the control run too. (A control whose opponent dies
    // to the first hit would spend four fewer draws for a reason that has
    // nothing to do with the status.)
    let mut control_rng = zeros();
    let mut control = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, TREECKO, 20, vec![TACKLE]),
        max_iv_mon(&dex, MARILL, 20, vec![TACKLE]),
        false,
        &mut control_rng,
    )
    .unwrap();
    let before_control = control_rng.draws();
    let control_events = control
        .take_turn(PlayerAction::UseMove(0), &mut control_rng)
        .unwrap();
    let control_draws = control_rng.draws() - before_control;
    assert!(
        control_events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "control: the player's Tackle lands"
    );

    // Now with the player paralysed. Draw 0 is `0 % 4 == 0`, so the
    // canceller immobilises.
    let mut player = max_iv_mon(&dex, TREECKO, 20, vec![TACKLE]);
    player.set_status(Status1::Paralysed);
    let pp_before = player.moves()[0].pp;

    let mut rng = zeros();
    let mut battle = Battle::new(
        Dex::new(),
        player,
        max_iv_mon(&dex, MARILL, 20, vec![TACKLE]),
        false,
        &mut rng,
    )
    .unwrap();
    let before = rng.draws();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::FullyParalysed { by_player: true }),
        "the paralysis must be reported: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "...and the move must not land: {events:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        pp_before,
        "a cancelled move keeps its PP -- the canceller runs before ppreduce"
    );
    assert_eq!(
        rng.draws() - before,
        control_draws + 1 - 4,
        "one extra draw for the paralysis roll, and four fewer for the \
         player's cancelled 4-draw Tackle"
    );
}

/// The other three residues let the move through, and the draw is still
/// spent -- so a paralysed battler that *does* move costs exactly one more
/// draw than an unparalysed one.
#[test]
fn a_paralysed_battler_that_gets_through_still_pays_for_the_roll() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, TREECKO, 50, vec![TACKLE]);
    player.set_status(Status1::Paralysed);
    let pp_before = player.moves()[0].pp;

    // Draw 1 for the paralysis roll: `1 % 4 != 0`, so the move proceeds.
    // The script is otherwise all zeros; the battle-start draw is the
    // leading 0, then the turn-number 0, the enemy's selection 0, then the
    // paralysis roll.
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(
        Dex::new(),
        player,
        max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]),
        false,
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events.contains(&BattleEvent::FullyParalysed { by_player: true }),
        "draw 1 is not 0 mod 4: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "the move goes through: {events:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        pp_before - 1,
        "...and a move that goes through does spend its PP"
    );
}

/// Paralysis quarters effective Speed, which is enough to flip the turn
/// order -- the most visible consequence of the status short of the
/// full-stop itself.
#[test]
fn paralysis_quarters_speed_enough_to_lose_the_first_move() {
    let dex = Dex::new();
    let fast = max_iv_mon(&dex, TREECKO, 20, vec![TACKLE]);
    let slow = max_iv_mon(&dex, MARILL, 20, vec![TACKLE]);
    assert!(
        fast.effective_speed() > slow.effective_speed(),
        "fixture: Treecko must outspeed Marill before the status"
    );

    let mut paralysed = fast.clone();
    paralysed.set_status(Status1::Paralysed);
    assert_eq!(
        paralysed.effective_speed(),
        fast.effective_speed() / 4,
        "quartered"
    );
    assert!(
        paralysed.effective_speed() < slow.effective_speed(),
        "and now slower, so the order really flips"
    );

    // Through the engine: the enemy's Tackle now lands before the player's.
    // Draw 1 at the paralysis roll so the player is not immobilised too.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(Dex::new(), paralysed, slow, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let first_hit = events
        .iter()
        .find(|e| matches!(e, BattleEvent::Hit { .. }))
        .expect("both sides Tackle");
    assert!(
        matches!(
            first_hit,
            BattleEvent::Hit {
                by_player: false,
                ..
            }
        ),
        "the paralysed player must move second: {events:?}"
    );
}

/// End-of-turn poison: `maxHP / 8` floored at 1, applied after **both**
/// battlers have acted, and drawing nothing.
#[test]
fn poison_ticks_at_the_end_of_the_turn_and_draws_nothing() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, TREECKO, 50, vec![TACKLE]);
    player.set_status(Status1::Poisoned);
    let max_hp = player.stats().max_hp;
    let expected = max_hp / 8;
    assert!(expected > 1, "fixture: a level-50 Treecko has enough HP");

    // Control run first, to isolate the residual's own cost. Both runs use
    // a level-50 opponent so nothing dies and the two turns are otherwise
    // identical.
    let mut control_rng = zeros();
    let mut control = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, TREECKO, 50, vec![TACKLE]),
        max_iv_mon(&dex, MARILL, 50, vec![TACKLE]),
        false,
        &mut control_rng,
    )
    .unwrap();
    let control_before = control_rng.draws();
    let _ = control
        .take_turn(PlayerAction::UseMove(0), &mut control_rng)
        .unwrap();
    let control_draws = control_rng.draws() - control_before;

    let mut rng = zeros();
    let mut battle = Battle::new(
        Dex::new(),
        player,
        max_iv_mon(&dex, MARILL, 50, vec![TACKLE]),
        false,
        &mut rng,
    )
    .unwrap();
    let before = rng.draws();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: true,
            damage: expected,
        }),
        "the poison tick must be reported with maxHP/8: {events:?}"
    );
    // It is the *last* thing in the turn: every move event precedes it.
    let poison_at = events
        .iter()
        .position(|e| matches!(e, BattleEvent::HurtByPoison { .. }))
        .expect("reported");
    assert!(
        events[..poison_at]
            .iter()
            .any(|e| matches!(e, BattleEvent::Hit { .. })),
        "the moves resolve before the residual: {events:?}"
    );
    assert_eq!(
        rng.draws() - before,
        control_draws,
        "ENDTURN_POISON contains no Random() at all"
    );
}

/// A poison tick that knocks the *opponent* out ends the battle and awards
/// experience — `HandleFaintedMonActions` runs `BattleScript_GiveExp` for
/// any battler at 0 HP, not only for one a move finished off.
#[test]
fn a_poison_tick_that_faints_the_wild_mon_still_awards_experience() {
    let dex = Dex::new();
    // A level-2 Rattata worn down to 1 HP: any tick finishes it.
    let mut enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
    enemy.set_status(Status1::Poisoned);
    let enemy_hp = enemy.current_hp();
    enemy.apply_damage(enemy_hp - 1);
    assert_eq!(enemy.current_hp(), 1);

    // The player misses on purpose (draw 95 against Tackle's 95 accuracy)
    // so the *poison* is unambiguously what lands the KO.
    let mut rng = SequenceRng::new([0, 0, 0, 95, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, TREECKO, 5, vec![TACKLE]),
        enemy,
        false,
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: false,
            damage: 1,
        }),
        "the tick is floored at 1: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the tick knocked it out: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, BattleEvent::ExpGained(_))),
        "a residual-damage faint still gives exp: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// Thunder Wave through the engine: the target is paralysed, and the status
/// is still there next turn (`status1` is not turn-scoped).
#[test]
fn thunder_wave_paralyses_the_target_for_the_rest_of_the_battle() {
    let dex = Dex::new();
    let mut rng = zeros();
    let mut battle = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, PLUSLE, 20, vec![THUNDER_WAVE, TACKLE]),
        max_iv_mon(&dex, MARILL, 20, vec![TACKLE]),
        false,
        &mut rng,
    )
    .unwrap();
    assert_eq!(battle.enemy().status(), Status1::Healthy, "setup");

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::StatusInflicted {
            by_player: true,
            move_id: THUNDER_WAVE,
            status: Status1::Paralysed,
        }),
        "{events:?}"
    );
    assert_eq!(battle.enemy().status(), Status1::Paralysed);

    // Next turn, still paralysed -- and a second Thunder Wave fails rather
    // than re-applying it.
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::MoveFailed {
            by_player: true,
            move_id: THUNDER_WAVE,
        }),
        "a second Thunder Wave is `But it failed!`: {events:?}"
    );
    assert_eq!(battle.enemy().status(), Status1::Paralysed);
}

/// Supersonic confuses, and the confusion then costs its holder turns: the
/// self-hit branch cancels the move without spending PP, and the counter
/// runs out on its own.
#[test]
fn supersonic_confuses_and_the_confusion_costs_the_target_its_turn() {
    let dex = Dex::new();
    // The enemy is the one that gets confused, so its own turns are the
    // ones the canceller eats.
    let mut rng = zeros();
    let mut battle = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, TENTACOOL, 30, vec![SUPERSONIC]),
        max_iv_mon(&dex, MARILL, 5, vec![TACKLE]),
        false,
        &mut rng,
    )
    .unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    // Every draw is 0, so the duration roll is `0 % 4 + 2 == 2`.
    assert!(
        events.contains(&BattleEvent::Confused {
            by_player: true,
            move_id: SUPERSONIC,
            turns: 2,
        }),
        "{events:?}"
    );
    // The enemy moved *after* being confused this turn, so its first
    // canceller pass already ran: draw 0 is even, which is the self-hit.
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::HurtItselfInConfusion {
                by_player: false,
                ..
            }
        )),
        "an even `Random() & 1` is the self-hit: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: false,
                ..
            }
        )),
        "...and the enemy's own move never happens: {events:?}"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        dex.move_data(TACKLE).unwrap().pp,
        "a confusion self-hit spends no PP either"
    );

    // Second turn: the counter reaches 0 and the enemy snaps out.
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::SnappedOutOfConfusion { by_player: false }),
        "{events:?}"
    );
    assert!(!battle.enemy().volatiles().is_confused());
}

/// Poison Sting's secondary: the same four draws an ordinary hit costs, but
/// the fourth now *lands*.
#[test]
fn poison_stings_secondary_chance_lands_on_a_low_roll_and_not_on_a_high_one() {
    let dex = Dex::new();
    let scenario = |effect_roll: u16| {
        let mut rng = SequenceRng::new([
            0, // battle start
            0, // turn number
            0, // the enemy's selection
            0, // player accuracy -> hit
            1, // crit -> none
            0, // damage roll
            effect_roll,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]);
        let mut battle = Battle::new(
            Dex::new(),
            max_iv_mon(&dex, TENTACOOL, 30, vec![POISON_STING]),
            max_iv_mon(&dex, MARILL, 30, vec![TACKLE]),
            false,
            &mut rng,
        )
        .unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        (battle.enemy().status(), events)
    };

    // Poison Sting's chance is 30: `0 % 100 < 30` fires.
    let (status, events) = scenario(0);
    assert_eq!(status, Status1::Poisoned);
    assert!(
        events.contains(&BattleEvent::StatusInflicted {
            by_player: true,
            move_id: POISON_STING,
            status: Status1::Poisoned,
        }),
        "{events:?}"
    );

    // `30 % 100 < 30` is false -- strictly less than.
    let (status, events) = scenario(30);
    assert_eq!(
        status,
        Status1::Healthy,
        "the roll must be strict: {events:?}"
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, BattleEvent::StatusInflicted { .. })));
}

/// A Poison-type target takes Poison Sting's damage and none of its poison
/// -- and the roll is still spent, so the draw count does not change.
#[test]
fn a_poison_type_target_is_damaged_but_never_poisoned() {
    let dex = Dex::new();
    let mut rng = zeros();
    let mut battle = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, TENTACOOL, 30, vec![POISON_STING]),
        // Tentacool is Water/Poison, so it cannot be poisoned at all.
        max_iv_mon(&dex, TENTACOOL, 30, vec![TACKLE]),
        false,
        &mut rng,
    )
    .unwrap();
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
        "the damage half still lands: {events:?}"
    );
    assert_eq!(battle.enemy().status(), Status1::Healthy);
}

/// Constrict's secondary is a stat drop rather than a status, and it lands
/// through the same `seteffectwithchance` roll -- reported as the ordinary
/// [`BattleEvent::StatFell`].
#[test]
fn constricts_secondary_lowers_the_targets_speed_by_one() {
    let dex = Dex::new();
    let mut rng = zeros();
    let mut battle = Battle::new(
        Dex::new(),
        max_iv_mon(&dex, TENTACOOL, 30, vec![CONSTRICT]),
        max_iv_mon(&dex, MARILL, 30, vec![TACKLE]),
        false,
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::StatFell {
                by_player: true,
                move_id: CONSTRICT,
                stat: ChangedStat::Speed,
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(
        battle.enemy().stages().speed,
        battle::StatStage::new(-1).unwrap()
    );
}
