//! [`Status1::Paralysed`], the full-paralysis attacker gate, and
//! [`BattleScript_EffectParalyze`] (Thunder Wave/Stun Spore/Glare), driven
//! through real turns.
//!
//! Unit-level draw shapes are pinned inside `battle::status1` (the
//! full-paralysis draw) and `battle::paralyze` (the type/status guards and
//! the accuracy draw). What is pinned **here** is the wiring only a turn can
//! show: that the full-paralysis draw runs before PP is ever touched, that a
//! cancelled mover keeps its PP while a mover whose move merely fails inside
//! its own script (immunity) still spends it, and that the quarter-speed
//! modifier really reorders who acts first.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, Dex, PlayerAction, Status1, STRUGGLE};

/// `MOVE_TACKLE`.
const TACKLE: MoveId = MoveId(33);
/// `MOVE_THUNDER_WAVE` (`EFFECT_PARALYZE`).
const THUNDER_WAVE: MoveId = MoveId(86);

/// `SPECIES_RATTATA`: base Speed 72, the fast mover in every fixture below.
const RATTATA: u16 = 19;
/// `SPECIES_BULBASAUR`: the enemy fixture `move_resolution.rs`'s own
/// forced-Struggle test survives a Rattata Tackle from, reused here so this
/// module does not need to re-derive that damage figure.
const BULBASAUR: u16 = 1;
/// `SPECIES_GASTLY`: Ghost/Poison, immune to the Normal-type Tackle used to
/// keep a target alive without hand-computing damage.
const GASTLY: u16 = 92;
/// `SPECIES_ZIGZAGOON`: an ordinary Normal-type target for Thunder Wave.
const ZIGZAGOON: u16 = 288;
/// `SPECIES_SANDSHREW`: pure Ground, immune to Thunder Wave's Electric type.
const SANDSHREW: u16 = 27;

#[test]
fn full_paralysis_cancels_before_the_no_pp_abort_and_retains_pp() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE, MoveId(45)]); // Tackle, Growl
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    enemy.set_status1(Status1::Paralysed);
    assert_eq!(enemy.moves()[0].pp, 0, "fixture sanity: slot 0 has no PP");

    // battle-start turn number, the turn's own turn number, the enemy's
    // rejection-loop pick (0 -> slot 0, the drained Tackle, since the loop
    // ignores PP), the player's immune Tackle (4 draws), the enemy's
    // full-paralysis draw (residue 0 -> cancelled).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::FullyParalyzed {
                by_player: false,
                move_id: TACKLE,
            },
        ],
        "the drained slot must report FullyParalyzed, never FailedNoPp -- \
         the canceller draw precedes the no-PP abort: {events:?}"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "cancellation never reaches ppreduce"
    );
    assert_eq!(rng.draws(), 8);
}

#[test]
fn a_fully_paralysed_mover_emits_no_move_event_and_keeps_its_pp() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    player.set_status1(Status1::Paralysed);
    let starting_pp = player.moves()[0].pp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the enemy's
    // (only) selection, the enemy's immune Tackle into the Ghost player (4
    // draws), the player's full-paralysis draw (residue 0 -> cancelled).
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
            BattleEvent::FullyParalyzed {
                by_player: true,
                move_id: TACKLE,
            },
        ]
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        starting_pp,
        "a cancelled move never spends PP"
    );
}

#[test]
fn paralysis_quarters_speed_and_removes_a_would_be_tie() {
    let dex = Dex::new();
    // A mirror match ties on raw Speed (module docs, `turn_engine/turn_order.rs`).
    // Paralysing the player breaks the tie in the *enemy's* favor without any
    // tie-break draw, proving turn order reads the quartered speed and not
    // the stage-scaled one.
    let mut player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    player.set_status1(Status1::Paralysed);
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the enemy's ordinary Tackle (4 draws: this mirror match
    // survives it, `turn_engine/turn_order.rs`'s own fixture), the player's
    // full-paralysis draw (residue 1 -> proceeds normally), then the
    // player's own ordinary Tackle (4 more draws).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        matches!(
            events[0],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "the enemy, no longer tied, must act first: {events:?}"
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
        "residue 1 (not 0 mod 4) lets the paralysed player act second: {events:?}"
    );
    assert_eq!(
        rng.draws(),
        12,
        "no tie-break draw: quartering already separated the two speeds"
    );
}

