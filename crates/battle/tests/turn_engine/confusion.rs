//! Confusion infliction and its pre-action ordering, driven through complete
//! turns. Most fixtures set the volatile directly via
//! [`battle::BattlePokemon::volatiles_mut`]; the final section inflicts it
//! with real [`battle::confuse::EFFECT_CONFUSE`] and
//! [`battle::EFFECT_CONFUSE_HIT`] moves.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, BattleOutcome, Dex, PlayerAction, StatStage, Status1};

const TACKLE: MoveId = MoveId::TACKLE;

/// `SPECIES_CHARMANDER`.
const CHARMANDER: u16 = 4;
/// `SPECIES_RATTATA`, the faster mover.
const RATTATA: u16 = 19;
/// `SPECIES_GASTLY`: Ghost/Poison, immune to Tackle.
const GASTLY: u16 = 92;
/// `SPECIES_ZIGZAGOON`.
const ZIGZAGOON: u16 = 288;

#[test]
fn a_duration_one_confusion_snaps_out_before_the_mover_s_own_hit() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.volatiles_mut().set_confusion(1);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    // Setup draws, then the player's one-shot Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::SnappedOutOfConfusion { by_player: true },
        "the terminal decrement precedes the move it gates: {events:?}"
    );
    assert!(
        matches!(
            events[1],
            BattleEvent::Hit {
                by_player: true,
                move_id: TACKLE,
                ..
            }
        ),
        "the turn continues into the ordinary move -- no self-hit exists yet: {events:?}"
    );
    assert!(!battle.player().volatiles().confused());
    assert_eq!(
        rng.draws(),
        7,
        "the decrement itself must draw nothing beyond the ordinary hit"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

#[test]
fn confusion_snaps_out_ahead_of_the_same_action_s_full_paralysis_draw() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    player.volatiles_mut().set_confusion(1);
    player.set_status1(Status1::Paralysed);
    let starting_pp = player.moves()[0].pp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // Setup draws, the enemy's immune Tackle, then the player's full-paralysis
    // draw (residue 0 -> cancelled).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::NoEffect {
                by_player: false,
                move_id: TACKLE,
            },
            BattleEvent::SnappedOutOfConfusion { by_player: true },
            BattleEvent::FullyParalyzed {
                by_player: true,
                move_id: TACKLE,
            },
        ],
        "the confusion decrement gates the same action ahead of the \
         full-paralysis draw: {events:?}"
    );
    assert!(!battle.player().volatiles().confused());
    assert_eq!(
        battle.player().moves()[0].pp,
        starting_pp,
        "a cancelled move never spends PP, and neither transition changes that"
    );
}

#[test]
fn each_battler_s_confusion_decrements_only_on_its_own_action() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    enemy.volatiles_mut().set_confusion(1);

    // Setup draws, the player's Tackle, then the enemy's Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        matches!(
            events[0],
            BattleEvent::Hit {
                by_player: true,
                move_id: TACKLE,
                ..
            }
        ),
        "the faster, unconfused player acts first: {events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::SnappedOutOfConfusion { by_player: false },
        "only the enemy's own action ticks its own confusion: {events:?}"
    );
    assert!(
        matches!(
            events[2],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "the enemy's move still resolves normally once confusion ends: {events:?}"
    );
    assert!(!battle.enemy().volatiles().confused());
    assert_eq!(rng.draws(), 11);
}

#[test]
fn a_multi_turn_confusion_persists_without_an_expiry_event() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.volatiles_mut().set_confusion(2);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
    let starting_pp = player.moves()[0].pp;

    // Setup draws, the confusion coin (residue 1 -> continue), then Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::Confused { by_player: true },
        "a still-active decrement announces before the coin draw: {events:?}"
    );
    assert!(
        matches!(
            events[1],
            BattleEvent::Hit {
                by_player: true,
                move_id: TACKLE,
                ..
            }
        ),
        "an odd coin lets the chosen move continue: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::SnappedOutOfConfusion { .. })),
        "one turn of a two-turn duration must not report the terminal event: {events:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        starting_pp - 1,
        "the move-through branch spends PP exactly like an ordinary action"
    );
    assert_eq!(
        rng.draws(),
        8,
        "one coin draw precedes the ordinary hit's own four"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert!(battle.player().volatiles().confused());
    assert_eq!(
        battle.player().volatiles().confusion_turns,
        1,
        "the counter still advances toward its own expiry"
    );
}

