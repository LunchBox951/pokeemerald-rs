//! Singles Trace (`pokeemerald/src/battle_util.c:2575`-`:2580`, `:3017`-`:3073`):
//! the entry queue copies the living opponent's ability once, with an ordered
//! event, leaves the saved slot alone, and draws no RNG. Forced replacements
//! and switches run in the crate's private `trace_tests`.
//!
//! Every script is exact: [`SequenceRng`] panics on an extra draw and
//! `assert_exhausted` fails on a missing one.

use crate::common::{
    admission_script, max_iv_mon, max_iv_mon_with_personality, script, SequenceRng,
    SECONDARY_ABILITY_PERSONALITY,
};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A public turn's seed draw plus the wild enemy's one-slot move choice.
const W: [u16; 2] = [0, 0];
/// A hit: passing accuracy, non-critical, damage roll, discarded effect chance.
const H: [u16; 4] = [0, 1, 0, 0];

fn traced(by_player: bool, ability: AbilityId) -> BattleEvent {
    BattleEvent::TracedAbility { by_player, ability }
}

fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: &[MoveId], slot: u32) -> BattlePokemon {
    max_iv_mon_with_personality(dex, species.0, level, moves.to_vec(), slot)
}

fn trace_holder(dex: &Dex, species: SpeciesId, level: u8, moves: &[MoveId]) -> BattlePokemon {
    let holder = mon(dex, species, level, moves, 0);
    assert_eq!(holder.ability(), AbilityId::TRACE);
    holder
}

#[test]
fn a_holder_copies_the_living_opponents_ability_once_and_the_copy_reaches_damage() {
    let dex = Dex::new();
    let player = trace_holder(&dex, SpeciesId::PORYGON2, 50, &[MoveId::TACKLE]);
    let enemy = mon(
        &dex,
        SpeciesId::AZUMARILL,
        50,
        &[MoveId::SPLASH],
        SECONDARY_ABILITY_PERSONALITY,
    );
    assert_eq!(enemy.ability(), AbilityId::HUGE_POWER);
    assert!(player.stats().speed > enemy.stats().speed, "no order draw");
    let enemy_hp = enemy.current_hp();
    let mut rng = SequenceRng::new(script(&[&C, &W, &H]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    assert_eq!(battle.player().ability(), AbilityId::HUGE_POWER);
    assert_eq!(battle.player().native_ability(), AbilityId::TRACE);
    assert_eq!(battle.player().ability_slot(), 0);

    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first[0], traced(true, AbilityId::HUGE_POWER));
    assert_eq!(
        first[1],
        BattleEvent::UsedMove {
            by_player: true,
            move_id: MoveId::TACKLE
        }
    );
    // Level 50 max-IV Porygon2 Attack 100 doubled by the copied Huge Power
    // against Azumarill's Defense 100: (22 * 35 * 200 / 100) / 50 + 2 = 32,
    // then STAB 1.5 = 48; the native Attack would deal 25.
    assert_eq!(enemy_hp - battle.enemy().current_hp(), 48);
    rng.assert_exhausted();
}

#[test]
fn the_copy_is_announced_with_the_first_turn_only() {
    let dex = Dex::new();
    let player = trace_holder(&dex, SpeciesId::PORYGON2, 50, &[MoveId::SPLASH]);
    let enemy = mon(
        &dex,
        SpeciesId::AZUMARILL,
        50,
        &[MoveId::SPLASH],
        SECONDARY_ABILITY_PERSONALITY,
    );
    let mut rng = SequenceRng::new(script(&[&C, &W, &W]));
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let copies = |events: &[BattleEvent]| {
        events
            .iter()
            .filter(|event| matches!(event, BattleEvent::TracedAbility { .. }))
            .count()
    };
    assert_eq!((copies(&first), copies(&second)), (1, 0));
    assert_eq!(battle.player().ability(), AbilityId::HUGE_POWER);
    rng.assert_exhausted();
}

#[test]
fn route_102_ralts_is_admitted_and_traces_the_player() {
    let dex = Dex::new();
    let player = mon(
        &dex,
        SpeciesId::AZUMARILL,
        50,
        &[MoveId::SPLASH],
        SECONDARY_ABILITY_PERSONALITY,
    );
    let ralts = mon(
        &dex,
        SpeciesId::RALTS,
        4,
        &[MoveId::GROWL],
        SECONDARY_ABILITY_PERSONALITY,
    );
    assert_eq!(ralts.ability(), AbilityId::TRACE);
    assert!(player.stats().speed > ralts.stats().speed);
    let mut rng = SequenceRng::new(admission_script(false));
    let mut battle = Battle::new(dex, player, ralts, false, &mut rng).unwrap();
    assert_eq!(battle.enemy().ability(), AbilityId::HUGE_POWER);
    assert_eq!(battle.enemy().ability_slot(), 1);
    rng.assert_exhausted();

    let mut rng = SequenceRng::new(script(&[&W, &[0]]));
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(events[0], traced(false, AbilityId::HUGE_POWER));
    rng.assert_exhausted();
}

#[test]
fn two_trace_holders_each_copy_once_player_first_whatever_the_speed() {
    for enemy_faster in [false, true] {
        let dex = Dex::new();
        let (player_species, enemy_species) = if enemy_faster {
            (SpeciesId::PORYGON, SpeciesId::PORYGON2)
        } else {
            (SpeciesId::PORYGON2, SpeciesId::PORYGON)
        };
        let player = trace_holder(&dex, player_species, 20, &[MoveId::SPLASH]);
        let enemy = trace_holder(&dex, enemy_species, 20, &[MoveId::SPLASH]);
        assert_eq!(
            enemy.stats().speed > player.stats().speed,
            enemy_faster,
            "the setup must order the two by Speed"
        );
        let mut rng = SequenceRng::new(script(&[&C, &W]));
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(
            events[..2],
            [
                traced(true, AbilityId::TRACE),
                traced(false, AbilityId::TRACE)
            ],
            "enemy_faster: {enemy_faster}"
        );
        assert_eq!(events.len(), 6, "two copies, then each side's Splash");
        assert_eq!(battle.player().ability(), AbilityId::TRACE);
        assert_eq!(battle.enemy().ability(), AbilityId::TRACE);
        rng.assert_exhausted();
    }
}

#[test]
fn a_holder_copies_shedinjas_wonder_guard() {
    let dex = Dex::new();
    let player = trace_holder(&dex, SpeciesId::PORYGON2, 20, &[MoveId::SPLASH]);
    let enemy = mon(&dex, SpeciesId::SHEDINJA, 20, &[MoveId::SPLASH], 0);
    assert_eq!(enemy.ability(), AbilityId::WONDER_GUARD);
    let mut rng = SequenceRng::new(admission_script(
        player.stats().speed == enemy.stats().speed,
    ));
    let battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(battle.player().ability(), AbilityId::WONDER_GUARD);
    rng.assert_exhausted();
}

#[test]
fn a_copied_ability_the_engine_refuses_stays_refused_before_any_draw() {
    let dex = Dex::new();
    let player = trace_holder(&dex, SpeciesId::PORYGON2, 20, &[MoveId::SPLASH]);
    let enemy = max_iv_mon(&dex, SpeciesId::MIGHTYENA.0, 20, vec![MoveId::SPLASH]);
    assert_eq!(enemy.ability(), AbilityId::INTIMIDATE);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(dex, player, enemy, false, &mut rng).err(),
        Some(BattleError::UnportedAbilityInteraction(
            AbilityId::INTIMIDATE
        ))
    );
    rng.assert_exhausted();
}

