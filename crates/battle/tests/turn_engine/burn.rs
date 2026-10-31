//! A burned battler: the Attack halving and its Guts exclusion, and the
//! end-of-turn residual. `Status1::Burned` is `STATUS1_BURN`
//! (`pokeemerald/include/constants/battle.h:118`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::hit::damage_before_roll;
use battle::status1::burn_residual_damage;
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
const WATER_GUN: MoveId = MoveId::WATER_GUN;
const SCRATCH: MoveId = MoveId::SCRATCH;

/// `SPECIES_RATTATA`: Normal-type, Guts in ability slot 1, faster than
/// [`ZIGZAGOON`] at equal levels.
const RATTATA: u16 = 19;
/// `SPECIES_ZIGZAGOON`: a Normal-type defender with no damage-reading ability.
const ZIGZAGOON: u16 = 288;
/// `SPECIES_SEVIPER`: Shed Skin in its only ability slot.
const SEVIPER: u16 = 379;
const GUTS_SLOT: u8 = 1;
const LEVEL: u8 = 20;

/// Draws for a turn where both battlers use Tackle: construction's turn-number
/// draw, the turn-start draw, the enemy's move selection, then per acting
/// battler accuracy, crit, damage roll, and a discarded secondary-effect draw.
const BOTH_BATTLERS_ATTACK: [u16; 11] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];

fn attacker(status: Status1, guts: bool) -> BattlePokemon {
    let mut mon = max_iv_mon(
        &Dex::new(),
        RATTATA,
        LEVEL,
        vec![TACKLE, WATER_GUN, SCRATCH],
    );
    if guts {
        mon = mon.with_ability_slot(GUTS_SLOT);
        assert_eq!(mon.ability(), AbilityId::GUTS);
    } else {
        assert_ne!(mon.ability(), AbilityId::GUTS);
    }
    mon.set_status1(status);
    mon
}

fn defender() -> BattlePokemon {
    max_iv_mon(&Dex::new(), ZIGZAGOON, LEVEL, vec![TACKLE])
}

/// `CalculateBaseDamage`'s physical chain for a same-level Normal attacker
/// using a Normal move on a neutral Normal defender, STAB applied: the
/// division by 50, the optional burn halving
/// (`pokeemerald/src/pokemon.c:3260`-`:3264`), the `+ 2`, then STAB.
fn expected_pre_roll(dex: &Dex, attack: u32, defense: u32, halved: bool) -> u32 {
    let power = u32::from(dex.move_data(TACKLE).unwrap().power);
    let level_multiplier = 2 * u32::from(LEVEL) / 5 + 2;
    let mut damage = attack * power * level_multiplier / defense / 50;
    if halved {
        damage /= 2;
    }
    (damage + 2) * 15 / 10
}

fn pre_roll(move_id: MoveId, attacker: &BattlePokemon, defender: &BattlePokemon) -> u32 {
    let mut no_draws = SequenceRng::new([]);
    damage_before_roll(
        &Dex::new(),
        move_id,
        attacker,
        defender,
        true,
        &mut no_draws,
    )
    .unwrap()
    .damage
}

#[test]
fn a_burned_attacker_halves_its_physical_damage_after_the_division_by_fifty() {
    let dex = Dex::new();
    let target = defender();
    let healthy = attacker(Status1::Healthy, false);
    let burned = attacker(Status1::Burned, false);
    let attack = healthy.stats().attack;
    let defense = target.stats().defense;

    let healthy_damage = pre_roll(TACKLE, &healthy, &target);
    let burned_damage = pre_roll(TACKLE, &burned, &target);

    assert_eq!(
        healthy_damage,
        expected_pre_roll(&dex, attack, defense, false)
    );
    assert_eq!(
        burned_damage,
        expected_pre_roll(&dex, attack, defense, true)
    );
    assert!(burned_damage < healthy_damage);
}

#[test]
fn burn_leaves_a_special_attackers_damage_alone() {
    let healthy = attacker(Status1::Healthy, false);
    let burned = attacker(Status1::Burned, false);
    let target = defender();

    assert_eq!(
        pre_roll(WATER_GUN, &burned, &target),
        pre_roll(WATER_GUN, &healthy, &target),
        "`CalculateBaseDamage` halves only inside the physical branch"
    );
}

#[test]
fn other_primary_statuses_do_not_halve_physical_damage() {
    let healthy = attacker(Status1::Healthy, false);
    let target = defender();
    for status in [Status1::Paralysed, Status1::Poisoned] {
        assert_eq!(
            pre_roll(TACKLE, &attacker(status, false), &target),
            pre_roll(TACKLE, &healthy, &target),
            "{status:?}"
        );
    }
}

