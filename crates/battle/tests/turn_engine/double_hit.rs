//! `EFFECT_DOUBLE_HIT` (Double Kick, Bonemerang) through a real turn: one
//! accuracy draw, no hit-count draw, then a critical and a damage draw per
//! strike and one trailing effect-chance draw
//! (`BattleScript_EffectDoubleHit`, `data/battle_scripts_1.s:839`-`:847`,
//! setting its counter with `setmultihitcounter 2`,
//! `src/battle_script_commands.c:7139`-`:7142`).
//!
//! Every script is exact: `SequenceRng` panics on a missing draw and each test
//! calls `assert_exhausted`, so an extra count draw, or a draw for a strike
//! that must not run, fails here.

use crate::common::{max_iv_mon, script, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction, Status1};

const DOUBLE_KICK: MoveId = MoveId::DOUBLE_KICK;
const BONEMERANG: MoveId = MoveId::BONEMERANG;
const DOUBLE_SLAP: MoveId = MoveId::DOUBLE_SLAP;
const TACKLE: MoveId = MoveId::TACKLE;

const RATTATA: u16 = 19;
const SQUIRTLE: u16 = 7;
const PIDGEY: u16 = 16;
const GASTLY: u16 = 92;
const SHROOMISH: u16 = 306;
const CARVANHA: u16 = 330;

/// The battle-start draw, the turn-number refresh and the enemy's move choice.
const TURN_SETUP: [u16; 3] = [0, 0, 0];
const ACCURACY_HITS: u16 = 0;
/// A Bonemerang draw above its 90 accuracy; every `Random() % 100 >= accuracy`
/// misses.
const BONEMERANG_MISSES: u16 = 90;
const NO_CRIT: u16 = 1;
const DAMAGE_ROLL: u16 = 0;
/// The draw a hit-count roll would consume; a fixed two-hit move must not.
const EFFECT_CHANCE: u16 = 0;
const STRIKE: [u16; 2] = [NO_CRIT, DAMAGE_ROLL];
/// The slower enemy's Tackle: accuracy, crit, damage, effect chance.
const ENEMY_TACKLE: [u16; 4] = [ACCURACY_HITS, NO_CRIT, DAMAGE_ROLL, 0];

const SPORE_TRIGGERS: u16 = 0;
const SELECTS_SLEEP: u16 = 1;
const SLEEP_FIVE_TURNS: u16 = 3;

fn faster_attacker(dex: &Dex, move_id: MoveId) -> BattlePokemon {
    max_iv_mon(dex, RATTATA, 60, vec![move_id])
}

fn slower_defender(dex: &Dex, species: u16) -> BattlePokemon {
    max_iv_mon(dex, species, 40, vec![TACKLE])
}

fn with_hp(mut mon: BattlePokemon, hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - hp);
    mon
}

fn take_turn(
    player: BattlePokemon,
    enemy: BattlePokemon,
    dex: Dex,
    draws: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(draws.iter().copied());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

fn hits_by_player(events: &[BattleEvent], move_id: MoveId) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                BattleEvent::Hit { by_player: true, move_id: hit_move, .. } if *hit_move == move_id
            )
        })
        .count()
}

fn multi_hit_report(events: &[BattleEvent], move_id: MoveId) -> Vec<u8> {
    events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::MultiHit {
                by_player: true,
                move_id: reported,
                hits,
            } if *reported == move_id => Some(*hits),
            _ => None,
        })
        .collect()
}

fn two_strike_script() -> Vec<u16> {
    script(&[
        &TURN_SETUP,
        &[ACCURACY_HITS],
        &STRIKE,
        &STRIKE,
        &[EFFECT_CHANCE],
        &ENEMY_TACKLE,
    ])
}

#[test]
fn double_kick_strikes_twice_on_one_accuracy_draw_and_no_count_draw() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, DOUBLE_KICK);
    let enemy = slower_defender(&dex, SQUIRTLE);
    let (battle, events) = take_turn(player, enemy, dex, &two_strike_script());

    assert_eq!(hits_by_player(&events, DOUBLE_KICK), 2, "{events:?}");
    assert_eq!(multi_hit_report(&events, DOUBLE_KICK), [2], "{events:?}");
    let first_hit = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .unwrap();
    assert!(
        matches!(
            events[first_hit + 1],
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        ),
        "the strikes are adjacent: {events:?}"
    );
    assert!(battle.enemy().current_hp() < battle.enemy().stats().max_hp);
}

