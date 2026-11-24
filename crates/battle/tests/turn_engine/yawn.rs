//! Yawn's drowsiness through real turns (`BattleScript_EffectYawn`,
//! `pokeemerald/data/battle_scripts_1.s:2451`-`:2473`; `ENDTURN_YAWN`,
//! `src/battle_util.c:1753`-`:1771`).
//!
//! Every script is exact: [`SequenceRng`] panics on an extra draw and
//! `assert_exhausted` fails on a missing one. The move itself and a first
//! tick spend nothing; only an expiry that can land sleep draws, once.

use crate::common::{max_iv_mon, script, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{initial_moveset, Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const YAWN: MoveId = MoveId::YAWN;
const SPLASH: MoveId = MoveId::SPLASH;
const SCRATCH: MoveId = MoveId::SCRATCH;
const YAWN_SLOT: usize = 0;
const SPLASH_SLOT: usize = 1;

const RATTATA: u16 = SpeciesId::RATTATA.0;
const ZIGZAGOON: u16 = SpeciesId::ZIGZAGOON.0;
const GASTLY: u16 = SpeciesId::GASTLY.0;
const HOOTHOOT: u16 = SpeciesId::HOOTHOOT.0;
const MANKEY: u16 = SpeciesId::MANKEY.0;
const SEVIPER: u16 = SpeciesId::SEVIPER.0;
const SLAKOTH: SpeciesId = SpeciesId::SLAKOTH;

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A public turn's seed draw plus the wild enemy's one-slot move choice.
const W: [u16; 2] = [0, 0];

/// A fast battler: it outspeeds every `slow` fixture, so neither the action
/// order nor the end-turn order ever spends a tie draw.
fn fast(dex: &Dex, moves: Vec<MoveId>) -> BattlePokemon {
    max_iv_mon(dex, RATTATA, 50, moves)
}

fn slow(dex: &Dex, species: u16, moves: Vec<MoveId>) -> BattlePokemon {
    max_iv_mon(dex, species, 5, moves)
}

/// Marks `mon` drowsy with one end-of-turn tick left.
fn one_tick_from_sleep(mon: &mut BattlePokemon) {
    mon.volatiles_mut().set_yawn();
    assert!(!mon.volatiles_mut().tick_yawn());
    assert_eq!(mon.volatiles().yawn_turns, 1);
}

fn asleep_events(events: &[BattleEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, BattleEvent::FellAsleepFromYawn { .. }))
        .count()
}

fn rng_for(groups: &[&[u16]]) -> SequenceRng {
    SequenceRng::new(script(groups))
}

fn start(player: BattlePokemon, enemy: BattlePokemon, rng: &mut SequenceRng) -> Battle {
    assert!(
        player.stats().speed != enemy.stats().speed,
        "equal Speed would add tie draws this script does not budget"
    );
    Battle::new(Dex::new(), player, enemy, false, rng).unwrap()
}

fn turn(battle: &mut Battle, slot: usize, rng: &mut SequenceRng) -> Vec<BattleEvent> {
    battle.take_turn(PlayerAction::UseMove(slot), rng).unwrap()
}

#[test]
fn a_yawn_leaves_two_ticks_minus_the_one_its_own_turn_ends_with_and_spends_no_draw() {
    let dex = Dex::new();
    let player = fast(&dex, vec![YAWN, SPLASH]);
    let yawn_pp = player.moves()[YAWN_SLOT].pp;
    let mut rng = rng_for(&[&C, &W]);
    let mut battle = start(player, slow(&dex, ZIGZAGOON, vec![SPLASH]), &mut rng);

    let events = turn(&mut battle, YAWN_SLOT, &mut rng);

    assert!(events.contains(&BattleEvent::MadeDrowsy {
        by_player: true,
        move_id: YAWN
    }));
    assert_eq!(asleep_events(&events), 0, "the first tick never sleeps");
    assert_eq!(battle.enemy().volatiles().yawn_turns, 1);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert_eq!(battle.player().moves()[YAWN_SLOT].pp, yawn_pp - 1);
    rng.assert_exhausted();
}

#[test]
fn expiry_draws_once_and_sleeps_for_the_masked_draw_plus_two_turns() {
    for (raw, turns) in [(0, 2), (1, 3), (2, 4), (3, 5), (65_535, 5), (4, 2)] {
        for target in [ZIGZAGOON, GASTLY] {
            let dex = Dex::new();
            let player = fast(&dex, vec![YAWN, SPLASH]);
            let mut rng = rng_for(&[&C, &W, &W, &[raw]]);
            let mut battle = start(player, slow(&dex, target, vec![SPLASH]), &mut rng);

            let first = turn(&mut battle, YAWN_SLOT, &mut rng);
            assert_eq!(rng.draws(), 3, "the move and the first tick draw nothing");
            let second = turn(&mut battle, SPLASH_SLOT, &mut rng);

            assert_eq!(asleep_events(&first), 0);
            assert_eq!(
                second.last(),
                Some(&BattleEvent::FellAsleepFromYawn { by_player: false }),
                "raw {raw}, target {target}"
            );
            assert_eq!(battle.enemy().status1(), Status1::Asleep { turns });
            assert_eq!(battle.enemy().volatiles().yawn_turns, 0);
            rng.assert_exhausted();
        }
    }
}

#[test]
fn the_enemy_can_yawn_the_player_and_a_second_yawn_fails_without_refreshing_it() {
    let dex = Dex::new();
    let player = slow(&dex, ZIGZAGOON, vec![SPLASH]);
    let enemy = fast(&dex, vec![YAWN]);
    let mut rng = rng_for(&[&C, &W, &W, &[3]]);
    let mut battle = start(player, enemy, &mut rng);

    let first = turn(&mut battle, 0, &mut rng);
    assert!(first.contains(&BattleEvent::MadeDrowsy {
        by_player: false,
        move_id: YAWN
    }));
    assert_eq!(battle.player().volatiles().yawn_turns, 1);

    let second = turn(&mut battle, 0, &mut rng);

    assert!(second.contains(&BattleEvent::ButItFailed {
        by_player: false,
        move_id: YAWN
    }));
    assert_eq!(
        second.last(),
        Some(&BattleEvent::FellAsleepFromYawn { by_player: true }),
        "the original drowsiness expires on schedule"
    );
    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 5 });
    rng.assert_exhausted();
}

