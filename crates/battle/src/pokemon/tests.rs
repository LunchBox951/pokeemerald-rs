//! Battle Pokémon model tests.

use super::{
    calc_max_hp, calc_stat, compute_stats, compute_stats_with_evs, BattlePokemon, Evs, Ivs,
    MoveSlot, PpBonuses, StatStages, MAX_IV, MAX_LEVEL, MAX_MON_MOVES, MIN_LEVEL, MOVE_NONE,
    SPECIES_NONE, SPECIES_OLD_UNOWN_B, SPECIES_OLD_UNOWN_Z, SPECIES_SHEDINJA,
};
use crate::ability::LIQUID_OOZE;
use crate::damage::MoveCategory;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::{Nature, Stat};
use crate::stat_change::CLEAR_BODY;
use crate::stat_stage::StatStage;
use assets::{AbilityId, MoveId, SpeciesId, SpeciesTable};

const MAX_IVS: Ivs = Ivs {
    hp: MAX_IV,
    attack: MAX_IV,
    defense: MAX_IV,
    speed: MAX_IV,
    sp_attack: MAX_IV,
    sp_defense: MAX_IV,
};
const MAX_EFFECTIVE_EV: u8 = 252;
const MAX_EFFECTIVE_EVS: Evs = Evs {
    hp: MAX_EFFECTIVE_EV,
    attack: MAX_EFFECTIVE_EV,
    defense: MAX_EFFECTIVE_EV,
    speed: MAX_EFFECTIVE_EV,
    sp_attack: MAX_EFFECTIVE_EV,
    sp_defense: MAX_EFFECTIVE_EV,
};
const BULBASAUR: SpeciesId = SpeciesId(1);
const BULBASAUR_BASE_HP: u8 = 45;
const BULBASAUR_BASE_ATTACK: u8 = 49;
const TACKLE: MoveId = MoveId(33);
const TACKLE_BASE_PP: u8 = 35;
const SCRATCH: MoveId = MoveId(10);
const TENTACOOL: SpeciesId = SpeciesId(72);
const ZIGZAGOON: SpeciesId = SpeciesId(288);
const PICKUP: AbilityId = AbilityId(53);
const HARDY_PERSONALITY: u32 = 0x1234_5663;
const ADAMANT_PERSONALITY: u32 = Nature::Adamant.id() as u32;
const PERSONALITY_NATURE_CYCLE: u32 = 25;
const WRAPPED_ADAMANT_PERSONALITY: u32 = ADAMANT_PERSONALITY + PERSONALITY_NATURE_CYCLE;
const THREE_PP_UPS_ON_FIRST_SLOT: PpBonuses = PpBonuses::from_bits(0b0000_0011);
const THREE_PP_UPS_ON_LAST_SLOT: PpBonuses = PpBonuses::from_bits(0b1100_0000);
const TACKLE_MAX_PP_WITH_THREE_UPS: u8 = 56;

#[test]
fn calc_max_hp_matches_hand_computed_bulbasaur_at_level_5() {
    assert_eq!(calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, MAX_IV, 0, 5), 21);
}

#[test]
fn calc_max_hp_applies_ev_contribution_before_level_scaling() {
    assert_eq!(
        calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, MAX_IV, MAX_EFFECTIVE_EV, 5),
        24
    );
    let no_evs = calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, MAX_IV, 0, 5);
    assert_eq!(
        no_evs,
        calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, MAX_IV, 3, 5),
        "EV division truncates before level scaling"
    );
    assert_eq!(
        no_evs,
        calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, MAX_IV, 7, 5),
        "level scaling can truncate an EV contribution"
    );
}

#[test]
fn calc_max_hp_forces_shedinja_to_one_regardless_of_inputs() {
    assert_eq!(
        calc_max_hp(
            SPECIES_SHEDINJA,
            u8::MAX,
            MAX_IV,
            MAX_EFFECTIVE_EV,
            MAX_LEVEL
        ),
        1
    );
}

#[test]
fn calc_stat_applies_nature_after_the_base_offset() {
    let adamant_attack = calc_stat(
        BULBASAUR_BASE_ATTACK,
        MAX_IV,
        0,
        5,
        Nature::Adamant,
        Stat::Attack,
    );
    assert_eq!(adamant_attack, 12);
    assert_eq!(
        calc_stat(
            BULBASAUR_BASE_ATTACK,
            MAX_IV,
            0,
            5,
            Nature::Hardy,
            Stat::Attack
        ),
        11
    );
}