#[test]
fn thunder_wave_inflicts_paralysis_which_then_gates_the_same_turns_later_mover() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![THUNDER_WAVE]);
    let thunder_wave_pp = dex.move_data(THUNDER_WAVE).unwrap().pp;
    let tackle_pp = dex.move_data(TACKLE).unwrap().pp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's Thunder Wave (one accuracy draw -- the type
    // and already-paralysed guards draw nothing), the enemy's own
    // full-paralysis draw against the status the player's move just wrote
    // (residue 0 -> cancelled).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Paralyzed {
                by_player: true,
                move_id: THUNDER_WAVE,
            },
            BattleEvent::FullyParalyzed {
                by_player: false,
                move_id: TACKLE,
            },
        ],
        "the paralysis this turn's earlier move wrote gates the later mover \
         in the very same turn: {events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Paralysed);
    assert_eq!(battle.player().moves()[0].pp, thunder_wave_pp - 1);
    assert_eq!(
        battle.enemy().moves()[0].pp,
        tackle_pp,
        "the cancelled Tackle never reached ppreduce"
    );
}

#[test]
fn a_type_immune_thunder_wave_still_spends_pp_unlike_a_cancelled_move() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![THUNDER_WAVE]);
    let thunder_wave_pp = dex.move_data(THUNDER_WAVE).unwrap().pp;
    let enemy = max_iv_mon(&dex, SANDSHREW, 5, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's immune Thunder Wave (0 draws: the type guard
    // precedes accuracy), the enemy's ordinary Tackle (4 draws).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::ButItFailed {
            by_player: true,
            move_id: THUNDER_WAVE,
        }
    );
    assert!(matches!(
        events[1],
        BattleEvent::Hit {
            by_player: false,
            move_id: TACKLE,
            ..
        }
    ));
    assert_eq!(
        battle.player().moves()[0].pp,
        thunder_wave_pp - 1,
        "ppreduce runs before typecalc's immunity verdict, unlike a \
         full-paralysis cancellation, which never reaches ppreduce"
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert_eq!(rng.draws(), 7);
}

#[test]
fn a_paralysed_forced_struggle_enemy_is_cancelled_before_the_struggle_error() {
    // Struggle still shares `BattleScript_HitFromAtkCanceler` with every
    // ordinary move (`data/battle_scripts_1.s:241`-`:247`), so a paralysed
    // enemy forced into it draws the same full-paralysis check first --
    // this crate's honest "cannot execute Struggle" stop only applies past
    // that gate, not before it.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, BULBASAUR, 5, vec![TACKLE]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    enemy.set_status1(Status1::Paralysed);

    // battle-start turn number, the turn's own turn number (no selection
    // draw: every enemy slot is spent, so the rejection loop short-circuits
    // to Struggle without drawing), the player's ordinary Tackle (4 draws;
    // this exact fixture is `move_resolution.rs`'s own forced-Struggle
    // pairing, so it is known to survive), the enemy's full-paralysis draw
    // against its forced Struggle (residue 0 -> cancelled).
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 0, 0]);
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
        "the player's earlier hit still committed: {events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::FullyParalyzed {
            by_player: false,
            move_id: STRUGGLE,
        },
        "cancelled by its own paralysis draw, not upstream's unmodelled \
         Struggle mechanics: {events:?}"
    );
    assert_eq!(rng.draws(), 7);
    assert!(
        battle.outcome().is_none(),
        "a full-paralysis cancellation is not an error: the turn completes \
         normally, unlike an uncancelled forced Struggle"
    );
}