#[test]
fn a_yawn_into_a_primary_status_fails_with_the_generic_message_and_no_draw() {
    for status in [
        Status1::Poisoned,
        Status1::Burned,
        Status1::Paralysed,
        Status1::Frozen,
        Status1::Asleep { turns: 3 },
    ] {
        let dex = Dex::new();
        let mut target = slow(&dex, ZIGZAGOON, vec![SPLASH]);
        target.set_status1(status);
        // Sleep, freeze and paralysis cancel the target's own action with
        // draws of their own, so only the move's events are asserted here;
        // the guarded-expiry test pins the draw counts.
        let mut rng = SequenceRng::new(vec![0; 16]);
        let mut battle = start(fast(&dex, vec![YAWN]), target, &mut rng);

        let events = turn(&mut battle, 0, &mut rng);

        assert!(
            events.contains(&BattleEvent::ButItFailed {
                by_player: true,
                move_id: YAWN
            }),
            "{status:?}: {events:?}"
        );
        assert_eq!(battle.enemy().volatiles().yawn_turns, 0, "{status:?}");
    }
}

#[test]
fn insomnia_and_vital_spirit_name_the_ability_even_when_a_status_would_also_fail_the_move() {
    for (species, ability) in [
        (HOOTHOOT, AbilityId::INSOMNIA),
        (MANKEY, AbilityId::VITAL_SPIRIT),
    ] {
        for statused in [false, true] {
            let dex = Dex::new();
            let mut target = slow(&dex, species, vec![SPLASH]);
            assert_eq!(target.ability(), ability);
            if statused {
                target.set_status1(Status1::Poisoned);
            }
            let mut rng = rng_for(&[&C, &W]);
            let mut battle = start(fast(&dex, vec![YAWN]), target, &mut rng);

            let events = turn(&mut battle, 0, &mut rng);

            assert!(
                events.contains(&BattleEvent::YawnAbilityProtected {
                    by_player: true,
                    move_id: YAWN,
                    ability
                }),
                "{ability:?} statused={statused}: {events:?}"
            );
            assert!(!events.contains(&BattleEvent::ButItFailed {
                by_player: true,
                move_id: YAWN
            }));
            assert_eq!(battle.enemy().volatiles().yawn_turns, 0);
            rng.assert_exhausted();
        }
    }
}