#[test]
fn calc_stat_applies_ev_contribution_before_level_scaling() {
    assert_eq!(
        calc_stat(
            BULBASAUR_BASE_ATTACK,
            MAX_IV,
            MAX_EFFECTIVE_EV,
            5,
            Nature::Hardy,
            Stat::Attack
        ),
        14
    );
}

#[test]
fn compute_stats_bundles_all_six_stats() {
    let dex = Dex::new();
    let bulbasaur = dex.species(BULBASAUR).unwrap();
    let stats = compute_stats(BULBASAUR, bulbasaur, 5, Nature::Hardy, MAX_IVS);
    assert_eq!(
        stats.max_hp,
        calc_max_hp(BULBASAUR, bulbasaur.hp, MAX_IV, 0, 5)
    );
    assert_eq!(
        stats.attack,
        calc_stat(bulbasaur.attack, MAX_IV, 0, 5, Nature::Hardy, Stat::Attack)
    );
    assert_eq!(
        stats.speed,
        calc_stat(bulbasaur.speed, MAX_IV, 0, 5, Nature::Hardy, Stat::Speed)
    );
}

#[test]
fn compute_stats_with_evs_matches_compute_stats_at_zero_evs() {
    let dex = Dex::new();
    let bulbasaur = dex.species(BULBASAUR).unwrap();
    assert_eq!(
        compute_stats(BULBASAUR, bulbasaur, 50, Nature::Adamant, MAX_IVS),
        compute_stats_with_evs(
            BULBASAUR,
            bulbasaur,
            50,
            Nature::Adamant,
            MAX_IVS,
            Evs::default()
        )
    );
}

#[test]
fn compute_stats_with_evs_applies_each_stat_byte() {
    let dex = Dex::new();
    let bulbasaur = dex.species(BULBASAUR).unwrap();
    let untrained = compute_stats_with_evs(
        BULBASAUR,
        bulbasaur,
        50,
        Nature::Hardy,
        MAX_IVS,
        Evs::default(),
    );
    let trained = compute_stats_with_evs(
        BULBASAUR,
        bulbasaur,
        50,
        Nature::Hardy,
        MAX_IVS,
        MAX_EFFECTIVE_EVS,
    );
    assert!(trained.max_hp > untrained.max_hp);
    assert!(trained.attack > untrained.attack);
    assert!(trained.defense > untrained.defense);
    assert!(trained.speed > untrained.speed);
    assert!(trained.sp_attack > untrained.sp_attack);
    assert!(trained.sp_defense > untrained.sp_defense);
}

#[test]
fn compute_stats_with_evs_forces_shedinja_to_one_hp_even_fully_ev_trained() {
    let dex = Dex::new();
    let shedinja = dex.species(SPECIES_SHEDINJA).unwrap();
    let trained_evs = Evs {
        hp: MAX_EFFECTIVE_EV,
        ..Evs::default()
    };
    let stats = compute_stats_with_evs(
        SPECIES_SHEDINJA,
        shedinja,
        MAX_LEVEL,
        Nature::Hardy,
        MAX_IVS,
        trained_evs,
    );
    assert_eq!(stats.max_hp, 1);
}

fn sample_mon(dex: &Dex) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        BULBASAUR,
        5,
        Ivs::default(),
        HARDY_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
}

#[test]
fn new_starts_at_full_hp_with_neutral_stages() {
    let dex = Dex::new();
    let mon = sample_mon(&dex);
    assert_eq!(mon.current_hp(), mon.stats().max_hp);
    assert!(!mon.is_fainted());
    assert_eq!(mon.stages(), StatStages::default());
    assert_eq!(
        mon.moves(),
        [MoveSlot {
            move_id: TACKLE,
            pp: TACKLE_BASE_PP,
        }]
    );
    assert_eq!(mon.move_at(0), Some(TACKLE));
    assert_eq!(mon.move_at(1), None);
}