#[test]
fn an_even_coin_cancels_the_chosen_move_for_a_self_hit_that_ignores_its_own_type_immunity() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    player.volatiles_mut().set_confusion(2);
    let starting_pp = player.moves()[0].pp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // Setup draws, the confusion coin (residue 0 -> self-hit), the self-hit
    // damage roll (residue 0), then the enemy's Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::Confused { by_player: true },
        "the still-active decrement announces before the coin draw: {events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::ConfusionSelfHit {
            by_player: true,
            damage: 5,
        },
        "an even coin cancels the move for a self-hit that damages its own \
         Ghost user despite Tackle's Normal type being an immunity: {events:?}"
    );
    assert_eq!(
        events[2],
        BattleEvent::NoEffect {
            by_player: false,
            move_id: TACKLE,
        },
        "the still-standing player still faces the enemy's own action after \
         the self-hit: {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "the chosen move never executes on the self-hit branch: {events:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        starting_pp,
        "a self-hit spends no PP"
    );
    assert_eq!(
        rng.draws(),
        9,
        "exactly one coin draw and one damage-variance draw precede the enemy's own hit"
    );
}

#[test]
fn a_self_hit_ignores_its_own_paralysis_and_draws_no_paralysis_bit() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    player.volatiles_mut().set_confusion(2);
    player.set_status1(Status1::Paralysed);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // The self-hit returns before the paralysis draw: only the coin and the
    // damage roll are drawn.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::FullyParalyzed { .. })),
        "the self-hit branch ends the action before the paralysis draw: {events:?}"
    );
    assert!(
        matches!(
            events[2],
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                ..
            }
        ),
        "the self-hit still runs despite the paralysis status: {events:?}"
    );
    assert_eq!(
        rng.draws(),
        9,
        "no extra draw is spent on the paralysis check this action skips"
    );
}

#[test]
fn a_self_hit_can_faint_its_own_user_and_skip_the_opponent_s_queued_action() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.volatiles_mut().set_confusion(2);
    player.apply_damage(player.stats().max_hp - 1);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    // Setup draws, then the confusion coin (residue 0 -> self-hit) and the
    // self-hit damage roll. The player faints and the enemy's Tackle never runs.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Confused { by_player: true },
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                damage: 1,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "the self-hit's own faint settles before the opponent ever acts, \
         ending the battle immediately since a wild player has no reserves: {events:?}"
    );
    assert_eq!(
        rng.draws(),
        5,
        "the opponent's queued Tackle draws nothing once the player has fainted"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
}

#[test]
fn a_self_hit_rates_against_the_users_own_attack_and_defense_stages() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    player.volatiles_mut().set_confusion(2);
    player.stages_mut().attack = StatStage::new(2).unwrap();
    player.stages_mut().defense = StatStage::new(-2).unwrap();
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // Same draws as the neutral-stage self-hit; only the stages differ.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[1],
        BattleEvent::ConfusionSelfHit {
            by_player: true,
            damage: 18,
        },
        "a raised Attack over a lowered Defense outdamages the neutral 5; \
         swapping the two stages would land below it: {events:?}"
    );
}

const CONFUSE_RAY: MoveId = MoveId::CONFUSE_RAY;
const SWEET_KISS: MoveId = MoveId::SWEET_KISS;
const PSYBEAM: MoveId = MoveId::PSYBEAM;
/// `SPECIES_SPINDA`: Own Tempo in its only ability slot.
const SPINDA: u16 = 308;

#[test]
fn confuse_ray_inflicts_a_fresh_confusion_that_the_target_immediately_ticks_through() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![CONFUSE_RAY]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // Setup draws, Confuse Ray (accuracy, duration residue 0 -> 2), the
    // enemy's decrement (2 -> 1) and coin (residue 1 -> continue), then Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::ConfusionInflicted {
            by_player: true,
            move_id: CONFUSE_RAY,
        },
        "{events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::Confused { by_player: false },
        "the enemy's own action immediately ticks the confusion the \
         player's move just wrote, before its own coin draw: {events:?}"
    );
    assert!(
        matches!(
            events[2],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "an odd coin lets the enemy's chosen move continue: {events:?}"
    );
    assert!(battle.enemy().volatiles().confused());
    assert_eq!(
        battle.enemy().volatiles().confusion_turns,
        1,
        "one action already ticked off the freshly rolled 2-turn duration"
    );
    assert_eq!(
        battle.enemy().status1(),
        Status1::Healthy,
        "confusion is a status2 volatile, independent of status1"
    );
    assert_eq!(
        rng.draws(),
        10,
        "the accuracy and duration draws, then the enemy's own coin draw and \
         ordinary four-draw Tackle, spend exactly the scripted sequence"
    );
}