#[test]
fn an_ability_holder_that_is_already_drowsy_names_the_ability_not_the_generic_failure() {
    let dex = Dex::new();
    let mut target = slow(&dex, HOOTHOOT, vec![SPLASH]);
    one_tick_from_sleep(&mut target);
    let mut rng = rng_for(&[&C, &W]);
    let mut battle = start(fast(&dex, vec![YAWN]), target, &mut rng);

    let events = turn(&mut battle, 0, &mut rng);

    assert!(events.contains(&BattleEvent::YawnAbilityProtected {
        by_player: true,
        move_id: YAWN,
        ability: AbilityId::INSOMNIA
    }));
    // The refused expiry is silent and draws nothing.
    assert_eq!(asleep_events(&events), 0);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert_eq!(battle.enemy().volatiles().yawn_turns, 0);
    rng.assert_exhausted();
}

#[test]
fn an_intervening_status_or_sleep_preventing_ability_consumes_no_expiry_draw() {
    for species in [ZIGZAGOON, HOOTHOOT, MANKEY] {
        for status in [
            Status1::Healthy,
            Status1::Poisoned,
            Status1::Burned,
            Status1::Asleep { turns: 4 },
        ] {
            let sleeping_guard = species != ZIGZAGOON && matches!(status, Status1::Asleep { .. });
            if sleeping_guard {
                // Sleep on an Insomnia or Vital Spirit holder is not a
                // reachable state, and the battle refuses to start it.
                continue;
            }
            let dex = Dex::new();
            let mut target = slow(&dex, species, vec![SPLASH]);
            one_tick_from_sleep(&mut target);
            target.set_status1(status);
            let sleeps_without_guard = species == ZIGZAGOON && status == Status1::Healthy;
            let mut rng = if sleeps_without_guard {
                rng_for(&[&C, &W, &[2]])
            } else {
                rng_for(&[&C, &W])
            };
            let mut battle = start(fast(&dex, vec![SPLASH]), target, &mut rng);

            let events = turn(&mut battle, 0, &mut rng);

            assert_eq!(
                asleep_events(&events),
                usize::from(sleeps_without_guard),
                "{species} {status:?}: {events:?}"
            );
            assert_eq!(battle.enemy().volatiles().yawn_turns, 0);
            rng.assert_exhausted();
        }
    }
}

fn shed_skin_seviper(dex: &Dex, status: Status1) -> BattlePokemon {
    let mut mon = max_iv_mon(dex, SEVIPER, 10, vec![SPLASH]);
    assert_eq!(mon.ability(), AbilityId::SHED_SKIN);
    one_tick_from_sleep(&mut mon);
    mon.set_status1(status);
    mon
}

fn shed_skin_turn(status: Status1, script_tail: &[u16]) -> (Battle, Vec<BattleEvent>, SequenceRng) {
    let dex = Dex::new();
    let player = shed_skin_seviper(&dex, status);
    let enemy = fast(&dex, vec![SPLASH]);
    assert!(enemy.stats().speed > player.stats().speed);
    let mut rng = rng_for(&[&C, &W, script_tail]);
    let mut battle = start(player, enemy, &mut rng);
    let events = turn(&mut battle, 0, &mut rng);
    (battle, events, rng)
}

#[test]
fn the_tick_runs_after_the_same_battlers_shed_skin_pass_so_a_healthy_holder_sleeps_uncured() {
    let (battle, events, rng) = shed_skin_turn(Status1::Healthy, &[3]);

    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 5 });
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::ShedSkinCured { .. })));
    rng.assert_exhausted();
}

