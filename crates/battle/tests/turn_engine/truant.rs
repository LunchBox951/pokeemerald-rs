//! Truant's alternating loafing turns through real turns (`CANCELER_TRUANT`,
//! `pokeemerald/src/battle_util.c:2086`-`:2097`; the end-turn toggle,
//! `:2653`-`:2655`; the switch-in set, `battle_script_commands.c:5258`-`:5263`).
//!
//! Every script is exact: [`SequenceRng`] panics on an extra draw and
//! `assert_exhausted` fails on a missing one. Actual voluntary switches and
//! the trainer paths run in the crate's private switch tests, because no
//! public action selects a switch.

use crate::common::{max_iv_mon, script, SequenceRng};
use assets::{MoveId, SpeciesId};
use battle::battle::{BattlerSide, LastLandedMove};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
const SPLASH: MoveId = MoveId::SPLASH;
const SLAKOTH: u16 = SpeciesId::SLAKOTH.0;
const SLAKING: u16 = SpeciesId::SLAKING.0;
const GASTLY: u16 = SpeciesId::GASTLY.0;
const RATTATA: u16 = SpeciesId::RATTATA.0;

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A public turn's seed draw plus the wild enemy's one-slot move choice.
const W: [u16; 2] = [0, 0];
/// A Tackle into a Ghost: passing accuracy, non-critical, damage roll,
/// discarded effect chance. The type immunity still spends all four.
const H: [u16; 4] = [0, 1, 0, 0];

fn tackle_no_effect(by_player: bool) -> BattleEvent {
    BattleEvent::NoEffect {
        by_player,
        move_id: TACKLE,
    }
}

fn fast_asleep(by_player: bool) -> BattleEvent {
    BattleEvent::FastAsleep {
        by_player,
        move_id: TACKLE,
    }
}

fn loafing(by_player: bool) -> BattleEvent {
    BattleEvent::LoafingAround {
        by_player,
        move_id: TACKLE,
    }
}

/// A slower, long-sleeping Tackle-immune partner that never acts or draws.
fn sleeping_gastly(dex: &Dex, level: u8) -> BattlePokemon {
    let mut mon = max_iv_mon(dex, GASTLY, level, vec![TACKLE]);
    mon.set_status1(Status1::Asleep { turns: 5 });
    mon
}

fn phase(battle: &Battle, side: BattlerSide) -> bool {
    battle.battler_state(side).truant_loafing()
}

fn holder(dex: &Dex, species: u16, level: u8) -> BattlePokemon {
    let mon = max_iv_mon(dex, species, level, vec![TACKLE]);
    assert_eq!(mon.ability(), assets::AbilityId::TRUANT);
    mon
}

#[test]
fn an_initial_holder_on_either_side_acts_loafs_then_acts() {
    for species in [SLAKOTH, SLAKING] {
        for holder_is_player in [true, false] {
            let dex = Dex::new();
            let level = 10;
            let (player, enemy) = if holder_is_player {
                (holder(&dex, species, level), sleeping_gastly(&dex, 1))
            } else {
                (sleeping_gastly(&dex, 1), holder(&dex, species, level))
            };
            assert!(
                if holder_is_player {
                    player.stats().speed > enemy.stats().speed
                } else {
                    enemy.stats().speed > player.stats().speed
                },
                "the holder must outspeed its partner so no order draw occurs"
            );
            let side = if holder_is_player {
                BattlerSide::Player
            } else {
                BattlerSide::Enemy
            };
            let mut rng = SequenceRng::new(script(&[&C, &W, &H, &W, &W, &H]));
            let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
            let pp_of = |battle: &Battle| {
                if holder_is_player {
                    battle.player().moves()[0].pp
                } else {
                    battle.enemy().moves()[0].pp
                }
            };
            let starting_pp = pp_of(&battle);
            assert!(
                !phase(&battle, side),
                "an initial holder starts able to act"
            );

            let turn = |battle: &mut Battle, rng: &mut SequenceRng| {
                battle.take_turn(PlayerAction::UseMove(0), rng).unwrap()
            };
            let partner_asleep = fast_asleep(!holder_is_player);

            let first = turn(&mut battle, &mut rng);
            assert_eq!(
                first,
                vec![tackle_no_effect(holder_is_player), partner_asleep],
                "{species} first turn"
            );
            assert_eq!(pp_of(&battle), starting_pp - 1);
            assert!(phase(&battle, side));

            let second = turn(&mut battle, &mut rng);
            assert_eq!(second, vec![loafing(holder_is_player), partner_asleep]);
            assert_eq!(pp_of(&battle), starting_pp - 1, "loafing spends no PP");
            assert!(!phase(&battle, side));

            let third = turn(&mut battle, &mut rng);
            assert_eq!(
                third,
                vec![tackle_no_effect(holder_is_player), partner_asleep]
            );
            assert_eq!(pp_of(&battle), starting_pp - 2);
            assert!(phase(&battle, side));
            rng.assert_exhausted();
        }
    }
}

