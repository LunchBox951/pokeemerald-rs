//! Applying Leech Seed: accuracy, the Grass and reseed failures, and the
//! Liquid Ooze refusal.

use super::*;

#[test]
fn a_landed_seed_is_recorded_and_drains_in_the_same_turn() {
    let player = wounded(mon(RATTATA, &[LEECH_SEED]), 30);
    let enemy = mon(WURMPLE, &[SPLASH]);
    let tick = enemy.stats().max_hp / 8;

    let (battle, events, rng) = wild_turn(player, enemy, &seeding_draws(0));

    rng.assert_exhausted();
    assert_eq!(rng.draws(), 4, "construction, turn start, pick, accuracy");
    let seeded = position(
        &events,
        &BattleEvent::Seeded {
            by_player: true,
            move_id: LEECH_SEED,
        },
    );
    let sapped = position(
        &events,
        &BattleEvent::SappedByLeechSeed {
            by_player: false,
            damage: tick,
        },
    );
    let healed = position(
        &events,
        &BattleEvent::LeechSeedHealed {
            by_player: true,
            healed: tick,
        },
    );
    assert!(seeded < sapped && sapped + 1 == healed, "{events:?}");
    assert_eq!(
        battle.enemy().volatiles().leech_seed_source,
        Some(BattlerSide::Player)
    );
    assert_eq!(battle.player().moves()[0].pp, 9, "PP is spent");
    assert_eq!(
        battle.enemy().current_hp(),
        battle.enemy().stats().max_hp - tick
    );
    assert_eq!(
        battle.player().current_hp(),
        battle.player().stats().max_hp - 30 + tick
    );
}

#[test]
fn the_seed_connects_on_a_ninety_percent_roll_and_misses_on_the_next() {
    // Rolls are `Random() % 100 + 1`, so a raw 89 is 90 <= 90 and 90 is 91.
    let (hit, events, _) = wild_turn(
        mon(RATTATA, &[LEECH_SEED]),
        mon(WURMPLE, &[SPLASH]),
        &seeding_draws(89),
    );
    assert!(hit.enemy().volatiles().seeded(), "{events:?}");

    let (miss, events, rng) = wild_turn(
        mon(RATTATA, &[LEECH_SEED]),
        mon(WURMPLE, &[SPLASH]),
        &seeding_draws(90),
    );
    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::Missed {
        by_player: true,
        move_id: LEECH_SEED,
    }));
    assert!(!miss.enemy().volatiles().seeded());
    assert!(!has_sap(&events), "{events:?}");
    assert_eq!(miss.player().moves()[0].pp, 9);
}

#[test]
fn a_grass_type_in_either_slot_refuses_the_seed_after_the_accuracy_draw() {
    // The slower Wurmple seeds last, so the enemy's own Splash cannot
    // overwrite the landed-move record the failed seed leaves.
    for species in [SHROOMISH, LOTAD] {
        let (battle, events, rng) = wild_turn(
            mon(WURMPLE, &[LEECH_SEED]),
            mon(species, &[SPLASH]),
            &seeding_draws(0),
        );

        rng.assert_exhausted();
        assert!(
            events.contains(&BattleEvent::NoEffect {
                by_player: true,
                move_id: LEECH_SEED,
            }),
            "species {species}: {events:?}"
        );
        assert!(!battle.enemy().volatiles().seeded(), "species {species}");
        assert!(!has_sap(&events), "species {species}: {events:?}");
        assert_eq!(battle.player().moves()[0].pp, 9, "species {species}");
        assert_eq!(
            battle.battler_state(BattlerSide::Enemy).last_landed_move(),
            LastLandedMove::Unavailable,
            "`MOVE_RESULT_MISSED` marks the move as having no effect"
        );
    }
}

#[test]
fn a_reseed_fails_after_the_accuracy_draw_and_keeps_the_original_drain() {
    let enemy = seeded_by(mon(WURMPLE, &[SPLASH]), BattlerSide::Player);
    let tick = enemy.stats().max_hp / 8;

    let (battle, events, rng) = wild_turn(mon(RATTATA, &[LEECH_SEED]), enemy, &seeding_draws(0));

    rng.assert_exhausted();
    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: true,
            move_id: LEECH_SEED,
        }),
        "{events:?}"
    );
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::Seeded { .. })));
    assert_eq!(battle.player().moves()[0].pp, 9);
    assert!(events.contains(&BattleEvent::SappedByLeechSeed {
        by_player: false,
        damage: tick,
    }));
    assert_eq!(
        battle.enemy().volatiles().leech_seed_source,
        Some(BattlerSide::Player)
    );
}

#[test]
fn the_enemy_can_seed_the_player_and_the_drain_heals_the_enemy() {
    let mut rng = SequenceRng::new(TURN_OVERHEAD.into_iter().chain([0]));
    let player = mon(WURMPLE, &[SPLASH]);
    let enemy = wounded(mon(RATTATA, &[LEECH_SEED]), 40);
    let tick = player.stats().max_hp / 8;
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::Seeded {
        by_player: false,
        move_id: LEECH_SEED,
    }));
    assert!(events.contains(&BattleEvent::SappedByLeechSeed {
        by_player: true,
        damage: tick,
    }));
    assert!(events.contains(&BattleEvent::LeechSeedHealed {
        by_player: false,
        healed: tick,
    }));
    assert_eq!(
        battle.player().volatiles().leech_seed_source,
        Some(BattlerSide::Enemy)
    );
}

#[test]
fn liquid_ooze_and_a_seeded_ooze_holder_are_refused_before_any_draw() {
    let ooze = || mon(TENTACOOL, &[SPLASH]).with_ability_slot(LIQUID_OOZE_SLOT);
    assert_eq!(ooze().ability(), AbilityId::LIQUID_OOZE);
    let refusal = BattleError::UnportedAbilityInteraction(AbilityId::LIQUID_OOZE);

    // The player selecting Leech Seed against an Ooze holder.
    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(
        Dex::new(),
        mon(RATTATA, &[LEECH_SEED]),
        ooze(),
        false,
        &mut rng,
    )
    .unwrap();
    let error = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(error.error(), refusal);
    assert_eq!(rng.draws(), 1, "only construction drew");

    // An enemy that knows Leech Seed against an Ooze player.
    let mut rng = SequenceRng::new([]);
    let constructed = Battle::new(
        Dex::new(),
        ooze(),
        mon(RATTATA, &[LEECH_SEED]),
        false,
        &mut rng,
    );
    assert_eq!(constructed.err(), Some(refusal));
    assert_eq!(rng.draws(), 0);

    // An Ooze holder that already carries a seed.
    let mut rng = SequenceRng::new([]);
    let seeded = seeded_by(ooze(), BattlerSide::Enemy);
    let constructed = Battle::new(Dex::new(), seeded, mon(RATTATA, &[SPLASH]), false, &mut rng);
    assert_eq!(constructed.err(), Some(refusal));
    assert_eq!(rng.draws(), 0);
}