#[test]
fn new_rejects_empty_and_overfull_movesets() {
    let dex = Dex::new();
    assert_eq!(
        BattlePokemon::new(&dex, BULBASAUR, 5, Ivs::default(), 0, vec![]),
        Err(BattleError::InvalidMoveCount(0))
    );
    let overfull_moves = vec![TACKLE; MAX_MON_MOVES + 1];
    assert_eq!(
        BattlePokemon::new(&dex, BULBASAUR, 5, Ivs::default(), 0, overfull_moves),
        Err(BattleError::InvalidMoveCount(MAX_MON_MOVES + 1))
    );
}

#[test]
fn new_rejects_move_none_placeholder_slots() {
    let dex = Dex::new();
    assert_eq!(
        BattlePokemon::new(
            &dex,
            BULBASAUR,
            5,
            Ivs::default(),
            0,
            vec![MOVE_NONE, TACKLE]
        ),
        Err(BattleError::PlaceholderMove(0))
    );
    assert_eq!(
        BattlePokemon::new(&dex, BULBASAUR, 5, Ivs::default(), 0, vec![MOVE_NONE]),
        Err(BattleError::PlaceholderMove(0))
    );
}

#[test]
fn new_rejects_levels_outside_the_upstream_range() {
    let dex = Dex::new();
    let build = |level| BattlePokemon::new(&dex, BULBASAUR, level, Ivs::default(), 0, vec![TACKLE]);
    let below_minimum = MIN_LEVEL - 1;
    let above_maximum = MAX_LEVEL + 1;
    assert_eq!(
        build(below_minimum),
        Err(BattleError::InvalidLevel(below_minimum))
    );
    assert_eq!(
        build(above_maximum),
        Err(BattleError::InvalidLevel(above_maximum))
    );
    assert_eq!(build(u8::MAX), Err(BattleError::InvalidLevel(u8::MAX)));
    assert!(build(MIN_LEVEL).is_ok());
    assert!(build(MAX_LEVEL).is_ok());
}

#[test]
fn new_rejects_ivs_above_the_five_bit_maximum() {
    let dex = Dex::new();
    let build = |ivs| BattlePokemon::new(&dex, BULBASAUR, 5, ivs, 0, vec![TACKLE]);
    for over in [
        Ivs {
            hp: MAX_IV + 1,
            ..Ivs::default()
        },
        Ivs {
            sp_defense: u8::MAX,
            ..Ivs::default()
        },
    ] {
        assert!(matches!(build(over), Err(BattleError::InvalidIv(_))));
    }
    assert_eq!(
        build(Ivs {
            speed: MAX_IV + 1,
            ..Ivs::default()
        }),
        Err(BattleError::InvalidIv(MAX_IV + 1))
    );
    assert!(build(MAX_IVS).is_ok());
}

#[test]
fn new_reports_unknown_species_and_moves() {
    let dex = Dex::new();
    let bad_species = SpeciesId(SpeciesTable::LEN_U16);
    assert_eq!(
        BattlePokemon::new(&dex, bad_species, 5, Ivs::default(), 0, vec![TACKLE]),
        Err(BattleError::UnknownSpecies(bad_species))
    );

    let bad_move = MoveId(60_000);
    assert_eq!(
        BattlePokemon::new(&dex, BULBASAUR, 5, Ivs::default(), 0, vec![bad_move]),
        Err(BattleError::UnknownMove(bad_move))
    );
}

#[test]
fn new_rejects_the_species_none_placeholder() {
    let dex = Dex::new();
    assert_eq!(
        BattlePokemon::new(&dex, SPECIES_NONE, 5, Ivs::default(), 0, vec![TACKLE]),
        Err(BattleError::PlaceholderSpecies)
    );
}

#[test]
fn new_rejects_the_old_unown_reserved_range_but_not_its_neighbours() {
    let dex = Dex::new();
    let reserved_midpoint = SpeciesId(u16::midpoint(SPECIES_OLD_UNOWN_B.0, SPECIES_OLD_UNOWN_Z.0));
    for species in [SPECIES_OLD_UNOWN_B, reserved_midpoint, SPECIES_OLD_UNOWN_Z] {
        assert_eq!(
            BattlePokemon::new(&dex, species, 5, Ivs::default(), 0, vec![TACKLE]),
            Err(BattleError::PlaceholderSpecies),
            "reserved id {} must be refused",
            species.0
        );
    }
    let before_reserved_range = SpeciesId(SPECIES_OLD_UNOWN_B.0 - 1);
    let after_reserved_range = SpeciesId(SPECIES_OLD_UNOWN_Z.0 + 1);
    for species in [before_reserved_range, after_reserved_range] {
        assert!(
            BattlePokemon::new(&dex, species, 5, Ivs::default(), 0, vec![TACKLE]).is_ok(),
            "real neighbour id {} must construct",
            species.0
        );
    }
}

