//! The power series, the start rules and PP.

use super::*;

/// A played series: the battle at its end, each attempt's damage and the
/// player's lock after it, and the RNG.
struct Series {
    battle: Battle,
    damages: Vec<u32>,
    locks: Vec<(Option<MoveId>, Option<u8>)>,
    rng: SequenceRng,
}

/// Plays `attempts` consecutive player Rollouts against a wild Wailord that
/// splashes, returning every hit's damage with the battle at the end.
fn play_series(player: BattlePokemon, attempts: usize) -> Series {
    let mut turns: Vec<&[u16]> = vec![&CONSTRUCTION];
    for _ in 0..attempts {
        turns.push(&WILD_TURN_OVERHEAD);
        turns.push(&LANDED);
    }
    let mut rng = SequenceRng::new(turn_script(&turns));
    let mut battle = wild_battle(player, target(WAILORD, &[SPLASH]), &mut rng);
    let mut damages = Vec::new();
    let mut locks = Vec::new();
    for _ in 0..attempts {
        let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);
        damages.push(player_hit_damage(&events));
        locks.push(player_lock(&battle));
    }
    Series {
        battle,
        damages,
        locks,
        rng,
    }
}

fn expected_damages(battle: &Battle, powers: [u32; 5]) -> Vec<u32> {
    let attack = battle.player().stats().attack;
    let defense = battle.enemy().stats().defense;
    powers
        .into_iter()
        .map(|power| neutral_damage(battle.player().level(), power, attack, defense))
        .collect()
}

#[test]
fn five_landed_attempts_deal_30_60_120_240_480_power_and_release_the_lock() {
    let Series {
        battle,
        damages,
        locks,
        rng,
    } = play_series(attacker(&[ROLLOUT, SPLASH]), 5);

    assert_eq!(
        damages,
        expected_damages(&battle, [30, 60, 120, 240, 480]),
        "the formula's powers"
    );
    assert!(
        damages.windows(2).all(|pair| pair[0] < pair[1]),
        "{damages:?}"
    );
    assert_eq!(
        locks,
        [
            (Some(ROLLOUT), Some(4)),
            (Some(ROLLOUT), Some(3)),
            (Some(ROLLOUT), Some(2)),
            (Some(ROLLOUT), Some(1)),
            (None, None),
        ]
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        ROLLOUT_PP - 1,
        "only the first attempt spends PP"
    );
    rng.assert_exhausted();
    assert_eq!(rng.draws(), 1 + 5 * 6, "construction plus six per turn");
}

#[test]
fn defense_curl_doubles_every_attempt_again() {
    // A level-30 attacker keeps the 960-power attempt inside Wailord's HP.
    let mut curled = max_iv_mon(&Dex::new(), RATTATA, 30, vec![ROLLOUT, SPLASH]);
    curled.volatiles_mut().set_defense_curl();
    let Series {
        battle,
        damages,
        rng,
        ..
    } = play_series(curled, 5);

    assert_eq!(damages, expected_damages(&battle, [60, 120, 240, 480, 960]));
    assert!(battle.player().volatiles().defense_curl, "the bit stays");
    rng.assert_exhausted();
}

#[test]
fn the_series_restarts_at_the_base_power_after_the_fifth_attempt() {
    let Series {
        mut battle,
        damages: first,
        ..
    } = play_series(attacker(&[ROLLOUT, SPLASH]), 5);
    let mut rng = SequenceRng::new(turn_script(&[&WILD_TURN_OVERHEAD, &LANDED]));

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert_eq!(player_hit_damage(&events), first[0]);
    assert_eq!(player_lock(&battle), (Some(ROLLOUT), Some(4)));
    assert_eq!(battle.player().moves()[0].pp, ROLLOUT_PP - 2);
    rng.assert_exhausted();
}

#[test]
fn a_first_attempt_that_misses_spends_pp_and_starts_no_series() {
    let mut rng = SequenceRng::new(turn_script(&[&CONSTRUCTION, &WILD_TURN_OVERHEAD, &MISSED]));
    let mut battle = wild_battle(
        attacker(&[ROLLOUT, SPLASH]),
        target(WAILORD, &[SPLASH]),
        &mut rng,
    );

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: true,
            move_id: ROLLOUT,
        }),
        "{events:?}"
    );
    assert_eq!(player_lock(&battle), (None, None));
    assert_eq!(battle.player().moves()[0].pp, ROLLOUT_PP - 1);
    rng.assert_exhausted();
    assert_eq!(rng.draws(), 1 + 2 + 1, "a miss draws only its accuracy");
}

