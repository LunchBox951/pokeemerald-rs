//! Color Change (`pokeemerald/src/battle_util.c:2749`-`:2766`): a surviving
//! holder takes the type of the move that damaged it, with no `Random()` call.
//!
//! Every script is exact: [`SequenceRng`] panics on an extra draw and
//! `assert_exhausted` fails on a missing one, so a stray ability draw fails
//! each test. Damage figures are hand-derived from the planted 100-point
//! stats below at level 50: a 40-power hit deals 19 at the top roll
//! (`22 * 40 * 100 / 100 / 50 + 2`), STAB makes it 28, and resistance halves
//! it to 9.

use crate::common::{max_iv_mon, SequenceRng, STRUGGLE_TARGET_PLAYER};
use assets::{AbilityId, MoveId, SpeciesId, Type};
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, StatStage, Stats};

const WATER_GUN: MoveId = MoveId::WATER_GUN;
const SPLASH: MoveId = MoveId::SPLASH;

const KECLEON_HP: u32 = 1000;

/// The constructor's turn-number seed; no Speed tie, so no order draw.
const C: [u16; 1] = [0];
/// A turn's seed draw plus the wild enemy's one-slot move choice.
const W: [u16; 2] = [0, 0];
/// A powered hit: passing accuracy, no critical, top damage roll, discarded
/// effect chance.
const H: [u16; 4] = [0, 1, 0, 0];

fn planted(mon: BattlePokemon, speed: u32, hp: u32) -> BattlePokemon {
    let stats = Stats {
        max_hp: KECLEON_HP,
        attack: 100,
        defense: 100,
        speed,
        sp_attack: 100,
        sp_defense: 100,
    };
    mon.with_cached_stats(stats, hp)
}

/// Level-50 Kecleon (Normal, Color Change) with Speed 100.
fn kecleon(moves: &[MoveId], speed: u32, hp: u32) -> BattlePokemon {
    let mon = max_iv_mon(&Dex::new(), SpeciesId::KECLEON.0, 50, moves.to_vec());
    assert_eq!(mon.ability(), AbilityId::COLOR_CHANGE);
    assert_eq!(mon.types(), [Type::Normal, Type::Normal]);
    planted(mon, speed, hp)
}

/// Level-50 Rattata (Normal) with Speed 200.
fn rattata(moves: &[MoveId], speed: u32) -> BattlePokemon {
    let mon = max_iv_mon(&Dex::new(), SpeciesId::RATTATA.0, 50, moves.to_vec());
    planted(mon, speed, KECLEON_HP)
}

fn type_changes(events: &[BattleEvent]) -> Vec<BattleEvent> {
    events
        .iter()
        .copied()
        .filter(|event| matches!(event, BattleEvent::TypeChanged { .. }))
        .collect()
}

fn hits(events: &[BattleEvent], by_player: bool) -> Vec<u32> {
    events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::Hit {
                by_player: who,
                damage,
                ..
            } if *who == by_player => Some(*damage),
            _ => None,
        })
        .collect()
}

fn changed(by_player: bool, move_id: MoveId, new_type: Type) -> BattleEvent {
    BattleEvent::TypeChanged {
        by_player,
        move_id,
        new_type,
    }
}

fn start(player: BattlePokemon, enemy: BattlePokemon, script: &[u16]) -> (Battle, SequenceRng) {
    let mut rng = SequenceRng::new(script.iter().copied());
    let battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    (battle, rng)
}

fn use_first_move(battle: &mut Battle, rng: &mut SequenceRng) -> Vec<BattleEvent> {
    battle.take_turn(PlayerAction::UseMove(0), rng).unwrap()
}