fn sleeping_holder_battle(sleep_turns: u8, rng: &mut SequenceRng) -> Battle {
    let dex = Dex::new();
    let mut player = holder(&dex, SLAKOTH, 10);
    player.set_status1(Status1::Asleep { turns: sleep_turns });
    let enemy = sleeping_gastly(&dex, 1);
    Battle::new(dex, player, enemy, false, rng).unwrap()
}

#[test]
fn sleep_advances_the_phase_and_waking_leads_straight_into_loafing() {
    let mut rng = SequenceRng::new(script(&[&C, &W, &W, &W, &H]));
    let mut battle = sleeping_holder_battle(2, &mut rng);
    let starting_pp = battle.player().moves()[0].pp;

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first, vec![fast_asleep(true), fast_asleep(false)]);
    assert!(
        phase(&battle, BattlerSide::Player),
        "the end-turn flip does not wait for a successful action"
    );

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        second,
        vec![
            BattleEvent::WokeUp { by_player: true },
            loafing(true),
            fast_asleep(false),
        ]
    );
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
    assert!(!phase(&battle, BattlerSide::Player));

    let third = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(third, vec![tackle_no_effect(true), fast_asleep(false)]);
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);
    rng.assert_exhausted();
}

#[test]
fn sleep_cancels_before_the_loafing_gate_and_the_phase_still_flips() {
    let mut rng = SequenceRng::new(script(&[&C, &W, &W, &W, &H]));
    let mut battle = sleeping_holder_battle(3, &mut rng);

    for _ in 0..2 {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(events, vec![fast_asleep(true), fast_asleep(false)]);
    }
    assert!(
        !phase(&battle, BattlerSide::Player),
        "two sleep-cancelled turns flip the phase twice"
    );
    let third = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        third,
        vec![
            BattleEvent::WokeUp { by_player: true },
            tackle_no_effect(true),
            fast_asleep(false),
        ]
    );
    rng.assert_exhausted();
}

#[test]
fn loafing_consumes_no_confusion_paralysis_or_move_draw() {
    let dex = Dex::new();
    let mut player = holder(&dex, SLAKING, 20);
    player.set_status1(Status1::Paralysed);
    player.volatiles_mut().set_confusion(3);
    let enemy = sleeping_gastly(&dex, 1);
    assert!(player.stats().speed > enemy.stats().speed);
    // The acting suffix is the confusion bit (odd: no self-hit), the
    // paralysis remainder (non-zero: acts), then the hit draws.
    let acting_suffix: Vec<u16> = [&[1u16, 1][..], &H].concat();
    let mut rng = SequenceRng::new(script(&[&C, &W, &acting_suffix, &W, &W, &acting_suffix]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let starting_pp = battle.player().moves()[0].pp;
    let confused = BattleEvent::Confused { by_player: true };

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        first,
        vec![confused, tackle_no_effect(true), fast_asleep(false)]
    );
    assert_eq!(rng.draws(), 1 + 2 + 6);

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(second, vec![loafing(true), fast_asleep(false)]);
    assert_eq!(
        rng.draws(),
        1 + 2 + 6 + 2,
        "the loafing turn draws only its seed and the enemy's move choice"
    );
    assert!(
        battle.player().volatiles().confused(),
        "confusion untouched"
    );
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);

    let third = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        third,
        vec![confused, tackle_no_effect(true), fast_asleep(false)]
    );
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 2);
    rng.assert_exhausted();
}