#[test]
fn a_shed_skin_cure_that_lands_first_lets_the_same_passs_yawn_put_the_holder_to_sleep() {
    let (battle, events, rng) = shed_skin_turn(Status1::Poisoned, &[0, 3]);

    let cured = events.iter().position(|event| {
        *event
            == BattleEvent::ShedSkinCured {
                by_player: true,
                status: Status1::Poisoned,
            }
    });
    let slept = events
        .iter()
        .position(|event| *event == BattleEvent::FellAsleepFromYawn { by_player: true });
    assert!(
        cured.is_some() && slept.is_some() && cured < slept,
        "{events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 5 });
    rng.assert_exhausted();
}

#[test]
fn a_missed_shed_skin_cure_leaves_the_poisoned_holder_without_a_sleep_draw() {
    let (battle, events, rng) = shed_skin_turn(Status1::Poisoned, &[1]);

    assert_eq!(asleep_events(&events), 0);
    assert_eq!(battle.player().status1(), Status1::Poisoned);
    assert_eq!(battle.player().volatiles().yawn_turns, 0);
    rng.assert_exhausted();
}

#[test]
fn a_drowsy_battler_that_faints_to_poison_before_the_tick_neither_draws_nor_sleeps() {
    let dex = Dex::new();
    let mut target = slow(&dex, ZIGZAGOON, vec![SPLASH]);
    one_tick_from_sleep(&mut target);
    target.set_status1(Status1::Poisoned);
    target.apply_damage(target.current_hp() - 1);
    let mut rng = rng_for(&[&C, &W]);
    let mut battle = start(fast(&dex, vec![SPLASH]), target, &mut rng);

    let events = turn(&mut battle, 0, &mut rng);

    assert_eq!(asleep_events(&events), 0, "{events:?}");
    assert_eq!(battle.enemy().volatiles().yawn_turns, 0);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    rng.assert_exhausted();
}

#[test]
fn the_sleep_a_yawn_leaves_follows_the_ordinary_sleep_lifecycle() {
    let dex = Dex::new();
    let player = fast(&dex, vec![YAWN, SPLASH]);
    let mut rng = rng_for(&[&C, &W, &W, &[0], &W, &W]);
    let mut battle = start(player, slow(&dex, ZIGZAGOON, vec![SPLASH]), &mut rng);
    turn(&mut battle, YAWN_SLOT, &mut rng);
    turn(&mut battle, SPLASH_SLOT, &mut rng);
    assert_eq!(battle.enemy().status1(), Status1::Asleep { turns: 2 });
    let splash_pp = battle.enemy().moves()[0].pp;

    let fast_asleep = turn(&mut battle, SPLASH_SLOT, &mut rng);
    assert!(fast_asleep.contains(&BattleEvent::FastAsleep {
        by_player: false,
        move_id: SPLASH
    }));
    assert_eq!(battle.enemy().status1(), Status1::Asleep { turns: 1 });
    assert_eq!(battle.enemy().moves()[0].pp, splash_pp);

    let woke = turn(&mut battle, SPLASH_SLOT, &mut rng);
    let woke_at = woke
        .iter()
        .position(|event| *event == BattleEvent::WokeUp { by_player: false })
        .unwrap();
    assert_eq!(
        woke[woke_at + 1],
        BattleEvent::UsedMove {
            by_player: false,
            move_id: SPLASH
        }
    );
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert_eq!(battle.enemy().moves()[0].pp, splash_pp - 1);
    rng.assert_exhausted();
}

#[test]
fn real_level_five_and_six_slakoth_know_scratch_and_yawn_and_pass_the_move_screen() {
    for level in [5, 6] {
        let moves = initial_moveset(SLAKOTH, level);
        assert_eq!(moves, vec![SCRATCH, YAWN], "level {level}");
        assert_eq!(
            battle::ensure_wild_startable(&Dex::new(), SLAKOTH, level),
            Ok(()),
            "level {level}"
        );
    }
}

#[test]
fn a_wild_slakoth_with_its_real_moveset_can_select_and_execute_yawn() {
    let dex = Dex::new();
    let slakoth = max_iv_mon(&dex, SLAKOTH.0, 5, initial_moveset(SLAKOTH, 5));
    assert_eq!(slakoth.ability(), AbilityId::TRUANT);
    // Slot draws are taken modulo four: one selects the second slot, Yawn.
    let mut rng = rng_for(&[&C, &[0, 1]]);
    let mut battle = start(fast(&dex, vec![SPLASH]), slakoth, &mut rng);

    let events = turn(&mut battle, 0, &mut rng);

    assert!(events.contains(&BattleEvent::MadeDrowsy {
        by_player: false,
        move_id: YAWN
    }));
    assert_eq!(battle.player().volatiles().yawn_turns, 1);
    rng.assert_exhausted();
}

#[test]
fn ordinary_hit_admission_still_refuses_the_zero_power_yawn_that_the_battle_admits() {
    let dex = Dex::new();
    assert_eq!(
        battle::hit::ensure_resolvable(&dex, YAWN),
        Err(battle::BattleError::NonDamagingMove(YAWN))
    );
    let mut rng = rng_for(&[&C]);
    Battle::new(
        dex.clone(),
        fast(&dex, vec![SPLASH]),
        slow(&dex, ZIGZAGOON, vec![YAWN]),
        false,
        &mut rng,
    )
    .expect("a wild mon knowing Yawn passes whole-moveset admission");
    rng.assert_exhausted();
}
