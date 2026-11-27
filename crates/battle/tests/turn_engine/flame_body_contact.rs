//! Flame Body's contact burn (`ABILITYEFFECT_ON_DAMAGE`, `battle_util.c:2836`-
//! `:2850`). Every script is exact: [`SequenceRng`] panics on an extra draw
//! and `assert_exhausted` fails on a missing one.

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::hit::damage_before_roll;
use battle::status1::burn_residual_damage;
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
const WATER_GUN: MoveId = MoveId::WATER_GUN;
const SLAM: MoveId = MoveId::SLAM;
const DOUBLE_SLAP: MoveId = MoveId::DOUBLE_SLAP;

const RATTATA: u16 = 19;
const MAGMAR: u16 = SpeciesId::MAGMAR.0;

const CONSTRUCTION_TURN_NUMBER: u16 = 0;
const TURN_START: u16 = 0;
const ENEMY_MOVE_SELECTION: u16 = 0;
const ACCURACY_HITS: u16 = 0;
/// `Random() % 100 >= 75` fails a 75-accuracy move's check.
const ACCURACY_MISSES: u16 = 99;
const NO_CRIT: u16 = 1;
const DAMAGE_ROLL: u16 = 0;
const DISCARDED_SECONDARY_EFFECT: u16 = 0;

/// The mover that acts first: turn setup, then one landed ordinary hit.
const FASTER_HITS: [u16; 7] = [
    CONSTRUCTION_TURN_NUMBER,
    TURN_START,
    ENEMY_MOVE_SELECTION,
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];
const SLOWER_HITS: [u16; 4] = [
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

/// Flame Body's roll is `Random() % 3`: zero burns.
const FLAME_BODY_TRIGGERS: u16 = 0;
const FLAME_BODY_MISSES: [u16; 2] = [1, 2];

fn script(prefix: &[u16], roll: u16, suffix: &[u16]) -> SequenceRng {
    let mut draws = prefix.to_vec();
    draws.push(roll);
    draws.extend_from_slice(suffix);
    SequenceRng::new(draws)
}

fn magmar(dex: &Dex, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let mon = max_iv_mon(dex, MAGMAR, level, moves);
    assert_eq!(mon.ability(), AbilityId::FLAME_BODY);
    mon
}

fn attacker(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    max_iv_mon(dex, species, level, moves)
}

fn flame_body_events(events: &[BattleEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, BattleEvent::BurnedByFlameBody { .. }))
        .count()
}

