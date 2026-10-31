//! The Effect Spore chance roll (`Random() % 10`), the `Random() & 3` status
//! selector with its zero redraw, the sleep-duration draw, and the status
//! landing on the right attacker (either side) and ordering against the
//! holder's faint.

use super::*;

#[test]
fn a_failed_chance_draws_once_and_leaves_the_attacker_healthy() {
    for failed_chance in [SPORE_MISSES, 9, 65_535] {
        let dex = Dex::new();
        let attacker = rattata(&dex, 5, vec![TACKLE]);
        let (battle, events) = rattata_hits_surviving_holder(attacker, &[failed_chance]);

        assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
        assert!(effect_spore_events(&events).is_empty(), "{events:?}");
    }
}

#[test]
fn the_chance_is_a_modulo_ten_roll_rather_than_a_raw_zero() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &[10, SELECTS_POISON]);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
}

#[test]
fn a_non_contact_hit_into_an_effect_spore_holder_draws_no_roll() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![WATER_GUN]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &[]);

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert!(effect_spore_events(&events).is_empty(), "{events:?}");
}

#[test]
fn a_selected_sleep_draws_its_two_to_five_turn_duration_after_the_selector() {
    for (duration_draw, turns) in [(0, 2), (1, 3), (2, 4), (3, 5)] {
        let dex = Dex::new();
        let attacker = rattata(&dex, 5, vec![TACKLE]);
        let contact = [SPORE_TRIGGERS, SELECTS_SLEEP, duration_draw];
        let (battle, events) = rattata_hits_surviving_holder(attacker, &contact);

        assert_eq!(
            battle.player().status1(),
            Status1::Asleep { turns },
            "{events:?}"
        );
        assert_eq!(effect_spore_events(&events), vec![&ASLEEP_EVENT]);
    }
}

#[test]
fn a_selected_poison_poisons_the_attacker() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &TRIGGER_POISON);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(effect_spore_events(&events), vec![&POISONED_EVENT]);
}

#[test]
fn a_selector_of_three_paralyses_the_attacker_instead_of_burning_it() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &TRIGGER_PARALYSIS);

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
    assert_eq!(effect_spore_events(&events), vec![&PARALYZED_EVENT]);
}

#[test]
fn the_selector_keeps_only_its_low_two_bits() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let (battle, events) = rattata_hits_surviving_holder(attacker, &[SPORE_TRIGGERS, 7]);

    assert_eq!(battle.player().status1(), Status1::Paralysed, "{events:?}");
}

#[test]
fn a_zero_selector_is_redrawn_until_a_nonzero_one_arrives() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 5, vec![TACKLE]);
    let contact = [
        SPORE_TRIGGERS,
        SELECTS_NOTHING,
        4,
        8,
        SELECTS_SLEEP,
        SLEEP_DURATION_DRAW_FOR_FIVE_TURNS,
    ];
    let (battle, events) = rattata_hits_surviving_holder(attacker, &contact);

    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
}

#[test]
fn each_status_lands_on_the_enemy_attacker_of_a_player_holder() {
    let enemy_event = |kind: fn(bool, MoveId) -> BattleEvent| kind(false, TACKLE);
    let outcomes = [
        (
            TRIGGER_SLEEP.to_vec(),
            Status1::Asleep { turns: 5 },
            enemy_event(|by_player, move_id| BattleEvent::AsleepByEffectSpore {
                by_player,
                move_id,
            }),
        ),
        (
            TRIGGER_POISON.to_vec(),
            Status1::Poisoned,
            enemy_event(|by_player, move_id| BattleEvent::PoisonedByEffectSpore {
                by_player,
                move_id,
            }),
        ),
        (
            TRIGGER_PARALYSIS.to_vec(),
            Status1::Paralysed,
            enemy_event(|by_player, move_id| BattleEvent::ParalyzedByEffectSpore {
                by_player,
                move_id,
            }),
        ),
    ];
    for (contact, expected_status, expected_event) in outcomes {
        let dex = Dex::new();
        let holder = shroomish(&dex, 5);
        let attacker = rattata(&dex, 5, vec![TACKLE]);
        // Equal levels leave the faster Rattata first on the enemy side too.
        let (battle, events) = take_player_move(
            holder,
            attacker,
            dex,
            &script(&[&FASTER_HITS, &contact, &SLOWER_HITS]),
        );

        assert_eq!(battle.enemy().status1(), expected_status, "{events:?}");
        assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
        assert_eq!(effect_spore_events(&events), vec![&expected_event]);
    }
}

#[test]
fn a_holder_that_faints_to_the_hit_still_rolls_after_its_faint_settles() {
    let dex = Dex::new();
    let attacker = rattata(&dex, 50, vec![TACKLE]);
    let holder = shroomish(&dex, 5);
    let (battle, events) = take_player_move(
        attacker,
        holder,
        dex,
        &script(&[&FASTER_HITS, &TRIGGER_SLEEP]),
    );

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 5 },
        "{events:?}"
    );
    assert!(
        position(&events, &BattleEvent::Fainted { by_player: false })
            < position(&events, &ASLEEP_EVENT),
        "{events:?}"
    );
}