#[test]
fn an_own_tempo_target_blocks_confuse_ray_before_any_draw() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![CONFUSE_RAY]);
    let enemy = max_iv_mon(&dex, SPINDA, 5, vec![TACKLE]);

    // Setup draws, then the enemy's Tackle: Own Tempo blocks the move before
    // any draw.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::OwnTempoProtected {
            by_player: true,
            move_id: CONFUSE_RAY,
        },
        "{events:?}"
    );
    assert!(
        matches!(
            events[1],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "{events:?}"
    );
    assert!(!battle.enemy().volatiles().confused());
}

#[test]
fn an_already_confused_target_refuses_confuse_ray_without_drawing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![CONFUSE_RAY]);
    let mut enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
    enemy.volatiles_mut().set_confusion(1);

    // Setup draws, Confuse Ray into an already-confused target (no draw), the
    // terminal decrement (no draw, no coin), then the enemy's Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::AlreadyConfused {
            by_player: true,
            move_id: CONFUSE_RAY,
        },
        "{events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::SnappedOutOfConfusion { by_player: false },
        "{events:?}"
    );
    assert!(!battle.enemy().volatiles().confused());
}

#[test]
fn a_missed_sweet_kiss_reports_missed_and_confuses_nothing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![SWEET_KISS]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // Setup draws, then Sweet Kiss missing (roll 95 -> 96 of 100), then the
    // enemy's Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 95, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::Missed {
            by_player: true,
            move_id: SWEET_KISS,
        },
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ConfusionInflicted { .. })),
        "{events:?}"
    );
    assert!(!battle.enemy().volatiles().confused());
}

#[test]
fn psybeam_applies_confusion_after_its_own_hit_and_chance_draw() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![PSYBEAM]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // Setup draws, Psybeam accuracy/crit/damage, the chance draw (residue 9),
    // the duration (residue 0 -> 2), the confusion coin (residue 1 -> continue),
    // then the enemy's Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 9, 0, 1, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        matches!(
            events[0],
            BattleEvent::Hit {
                by_player: true,
                move_id: PSYBEAM,
                ..
            }
        ),
        "{events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::ConfusionInflicted {
            by_player: true,
            move_id: PSYBEAM,
        },
        "{events:?}"
    );
    assert_eq!(
        events[2],
        BattleEvent::Confused { by_player: false },
        "{events:?}"
    );
    assert!(
        matches!(
            events[3],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "{events:?}"
    );
    assert!(battle.enemy().volatiles().confused());
    assert_eq!(battle.enemy().volatiles().confusion_turns, 1);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert_eq!(
        rng.draws(),
        13,
        "the hit's own four draws, the chance and duration draws, then the \
         enemy's own coin draw and ordinary four-draw Tackle, spend exactly \
         the scripted sequence"
    );
}

/// A lethal hit suppresses confusion and its duration draw even after a
/// successful chance roll (`pokeemerald/src/battle_script_commands.c:2261`-`:2264`,
/// `:2533`-`:2544`).
#[test]
fn a_lethal_psybeam_never_draws_a_duration_or_inflicts_confusion() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 50, vec![PSYBEAM]);
    let mut enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    // One HP left: any Psybeam hit is lethal.
    enemy.apply_damage(enemy.stats().max_hp - 1);

    // Setup draws, Psybeam accuracy/crit/damage, then a successful chance draw
    // (residue 9) that must draw no duration.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 9]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "fixture sanity -- the hit must be lethal: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ConfusionInflicted { .. })),
        "a fainted target must never be confused: {events:?}"
    );
    assert_eq!(
        rng.draws(),
        7,
        "no duration draw follows the successful chance roll once the hit \
         proves lethal"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

#[test]
fn an_own_tempo_target_is_hit_by_psybeam_but_never_confused() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![PSYBEAM]);
    let enemy = max_iv_mon(&dex, SPINDA, 5, vec![TACKLE]);

    // Setup draws, Psybeam accuracy/crit/damage, a chance draw that clears
    // (residue 9) but is discarded by Own Tempo, then the enemy's Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 9, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        matches!(
            events[0],
            BattleEvent::Hit {
                by_player: true,
                move_id: PSYBEAM,
                ..
            }
        ),
        "{events:?}"
    );
    assert!(
        matches!(
            events[1],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "Own Tempo lands no confusion, so the enemy's own action never ticks \
         or draws a coin: {events:?}"
    );
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(!battle.enemy().volatiles().confused());
}
