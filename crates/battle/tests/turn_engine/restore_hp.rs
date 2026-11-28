//! Recover and Slack Off through real turns: `BattleScript_EffectRestoreHp`
//! (`pokeemerald/data/battle_scripts_1.s:672`-`:684`) with its
//! `Cmd_tryhealhalfhealth` branch (`src/battle_script_commands.c:6615`-
//! `:6631`) and `BattleScript_AlreadyAtFullHp` (`:2042`-`:2046`).
//!
//! Every script is exact: [`SequenceRng`] panics on an extra draw and
//! `assert_exhausted` fails on a missing one. The heal and the full-HP
//! failure both spend only the turn's own draws.

use crate::common::{max_iv_mon, script, SequenceRng};
use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const RECOVER: MoveId = MoveId::RECOVER;
const SLACK_OFF: MoveId = MoveId::SLACK_OFF;
const SPLASH: MoveId = MoveId::SPLASH;

const RATTATA: u16 = SpeciesId::RATTATA.0;
const ZIGZAGOON: u16 = SpeciesId::ZIGZAGOON.0;
const SHEDINJA: u16 = SpeciesId::SHEDINJA.0;
const WURMPLE: u16 = SpeciesId::WURMPLE.0;
const ZAPDOS: u16 = SpeciesId::ZAPDOS.0;

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A public turn's seed draw plus the wild enemy's one-slot move choice.
const W: [u16; 2] = [0, 0];

fn rng_for(groups: &[&[u16]]) -> SequenceRng {
    SequenceRng::new(script(groups))
}

/// A level 50 Rattata: maximum HP 105, odd, and faster than every `slow`
/// fixture, so neither the action order nor the end-turn order draws a tie.
fn healer(dex: &Dex, moves: Vec<MoveId>) -> BattlePokemon {
    max_iv_mon(dex, RATTATA, 50, moves)
}

fn slow(dex: &Dex, species: u16) -> BattlePokemon {
    max_iv_mon(dex, species, 5, vec![SPLASH])
}

fn at_hp(mut mon: BattlePokemon, current_hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.current_hp() - current_hp);
    mon
}

fn start(player: BattlePokemon, enemy: BattlePokemon, rng: &mut SequenceRng) -> Battle {
    assert!(
        player.stats().speed != enemy.stats().speed,
        "equal Speed would add tie draws this script does not budget"
    );
    Battle::new(Dex::new(), player, enemy, false, rng).unwrap()
}

fn regained(by_player: bool, move_id: MoveId, healed: u32) -> BattleEvent {
    BattleEvent::RegainedHealth {
        by_player,
        move_id,
        healed,
    }
}

fn used(by_player: bool, move_id: MoveId) -> BattleEvent {
    BattleEvent::UsedMove { by_player, move_id }
}

#[test]
fn the_fixture_has_the_odd_maximum_hp_the_odd_case_needs() {
    let dex = Dex::new();

    assert_eq!(healer(&dex, vec![RECOVER]).stats().max_hp, 105);
}

#[test]
fn recover_heals_half_of_an_odd_maximum_rounded_down() {
    for move_id in [RECOVER, SLACK_OFF] {
        let dex = Dex::new();
        let player = at_hp(healer(&dex, vec![move_id]), 10);
        let pp = player.moves()[0].pp;
        let mut rng = rng_for(&[&C, &W]);
        let mut battle = start(player, slow(&dex, ZIGZAGOON), &mut rng);

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert_eq!(battle.player().current_hp(), 62, "{move_id:?}: 10 + 52");
        assert_eq!(battle.player().moves()[0].pp, pp - 1);
        assert_eq!(events[0], used(true, move_id));
        assert_eq!(events[1], regained(true, move_id, 52), "{move_id:?}");
        assert_eq!(battle.enemy().current_hp(), battle.enemy().stats().max_hp);
        rng.assert_exhausted();
    }
}

