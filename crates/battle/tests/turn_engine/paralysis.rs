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

use crate::common::{max_iv_mon, SequenceRng, MAX_IVS};
use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1, STRUGGLE};

/// `MOVE_TACKLE`.
const TACKLE: MoveId = MoveId(33);
/// `MOVE_THUNDER_WAVE` (`EFFECT_PARALYZE`).
const THUNDER_WAVE: MoveId = MoveId(86);
/// `MOVE_ABSORB` (`EFFECT_ABSORB`), the drain pipeline's own double-faint
/// fixture move (`turn_engine/pipelines.rs`).
const ABSORB: MoveId = MoveId(71);

/// `SPECIES_RATTATA`: base Speed 72, the fast mover in every fixture below.
const RATTATA: u16 = 19;
/// `SPECIES_CHARMANDER`, level 50 against level-2 Rattata: one-shots it, the
/// same overkill fixture `move_resolution.rs`'s own test uses.
const CHARMANDER: u16 = 4;
/// `SPECIES_BULBASAUR`: the enemy fixture `move_resolution.rs`'s own
/// forced-Struggle test survives a Rattata Tackle from, reused here so this
/// module does not need to re-derive that damage figure.
const BULBASAUR: u16 = 1;
/// `SPECIES_TENTACOOL`: Clear Body in ability slot 0, **Liquid Ooze** in slot
/// 1, so an odd personality (`turn_engine/pipelines.rs`'s own convention)
/// fields the ability whose recoil produces this module's double faint.
const TENTACOOL: u16 = 72;
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
        BattleEvent::NoEffect {
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

/// `data/battle_scripts_1.s:1013`-`:1014`: `typecalc` then
/// `jumpifmovehadnoeffect BattleScript_ButItFailed`. `typecalc`'s
/// `ModulateDmgByType(TYPE_MUL_NO_EFFECT)` sets `MOVE_RESULT_DOESNT_AFFECT_FOE`
/// (`src/battle_script_commands.c:1327`), and `BattleScript_ButItFailed`
/// (`:2058`-`:2061`) only *adds* `MOVE_RESULT_FAILED` before `resultmessage`.
/// With both bits set and `MOVE_RESULT_MISSED` clear, `Cmd_resultmessage`
/// falls to its `default:` arm and picks `STRINGID_ITDOESNTAFFECT`
/// (`src/battle_script_commands.c:2090`-`:2093`), not `STRINGID_BUTITFAILED`
/// -- the same verdict an ordinary type-immune hit already gets.
#[test]
fn a_type_immune_thunder_wave_reports_no_effect_not_a_plain_failure() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![THUNDER_WAVE]);
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
        BattleEvent::NoEffect {
            by_player: true,
            move_id: THUNDER_WAVE,
        },
        "Electric cannot touch a pure Ground target, so typecalc's \
         DOESNT_AFFECT_FOE decides the message: {events:?}"
    );
}

/// `Cmd_cleareffectsonfaint`'s `hp == 0` branch zeroes the corpse's
/// `status1` before `FaintClearSetData` runs
/// (`battle_script_commands.c:3063`-`:3077`), so a paralysed battler that
/// faints leaves battle healthy.
#[test]
fn a_faint_clears_the_corpse_primary_status() {
    let dex = Dex::new();
    // Level 50 Charmander one-shots a level 2 Rattata, so one turn reaches
    // the faint; the paralysed enemy never acts, so it never draws.
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);
    enemy.set_status1(Status1::Paralysed);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "fixture sanity: the enemy must faint this turn: {events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.enemy().status1(),
        Status1::Healthy,
        "faint settlement must zero the corpse's primary status"
    );
}

/// The double-faint arm of the drain pipeline (`execute_drain_move`) settles
/// both corpses through its own loop, not [`Battle::settle_faint`], so it
/// needs its own pin: a Liquid Ooze kill zeroes the recoiling *attacker's*
/// primary status exactly as it zeroes the *target's*.
#[test]
fn a_double_faint_clears_both_corpses_primary_status() {
    let dex = Dex::new();
    let mut player =
        BattlePokemon::new(&dex, SpeciesId(BULBASAUR), 50, MAX_IVS, 0, vec![ABSORB]).unwrap();
    let player_max_hp = player.stats().max_hp;
    // Leave the attacker on less HP than the Liquid Ooze recoil will take,
    // matching `turn_engine/pipelines.rs`'s own fixture exactly.
    player.apply_damage(player_max_hp - 6);
    player.set_status1(Status1::Paralysed);
    // Odd personality selects ability slot 1: Liquid Ooze, not Clear Body.
    let mut enemy =
        BattlePokemon::new(&dex, SpeciesId(TENTACOOL), 5, MAX_IVS, 1, vec![TACKLE]).unwrap();
    enemy.set_status1(Status1::Paralysed);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the paralysed *attacker's* own full-paralysis draw
    // (residue 1 -> proceeds -- `Battle::act` gates the user's own move
    // too, not only a defender), then Absorb's 3 draws (accuracy, crit,
    // damage roll -- drain has no trailing effect-chance draw).
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true })
            && events.contains(&BattleEvent::Fainted { by_player: false }),
        "fixture sanity: both sides must faint this turn: {events:?}"
    );
    assert_eq!(
        battle.player().status1(),
        Status1::Healthy,
        "the recoiling attacker's own corpse is cleared too"
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

/// The mirror of [`a_faint_clears_the_corpse_primary_status`]: nothing
/// clears paralysis just because the *other* battler went down. Upstream's
/// `Cmd_cleareffectsonfaint` only ever touches the fainted battler's own
/// `status1` (`battle_script_commands.c:3063`-`:3068`).
#[test]
fn a_surviving_paralysed_winner_keeps_its_status_after_the_opponent_faints() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.set_status1(Status1::Paralysed);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's full-paralysis draw (residue 1 -> proceeds),
    // the player's ordinary Tackle (4 draws) that one-shots the enemy.
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "fixture sanity: the enemy must faint this turn: {events:?}"
    );
    assert_eq!(
        battle.player().status1(),
        Status1::Paralysed,
        "the winner did not faint, so its own paralysis outlives the win"
    );
}
