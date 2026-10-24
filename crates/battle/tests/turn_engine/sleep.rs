//! [`Status1::Asleep`] and the sleep canceller ahead of the confusion and
//! full-paralysis cancellers, driven through complete turns.
//!
//! Unit-level shapes (the duration draw, the decrement, the infliction
//! guards) are pinned inside `battle::status1`. Pinned **here** is the wiring
//! only a turn can show: that a remaining duration cancels the move before
//! any other canceller draws or PP is touched, that Early Bird's two-turn
//! decrement shortens the sleep, and that an expired sleep lets the same
//! action go on to act.
//!
//! Every fixture pits a sleeper (Gastly, or Kangaskhan for Early Bird) against
//! a slower level-4 Gastly that sleeps through the whole test, so the only
//! draws a script holds are setup, selection, and the sleeper's own Tackle
//! into a Ghost.

use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
/// `SPECIES_KANGASKHAN`: Early Bird in slot 0, no second ability.
const KANGASKHAN: u16 = 115;
/// `SPECIES_GASTLY`: Ghost/Poison, immune to Tackle, and slower than
/// Kangaskhan.
const GASTLY: u16 = 92;

/// The battle-start draw.
const SETUP: [u16; 1] = [0];
/// One turn's number draw and the enemy's selection draw, which a sleeping
/// enemy still makes.
const TURN: [u16; 2] = [0, 0];
/// One Tackle into an immune Ghost: four draws.
const IMMUNE_TACKLE: [u16; 4] = [0, 1, 0, 0];

fn sleeper(dex: &Dex, species: u16, turns: u8) -> BattlePokemon {
    let mut mon = max_iv_mon_with_personality(dex, species, 5, vec![TACKLE], 0);
    mon.set_status1(Status1::Asleep { turns });
    mon
}

fn sleeping_gastly(dex: &Dex) -> BattlePokemon {
    let mut mon = max_iv_mon_with_personality(dex, GASTLY, 4, vec![TACKLE], 0);
    mon.set_status1(Status1::Asleep { turns: 5 });
    mon
}

fn fast_asleep(by_player: bool) -> BattleEvent {
    BattleEvent::FastAsleep {
        by_player,
        move_id: TACKLE,
    }
}

#[test]
fn a_remaining_sleep_duration_cancels_the_move_without_spending_pp_or_a_draw() {
    let dex = Dex::new();
    let player = sleeper(&dex, GASTLY, 3);
    let starting_pp = player.moves()[0].pp;
    let enemy = sleeping_gastly(&dex);

    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(events, vec![fast_asleep(true), fast_asleep(false)]);
    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 2 });
    assert_eq!(battle.enemy().status1(), Status1::Asleep { turns: 4 });
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
    rng.assert_exhausted();
}

#[test]
fn a_normal_sleeper_wakes_on_the_action_that_exhausts_its_duration_and_then_acts() {
    let dex = Dex::new();
    let player = sleeper(&dex, GASTLY, 2);
    let starting_pp = player.moves()[0].pp;
    let enemy = sleeping_gastly(&dex);

    let mut rng = SequenceRng::new([&SETUP[..], &TURN, &TURN, &IMMUNE_TACKLE].concat());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first, vec![fast_asleep(true), fast_asleep(false)]);
    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 1 });

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        second,
        vec![
            BattleEvent::WokeUp { by_player: true },
            BattleEvent::UsedMove {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
            fast_asleep(false),
        ],
        "expiry lets the selected move proceed in the same action: {second:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);
    rng.assert_exhausted();
}

