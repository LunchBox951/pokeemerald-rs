//! The attacker's Synchronize reflecting an Effect Spore poison or paralysis
//! back onto a living, unstatused holder (never sleep, never a statused or
//! fainted holder), and the event order of the status and its reflection.

use super::*;

#[test]
fn a_synchronize_attacker_reflects_effect_spore_poison_onto_a_living_holder() {
    let dex = Dex::new();
    let (battle, events) = rattata_hits_surviving_holder(synchronizer(&dex), &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Poisoned, "{events:?}");
    assert!(
        position(&events, &POISONED_EVENT)
            < position(
                &events,
                &BattleEvent::PoisonedByAttackerSynchronize {
                    by_player: true,
                    move_id: TACKLE,
                }
            ),
        "{events:?}"
    );
}

#[test]
fn a_synchronize_attacker_reflects_effect_spore_paralysis_onto_a_living_holder() {
    let dex = Dex::new();
    let (battle, events) = rattata_hits_surviving_holder(
        synchronizer(&dex),
        &script(&[&TRIGGER_PARALYSIS, &[FULL_PARALYSIS_AVOIDED]]),
    );

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Paralysed, "{events:?}");
    assert!(
        position(&events, &PARALYZED_EVENT)
            < position(
                &events,
                &BattleEvent::ParalyzedByAttackerSynchronize {
                    by_player: true,
                    move_id: TACKLE,
                }
            ),
        "{events:?}"
    );
}

#[test]
fn a_synchronize_attacker_never_reflects_effect_spore_sleep() {
    let dex = Dex::new();
    let (battle, events) = rattata_hits_surviving_holder(synchronizer(&dex), &TRIGGER_SLEEP);

    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy, "{events:?}");
}

#[test]
fn a_synchronize_attacker_does_not_reflect_onto_a_holder_that_already_has_a_status() {
    let dex = Dex::new();
    let attacker = synchronizer(&dex);
    let holder = with_status(shroomish(&dex, 5), Status1::Paralysed);
    let (battle, events) = take_player_move(
        attacker,
        holder,
        dex,
        &script(&[
            &FASTER_HITS,
            &TRIGGER_POISON,
            &[FULL_PARALYSIS_AVOIDED],
            &SLOWER_HITS,
        ]),
    );

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Paralysed, "{events:?}");
}

#[test]
fn a_synchronize_attacker_does_not_reflect_onto_a_holder_that_fainted_to_the_hit() {
    let dex = Dex::new();
    let attacker = synchronizer_at_level(&dex, 50);
    let holder = shroomish(&dex, 5);
    let (battle, events) = take_player_move(
        attacker,
        holder,
        dex,
        &script(&[&FASTER_HITS, &TRIGGER_POISON]),
    );

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::PoisonedByAttackerSynchronize { .. })),
        "{events:?}"
    );
}
