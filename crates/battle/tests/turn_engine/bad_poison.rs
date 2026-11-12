//! A badly poisoned battler: `ENDTURN_BAD_POISON`'s counter semantics, cap
//! and absence of RNG draws, and Shed Skin's cure of it. `Status1::BadPoisoned`
//! is `STATUS1_TOXIC_POISON | STATUS1_TOXIC_TURN(counter)`
//! (`pokeemerald/include/constants/battle.h:117`, `:123`-`:125`).
//!
//! The counter lives in the status word, but only the party's copy of that
//! word survives a battle: `ENDTURN_BAD_POISON` advances the battle copy and
//! emits no status write-back (`pokeemerald/src/battle_util.c:1536`-`:1549`),
//! so `BattlePokemon::status1` keeps the stored counter these tests start from.

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::status1::bad_poison_residual_damage;
use battle::{
    Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction, Stats, Status1,
};

const SPLASH: MoveId = MoveId::SPLASH;

/// `SPECIES_RATTATA`: faster than [`ZIGZAGOON`] at equal levels, so no order
/// tie draw is spent.
const RATTATA: u16 = 19;
/// `SPECIES_ZIGZAGOON`: no ability the residual pass reads.
const ZIGZAGOON: u16 = 288;
/// `SPECIES_SEVIPER`: Shed Skin in its only ability slot.
const SEVIPER: u16 = 379;
const LEVEL: u8 = 20;

/// A Splash-versus-Splash turn after construction: the turn-start draw and
/// the enemy's move selection. Splash and the residual pass draw nothing.
const SPLASH_TURN: [u16; 2] = [0, 0];

fn toxic_rattata(max_hp: u32, current_hp: u32, status: Status1) -> BattlePokemon {
    let mon = max_iv_mon(&Dex::new(), RATTATA, LEVEL, vec![SPLASH]);
    let block = Stats {
        max_hp,
        ..mon.stats()
    };
    let mut mon = mon.with_cached_stats(block, current_hp);
    mon.set_status1(status);
    mon
}

fn zigzagoon() -> BattlePokemon {
    max_iv_mon(&Dex::new(), ZIGZAGOON, LEVEL, vec![SPLASH])
}

/// The scripted battle every test shares: construction's seed draw, then
/// `turns` Splash-versus-Splash turns.
fn battle_of(player: BattlePokemon, enemy: BattlePokemon, turns: usize) -> (Battle, SequenceRng) {
    let mut script = vec![0];
    for _ in 0..turns {
        script.extend(SPLASH_TURN);
    }
    let mut rng = SequenceRng::new(script);
    let battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    (battle, rng)
}

fn poison_damage(events: &[BattleEvent], by_player: bool) -> Option<u32> {
    events.iter().find_map(|event| match event {
        BattleEvent::HurtByPoison {
            by_player: side,
            damage,
        } if *side == by_player => Some(*damage),
        _ => None,
    })
}

#[test]
fn the_damage_helper_multiplies_the_floored_base_by_the_incremented_counter() {
    // `max(1, maxHP / 16)` first, then the counter: `maxHP * counter / 16`
    // would give 5 for (47, 3) and 3 for (16, 3).
    for (max_hp, counter, expected) in [
        (48, 1, 3),
        (48, 2, 6),
        (48, 3, 9),
        (47, 3, 6),
        (31, 2, 2),
        (16, 1, 1),
        (15, 2, 2),
        (1, 15, 15),
        (0, 4, 4),
        (160, 15, 150),
    ] {
        assert_eq!(
            bad_poison_residual_damage(max_hp, counter),
            expected,
            "max HP {max_hp}, counter {counter}"
        );
    }
}

#[test]
fn a_fresh_toxic_battler_takes_base_times_a_counter_that_grows_each_turn() {
    let (mut battle, mut rng) = battle_of(
        toxic_rattata(48, 48, Status1::BadPoisoned { counter: 0 }),
        zigzagoon(),
        6,
    );

    let mut hp = 48;
    for expected in [3, 6, 9, 12, 15] {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        hp -= expected;
        assert_eq!(poison_damage(&events, true), Some(expected), "{events:?}");
        assert_eq!(battle.player().current_hp(), hp);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, BattleEvent::HurtByBurn { .. })),
            "{events:?}"
        );
        assert_eq!(
            poison_damage(&events, false),
            None,
            "the healthy enemy takes no tick: {events:?}"
        );
        assert_eq!(
            battle.player().status1(),
            Status1::BadPoisoned { counter: 0 },
            "the residual never writes the status word"
        );
    }

    // The sixth tick asks for 18 but only 3 HP remain: it is capped, then
    // faint settlement clears the corpse's whole primary status.
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(poison_damage(&events, true), Some(3), "{events:?}");
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "{events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    rng.assert_exhausted();
}

