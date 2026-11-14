//! The infatuation coin at `CANCELER_IN_LOVE`: after sleep, freeze, Truant,
//! confusion and paralysis, before the thaw and PP handling.
//!
//! The infatuated player is the slower mover, so the opponent has already
//! acted when the coin is drawn and a cancelled move's record is the last
//! write to the opponent's history.

use super::*;
use assets::AbilityId;
use battle::battle::LastLandedMove;
use battle::Status1;

const FLAME_WHEEL: MoveId = MoveId::FLAME_WHEEL;

/// `Random() % 5 == 0` thaws; any other residue keeps a battler frozen.
const THAWS: u16 = 0;
const STAYS_FROZEN: u16 = 1;
/// `Random() % 4 == 0` is full paralysis.
const FULL_PARALYSIS: u16 = 0;
const PARALYSIS_AVOIDED: u16 = 1;
const CONFUSION_LETS_MOVE_THROUGH: u16 = 1;
const CONFUSION_SELF_HIT: u16 = 0;
const SELF_HIT_VARIANCE: u16 = 0;
/// A landing Flame Wheel: accuracy, non-critical, damage roll, and a
/// secondary-effect draw that does not burn.
const FLAME_WHEEL_HIT: [u16; 4] = [0, 1, 0, 99];

fn infatuated_player(moves: &[MoveId]) -> BattlePokemon {
    infatuated_by(mon(ZIGZAGOON, MALE, moves), BattlerSide::Enemy)
}

fn opponent() -> BattlePokemon {
    mon(RATTATA, FEMALE, &[SPLASH])
}

fn run(player: BattlePokemon, groups: &[&[u16]]) -> (Battle, Vec<BattleEvent>) {
    let (battle, events, rng) = wild_turn(player, opponent(), groups);
    rng.assert_exhausted();
    (battle, events)
}

fn has_in_love(events: &[BattleEvent]) -> bool {
    events.contains(&in_love(true))
}

fn tackle_pp(battle: &Battle) -> u8 {
    battle.player().moves()[0].pp
}

const TACKLE_PP: u8 = 35;

#[test]
fn an_even_coin_cancels_the_move_before_pp_and_any_move_draw() {
    for coin in [0, 2] {
        let (battle, events) = run(infatuated_player(&[TACKLE, SPLASH]), &[&PREFIX, &[coin]]);

        let love = position(&events, &in_love(true));
        let cancelled = position(&events, &immobilized(true, TACKLE));
        assert!(love < cancelled, "{events:?}");
        assert!(!events.contains(&used_move(true, TACKLE)), "{events:?}");
        assert_eq!(tackle_pp(&battle), TACKLE_PP);
        assert_eq!(
            battle.battler_state(BattlerSide::Enemy).last_landed_move(),
            LastLandedMove::Unavailable
        );
        assert!(
            battle.player().volatiles().infatuated(),
            "infatuation has no duration"
        );
    }
}

#[test]
fn an_odd_coin_announces_the_infatuation_and_lets_the_move_through() {
    for coin in [1, 3] {
        let (battle, events) = run(infatuated_player(&[SPLASH, TACKLE]), &[&PREFIX, &[coin]]);

        let love = position(&events, &in_love(true));
        let used = position(&events, &used_move(true, SPLASH));
        assert!(love < used, "{events:?}");
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, BattleEvent::ImmobilizedByLove { .. })),
            "{events:?}"
        );
        assert_eq!(battle.player().moves()[0].pp, 39, "Splash's PP is spent");
    }
}

#[test]
fn a_battler_that_is_not_infatuated_draws_no_coin() {
    let (_, events) = run(mon(ZIGZAGOON, MALE, &[SPLASH]), &[&PREFIX]);

    assert!(!has_in_love(&events), "{events:?}");
}

