//! `Cmd_ppreduce` (`battle_script_commands.c:1205`-`:1237`): for single-target
//! moves, one PP is added to the cost only when the target is a different
//! battler with Pressure (`:1224`-`:1225`); the subtraction saturates at zero.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, Dex, PlayerAction};

const SCRATCH: MoveId = MoveId::SCRATCH;
/// Targets its user, so it never receives Pressure's extra cost.
const SWORDS_DANCE: MoveId = MoveId::SWORDS_DANCE;

const RATTATA: u16 = 19;
/// Ghost, so Scratch has no effect and only the PP spend is observable.
const DUSCLOPS: u16 = 362;
/// Faster than Dusclops. With Pressure on both sides, an implementation that
/// checks the opposing battler without exempting self-targeting moves would
/// double Swords Dance's cost; a one-sided fixture would miss that bug.
const ABSOL: u16 = 376;

/// `Battle::new`: the initial random-turn number
/// (`battle_main.c:3140`). One draw.
const INITIAL_TURN_SEED: [u16; 1] = [0];
/// Start of each turn: the random-turn number (`battle_main.c:3923` for the
/// first turn, `:4013` after) and the wild enemy's move-slot pick
/// (`battle_controller_opponent.c:1599`, the one draw outside the two
/// battle files). Two draws.
const TURN_SETUP: [u16; 2] = [0, 0];
/// Scratch into a Ghost: accuracy (`battle_script_commands.c:1176`),
/// critical roll (`:1282`), damage variance (`:1641`, drawn even though the
/// target is immune) and the secondary-effect chance (`:2923`). Four draws.
/// Swords Dance, turn order and Pressure's PP spend draw nothing.
const IMMUNE_SCRATCH: [u16; 4] = [0, 0, 0, 0];

#[test]
fn pressure_doubles_pp_cost_against_a_distinct_target_but_not_for_a_self_target() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, ABSOL, 5, vec![SCRATCH]);
    let enemy = max_iv_mon(&dex, DUSCLOPS, 5, vec![SWORDS_DANCE]);
    assert_eq!(
        player.ability(),
        assets::AbilityId::PRESSURE,
        "fixture sanity: Absol's only ability slot is Pressure"
    );
    assert_eq!(
        enemy.ability(),
        assets::AbilityId::PRESSURE,
        "fixture sanity: Dusclops' only ability slot is Pressure"
    );
    let scratch_pp = player.moves()[0].pp;
    let swords_dance_pp = enemy.moves()[0].pp;

    // Exactly the turn's draws; Pressure itself must consume none.
    let mut rng = SequenceRng::new(
        INITIAL_TURN_SEED
            .into_iter()
            .chain(TURN_SETUP)
            .chain(IMMUNE_SCRATCH),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "construction draws only the turn seed");
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: SCRATCH,
        }),
        "fixture sanity: Scratch's Normal typing cannot touch a Ghost: {events:?}"
    );
    assert_eq!(
        rng.draws(),
        7,
        "turn seed + enemy slot pick + Scratch's four draws; Pressure draws none"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        scratch_pp - 2,
        "a single-target move against a distinct Pressure holder spends two PP"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        swords_dance_pp - 1,
        "a self-targeting move never receives Pressure's increment, even \
         though the user's own opponent also carries Pressure"
    );
}

#[test]
fn two_turns_against_a_pressure_holder_drain_a_three_pp_slot_to_zero_not_one() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 5, vec![SCRATCH]);
    for _ in 0..player.moves()[0].pp - 3 {
        player.deduct_pp(0).unwrap();
    }
    assert_eq!(player.moves()[0].pp, 3, "fixture sanity: three PP left");
    let enemy = max_iv_mon(&dex, DUSCLOPS, 5, vec![SWORDS_DANCE]);

    // Construction, then two identical turns (setup + immune Scratch each).
    let mut rng = SequenceRng::new(
        INITIAL_TURN_SEED
            .into_iter()
            .chain(TURN_SETUP)
            .chain(IMMUNE_SCRATCH)
            .chain(TURN_SETUP)
            .chain(IMMUNE_SCRATCH),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "construction draws only the turn seed");

    let first_turn = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("the first two-PP cost leaves one PP, no error");
    assert!(
        first_turn.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: SCRATCH,
        }),
        "{first_turn:?}"
    );
    assert_eq!(rng.draws(), 7, "first turn: seed + slot pick + Scratch x4");
    assert_eq!(
        battle.player().moves()[0].pp,
        1,
        "three PP minus a two-PP Pressure cost leaves one"
    );

    let second_turn = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("a two-PP cost against a one-PP slot saturates rather than erroring");
    assert!(
        second_turn.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: SCRATCH,
        }),
        "{second_turn:?}"
    );
    assert_eq!(rng.draws(), 13, "second turn draws the same six values");
    assert_eq!(
        battle.player().moves()[0].pp,
        0,
        "a two-PP cost against a one-PP slot saturates at zero \
         (`battle_script_commands.c:1234`-`:1237`)"
    );
}
