//! `ABILITYEFFECT_ABSORBING` (`battle_util.c:2676`-`:2744`): Volt Absorb,
//! Water Absorb and Flash Fire end an ordinary hit after its accuracy stage
//! and before the critical, variance and secondary draws. PP is spent once
//! before execution regardless (`Cmd_ppreduce`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::MoveId;
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction};

const WATER_GUN: MoveId = MoveId::WATER_GUN;
/// Electric, power 7, `EFFECT_ALWAYS_HIT`: consumes no accuracy draw.
const SHOCK_WAVE: MoveId = MoveId::SHOCK_WAVE;
const SCRATCH: MoveId = MoveId::SCRATCH;
/// Self-targeting and draw-free, so the opposing side adds nothing to the
/// script.
const SWORDS_DANCE: MoveId = MoveId::SWORDS_DANCE;
const FOCUS_ENERGY: MoveId = MoveId::FOCUS_ENERGY;

const RATTATA: u16 = 19;
const VAPOREON: u16 = 134;
const JOLTEON: u16 = 135;
const FLAREON: u16 = 136;
const MUDKIP: u16 = 283;

/// `TRAINER_GRUNT_AQUA_HIDEOUT_1`, scored by `AI_CheckBadMove` alone.
const GRUNT_AQUA_HIDEOUT_1: TrainerId = TrainerId(2);

/// `Battle::new` turn seed (`battle_main.c:3140`).
const CONSTRUCTION_SEED: u16 = 0;
/// Per wild turn: the random-turn number and the enemy move-slot pick.
const WILD_TURN_SETUP: [u16; 2] = [0, 0];
const ACCURACY_HIT: u16 = 0;

fn injured(mut mon: BattlePokemon, hp_lost: u32) -> BattlePokemon {
    mon.apply_damage(hp_lost);
    mon
}

/// Runs one wild turn where the player's `move_id` meets `enemy`, whose only
/// move is draw-free Swords Dance, and returns the battle, events and RNG.
fn run_wild_turn(
    attacker_species: u16,
    move_id: MoveId,
    enemy: BattlePokemon,
    move_draws: &[u16],
) -> (Battle, Vec<BattleEvent>, SequenceRng) {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, attacker_species, 5, vec![move_id]);
    let mut rng = SequenceRng::new(
        [CONSTRUCTION_SEED]
            .into_iter()
            .chain(WILD_TURN_SETUP)
            .chain(move_draws.iter().copied()),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (battle, events, rng)
}

fn no_damage_event(events: &[BattleEvent]) -> bool {
    !events
        .iter()
        .any(|event| matches!(event, BattleEvent::Hit { .. }))
}

#[test]
fn water_absorb_heals_a_quarter_after_exactly_one_accuracy_draw() {
    let dex = Dex::new();
    let vaporeon = injured(max_iv_mon(&dex, VAPOREON, 5, vec![SWORDS_DANCE]), 15);
    assert_eq!(vaporeon.ability(), assets::AbilityId::WATER_ABSORB);
    let max_hp = vaporeon.stats().max_hp;
    let hp_before = vaporeon.current_hp();
    let quarter = max_hp / 4;
    assert!(hp_before + quarter < max_hp, "fixture sanity: no HP cap");
    let pp = max_iv_mon(&dex, MUDKIP, 5, vec![WATER_GUN]).moves()[0].pp;

    let (battle, events, rng) = run_wild_turn(MUDKIP, WATER_GUN, vaporeon, &[ACCURACY_HIT]);

    assert!(events.contains(&BattleEvent::AbilityHealed {
        by_player: true,
        move_id: WATER_GUN,
        healed: quarter,
        ability: assets::AbilityId::WATER_ABSORB,
    }));
    assert!(no_damage_event(&events), "{events:?}");
    assert_eq!(battle.enemy().current_hp(), hp_before + quarter);
    assert_eq!(battle.player().moves()[0].pp, pp - 1);
    assert_eq!(rng.draws(), 4, "seed, setup, and one accuracy draw only");
    rng.assert_exhausted();
}

#[test]
fn volt_absorb_heals_a_quarter_from_an_always_hit_move_with_no_draws() {
    let dex = Dex::new();
    let jolteon = injured(max_iv_mon(&dex, JOLTEON, 5, vec![SWORDS_DANCE]), 15);
    assert_eq!(jolteon.ability(), assets::AbilityId::VOLT_ABSORB);
    let quarter = jolteon.stats().max_hp / 4;
    let hp_before = jolteon.current_hp();
    let pp = max_iv_mon(&dex, RATTATA, 5, vec![SHOCK_WAVE]).moves()[0].pp;

    let (battle, events, rng) = run_wild_turn(RATTATA, SHOCK_WAVE, jolteon, &[]);

    assert!(events.contains(&BattleEvent::AbilityHealed {
        by_player: true,
        move_id: SHOCK_WAVE,
        healed: quarter,
        ability: assets::AbilityId::VOLT_ABSORB,
    }));
    assert!(no_damage_event(&events), "{events:?}");
    assert_eq!(battle.enemy().current_hp(), hp_before + quarter);
    assert_eq!(battle.player().moves()[0].pp, pp - 1);
    assert_eq!(rng.draws(), 3, "seed and setup; Shock Wave never rolls");
    rng.assert_exhausted();
}