#[test]
fn water_gun_changes_a_surviving_kecleon_and_later_attacks_see_the_new_typing() {
    // Rattata (Speed 200) acts first each turn; Kecleon answers with its own
    // Water Gun. Turn one: 3 setup draws precede two hits. Turn two repeats it.
    let script = [&C[..], &W, &H, &H, &W, &H, &H].concat();
    let (mut battle, mut rng) = start(
        rattata(&[WATER_GUN], 200),
        kecleon(&[WATER_GUN], 100, KECLEON_HP),
        &script,
    );

    let first = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&first, true), vec![19], "{first:?}");
    assert_eq!(
        type_changes(&first),
        vec![changed(true, WATER_GUN, Type::Water)]
    );
    assert_eq!(battle.enemy().types(), [Type::Water, Type::Water]);
    assert_eq!(battle.enemy().species_types(), [Type::Normal, Type::Normal]);
    // Kecleon's reply now has STAB against Normal Rattata: 19 * 3 / 2.
    assert_eq!(hits(&first, false), vec![28], "{first:?}");
    let hit = first
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
    let change = first
        .iter()
        .position(|event| matches!(event, BattleEvent::TypeChanged { .. }))
        .unwrap();
    assert!(hit < change, "{first:?}");

    let second = use_first_move(&mut battle, &mut rng);

    // Water now resists Water: 19 / 2, and no second change is announced.
    assert_eq!(hits(&second, true), vec![9], "{second:?}");
    assert_eq!(type_changes(&second), vec![]);
    assert_eq!(battle.enemy().types(), [Type::Water, Type::Water]);
    assert_eq!(battle.enemy().ability(), AbilityId::COLOR_CHANGE);
    rng.assert_exhausted();
}

#[test]
fn a_player_holder_changes_too_and_the_event_names_the_attackers_side() {
    // Kecleon (Speed 200) uses Splash, so only Rattata's hit draws.
    let script = [&C[..], &W, &H].concat();
    let (mut battle, mut rng) = start(
        kecleon(&[SPLASH], 200, KECLEON_HP),
        rattata(&[WATER_GUN], 100),
        &script,
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&events, false), vec![19]);
    assert_eq!(
        type_changes(&events),
        vec![changed(false, WATER_GUN, Type::Water)]
    );
    assert_eq!(battle.player().types(), [Type::Water, Type::Water]);
    rng.assert_exhausted();
}

#[test]
fn a_non_contact_move_changes_the_type_without_an_ability_draw() {
    let script = [&C[..], &W, &H].concat();
    let (mut battle, mut rng) = start(
        rattata(&[WATER_GUN], 200),
        kecleon(&[SPLASH], 100, KECLEON_HP),
        &script,
    );
    assert!(!Dex::new()
        .move_data(WATER_GUN)
        .unwrap()
        .flags
        .makes_contact());

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(type_changes(&events).len(), 1, "{events:?}");
    // The script holds exactly one hit's draws: nothing was left for Color Change.
    rng.assert_exhausted();
}

#[test]
fn a_hit_of_a_type_the_holder_already_has_changes_nothing() {
    let script = [&C[..], &W, &H].concat();
    let (mut battle, mut rng) = start(
        rattata(&[MoveId::POUND], 200),
        kecleon(&[SPLASH], 100, KECLEON_HP),
        &script,
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&events, true).len(), 1);
    assert_eq!(type_changes(&events), vec![]);
    assert_eq!(battle.enemy().types(), [Type::Normal, Type::Normal]);
    rng.assert_exhausted();
}

#[test]
fn an_immune_move_changes_nothing() {
    // Shadow Punch never misses, so it spends no accuracy draw; Normal is
    // immune to Ghost, so the hit stops before its damage roll.
    let (mut battle, mut rng) = start(
        rattata(&[MoveId::SHADOW_PUNCH], 200),
        kecleon(&[SPLASH], 100, KECLEON_HP),
        &[0, 0, 0, 1, 0, 0],
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&events, true), Vec::<u32>::new());
    assert_eq!(type_changes(&events), vec![]);
    assert_eq!(battle.enemy().current_hp(), KECLEON_HP);
    assert_eq!(battle.enemy().types(), [Type::Normal, Type::Normal]);
    rng.assert_exhausted();
}

#[test]
fn a_missed_move_changes_nothing() {
    let mut attacker = rattata(&[WATER_GUN], 200);
    attacker.stages_mut().accuracy = StatStage::MIN;
    // Accuracy roll 99 is over Water Gun's 100 scaled by the -6 stage.
    let (mut battle, mut rng) = start(
        attacker,
        kecleon(&[SPLASH], 100, KECLEON_HP),
        &[0, 0, 0, 99],
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&events, true), Vec::<u32>::new());
    assert_eq!(type_changes(&events), vec![]);
    assert_eq!(battle.enemy().current_hp(), KECLEON_HP);
    assert_eq!(battle.enemy().types(), [Type::Normal, Type::Normal]);
    rng.assert_exhausted();
}