#[test]
fn bonemerang_strikes_twice_on_one_accuracy_draw_and_no_count_draw() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, BONEMERANG);
    let enemy = slower_defender(&dex, SQUIRTLE);
    let (_battle, events) = take_turn(player, enemy, dex, &two_strike_script());

    assert_eq!(hits_by_player(&events, BONEMERANG), 2, "{events:?}");
    assert_eq!(multi_hit_report(&events, BONEMERANG), [2], "{events:?}");
}

/// The same fixture with a variable-count move still draws its count: the
/// script grows by exactly that one draw.
#[test]
fn a_variable_count_move_still_spends_its_hit_count_draw() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, DOUBLE_SLAP);
    let enemy = slower_defender(&dex, SQUIRTLE);
    let draws = script(&[
        &TURN_SETUP,
        &[ACCURACY_HITS],
        &[0],
        &STRIKE,
        &STRIKE,
        &[EFFECT_CHANCE],
        &ENEMY_TACKLE,
    ]);
    let (_battle, events) = take_turn(player, enemy, dex, &draws);

    assert_eq!(hits_by_player(&events, DOUBLE_SLAP), 2, "{events:?}");
}

#[test]
fn a_missed_bonemerang_spends_one_draw_and_no_strike_draws() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, BONEMERANG);
    let enemy = slower_defender(&dex, SQUIRTLE);
    let draws = script(&[&TURN_SETUP, &[BONEMERANG_MISSES], &ENEMY_TACKLE]);
    let (_battle, events) = take_turn(player, enemy, dex, &draws);

    assert!(
        events.contains(&BattleEvent::Missed {
            by_player: true,
            move_id: BONEMERANG,
        }),
        "{events:?}"
    );
    assert_eq!(hits_by_player(&events, BONEMERANG), 0, "{events:?}");
}

/// The first strike knocks the target out: the second strike's critical and
/// damage draws are never spent, and the one landed strike is reported.
#[test]
fn a_target_fainting_on_the_first_strike_skips_the_second_strike_draws() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, DOUBLE_KICK);
    let enemy = with_hp(slower_defender(&dex, SQUIRTLE), 1);
    let draws = script(&[&TURN_SETUP, &[ACCURACY_HITS], &STRIKE, &[EFFECT_CHANCE]]);
    let (battle, events) = take_turn(player, enemy, dex, &draws);

    assert_eq!(hits_by_player(&events, DOUBLE_KICK), 1, "{events:?}");
    assert_eq!(multi_hit_report(&events, DOUBLE_KICK), [1], "{events:?}");
    assert!(battle.enemy().is_fainted());
    assert!(
        events.contains(&BattleEvent::Ended(BattleOutcome::PlayerWon)),
        "{events:?}"
    );
}

/// Bonemerang is a Ground move: Flying is immune, and the immunity report
/// replaces the hit-count report. The blocked strike still spends its
/// critical draw (never a damage draw), exactly as the variable-count loop
/// does.
#[test]
fn bonemerang_is_blocked_by_a_flying_target_without_a_count_draw() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, BONEMERANG);
    let enemy = slower_defender(&dex, PIDGEY);
    let draws = script(&[
        &TURN_SETUP,
        &[ACCURACY_HITS],
        &[NO_CRIT],
        &[EFFECT_CHANCE],
        &ENEMY_TACKLE,
    ]);
    let (battle, events) = take_turn(player, enemy, dex, &draws);

    assert!(
        events.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: BONEMERANG,
        }),
        "{events:?}"
    );
    assert_eq!(hits_by_player(&events, BONEMERANG), 0, "{events:?}");
    assert!(
        multi_hit_report(&events, BONEMERANG).is_empty(),
        "{events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), battle.enemy().stats().max_hp);
}