#[test]
fn sleep_cancels_first_with_no_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.set_status1(Status1::Asleep { turns: 2 });

    let (battle, events) = run(player, &[&PREFIX]);

    assert!(
        events.contains(&BattleEvent::FastAsleep {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
    assert!(!has_in_love(&events), "{events:?}");
    assert_eq!(tackle_pp(&battle), TACKLE_PP);
}

#[test]
fn a_battler_that_just_woke_reaches_the_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.set_status1(Status1::Asleep { turns: 1 });

    let (_, events) = run(player, &[&PREFIX, &[LOVE_IMMOBILIZES]]);

    let woke = position(&events, &BattleEvent::WokeUp { by_player: true });
    let love = position(&events, &in_love(true));
    assert!(woke < love, "{events:?}");
    assert!(events.contains(&immobilized(true, TACKLE)), "{events:?}");
}

#[test]
fn a_frozen_battler_that_stays_frozen_never_reaches_the_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.set_status1(Status1::Frozen);

    let (_, events) = run(player, &[&PREFIX, &[STAYS_FROZEN]]);

    assert!(
        events.contains(&BattleEvent::FrozenSolid {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
    assert!(!has_in_love(&events), "{events:?}");
}

#[test]
fn a_battler_that_thaws_reaches_the_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.set_status1(Status1::Frozen);

    let (battle, events) = run(player, &[&PREFIX, &[THAWS], &[LOVE_IMMOBILIZES]]);

    let thawed = position(&events, &BattleEvent::Thawed { by_player: true });
    let love = position(&events, &in_love(true));
    assert!(thawed < love, "{events:?}");
    assert!(events.contains(&immobilized(true, TACKLE)), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
}

#[test]
fn a_thaw_hit_move_stays_frozen_when_the_coin_cancels_it() {
    let mut player = infatuated_player(&[FLAME_WHEEL]);
    player.set_status1(Status1::Frozen);

    let (battle, events) = run(player, &[&PREFIX, &[STAYS_FROZEN], &[LOVE_IMMOBILIZES]]);

    assert!(
        events.contains(&immobilized(true, FLAME_WHEEL)),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ThawedByMove { .. })),
        "{events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Frozen);
    assert_eq!(tackle_pp(&battle), 25, "Flame Wheel keeps its PP");
}

#[test]
fn a_thaw_hit_move_thaws_after_the_coin_lets_it_through() {
    let mut player = infatuated_player(&[FLAME_WHEEL]);
    player.set_status1(Status1::Frozen);

    let (battle, events) = run(
        player,
        &[
            &PREFIX,
            &[STAYS_FROZEN],
            &[LOVE_LETS_MOVE_THROUGH],
            &FLAME_WHEEL_HIT,
        ],
    );

    let love = position(&events, &in_love(true));
    let thaw = position(
        &events,
        &BattleEvent::ThawedByMove {
            by_player: true,
            move_id: FLAME_WHEEL,
        },
    );
    let used = position(&events, &used_move(true, FLAME_WHEEL));
    assert!(love < thaw && thaw < used, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
}

#[test]
fn a_loafing_truant_holder_never_reaches_the_coin() {
    let holder = infatuated_by(mon(SpeciesId::SLAKOTH, MALE, &[SPLASH]), BattlerSide::Enemy);
    assert_eq!(holder.ability(), AbilityId::TRUANT);
    let mut rng = SequenceRng::new(
        [
            &[0][..],
            // Turn one: the holder's counter is clear, so the coin is drawn.
            &[0, 0, LOVE_LETS_MOVE_THROUGH],
            // Turn two: the end-turn toggle set the counter, so it loafs
            // before the coin.
            &[0, 0],
        ]
        .concat(),
    );
    let mut battle = Battle::new(Dex::new(), holder, opponent(), false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(has_in_love(&first), "{first:?}");

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    rng.assert_exhausted();
    assert!(
        second.contains(&BattleEvent::LoafingAround {
            by_player: true,
            move_id: SPLASH,
        }),
        "{second:?}"
    );
    assert!(!has_in_love(&second), "{second:?}");
}

#[test]
fn a_confusion_self_hit_ends_the_action_before_the_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.volatiles_mut().set_confusion(3);

    let (_, events) = run(
        player,
        &[&PREFIX, &[CONFUSION_SELF_HIT], &[SELF_HIT_VARIANCE]],
    );

    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::ConfusionSelfHit {
                by_player: true,
                ..
            }
        )),
        "{events:?}"
    );
    assert!(!has_in_love(&events), "{events:?}");
}

#[test]
fn a_surviving_confusion_coin_precedes_the_infatuation_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.volatiles_mut().set_confusion(3);

    let (battle, events) = run(
        player,
        &[&PREFIX, &[CONFUSION_LETS_MOVE_THROUGH], &[LOVE_IMMOBILIZES]],
    );

    let confused = position(&events, &BattleEvent::Confused { by_player: true });
    let love = position(&events, &in_love(true));
    let cancelled = position(&events, &immobilized(true, TACKLE));
    assert!(confused < love && love < cancelled, "{events:?}");
    assert_eq!(battle.player().volatiles().confusion_turns, 2);
}