#[test]
fn absorbed_healing_is_capped_at_max_hp_and_reports_the_actual_gain() {
    let dex = Dex::new();
    let vaporeon = injured(max_iv_mon(&dex, VAPOREON, 5, vec![SWORDS_DANCE]), 1);

    let (battle, events, rng) = run_wild_turn(MUDKIP, WATER_GUN, vaporeon, &[ACCURACY_HIT]);

    assert!(events.contains(&BattleEvent::AbilityHealed {
        by_player: true,
        move_id: WATER_GUN,
        healed: 1,
        ability: assets::AbilityId::WATER_ABSORB,
    }));
    assert_eq!(battle.enemy().current_hp(), battle.enemy().stats().max_hp);
    rng.assert_exhausted();
}

#[test]
fn an_absorbing_target_at_full_hp_reports_the_ability_made_the_move_useless_and_still_spends_pp() {
    let dex = Dex::new();
    let vaporeon = max_iv_mon(&dex, VAPOREON, 5, vec![SWORDS_DANCE]);
    let max_hp = vaporeon.stats().max_hp;
    let pp = max_iv_mon(&dex, MUDKIP, 5, vec![WATER_GUN]).moves()[0].pp;

    let (battle, events, rng) = run_wild_turn(MUDKIP, WATER_GUN, vaporeon, &[ACCURACY_HIT]);

    assert!(events.contains(&BattleEvent::AbsorbMadeMoveUseless {
        by_player: true,
        move_id: WATER_GUN,
        ability: assets::AbilityId::WATER_ABSORB,
    }));
    assert!(no_damage_event(&events), "{events:?}");
    assert_eq!(battle.enemy().current_hp(), max_hp);
    assert_eq!(battle.player().moves()[0].pp, pp - 1);
    rng.assert_exhausted();
}

#[test]
fn a_move_of_another_type_still_hits_an_absorbing_target() {
    let dex = Dex::new();
    let vaporeon = max_iv_mon(&dex, VAPOREON, 5, vec![SWORDS_DANCE]);

    let (_, events, rng) = run_wild_turn(MUDKIP, SCRATCH, vaporeon, &[0; 4]);

    assert!(events.iter().any(|event| matches!(
        event,
        BattleEvent::Hit {
            by_player: true,
            move_id: SCRATCH,
            ..
        }
    )));
    rng.assert_exhausted();
}

#[test]
fn flash_fire_has_no_executable_fire_move_to_absorb_and_water_still_hits_it() {
    let dex = Dex::new();
    let flareon = max_iv_mon(&dex, FLAREON, 5, vec![SWORDS_DANCE]);
    assert_eq!(flareon.ability(), assets::AbilityId::FLASH_FIRE);

    let (_, events, rng) = run_wild_turn(MUDKIP, WATER_GUN, flareon, &[0; 4]);

    assert!(events.iter().any(|event| matches!(
        event,
        BattleEvent::Hit {
            move_id: WATER_GUN,
            ..
        }
    )));
    rng.assert_exhausted();
}

/// The trainer's only move is Water Gun against a Water Absorb holder:
/// `AI_CheckBadMove` scores it down but, as the sole move, it is still
/// chosen and executed -- and absorbed, dealing no damage.
#[test]
fn a_trainer_whose_only_move_is_absorbed_deals_no_damage() {
    let dex = Dex::new();
    let vaporeon = injured(max_iv_mon(&dex, VAPOREON, 5, vec![FOCUS_ENERGY]), 15);
    let hp_before = vaporeon.current_hp();
    let quarter = vaporeon.stats().max_hp / 4;
    let enemy = max_iv_mon(&dex, MUDKIP, 5, vec![WATER_GUN]);
    let pp = enemy.moves()[0].pp;
    let mut rng = SequenceRng::new(
        [CONSTRUCTION_SEED, 0] // construction seed, turn seed
            .into_iter()
            .chain([0; 4]) // simulated-damage rolls
            .chain([0]) // final move pick
            .chain([ACCURACY_HIT]), // absorbed Water Gun
    );
    let mut battle =
        Battle::new_trainer(dex, vaporeon, GRUNT_AQUA_HIDEOUT_1, vec![enemy], &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(events.contains(&BattleEvent::AbilityHealed {
        by_player: false,
        move_id: WATER_GUN,
        healed: quarter,
        ability: assets::AbilityId::WATER_ABSORB,
    }));
    assert!(no_damage_event(&events), "{events:?}");
    assert_eq!(battle.player().current_hp(), hp_before + quarter);
    assert_eq!(battle.enemy().moves()[0].pp, pp - 1);
    assert_eq!(rng.draws(), 8);
    rng.assert_exhausted();
}