#[test]
fn a_loaded_counter_resumes_from_the_stored_value() {
    let (mut battle, mut rng) = battle_of(
        toxic_rattata(48, 48, Status1::BadPoisoned { counter: 5 }),
        zigzagoon(),
        2,
    );

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        poison_damage(&first, true),
        Some(18),
        "counter 5 ticks to 6"
    );
    assert_eq!(battle.player().current_hp(), 30);
    assert_eq!(
        battle.player().status1(),
        Status1::BadPoisoned { counter: 5 },
        "the stored word still holds the loaded counter"
    );

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(poison_damage(&second, true), Some(21), "then 7");
    rng.assert_exhausted();
}

#[test]
fn the_counter_stops_at_fifteen() {
    // Counter 14 ticks to 15; counter 15 stays 15 rather than wrapping to 0
    // (`status1 += STATUS1_TOXIC_TURN(1)` is skipped at 15).
    for stored in [14, 15] {
        let (mut battle, mut rng) = battle_of(
            toxic_rattata(160, 160, Status1::BadPoisoned { counter: stored }),
            zigzagoon(),
            1,
        );

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert_eq!(poison_damage(&events, true), Some(150), "stored {stored}");
        rng.assert_exhausted();
    }
}

#[test]
fn bad_poison_ticks_in_the_rebuilt_end_turn_order_beside_a_burn() {
    let mut enemy = zigzagoon();
    enemy.set_status1(Status1::Burned);
    let (mut battle, mut rng) = battle_of(
        toxic_rattata(160, 160, Status1::BadPoisoned { counter: 0 }),
        enemy,
        1,
    );

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let toxic = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: true,
                    ..
                }
            )
        })
        .unwrap_or_else(|| panic!("the toxic side must tick: {events:?}"));
    let burn = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByBurn {
                    by_player: false,
                    ..
                }
            )
        })
        .unwrap_or_else(|| panic!("the burned side must tick: {events:?}"));
    assert!(toxic < burn, "the faster Rattata ticks first: {events:?}");
    assert_eq!(poison_damage(&events, true), Some(10));
    rng.assert_exhausted();
}

#[test]
fn a_toxic_enemy_ticks_and_faints_through_the_same_residual() {
    let mut enemy = zigzagoon();
    let block = Stats {
        max_hp: 160,
        ..enemy.stats()
    };
    enemy = enemy.with_cached_stats(block, 5);
    enemy.set_status1(Status1::BadPoisoned { counter: 0 });
    let (mut battle, mut rng) = battle_of(toxic_rattata(160, 160, Status1::Healthy), enemy, 1);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(poison_damage(&events, false), Some(5), "{events:?}");
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "{events:?}"
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    rng.assert_exhausted();
}

fn seviper(status: Status1) -> BattlePokemon {
    let mon = max_iv_mon(&Dex::new(), SEVIPER, LEVEL, vec![SPLASH]);
    assert_eq!(mon.ability(), AbilityId::SHED_SKIN);
    let block = Stats {
        max_hp: 48,
        ..mon.stats()
    };
    let mut mon = mon.with_cached_stats(block, 48);
    mon.set_status1(status);
    mon
}

#[test]
fn a_shed_skin_cure_clears_bad_poison_before_its_tick() {
    let toxic = Status1::BadPoisoned { counter: 5 };
    let mut rng = SequenceRng::new([0, 0, 0, 0]);
    let mut battle = Battle::new(Dex::new(), seviper(toxic), zigzagoon(), false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::ShedSkinCured {
            by_player: true,
            status: toxic,
        }),
        "{events:?}"
    );
    assert_eq!(poison_damage(&events, true), None, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.player().current_hp(), 48);
    // Cure draw + construction + turn start + enemy selection, nothing more.
    rng.assert_exhausted();
}

#[test]
fn a_missed_shed_skin_draw_leaves_bad_poison_ticking() {
    let toxic = Status1::BadPoisoned { counter: 5 };
    let mut rng = SequenceRng::new([0, 0, 0, 1]);
    let mut battle = Battle::new(Dex::new(), seviper(toxic), zigzagoon(), false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ShedSkinCured { .. })),
        "{events:?}"
    );
    assert_eq!(poison_damage(&events, true), Some(18), "{events:?}");
    assert_eq!(battle.player().status1(), toxic);
    rng.assert_exhausted();
}

#[test]
fn an_ordinary_poison_tick_is_untouched_beside_it() {
    let mut player = toxic_rattata(160, 160, Status1::Poisoned);
    player.set_status1(Status1::Poisoned);
    let (mut battle, mut rng) = battle_of(player, zigzagoon(), 1);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        poison_damage(&events, true),
        Some(20),
        "maxHP / 8: {events:?}"
    );
    rng.assert_exhausted();
}
