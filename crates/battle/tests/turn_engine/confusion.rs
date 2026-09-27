//! Confusion's duration-bearing volatile, driven through real turns.
//!
//! Unit-level decrement and expiry shapes are pinned inside
//! `battle::volatile`'s own tests. What is pinned **here** is the wiring only
//! a turn can show: that the decrement runs before the full-paralysis draw,
//! that it consumes no RNG of its own, and that it applies independently to
//! each battler's own action. No move in this crate can yet write confusion,
//! so every fixture below sets it directly through
//! [`battle::BattlePokemon::volatiles_mut`].

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, BattleOutcome, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId(33);

/// `SPECIES_CHARMANDER`, level 50 against level-2 Rattata: one-shots it, the
/// same overkill fixture `turn_engine/move_resolution.rs`'s own test uses.
const CHARMANDER: u16 = 4;
/// `SPECIES_RATTATA`: base Speed 72, the fast mover in every fixture below.
const RATTATA: u16 = 19;
/// `SPECIES_GASTLY`: Ghost/Poison, immune to the Normal-type Tackle used to
/// keep a target alive without hand-computing damage.
const GASTLY: u16 = 92;
/// `SPECIES_ZIGZAGOON`: pure Normal, an ordinary Tackle target and user.
const ZIGZAGOON: u16 = 288;

#[test]
fn a_duration_one_confusion_snaps_out_before_the_mover_s_own_hit() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.volatiles_mut().set_confusion(1);
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, then the player's ordinary one-shot Tackle (4 draws): the
    // confusion decrement itself consumes nothing.
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

    // battle-start turn number, the turn's own turn number, the enemy's
    // (only) selection, the enemy's immune Tackle into the Ghost player (4
    // draws), the player's full-paralysis draw (residue 0 -> cancelled) --
    // the same shape as `turn_engine/paralysis.rs`'s own fixture, since the
    // confusion decrement between them draws nothing.
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

    // battle-start turn number, the turn's own turn number, the enemy's
    // selection, the player's ordinary Tackle (4 draws: Rattata's higher
    // Speed acts first), the enemy's ordinary Tackle (4 more draws) -- the
    // enemy's own confusion decrement between them draws nothing.
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

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::SnappedOutOfConfusion { .. })),
        "one turn of a two-turn duration must not report the terminal event: {events:?}"
    );
    assert!(battle.player().volatiles().confused());
    assert_eq!(
        battle.player().volatiles().confusion_turns,
        1,
        "the counter still advances toward its own expiry"
    );
}