#[test]
fn nature_is_derived_from_the_personality_value() {
    let dex = Dex::new();
    let build = |personality| {
        BattlePokemon::new(&dex, BULBASAUR, 5, MAX_IVS, personality, vec![TACKLE]).unwrap()
    };
    let adamant = build(ADAMANT_PERSONALITY);
    assert_eq!(adamant.nature(), Nature::Adamant);
    let bulbasaur = dex.species(BULBASAUR).unwrap();
    assert_eq!(
        adamant.stats(),
        compute_stats(BULBASAUR, bulbasaur, 5, Nature::Adamant, MAX_IVS)
    );
    assert_eq!(build(WRAPPED_ADAMANT_PERSONALITY).nature(), Nature::Adamant);
    assert_eq!(build(Nature::Hardy.id().into()).nature(), Nature::Hardy);
}

#[test]
fn ability_is_derived_from_the_personality_parity() {
    let dex = Dex::new();
    let build = |species: SpeciesId, personality: u32| {
        BattlePokemon::new(&dex, species, 5, MAX_IVS, personality, vec![TACKLE]).unwrap()
    };
    let even_personality = 0x88;
    let odd_personality = even_personality + 1;
    assert_eq!(build(TENTACOOL, even_personality).ability(), CLEAR_BODY);
    assert_eq!(build(TENTACOOL, odd_personality).ability(), LIQUID_OOZE);
    assert_eq!(build(ZIGZAGOON, even_personality).ability(), PICKUP);
    assert_eq!(build(ZIGZAGOON, odd_personality).ability(), PICKUP);
    assert_eq!(build(ZIGZAGOON, odd_personality).ability_slot(), 0);
    assert_eq!(build(TENTACOOL, odd_personality).ability_slot(), 1);
}

#[test]
fn apply_damage_saturates_at_zero_and_marks_fainted() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let max_hp = mon.stats().max_hp;
    mon.apply_damage(max_hp + 1000);
    assert_eq!(mon.current_hp(), 0);
    assert!(mon.is_fainted());
}

#[test]
fn new_gives_shedinja_one_max_hp_regardless_of_level_or_ivs() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(&dex, SPECIES_SHEDINJA, 20, MAX_IVS, 0, vec![SCRATCH])
        .expect("Shedinja at level 20 with Scratch is representable");
    assert_eq!(mon.stats().max_hp, 1);
    assert_eq!(mon.current_hp(), 1);
    assert!(!mon.is_fainted());
}

#[test]
fn reconciling_experience_keeps_shedinja_at_one_max_hp_across_a_level_up() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(&dex, SPECIES_SHEDINJA, 20, MAX_IVS, 0, vec![SCRATCH])
        .expect("Shedinja at level 20 with Scratch is representable");
    let growth_rate = dex.species(SPECIES_SHEDINJA).unwrap().growth_rate;
    let level_25_experience = assets::experience_for_level(growth_rate, 25).unwrap();

    mon.reconcile_saved_experience(level_25_experience);

    assert_eq!(mon.level(), 25, "fixture sanity: the level moved");
    assert_eq!(
        mon.stats().max_hp,
        1,
        "max HP stays pinned across the level-up"
    );
    assert_eq!(mon.current_hp(), 1, "still alive at Shedinja's one point");
    assert!(!mon.is_fainted());
}

#[test]
fn attacking_and_defending_stat_select_by_category() {
    let dex = Dex::new();
    let mon = sample_mon(&dex);
    assert_eq!(
        mon.attacking_stat(MoveCategory::Physical),
        (mon.stats().attack, StatStage::NEUTRAL)
    );
    assert_eq!(
        mon.attacking_stat(MoveCategory::Special),
        (mon.stats().sp_attack, StatStage::NEUTRAL)
    );
    assert_eq!(
        mon.defending_stat(MoveCategory::Physical),
        (mon.stats().defense, StatStage::NEUTRAL)
    );
    assert_eq!(
        mon.defending_stat(MoveCategory::Special),
        (mon.stats().sp_defense, StatStage::NEUTRAL)
    );
}

