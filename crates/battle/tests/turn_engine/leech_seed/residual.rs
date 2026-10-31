//! The `ENDTURN_LEECH_SEED` drain: amounts, caps, residual order, the absent
//! seeder, and clearing on faint.

use super::*;

/// The drain turn alone: both sides Splash, so the only draws are the turn's
/// overhead, and the drain itself adds none.
fn drain_turn(player: BattlePokemon, enemy: BattlePokemon) -> (Battle, Vec<BattleEvent>) {
    let (battle, events, rng) = wild_turn(player, enemy, &TURN_OVERHEAD);
    rng.assert_exhausted();
    assert_eq!(rng.draws(), 3);
    (battle, events)
}

#[test]
fn the_drain_is_an_eighth_of_the_seeded_max_hp_and_heals_the_same_amount() {
    let enemy = seeded_by(mon(WURMPLE, &[SPLASH]), BattlerSide::Player);
    let tick = leech_seed_residual_damage(enemy.stats().max_hp);
    assert_eq!(tick, enemy.stats().max_hp / 8);

    let (battle, events) = drain_turn(wounded(mon(RATTATA, &[SPLASH]), 30), enemy);

    assert!(events.contains(&BattleEvent::SappedByLeechSeed {
        by_player: false,
        damage: tick,
    }));
    assert!(events.contains(&BattleEvent::LeechSeedHealed {
        by_player: true,
        healed: tick,
    }));
    assert_eq!(
        battle.enemy().current_hp(),
        battle.enemy().stats().max_hp - tick
    );
    assert!(battle.enemy().volatiles().seeded(), "the seed has no timer");
}

#[test]
fn the_heal_is_capped_at_the_seeders_max_hp_but_the_victim_still_loses_the_full_tick() {
    for (missing, expected_heal) in [(3_u32, 3_u32), (0, 0)] {
        let enemy = seeded_by(mon(WURMPLE, &[SPLASH]), BattlerSide::Player);
        let tick = leech_seed_residual_damage(enemy.stats().max_hp);
        assert!(tick > expected_heal);

        let (battle, events) = drain_turn(wounded(mon(RATTATA, &[SPLASH]), missing), enemy);

        assert!(
            events.contains(&BattleEvent::SappedByLeechSeed {
                by_player: false,
                damage: tick,
            }),
            "{events:?}"
        );
        assert!(
            events.contains(&BattleEvent::LeechSeedHealed {
                by_player: true,
                healed: expected_heal,
            }),
            "missing {missing}: {events:?}"
        );
        assert_eq!(battle.player().current_hp(), battle.player().stats().max_hp);
    }
}

#[test]
fn a_victim_below_one_tick_loses_and_heals_only_its_remaining_hp_then_faints() {
    let enemy = with_hp(seeded_by(mon(WURMPLE, &[SPLASH]), BattlerSide::Player), 10);
    assert!(leech_seed_residual_damage(enemy.stats().max_hp) > 10);

    let (battle, events) = drain_turn(wounded(mon(RATTATA, &[SPLASH]), 30), enemy);

    let sapped = position(
        &events,
        &BattleEvent::SappedByLeechSeed {
            by_player: false,
            damage: 10,
        },
    );
    let healed = position(
        &events,
        &BattleEvent::LeechSeedHealed {
            by_player: true,
            healed: 10,
        },
    );
    let fainted = position(&events, &BattleEvent::Fainted { by_player: false });
    assert!(sapped < healed && healed < fainted, "{events:?}");
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(
        battle.enemy().volatiles().leech_seed_source,
        None,
        "faint clears the seed"
    );
}

#[test]
fn a_one_hp_victim_still_loses_the_minimum_of_one() {
    let enemy = seeded_by(mon(SHEDINJA, &[SPLASH]), BattlerSide::Player);
    assert_eq!(enemy.stats().max_hp, 1);
    assert_eq!(leech_seed_residual_damage(1), 1);

    let (_, events) = drain_turn(wounded(mon(RATTATA, &[SPLASH]), 5), enemy);

    assert!(events.contains(&BattleEvent::SappedByLeechSeed {
        by_player: false,
        damage: 1,
    }));
    assert!(events.contains(&BattleEvent::LeechSeedHealed {
        by_player: true,
        healed: 1,
    }));
}

#[test]
fn the_drain_precedes_the_poison_and_burn_ticks_of_the_same_battler() {
    for status in [Status1::Poisoned, Status1::Burned] {
        let mut enemy = with_hp(mon(WURMPLE, &[SPLASH]), 100);
        enemy.set_status1(status);
        let enemy = seeded_by(enemy, BattlerSide::Player);
        let max_hp = enemy.stats().max_hp;
        let seed_tick = leech_seed_residual_damage(max_hp);
        let status_tick = if status == Status1::Poisoned {
            poison_residual_damage(max_hp)
        } else {
            burn_residual_damage(max_hp)
        };

        let (battle, events) = drain_turn(mon(RATTATA, &[SPLASH]), enemy);

        let sapped = position(
            &events,
            &BattleEvent::SappedByLeechSeed {
                by_player: false,
                damage: seed_tick,
            },
        );
        let status_event = if status == Status1::Poisoned {
            BattleEvent::HurtByPoison {
                by_player: false,
                damage: status_tick,
            }
        } else {
            BattleEvent::HurtByBurn {
                by_player: false,
                damage: status_tick,
            }
        };
        assert!(sapped < position(&events, &status_event), "{events:?}");
        assert_eq!(battle.enemy().current_hp(), 100 - seed_tick - status_tick);
    }
}