#[test]
fn a_loafing_action_records_its_cancelled_move_against_the_target() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, 100, vec![SPLASH]);
    let enemy = holder(&dex, SLAKING, 10);
    assert!(player.stats().speed > enemy.stats().speed);
    let mut rng = SequenceRng::new(script(&[&C, &W, &H, &W]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let splash = BattleEvent::NothingHappened {
        by_player: true,
        move_id: SPLASH,
    };

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first, vec![splash, tackle_no_effect(false)]);
    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(second, vec![splash, loafing(false)]);

    let state = battle.battler_state(BattlerSide::Player);
    assert_eq!(state.last_hitter(), Some(BattlerSide::Enemy));
    assert_eq!(
        state.last_landed_move(),
        LastLandedMove::Unavailable,
        "`moveendto MOVEEND_NEXT_TARGET` runs MOVEEND_UPDATE_LAST_MOVES first"
    );
    rng.assert_exhausted();
}

fn one_hp(mut mon: BattlePokemon) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - 1);
    mon
}

#[test]
fn a_reserve_sent_out_after_the_residual_faint_loafs_on_its_first_action() {
    let dex = Dex::new();
    let mut player = one_hp(max_iv_mon(&dex, RATTATA, 1, vec![TACKLE]));
    player.set_status1(Status1::Poisoned);
    let reserve = holder(&dex, SLAKING, 10);
    let enemy = sleeping_gastly(&dex, 10);
    let mut rng = SequenceRng::new(script(&[&C, &W, &H, &W, &W, &H]));
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        first,
        vec![
            fast_asleep(false),
            tackle_no_effect(true),
            BattleEvent::HurtByPoison {
                by_player: true,
                damage: 1,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::PlayerSentOut {
                species: SpeciesId::SLAKING,
                reserves_remaining: 0,
            },
        ]
    );
    assert!(
        phase(&battle, BattlerSide::Player),
        "no abilities pass follows a post-residual send-out"
    );
    let starting_pp = battle.player().moves()[0].pp;

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(second, vec![loafing(true), fast_asleep(false)]);
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
    assert!(!phase(&battle, BattlerSide::Player));

    let third = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(third, vec![tackle_no_effect(true), fast_asleep(false)]);
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);
    rng.assert_exhausted();
}

#[test]
fn a_reserve_sent_out_after_an_action_faint_acts_on_its_first_action() {
    let dex = Dex::new();
    let player = one_hp(max_iv_mon(&dex, RATTATA, 1, vec![TACKLE]));
    let reserve = holder(&dex, SLAKING, 10);
    let enemy = max_iv_mon(&dex, GASTLY, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new(script(&[&C, &W, &H, &W, &H, &H]));
    let mut battle =
        Battle::new_with_player_reserves(dex, player, vec![reserve], enemy, false, &mut rng)
            .unwrap();

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        first,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                damage: 1,
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::PlayerSentOut {
                species: SpeciesId::SLAKING,
                reserves_remaining: 0,
            },
        ]
    );
    assert!(
        !phase(&battle, BattlerSide::Player),
        "the same turn's abilities pass flipped the fresh phase"
    );
    let starting_pp = battle.player().moves()[0].pp;

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(second[0], tackle_no_effect(true));
    assert!(!second.contains(&loafing(true)));
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);
    rng.assert_exhausted();
}