#[test]
fn the_heal_caps_at_maximum_hp_and_reports_the_hp_actually_gained() {
    for (start_hp, gained) in [(104, 1), (100, 5), (53, 52), (52, 52), (1, 52)] {
        let dex = Dex::new();
        let player = at_hp(healer(&dex, vec![RECOVER]), start_hp);
        let mut rng = rng_for(&[&C, &W]);
        let mut battle = start(player, slow(&dex, ZIGZAGOON), &mut rng);

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert!(
            events.contains(&regained(true, RECOVER, gained)),
            "from {start_hp}: {events:?}"
        );
        assert_eq!(
            battle.player().current_hp(),
            (start_hp + gained).min(105),
            "from {start_hp}"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn a_small_even_maximum_heals_exactly_half_and_caps_the_rest() {
    for (start_hp, gained) in [(1, 10), (10, 10), (11, 9), (19, 1)] {
        let dex = Dex::new();
        let player = at_hp(max_iv_mon(&dex, ZIGZAGOON, 5, vec![RECOVER]), start_hp);
        assert_eq!(player.stats().max_hp, 20);
        let mut rng = rng_for(&[&C, &W]);
        let mut battle = start(player, slow(&dex, WURMPLE), &mut rng);

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert!(
            events.contains(&regained(true, RECOVER, gained)),
            "from {start_hp}: {events:?}"
        );
        assert_eq!(battle.player().current_hp(), start_hp + gained);
        rng.assert_exhausted();
    }
}

#[test]
fn the_enemy_recovers_with_the_same_rule_and_the_event_names_its_side() {
    let dex = Dex::new();
    let enemy = at_hp(healer(&dex, vec![SLACK_OFF]), 90);
    let pp = enemy.moves()[0].pp;
    let mut rng = rng_for(&[&C, &W]);
    let mut battle = start(slow(&dex, ZIGZAGOON), enemy, &mut rng);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&regained(false, SLACK_OFF, 15)),
        "{events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), 105);
    assert_eq!(battle.enemy().moves()[0].pp, pp - 1);
    assert_eq!(battle.player().current_hp(), battle.player().stats().max_hp);
    rng.assert_exhausted();
}

#[test]
fn a_one_hp_maximum_is_always_full_so_the_minimum_of_one_never_heals() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, SHEDINJA, 50, vec![RECOVER]);
    assert_eq!(player.stats().max_hp, 1);
    let mut rng = rng_for(&[&C, &W]);
    let mut battle = start(player, slow(&dex, ZIGZAGOON), &mut rng);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[1],
        BattleEvent::HpAlreadyFull {
            by_player: true,
            move_id: RECOVER
        }
    );
    assert_eq!(battle.player().current_hp(), 1);
    rng.assert_exhausted();
}

#[test]
fn at_full_hp_the_move_fails_spends_pp_and_leaves_hp_and_status_alone() {
    for move_id in [RECOVER, SLACK_OFF] {
        let dex = Dex::new();
        let mut player = healer(&dex, vec![move_id]);
        player.set_status1(Status1::Burned);
        let pp = player.moves()[0].pp;
        let mut rng = rng_for(&[&C, &W]);
        let mut battle = start(player, slow(&dex, ZIGZAGOON), &mut rng);

        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert_eq!(events[0], used(true, move_id));
        assert_eq!(
            events[1],
            BattleEvent::HpAlreadyFull {
                by_player: true,
                move_id
            },
            "{move_id:?}"
        );
        assert!(events.iter().all(|event| !matches!(
            event,
            BattleEvent::RegainedHealth { .. } | BattleEvent::ButItFailed { .. }
        )));
        assert_eq!(battle.player().moves()[0].pp, pp - 1);
        assert_eq!(battle.player().status1(), Status1::Burned);
        // Only the end-of-turn burn costs HP; the move healed nothing.
        assert_eq!(battle.player().current_hp(), 105 - 105 / 8);
        rng.assert_exhausted();
    }
}

#[test]
fn the_enemy_at_full_hp_fails_the_same_way_and_keeps_its_status() {
    let dex = Dex::new();
    let mut enemy = healer(&dex, vec![RECOVER]);
    enemy.set_status1(Status1::Poisoned);
    let pp = enemy.moves()[0].pp;
    let mut rng = rng_for(&[&C, &W]);
    let mut battle = start(slow(&dex, ZIGZAGOON), enemy, &mut rng);

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::HpAlreadyFull {
            by_player: false,
            move_id: RECOVER
        }),
        "{events:?}"
    );
    assert_eq!(battle.enemy().moves()[0].pp, pp - 1);
    assert_eq!(battle.enemy().status1(), Status1::Poisoned);
    rng.assert_exhausted();
}

#[test]
fn a_pressure_holder_does_not_double_the_self_targeted_pp_cost() {
    let dex = Dex::new();
    let player = at_hp(healer(&dex, vec![RECOVER]), 10);
    let pp = player.moves()[0].pp;
    let mut rng = rng_for(&[&C, &W]);
    let mut battle = start(player, max_iv_mon(&dex, ZAPDOS, 5, vec![SPLASH]), &mut rng);

    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(battle.player().moves()[0].pp, pp - 1);
    rng.assert_exhausted();
}