#[test]
fn a_seed_that_leaves_less_than_a_status_tick_caps_that_tick_and_one_that_kills_skips_it() {
    // 20 HP: the seed takes a full tick (15 at 120 max HP), the poison tick
    // is capped at the 5 left.
    let mut enemy = with_hp(mon(WURMPLE, &[SPLASH]), 20);
    enemy.set_status1(Status1::Poisoned);
    let enemy = seeded_by(enemy, BattlerSide::Player);
    let seed_tick = leech_seed_residual_damage(enemy.stats().max_hp);
    let (battle, events) = drain_turn(mon(RATTATA, &[SPLASH]), enemy);
    assert!(events.contains(&BattleEvent::HurtByPoison {
        by_player: false,
        damage: 20 - seed_tick,
    }));
    assert_eq!(battle.enemy().current_hp(), 0);

    // 10 HP: the seed itself is lethal and no status tick is reported.
    let mut enemy = with_hp(mon(WURMPLE, &[SPLASH]), 10);
    enemy.set_status1(Status1::Burned);
    let enemy = seeded_by(enemy, BattlerSide::Player);
    let (_, events) = drain_turn(mon(RATTATA, &[SPLASH]), enemy);
    assert!(events.contains(&BattleEvent::SappedByLeechSeed {
        by_player: false,
        damage: 10,
    }));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::HurtByBurn { .. })),
        "{events:?}"
    );
}

#[test]
fn a_lethal_drain_does_not_cut_off_the_other_battlers_residuals() {
    // The faster seeded Rattata dies to the drain; `LeechSeedTurnDrain` has no
    // `checkteamslost`, so the slower poisoned Wurmple (healed by that drain)
    // still takes its own tick and dies too.
    let enemy = with_hp(seeded_by(mon(RATTATA, &[SPLASH]), BattlerSide::Player), 1);
    let mut player = with_hp(mon(WURMPLE, &[SPLASH]), 1);
    player.set_status1(Status1::Poisoned);

    let (battle, events, rng) = wild_turn(player, enemy, &TURN_OVERHEAD);

    rng.assert_exhausted();
    let ordered = [
        BattleEvent::SappedByLeechSeed {
            by_player: false,
            damage: 1,
        },
        BattleEvent::LeechSeedHealed {
            by_player: true,
            healed: 1,
        },
        BattleEvent::Fainted { by_player: false },
        BattleEvent::HurtByPoison {
            by_player: true,
            damage: 2,
        },
        BattleEvent::Fainted { by_player: true },
    ];
    let positions: Vec<usize> = ordered
        .iter()
        .map(|event| position(&events, event))
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
}

#[test]
fn an_absent_seeder_skips_the_drain_and_the_replacement_receives_it_later() {
    // The faster poisoned Rattata faints to its own tick before the seeded
    // Wurmple's pass, so the seeder's slot is empty for that drain. A
    // reserve keeps the battle going.
    let mut rattata = with_hp(mon(RATTATA, &[SPLASH]), 5);
    rattata.set_status1(Status1::Poisoned);
    let reserve = wounded(mon(ZIGZAGOON, &[SPLASH]), 30);
    let enemy = seeded_by(mon(WURMPLE, &[SPLASH]), BattlerSide::Player);
    let tick = leech_seed_residual_damage(enemy.stats().max_hp);

    let mut rng = SequenceRng::new(TURN_OVERHEAD);
    let mut battle = Battle::new_with_player_reserves(
        Dex::new(),
        rattata,
        vec![reserve],
        enemy,
        false,
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert!(events.contains(&BattleEvent::Fainted { by_player: true }));
    assert!(!has_sap(&events), "{events:?}");
    assert_eq!(battle.enemy().current_hp(), battle.enemy().stats().max_hp);
    assert_eq!(
        battle.enemy().volatiles().leech_seed_source,
        Some(BattlerSide::Player),
        "the seed outlives its seeder's faint"
    );
    assert_eq!(battle.player().species().0, ZIGZAGOON);

    let mut rng = SequenceRng::new([0, 0]);
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::SappedByLeechSeed {
        by_player: false,
        damage: tick,
    }));
    assert!(events.contains(&BattleEvent::LeechSeedHealed {
        by_player: true,
        healed: tick,
    }));
}

#[test]
fn a_seeded_battler_that_faints_hands_its_replacement_no_seed() {
    let player = with_hp(seeded_by(mon(RATTATA, &[SPLASH]), BattlerSide::Enemy), 1);
    let reserve = mon(ZIGZAGOON, &[SPLASH]);
    let enemy = mon(WURMPLE, &[SPLASH]);

    let mut rng = SequenceRng::new(TURN_OVERHEAD);
    let mut battle =
        Battle::new_with_player_reserves(Dex::new(), player, vec![reserve], enemy, false, &mut rng)
            .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(events.contains(&BattleEvent::SappedByLeechSeed {
        by_player: true,
        damage: 1,
    }));
    assert!(events.contains(&BattleEvent::Fainted { by_player: true }));
    assert_eq!(battle.player().species().0, ZIGZAGOON);
    assert!(!battle.player().volatiles().seeded());
}