#[test]
fn a_fixed_damage_move_with_base_power_one_still_changes_the_type() {
    // Dragon Rage's table power is 1, not 0
    // (`pokeemerald/src/data/battle_moves.h:1069`-`:1073`).
    let (mut battle, mut rng) = start(
        rattata(&[MoveId::DRAGON_RAGE], 200),
        kecleon(&[SPLASH], 100, KECLEON_HP),
        &[0, 0, 0, 0, 0],
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&events, true), vec![40], "{events:?}");
    assert_eq!(
        type_changes(&events),
        vec![changed(true, MoveId::DRAGON_RAGE, Type::Dragon)]
    );
    assert_eq!(battle.enemy().types(), [Type::Dragon, Type::Dragon]);
    rng.assert_exhausted();
}

#[test]
fn struggle_never_changes_the_holder() {
    // Rattata's lone Water Gun has one PP: it changes Kecleon to Water, then
    // the empty slot forces Struggle, a Normal hit Kecleon no longer has.
    let mut attacker = rattata(&[WATER_GUN], 100);
    for _ in 1..attacker.moves()[0].pp {
        attacker.deduct_pp(0).unwrap();
    }
    let script = [
        &C[..],
        &W,
        &H,
        // Turn two: seed draw, Struggle's target draw, then a no-effect-chance hit.
        &[0, STRUGGLE_TARGET_PLAYER, 0, 1, 0],
    ]
    .concat();
    let (mut battle, mut rng) = start(kecleon(&[SPLASH], 200, KECLEON_HP), attacker, &script);

    let first = use_first_move(&mut battle, &mut rng);
    assert_eq!(type_changes(&first).len(), 1, "{first:?}");
    assert_eq!(battle.player().types(), [Type::Water, Type::Water]);

    let second = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&second, false).len(), 1, "{second:?}");
    assert!(second.iter().any(|event| matches!(
        event,
        BattleEvent::UsedMove { move_id, .. } if *move_id == MoveId::STRUGGLE
    )));
    assert_eq!(type_changes(&second), vec![]);
    assert_eq!(battle.player().types(), [Type::Water, Type::Water]);
    rng.assert_exhausted();
}

#[test]
fn a_holder_the_hit_faints_does_not_change() {
    let script = [&C[..], &W, &H].concat();
    let (mut battle, mut rng) = start(
        rattata(&[WATER_GUN], 200),
        kecleon(&[SPLASH], 100, 1),
        &script,
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert!(battle.enemy().is_fainted());
    assert_eq!(type_changes(&events), vec![]);
    assert_eq!(battle.enemy().types(), [Type::Normal, Type::Normal]);
    rng.assert_exhausted();
}

#[test]
fn a_changed_holder_that_later_faints_is_back_to_its_species_typing() {
    // 25 HP: the first Water Gun (19) leaves 6, the second (9 against Water)
    // faints it. `FaintClearSetData` restores both type slots
    // (`pokeemerald/src/battle_main.c:3350`-`:3351`).
    let script = [&C[..], &W, &H, &W, &H].concat();
    let (mut battle, mut rng) = start(
        rattata(&[WATER_GUN], 200),
        kecleon(&[SPLASH], 100, 25),
        &script,
    );

    use_first_move(&mut battle, &mut rng);
    assert_eq!(battle.enemy().types(), [Type::Water, Type::Water]);
    use_first_move(&mut battle, &mut rng);

    assert!(battle.enemy().is_fainted());
    assert_eq!(battle.enemy().types(), [Type::Normal, Type::Normal]);
    assert_eq!(battle.enemy().ability(), AbilityId::COLOR_CHANGE);
    rng.assert_exhausted();
}

#[test]
fn the_changed_type_applies_to_the_following_hits_of_a_multi_hit_move() {
    // Bullet Seed (Grass, power 10): the first hit is neutral against Normal
    // and deals (22 * 10 / 50) + 2 = 6; Kecleon is then Grass, which resists
    // the second hit down to 3.
    let two_hits: [u16; 7] = [
        0, // accuracy
        0, // hit count: two
        1, 0, // first strike: no crit, top roll
        1, 0, // second strike
        0, // trailing effect chance
    ];
    let script = [&C[..], &W, &two_hits].concat();
    let (mut battle, mut rng) = start(
        rattata(&[MoveId::BULLET_SEED], 200),
        kecleon(&[SPLASH], 100, KECLEON_HP),
        &script,
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(hits(&events, true), vec![6, 3], "{events:?}");
    assert_eq!(
        type_changes(&events),
        vec![changed(true, MoveId::BULLET_SEED, Type::Grass)]
    );
    let positions: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            matches!(
                event,
                BattleEvent::Hit { .. } | BattleEvent::TypeChanged { .. }
            )
        })
        .map(|(index, _)| index)
        .collect();
    assert!(matches!(events[positions[0]], BattleEvent::Hit { .. }));
    assert!(matches!(
        events[positions[1]],
        BattleEvent::TypeChanged { .. }
    ));
    assert!(matches!(events[positions[2]], BattleEvent::Hit { .. }));
    assert_eq!(battle.enemy().current_hp(), KECLEON_HP - 9);
    rng.assert_exhausted();
}