#[test]
fn effective_speed_applies_the_speed_stage() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    assert_eq!(mon.effective_speed(), mon.stats().speed);
    mon.stages_mut().speed = StatStage::new(2).unwrap();
    assert_eq!(mon.effective_speed(), mon.stats().speed * 2);
}

#[test]
fn deduct_pp_decrements_and_reports_exhaustion() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let starting_pp = mon.moves()[0].pp;
    mon.deduct_pp(0).unwrap();
    assert_eq!(mon.moves()[0].pp, starting_pp - 1);

    assert_eq!(mon.deduct_pp(5), Err(BattleError::InvalidMoveSlot(5)));

    for _ in 0..(starting_pp - 1) {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 0);
    assert_eq!(mon.deduct_pp(0), Err(BattleError::NoPpRemaining(0)));
}

#[test]
fn heal_restores_hp_and_all_move_pp() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let max_hp = mon.stats().max_hp;
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;

    mon.apply_damage(max_hp);
    mon.deduct_pp(0).unwrap();
    assert!(mon.is_fainted());
    assert!(mon.moves()[0].pp < base_pp);

    mon.heal(&dex).unwrap();
    assert_eq!(mon.current_hp(), max_hp);
    assert!(!mon.is_fainted());
    assert_eq!(mon.moves()[0].pp, base_pp);
}

#[test]
fn heal_is_a_no_op_on_an_already_full_mon() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let before = mon.clone();
    mon.heal(&dex).unwrap();
    assert_eq!(mon, before);
}

#[test]
fn ivs_validate_zero_through_max_iv() {
    assert!(Ivs::default().is_valid());
    assert!(MAX_IVS.is_valid());
    assert_eq!(MAX_IVS.as_array(), [MAX_IV; 6]);
    assert!(!Ivs {
        attack: MAX_IV + 1,
        ..Ivs::default()
    }
    .is_valid());
}

#[test]
fn a_new_mon_carries_no_pp_ups() {
    let dex = Dex::new();
    let mon = sample_mon(&dex);
    assert_eq!(mon.pp_bonuses(), PpBonuses::NONE);
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), mon.moves()[0].pp);
    assert_eq!(
        mon.max_pp(&dex, 1),
        Err(BattleError::InvalidMoveSlot(1)),
        "a slot this mon does not have has no maximum"
    );
}

#[test]
fn adopting_pp_bonuses_raises_and_fills_the_slot() {
    let dex = Dex::new();
    let base_pp = sample_mon(&dex).moves()[0].pp;
    let mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_FIRST_SLOT)
        .unwrap();

    assert_eq!(mon.pp_bonuses().get(0), 3);
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), TACKLE_MAX_PP_WITH_THREE_UPS);
    assert_eq!(mon.moves()[0].pp, TACKLE_MAX_PP_WITH_THREE_UPS);
    assert!(mon.moves()[0].pp > base_pp);
}

#[test]
fn pp_bonus_bits_for_unfilled_slots_are_carried_untouched() {
    let dex = Dex::new();
    let mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_LAST_SLOT)
        .unwrap();
    assert_eq!(mon.pp_bonuses().bits(), THREE_PP_UPS_ON_LAST_SLOT.bits());
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), mon.moves()[0].pp);
}

#[test]
fn heal_restores_pp_to_the_pp_up_adjusted_maximum() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_FIRST_SLOT)
        .unwrap();
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;

    for _ in 0..20 {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 36);

    mon.heal(&dex).unwrap();
    assert_eq!(mon.moves()[0].pp, TACKLE_MAX_PP_WITH_THREE_UPS);
    assert!(
        mon.moves()[0].pp > base_pp,
        "healing to base PP would silently strip the PP Ups"
    );
}

#[test]
fn an_upgraded_slot_spends_its_whole_upgraded_capacity() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, THREE_PP_UPS_ON_FIRST_SLOT)
        .unwrap();
    for _ in 0..TACKLE_MAX_PP_WITH_THREE_UPS {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 0);
    assert_eq!(mon.deduct_pp(0), Err(BattleError::NoPpRemaining(0)));
}