/// One turn where the player (the faster mover) hits the enemy holder with
/// `moves[0]`, returning the battle and events after the exact `rng` ran.
fn player_attacks_enemy_holder(
    player: BattlePokemon,
    enemy: BattlePokemon,
    mut rng: SequenceRng,
) -> (Battle, Vec<BattleEvent>) {
    assert!(
        player.speed_for_turn_order() > enemy.speed_for_turn_order(),
        "the script assumes the player moves first"
    );
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

#[test]
fn a_zero_roll_burns_a_player_attacker_through_an_enemy_holder() {
    let dex = Dex::new();
    let player = attacker(&dex, RATTATA, 30, vec![TACKLE]);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert_eq!(battle.player().status1(), Status1::Burned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
    assert!(
        events.contains(&BattleEvent::BurnedByFlameBody {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}

#[test]
fn a_zero_roll_burns_an_enemy_attacker_through_a_player_holder() {
    let dex = Dex::new();
    let player = magmar(&dex, 20, vec![TACKLE]);
    let enemy = attacker(&dex, RATTATA, 30, vec![TACKLE]);
    assert!(enemy.speed_for_turn_order() > player.speed_for_turn_order());

    let mut rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert_eq!(battle.enemy().status1(), Status1::Burned, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert!(
        events.contains(&BattleEvent::BurnedByFlameBody {
            by_player: false,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}

#[test]
fn a_nonzero_roll_spends_its_draw_and_leaves_the_attacker_healthy() {
    for roll in FLAME_BODY_MISSES {
        let dex = Dex::new();
        let player = attacker(&dex, RATTATA, 30, vec![TACKLE]);
        let enemy = magmar(&dex, 20, vec![TACKLE]);

        let rng = script(&FASTER_HITS, roll, &SLOWER_HITS);
        let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

        assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
        assert_eq!(flame_body_events(&events), 0, "{events:?}");
    }
}

#[test]
fn a_non_contact_hit_into_a_flame_body_holder_draws_no_roll() {
    let dex = Dex::new();
    let player = attacker(&dex, RATTATA, 30, vec![WATER_GUN]);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let mut draws = FASTER_HITS.to_vec();
    draws.extend_from_slice(&SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, SequenceRng::new(draws));

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(flame_body_events(&events), 0, "{events:?}");
}

#[test]
fn a_missed_contact_move_deals_no_damage_and_draws_no_roll() {
    let dex = Dex::new();
    let player = attacker(&dex, RATTATA, 30, vec![SLAM]);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let mut draws = vec![
        CONSTRUCTION_TURN_NUMBER,
        TURN_START,
        ENEMY_MOVE_SELECTION,
        ACCURACY_MISSES,
    ];
    draws.extend_from_slice(&SLOWER_HITS);
    let enemy_hp = enemy.current_hp();
    let (battle, events) = player_attacks_enemy_holder(player, enemy, SequenceRng::new(draws));

    assert_eq!(battle.enemy().current_hp(), enemy_hp, "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(flame_body_events(&events), 0, "{events:?}");
}

#[test]
fn a_holder_that_faints_to_the_hit_still_rolls_and_burns_the_attacker() {
    let dex = Dex::new();
    let player = attacker(&dex, RATTATA, 50, vec![TACKLE]);
    let enemy = magmar(&dex, 5, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &[]);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Burned, "{events:?}");
    assert_eq!(flame_body_events(&events), 1, "{events:?}");
}

#[test]
fn a_fainted_holder_that_rolls_nonzero_leaves_the_attacker_healthy() {
    let dex = Dex::new();
    let player = attacker(&dex, RATTATA, 50, vec![TACKLE]);
    let enemy = magmar(&dex, 5, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_MISSES[0], &[]);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert!(battle.enemy().is_fainted(), "{events:?}");
    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
}

#[test]
fn a_multi_hit_contact_move_rolls_flame_body_after_every_hit() {
    const HIT_COUNT_TWO: u16 = 0;
    const DOUBLE_SLAP_EFFECT_CHANCE: u16 = 0;
    for (first, second, burns) in [
        (FLAME_BODY_MISSES[0], FLAME_BODY_TRIGGERS, 1),
        (FLAME_BODY_TRIGGERS, FLAME_BODY_TRIGGERS, 1),
        (FLAME_BODY_MISSES[0], FLAME_BODY_MISSES[1], 0),
    ] {
        let dex = Dex::new();
        let player = attacker(&dex, RATTATA, 60, vec![DOUBLE_SLAP]);
        let enemy = magmar(&dex, 40, vec![TACKLE]);

        let rng = SequenceRng::new([
            CONSTRUCTION_TURN_NUMBER,
            TURN_START,
            ENEMY_MOVE_SELECTION,
            ACCURACY_HITS,
            HIT_COUNT_TWO,
            NO_CRIT,
            DAMAGE_ROLL,
            first,
            NO_CRIT,
            DAMAGE_ROLL,
            second,
            DOUBLE_SLAP_EFFECT_CHANCE,
            ACCURACY_HITS,
            NO_CRIT,
            DAMAGE_ROLL,
            DISCARDED_SECONDARY_EFFECT,
        ]);
        let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

        let status = if burns == 1 {
            Status1::Burned
        } else {
            Status1::Healthy
        };
        assert_eq!(battle.player().status1(), status, "{events:?}");
        assert_eq!(flame_body_events(&events), burns, "{events:?}");
    }
}

#[test]
fn the_burn_persists_ticks_its_literal_residual_and_halves_physical_damage() {
    let dex = Dex::new();
    let player = attacker(&dex, RATTATA, 30, vec![TACKLE]);
    let enemy = magmar(&dex, 20, vec![TACKLE]);
    let max_hp = player.stats().max_hp;
    let healthy_before = player.clone();

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert_eq!(battle.player().status1(), Status1::Burned, "persists");
    assert_eq!(burn_residual_damage(max_hp), max_hp / 8);
    assert!(
        events.contains(&BattleEvent::HurtByBurn {
            by_player: true,
            damage: burn_residual_damage(max_hp),
        }),
        "{events:?}"
    );
    let target = battle.enemy().clone();
    let mut no_draws = SequenceRng::new([]);
    let burned_damage =
        damage_before_roll(&dex, TACKLE, battle.player(), &target, true, &mut no_draws)
            .unwrap()
            .damage;
    let healthy_damage =
        damage_before_roll(&dex, TACKLE, &healthy_before, &target, true, &mut no_draws)
            .unwrap()
            .damage;
    assert!(
        burned_damage < healthy_damage,
        "{burned_damage} {healthy_damage}"
    );
}

#[test]
fn fire_typing_prevents_the_burn_after_the_activation_draw() {
    let dex = Dex::new();
    let player = attacker(&dex, SpeciesId::CHARMANDER.0, 30, vec![TACKLE]);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(flame_body_events(&events), 0, "{events:?}");
}

#[test]
fn water_veil_prevents_the_burn_after_the_activation_draw() {
    let dex = Dex::new();
    let player = attacker(&dex, SpeciesId::GOLDEEN.0, 30, vec![TACKLE]).with_ability_slot(1);
    assert_eq!(player.ability(), AbilityId::WATER_VEIL);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert_eq!(battle.player().status1(), Status1::Healthy, "{events:?}");
    assert_eq!(flame_body_events(&events), 0, "{events:?}");
}

#[test]
fn an_existing_primary_status_prevents_the_burn_after_the_activation_draw() {
    let dex = Dex::new();
    let mut player = attacker(&dex, RATTATA, 30, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert_eq!(battle.player().status1(), Status1::Poisoned, "{events:?}");
    assert_eq!(flame_body_events(&events), 0, "{events:?}");
}

#[test]
fn shield_dust_does_not_stop_the_ability_burn() {
    let dex = Dex::new();
    let player = attacker(&dex, SpeciesId::DUSTOX.0, 40, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SHIELD_DUST);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert_eq!(battle.player().status1(), Status1::Burned, "{events:?}");
    assert_eq!(flame_body_events(&events), 1, "{events:?}");
}

#[test]
fn a_synchronize_attacker_cannot_pass_the_burn_to_a_fire_typed_holder() {
    let dex = Dex::new();
    let player = attacker(&dex, SpeciesId::RALTS.0, 50, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SYNCHRONIZE);
    let enemy = magmar(&dex, 20, vec![TACKLE]);

    let rng = script(&FASTER_HITS, FLAME_BODY_TRIGGERS, &SLOWER_HITS);
    let (battle, events) = player_attacks_enemy_holder(player, enemy, rng);

    assert_eq!(battle.player().status1(), Status1::Burned, "{events:?}");
    assert_eq!(battle.enemy().status1(), Status1::Healthy, "{events:?}");
    assert!(
        events.contains(&BattleEvent::AttackerSynchronizeFireTypeProtected {
            by_player: true,
            move_id: TACKLE,
        }),
        "{events:?}"
    );
}