#[test]
fn a_multi_hit_strike_that_faints_the_holder_does_not_change_it() {
    let script = [&C[..], &W, &[0, 0, 1, 0, 0]].concat();
    let (mut battle, mut rng) = start(
        rattata(&[MoveId::BULLET_SEED], 200),
        kecleon(&[SPLASH], 100, 6),
        &script,
    );

    let events = use_first_move(&mut battle, &mut rng);

    assert!(battle.enemy().is_fainted());
    assert_eq!(hits(&events, true), vec![6]);
    assert_eq!(type_changes(&events), vec![]);
    rng.assert_exhausted();
}

#[test]
fn a_traced_color_change_changes_its_holder_and_leaves_the_native_ability() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, SpeciesId::PORYGON.0, 60, vec![SPLASH]);
    assert_eq!(player.native_ability(), AbilityId::TRACE);
    let enemy = kecleon(&[WATER_GUN], 10, KECLEON_HP);
    assert!(player.stats().speed > enemy.stats().speed, "no order draw");
    let script = [&C[..], &W, &H].concat();
    let (mut battle, mut rng) = start(player, enemy, &script);
    assert_eq!(battle.player().ability(), AbilityId::COLOR_CHANGE);

    let events = use_first_move(&mut battle, &mut rng);

    assert_eq!(
        type_changes(&events),
        vec![changed(false, WATER_GUN, Type::Water)]
    );
    assert_eq!(battle.player().types(), [Type::Water, Type::Water]);
    assert_eq!(battle.player().native_ability(), AbilityId::TRACE);
    rng.assert_exhausted();
}

#[test]
fn a_new_battle_starts_every_entrant_in_its_species_typing() {
    let script = [&C[..], &W, &H].concat();
    let (mut battle, mut rng) = start(
        rattata(&[WATER_GUN], 200),
        kecleon(&[SPLASH], 100, KECLEON_HP),
        &script,
    );
    use_first_move(&mut battle, &mut rng);
    let carried = battle.enemy().clone();
    assert_eq!(carried.types(), [Type::Water, Type::Water]);

    // The same still-Water mon enters as the wild enemy of a fresh battle, then
    // as the player's lead, then as a player reserve.
    let (as_enemy, _) = start(rattata(&[SPLASH], 200), carried.clone(), &[0]);
    assert_eq!(as_enemy.enemy().types(), [Type::Normal, Type::Normal]);
    let (as_lead, _) = start(carried.clone(), rattata(&[SPLASH], 10), &[0]);
    assert_eq!(as_lead.player().types(), [Type::Normal, Type::Normal]);
    let mut rng = SequenceRng::new([0]);
    let with_reserve = Battle::new_with_player_reserves(
        Dex::new(),
        rattata(&[SPLASH], 200),
        vec![carried],
        rattata(&[SPLASH], 10),
        false,
        &mut rng,
    )
    .unwrap();
    assert!(with_reserve
        .player_members()
        .all(|member| member.types() == member.species_types()));
}