#[test]
fn early_bird_loses_two_turns_per_action_and_wakes_sooner() {
    let dex = Dex::new();
    let player = sleeper(&dex, KANGASKHAN, 3);
    let enemy = sleeping_gastly(&dex);

    let mut rng = SequenceRng::new([&SETUP[..], &TURN, &TURN, &IMMUNE_TACKLE].concat());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first, vec![fast_asleep(true), fast_asleep(false)]);
    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 1 },
        "Early Bird subtracts two"
    );
    assert_eq!(
        battle.enemy().status1(),
        Status1::Asleep { turns: 4 },
        "the Gastly without Early Bird subtracts one"
    );

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        second[0],
        BattleEvent::WokeUp { by_player: true },
        "one turn left is below the two-turn decrement: {second:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    rng.assert_exhausted();
}

#[test]
fn sleep_cancels_ahead_of_confusion_and_full_paralysis_draws() {
    let dex = Dex::new();
    let mut player = sleeper(&dex, GASTLY, 3);
    player.volatiles_mut().set_confusion(3);
    let enemy = sleeping_gastly(&dex);

    // Setup and selection only: neither confusion's coin nor a paralysis
    // draw may run while the move is cancelled for sleep.
    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(events, vec![fast_asleep(true), fast_asleep(false)]);
    assert!(
        battle.player().volatiles().confused(),
        "a cancelled action never decrements confusion"
    );
    rng.assert_exhausted();
}

#[test]
fn a_waking_battler_goes_on_to_tick_its_confusion_in_the_same_action() {
    let dex = Dex::new();
    let mut player = sleeper(&dex, GASTLY, 1);
    player.volatiles_mut().set_confusion(1);
    let enemy = sleeping_gastly(&dex);

    let mut rng = SequenceRng::new([&SETUP[..], &TURN, &IMMUNE_TACKLE].concat());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::WokeUp { by_player: true },
            BattleEvent::SnappedOutOfConfusion { by_player: true },
            BattleEvent::UsedMove {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
            fast_asleep(false),
        ]
    );
    rng.assert_exhausted();
}

#[test]
fn a_sleeper_that_can_wake_mid_turn_keeps_the_serene_grace_poison_screen_closed() {
    // A faster sleeper wakes on its own action, so by the time Poison Sting
    // runs its target can be poisoned and the unported doubled chance would
    // apply. Admission must refuse before any draw, for a normal sleeper
    // about to expire and for an Early Bird sleeper alike. It stays
    // conservative for a longer sleeper too: the screen reads no duration.
    for (species, turns) in [
        (SpeciesId::ZIGZAGOON, 1),
        (SpeciesId::KANGASKHAN, 2),
        (SpeciesId::ZIGZAGOON, 5),
    ] {
        let dex = Dex::new();
        let mut player = max_iv_mon(&dex, species.0, 10, vec![TACKLE]);
        player.set_status1(Status1::Asleep { turns });
        let enemy = max_iv_mon(&dex, SpeciesId::DUNSPARCE.0, 10, vec![MoveId::POISON_STING]);
        assert_eq!(enemy.ability(), AbilityId::SERENE_GRACE);

        let mut rng = SequenceRng::new([]);
        let refusal = Battle::new(dex, player, enemy, false, &mut rng).err();
        assert_eq!(
            refusal,
            Some(BattleError::UnportedAbilityInteraction(
                AbilityId::SERENE_GRACE
            )),
            "{species:?} asleep for {turns}"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn a_zero_turn_sleep_field_is_a_healthy_battler_and_never_wakes() {
    // Upstream's sleep bits at zero are a healthy battler: `CANCELER_ASLEEP`
    // never fires, so no wake-up message precedes the move.
    let dex = Dex::new();
    let player = sleeper(&dex, GASTLY, 0);
    let enemy = sleeping_gastly(&dex);
    let mut rng = SequenceRng::new([&SETUP[..], &TURN, &IMMUNE_TACKLE].concat());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert!(
        !battle.player().status1().is_asleep(),
        "a zero sleep duration is not asleep"
    );
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        !events.contains(&BattleEvent::WokeUp { by_player: true }),
        "a battler with no sleep left has nothing to wake from: {events:?}"
    );
}