#[test]
fn snapping_out_of_confusion_draws_no_confusion_coin_before_the_infatuation_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.volatiles_mut().set_confusion(1);

    let (_, events) = run(player, &[&PREFIX, &[LOVE_IMMOBILIZES]]);

    let snapped = position(
        &events,
        &BattleEvent::SnappedOutOfConfusion { by_player: true },
    );
    let love = position(&events, &in_love(true));
    assert!(snapped < love, "{events:?}");
}

#[test]
fn full_paralysis_ends_the_action_before_the_coin() {
    let mut player = infatuated_player(&[TACKLE]);
    player.set_status1(Status1::Paralysed);

    let (_, events) = run(player, &[&PREFIX, &[FULL_PARALYSIS]]);

    assert!(
        events.contains(&BattleEvent::FullyParalyzed {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
    assert!(!has_in_love(&events), "{events:?}");
}

#[test]
fn the_confusion_paralysis_and_infatuation_coins_run_in_that_order() {
    let mut player = infatuated_player(&[TACKLE]);
    player.volatiles_mut().set_confusion(3);
    player.set_status1(Status1::Paralysed);

    let (battle, events) = run(
        player,
        &[
            &PREFIX,
            &[CONFUSION_LETS_MOVE_THROUGH],
            &[PARALYSIS_AVOIDED],
            &[LOVE_IMMOBILIZES],
        ],
    );

    let confused = position(&events, &BattleEvent::Confused { by_player: true });
    let love = position(&events, &in_love(true));
    assert!(confused < love, "{events:?}");
    assert!(events.contains(&immobilized(true, TACKLE)), "{events:?}");
    assert_eq!(tackle_pp(&battle), TACKLE_PP);
}

mod rollout {
    use super::*;

    const ROLLOUT: MoveId = MoveId::ROLLOUT;
    const WAILORD: SpeciesId = SpeciesId::WAILORD;
    /// High enough to survive the whole series.
    const TARGET_LEVEL: u8 = 100;
    /// A landed attempt: accuracy, critical-hit, variance and effect-chance
    /// draws.
    const LANDED: [u16; 4] = [89, 1, 0, 0];
    const OVERHEAD: [u16; 2] = [0, 0];

    fn wailord() -> BattlePokemon {
        crate::common::max_iv_mon_with_personality(
            &Dex::new(),
            WAILORD.0,
            TARGET_LEVEL,
            vec![SPLASH],
            FEMALE,
        )
    }

    fn lock(battle: &Battle) -> (Option<MoveId>, Option<u8>) {
        let state = battle.battler_state(BattlerSide::Player);
        (state.locked_move(), state.lock_attempts_left())
    }

    /// Starts a series under an infatuation whose first coin lets it through.
    fn series_started(
        second_coin: u16,
        groups: &[&[u16]],
    ) -> (Battle, Vec<BattleEvent>, SequenceRng) {
        let player = infatuated_player(&[ROLLOUT, SPLASH]);
        let mut rng = SequenceRng::new(
            [
                &[0][..],
                &OVERHEAD,
                &[LOVE_LETS_MOVE_THROUGH],
                &LANDED,
                &OVERHEAD,
                &[second_coin],
                &groups.concat(),
            ]
            .concat(),
        );
        let mut battle = Battle::new(Dex::new(), player, wailord(), false, &mut rng).unwrap();
        battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(lock(&battle), (Some(ROLLOUT), Some(4)));
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        (battle, events, rng)
    }

    #[test]
    fn an_even_coin_releases_the_series_without_a_continuation_attempt() {
        let (mut battle, events, mut rng) = series_started(
            LOVE_IMMOBILIZES,
            &[
                // Turn three: a free selection, the coin, and Splash.
                &OVERHEAD,
                &[LOVE_LETS_MOVE_THROUGH],
            ],
        );

        assert!(events.contains(&immobilized(true, ROLLOUT)), "{events:?}");
        assert_eq!(lock(&battle), (None, None));
        assert_eq!(
            battle.player().moves()[0].pp,
            19,
            "only the first attempt cost PP"
        );

        let events = battle
            .take_turn(PlayerAction::UseMove(1), &mut rng)
            .unwrap();
        rng.assert_exhausted();
        assert!(events.contains(&used_move(true, SPLASH)), "{events:?}");
    }

    #[test]
    fn an_odd_coin_advances_the_series() {
        let (battle, events, rng) = series_started(LOVE_LETS_MOVE_THROUGH, &[&LANDED]);

        rng.assert_exhausted();
        assert!(events.contains(&in_love(true)), "{events:?}");
        assert_eq!(lock(&battle), (Some(ROLLOUT), Some(3)));
    }
}