#[test]
fn a_status_the_copied_ability_would_cure_is_refused_before_any_draw() {
    let dex = Dex::new();
    let mut player = trace_holder(&dex, SpeciesId::PORYGON2, 20, &[MoveId::SPLASH]);
    player.set_status1(Status1::Paralysed);
    let enemy = max_iv_mon(&dex, SpeciesId::PERSIAN.0, 20, vec![MoveId::SPLASH]);
    assert_eq!(enemy.ability(), AbilityId::LIMBER);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(dex, player, enemy, false, &mut rng).err(),
        Some(BattleError::UnportedAbilityInteraction(AbilityId::LIMBER))
    );
    rng.assert_exhausted();
}

#[test]
fn a_copied_serene_grace_on_a_poison_hit_move_is_refused_before_any_draw() {
    let dex = Dex::new();
    let player = mon(
        &dex,
        SpeciesId::TOGEPI,
        20,
        &[MoveId::SPLASH],
        SECONDARY_ABILITY_PERSONALITY,
    );
    assert_eq!(player.ability(), AbilityId::SERENE_GRACE);
    let enemy = trace_holder(&dex, SpeciesId::PORYGON2, 20, &[MoveId::POISON_STING]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(dex, player, enemy, false, &mut rng).err(),
        Some(BattleError::UnportedAbilityInteraction(
            AbilityId::SERENE_GRACE
        ))
    );
    rng.assert_exhausted();
}

#[test]
fn a_holder_is_not_screened_against_an_ability_only_its_own_side_carries() {
    let dex = Dex::new();
    let mut player = trace_holder(&dex, SpeciesId::PORYGON2, 20, &[MoveId::SPLASH]);
    player.set_status1(Status1::Paralysed);
    let limber_reserve = max_iv_mon(&dex, SpeciesId::PERSIAN.0, 20, vec![MoveId::SPLASH]);
    assert_eq!(limber_reserve.ability(), AbilityId::LIMBER);
    let enemy = max_iv_mon(&dex, SpeciesId::KOFFING.0, 20, vec![MoveId::TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::LEVITATE);
    let speed_tie = player.stats().speed == enemy.stats().speed;
    let mut rng = SequenceRng::new(admission_script(speed_tie));
    let battle =
        Battle::new_with_player_reserves(dex, player, vec![limber_reserve], enemy, false, &mut rng)
            .unwrap();
    assert_eq!(battle.player().ability(), AbilityId::LEVITATE);
    rng.assert_exhausted();
}

#[test]
fn an_opposing_holder_can_relay_the_own_sides_ability_so_it_is_screened() {
    let dex = Dex::new();
    let mut player = trace_holder(&dex, SpeciesId::PORYGON2, 20, &[MoveId::SPLASH]);
    player.set_status1(Status1::Paralysed);
    let limber_reserve = max_iv_mon(&dex, SpeciesId::PERSIAN.0, 20, vec![MoveId::SPLASH]);
    let enemy = trace_holder(&dex, SpeciesId::PORYGON, 20, &[MoveId::SPLASH]);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new_with_player_reserves(dex, player, vec![limber_reserve], enemy, false, &mut rng)
            .err(),
        Some(BattleError::UnportedAbilityInteraction(AbilityId::LIMBER))
    );
    rng.assert_exhausted();
}