#[test]
fn bonemerang_is_blocked_by_levitate() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, BONEMERANG);
    let enemy = slower_defender(&dex, GASTLY);
    assert_eq!(enemy.ability(), AbilityId::LEVITATE);
    let draws = script(&[
        &TURN_SETUP,
        &[ACCURACY_HITS],
        &[NO_CRIT],
        &[EFFECT_CHANCE],
        &ENEMY_TACKLE,
    ]);
    let (_battle, events) = take_turn(player, enemy, dex, &draws);

    assert!(
        events.contains(&BattleEvent::LevitateBlocked {
            by_player: true,
            move_id: BONEMERANG,
        }),
        "{events:?}"
    );
    assert_eq!(hits_by_player(&events, BONEMERANG), 0, "{events:?}");
}

/// Double Kick is Fighting: a Ghost target is immune.
#[test]
fn double_kick_is_blocked_by_a_ghost_target() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, DOUBLE_KICK);
    let enemy = slower_defender(&dex, GASTLY);
    let draws = script(&[
        &TURN_SETUP,
        &[ACCURACY_HITS],
        &[NO_CRIT],
        &[EFFECT_CHANCE],
        &ENEMY_TACKLE,
    ]);
    let (_battle, events) = take_turn(player, enemy, dex, &draws);

    assert!(
        events.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: DOUBLE_KICK,
        }),
        "{events:?}"
    );
    assert_eq!(hits_by_player(&events, DOUBLE_KICK), 0, "{events:?}");
}

/// Double Kick makes contact: Effect Spore's sleep ends the series after the
/// first strike without drawing for the second.
#[test]
fn contact_induced_sleep_stops_double_kick_after_one_strike() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, DOUBLE_KICK);
    let enemy = slower_defender(&dex, SHROOMISH);
    assert_eq!(enemy.ability(), AbilityId::EFFECT_SPORE);
    let draws = script(&[
        &TURN_SETUP,
        &[ACCURACY_HITS],
        &STRIKE,
        &[SPORE_TRIGGERS, SELECTS_SLEEP, SLEEP_FIVE_TURNS],
        &[EFFECT_CHANCE],
        &ENEMY_TACKLE,
    ]);
    let (battle, events) = take_turn(player, enemy, dex, &draws);

    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 5 });
    assert_eq!(hits_by_player(&events, DOUBLE_KICK), 1, "{events:?}");
    assert_eq!(multi_hit_report(&events, DOUBLE_KICK), [1], "{events:?}");
}

/// Bonemerang makes no contact, so Effect Spore never rolls and both strikes
/// run.
#[test]
fn bonemerang_never_rolls_a_contact_ability() {
    let dex = Dex::new();
    let player = faster_attacker(&dex, BONEMERANG);
    let enemy = slower_defender(&dex, SHROOMISH);
    let draws = script(&[
        &TURN_SETUP,
        &[ACCURACY_HITS],
        &STRIKE,
        &STRIKE,
        &[EFFECT_CHANCE],
        &ENEMY_TACKLE,
    ]);
    let (battle, events) = take_turn(player, enemy, dex, &draws);

    assert_eq!(hits_by_player(&events, BONEMERANG), 2, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
}

/// Rough Skin's recoil knocks a one-HP attacker out after the first strike;
/// the second strike's draws are never spent and no count is reported.
#[test]
fn an_attacker_fainting_to_contact_stops_double_kick_without_a_report() {
    let dex = Dex::new();
    let player = with_hp(faster_attacker(&dex, DOUBLE_KICK), 1);
    let enemy = slower_defender(&dex, CARVANHA);
    assert_eq!(enemy.ability(), AbilityId::ROUGH_SKIN);
    let draws = script(&[&TURN_SETUP, &[ACCURACY_HITS], &STRIKE, &[EFFECT_CHANCE]]);
    let (battle, events) = take_turn(player, enemy, dex, &draws);

    assert!(battle.player().is_fainted(), "{events:?}");
    assert_eq!(hits_by_player(&events, DOUBLE_KICK), 1, "{events:?}");
    assert!(
        multi_hit_report(&events, DOUBLE_KICK).is_empty(),
        "{events:?}"
    );
}