#[test]
fn a_burned_guts_holder_keeps_its_boosted_attack_and_is_not_halved() {
    let dex = Dex::new();
    let target = defender();
    let healthy_guts = attacker(Status1::Healthy, true);
    let burned_guts = attacker(Status1::Burned, true);
    let poisoned_guts = attacker(Status1::Poisoned, true);
    let defense = target.stats().defense;
    let boosted_attack = healthy_guts.stats().attack * 150 / 100;

    let burned_damage = pre_roll(TACKLE, &burned_guts, &target);

    assert_eq!(
        burned_damage,
        expected_pre_roll(&dex, boosted_attack, defense, false)
    );
    assert_eq!(
        burned_damage,
        pre_roll(TACKLE, &poisoned_guts, &target),
        "burn adds nothing on top of Guts' 150%"
    );
    assert!(burned_damage > pre_roll(TACKLE, &healthy_guts, &target));
}

#[test]
fn the_burn_halving_draws_nothing_extra_from_the_rng() {
    let target = defender();
    let burned = attacker(Status1::Burned, false);
    let healthy = attacker(Status1::Healthy, false);
    for mon in [&burned, &healthy] {
        let mut rng = SequenceRng::new([1]);
        damage_before_roll(&Dex::new(), TACKLE, mon, &target, false, &mut rng).unwrap();
        rng.assert_exhausted();
    }
}

fn take_tackle_turn(player: BattlePokemon, enemy: BattlePokemon) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(BOTH_BATTLERS_ATTACK);
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    // An exact script: a burn tick that drew from the RNG would panic above
    // and one that skipped a scripted draw would fail here.
    rng.assert_exhausted();
    (battle, events)
}

#[test]
fn a_burned_battler_loses_an_eighth_of_its_max_hp_at_end_of_turn_with_no_draw() {
    let player = attacker(Status1::Burned, false);
    let max_hp = player.stats().max_hp;

    let (battle, events) = take_tackle_turn(player, defender());

    assert!(
        events.contains(&BattleEvent::HurtByBurn {
            by_player: true,
            damage: burn_residual_damage(max_hp),
        }),
        "{events:?}"
    );
    assert_eq!(burn_residual_damage(max_hp), max_hp / 8);
    assert!(!events
        .iter()
        .any(|event| matches!(event, BattleEvent::HurtByPoison { .. })));
    assert_eq!(battle.player().status1(), Status1::Burned, "burn persists");
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::HurtByBurn {
                by_player: false,
                ..
            }
        )),
        "the healthy enemy takes no tick: {events:?}"
    );
}

#[test]
fn a_burned_battlers_halved_tackle_lands_for_the_halved_damage_in_a_real_turn() {
    let burned = attacker(Status1::Burned, false);
    let target = defender();
    // The turn's damage roll draw is zero: a 100% roll.
    let expected = expected_pre_roll(
        &Dex::new(),
        burned.stats().attack,
        target.stats().defense,
        true,
    );

    let (_, events) = take_tackle_turn(burned, target);

    assert!(
        events.contains(&BattleEvent::Hit {
            by_player: true,
            move_id: TACKLE,
            damage: expected,
            is_critical: false,
        }),
        "{events:?}"
    );
}

#[test]
fn both_battlers_burned_tick_in_the_rebuilt_end_turn_order() {
    let player = attacker(Status1::Burned, false);
    let mut enemy = defender();
    enemy.set_status1(Status1::Burned);

    let (_, events) = take_tackle_turn(player, enemy);

    let tick = |by_player: bool| {
        events
            .iter()
            .position(|event| {
                matches!(event, BattleEvent::HurtByBurn { by_player: side, .. } if *side == by_player)
            })
            .unwrap_or_else(|| panic!("side {by_player} must tick: {events:?}"))
    };
    assert!(
        tick(true) < tick(false),
        "the faster Rattata ticks first: {events:?}"
    );
}

#[test]
fn a_lethal_burn_tick_faints_the_battler_and_ends_the_battle() {
    let player = attacker(Status1::Healthy, false);
    let mut enemy = defender();
    // Park the enemy one burn tick above the player's Tackle damage.
    let mut probe = SequenceRng::new([1, 0]);
    let tackle_damage =
        match battle::damage_core(&Dex::new(), TACKLE, &player, &enemy, false, &mut probe).unwrap()
        {
            battle::HitOutcome::Hit { damage, .. } => damage,
            other => panic!("Tackle must damage Zigzagoon: {other:?}"),
        };
    let tick = burn_residual_damage(enemy.stats().max_hp);
    enemy.apply_damage(enemy.stats().max_hp - (tackle_damage + tick));
    enemy.set_status1(Status1::Burned);

    let (battle, events) = take_tackle_turn(player, enemy);

    assert!(
        events.contains(&BattleEvent::HurtByBurn {
            by_player: false,
            damage: tick,
        }),
        "{events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "{events:?}"
    );
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.enemy().status1(),
        Status1::Healthy,
        "faint settlement clears the corpse's primary status"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

#[test]
fn a_shed_skin_cure_clears_the_burn_before_its_tick() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SHED_SKIN);
    player.set_status1(Status1::Burned);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(
        BOTH_BATTLERS_ATTACK
            .into_iter()
            .chain([0])
            .collect::<Vec<_>>(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();

    assert!(
        events.contains(&BattleEvent::ShedSkinCured {
            by_player: true,
            status: Status1::Burned,
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::HurtByBurn { .. })),
        "the cure precedes ENDTURN_BURN: {events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
}