#[test]
fn a_miss_after_a_landed_attempt_cancels_the_series_and_spends_no_pp() {
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &LANDED,
        &WILD_TURN_OVERHEAD,
        &MISSED,
        &WILD_TURN_OVERHEAD,
        &LANDED,
    ]));
    let mut battle = wild_battle(
        attacker(&[ROLLOUT, SPLASH]),
        target(WAILORD, &[SPLASH]),
        &mut rng,
    );
    let first = take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    assert_eq!(player_lock(&battle), (Some(ROLLOUT), Some(4)));

    let missed = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(missed.contains(&BattleEvent::Missed {
        by_player: true,
        move_id: ROLLOUT,
    }));
    assert_eq!(player_lock(&battle), (None, None));
    assert_eq!(battle.player().moves()[0].pp, ROLLOUT_PP - 1);

    // The next selection is free again and starts over at the base power.
    let restarted = take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    assert_eq!(player_hit_damage(&restarted), player_hit_damage(&first));
    assert_eq!(battle.player().moves()[0].pp, ROLLOUT_PP - 2);
    rng.assert_exhausted();
}

#[test]
fn locked_attempts_run_from_an_empty_slot_without_spending_pp() {
    let mut player = attacker(&[ROLLOUT, SPLASH]);
    for _ in 0..ROLLOUT_PP - 1 {
        player.deduct_pp(0).unwrap();
    }
    assert_eq!(player.moves()[0].pp, 1);
    let mut turns: Vec<&[u16]> = vec![&CONSTRUCTION];
    for _ in 0..5 {
        turns.push(&WILD_TURN_OVERHEAD);
        turns.push(&LANDED);
    }
    let mut rng = SequenceRng::new(turn_script(&turns));
    let mut battle = wild_battle(player, target(WAILORD, &[SPLASH]), &mut rng);

    for attempt in 1..=5 {
        let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, BattleEvent::FailedNoPp { .. })),
            "attempt {attempt}: {events:?}"
        );
        assert_eq!(battle.player().moves()[0].pp, 0, "attempt {attempt}");
    }
    assert_eq!(player_lock(&battle), (None, None));
    rng.assert_exhausted();
}

#[test]
fn pressure_doubles_only_the_first_attempts_pp() {
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &LANDED,
        &WILD_TURN_OVERHEAD,
        &LANDED,
    ]));
    let pressure_target = target(DUSCLOPS, &[SPLASH]);
    assert_eq!(pressure_target.ability(), assets::AbilityId::PRESSURE);
    let mut battle = wild_battle(attacker(&[ROLLOUT, SPLASH]), pressure_target, &mut rng);

    take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert_eq!(
        battle.player().moves()[0].pp,
        ROLLOUT_PP - 2,
        "Pressure's two PP once, the locked attempt none"
    );
    rng.assert_exhausted();
}

#[test]
fn ice_ball_uses_the_same_series() {
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &LANDED,
        &WILD_TURN_OVERHEAD,
        &LANDED,
    ]));
    let mut battle = wild_battle(
        attacker(&[ICE_BALL, SPLASH]),
        target(WAILORD, &[SPLASH]),
        &mut rng,
    );

    let first = player_hit_damage(&take(&mut battle, PlayerAction::UseMove(0), &mut rng));
    assert_eq!(player_lock(&battle), (Some(ICE_BALL), Some(4)));
    let second = player_hit_damage(&take(&mut battle, PlayerAction::UseMove(0), &mut rng));

    assert!(second > first);
    assert_eq!(player_lock(&battle), (Some(ICE_BALL), Some(3)));
}

#[test]
fn a_type_block_cancels_with_one_draw_and_no_damage_draws() {
    // Ice Ball into Shedinja is not strictly super effective: Wonder Guard
    // blocks it at `typecalc2`, ahead of the critical-hit and variance draws.
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &[ACCURACY_HITS],
    ]));
    let mut battle = wild_battle(
        attacker(&[ICE_BALL, SPLASH]),
        target(SHEDINJA, &[SPLASH]),
        &mut rng,
    );

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(
        events.contains(&BattleEvent::WonderGuardBlocked {
            by_player: true,
            move_id: ICE_BALL,
        }),
        "{events:?}"
    );
    assert_eq!(player_lock(&battle), (None, None));
    assert_eq!(battle.player().moves()[0].pp, 19);
    rng.assert_exhausted();
}

#[test]
fn rollout_is_super_effective_against_shedinja_and_not_blocked() {
    let mut rng = SequenceRng::new(turn_script(&[&CONSTRUCTION, &WILD_TURN_OVERHEAD, &LANDED]));
    let mut battle = wild_battle(
        attacker(&[ROLLOUT, SPLASH]),
        target(SHEDINJA, &[SPLASH]),
        &mut rng,
    );

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: true,
                move_id: ROLLOUT,
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(
        player_lock(&battle),
        (Some(ROLLOUT), Some(4)),
        "a fainted target does not release the attacker"
    );
}
