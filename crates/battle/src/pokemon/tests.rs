//! Battle Pokémon model tests.

use super::{
    calc_max_hp, calc_stat, compute_stats, compute_stats_with_evs, BattlePokemon, Evs, Ivs,
    MoveSlot, PpBonuses, StatStages, MAX_IV, MAX_LEVEL, MAX_MON_MOVES, MAX_PER_STAT_EVS,
    MAX_TOTAL_EVS, MIN_LEVEL, MOVE_NONE, SPECIES_NONE, SPECIES_OLD_UNOWN_B, SPECIES_OLD_UNOWN_Z,
    SPECIES_SHEDINJA,
};
use crate::ability::LIQUID_OOZE;
use crate::damage::MoveCategory;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::{Nature, Stat};
use crate::stat_change::CLEAR_BODY;
use crate::stat_stage::StatStage;
use crate::status1::Status1;
use assets::{AbilityId, EvYield, MoveId, SpeciesId, SpeciesTable};

/// Upstream stores each IV in five bits (`MAX_IV_MASK` = 31,
/// `pokeemerald/include/constants/pokemon.h:201`), so 32 is the first
/// unrepresentable value. Both are pinned here as literals rather than read
/// back from production's [`MAX_IV`], so that moving that constant fails these
/// tests instead of moving the fixture, the accepted boundary, and the
/// rejected input together.
const PINNED_MAX_IV: u8 = 31;
const FIRST_UNREPRESENTABLE_IV: u8 = 32;
const MAX_IVS: Ivs = Ivs {
    hp: PINNED_MAX_IV,
    attack: PINNED_MAX_IV,
    defense: PINNED_MAX_IV,
    speed: PINNED_MAX_IV,
    sp_attack: PINNED_MAX_IV,
    sp_defense: PINNED_MAX_IV,
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
/// Upstream's level range (`pokeemerald/include/constants/pokemon.h:145`-`:146`)
/// and move-slot count (`include/constants/global.h:82`), pinned as literals
/// for the same reason as [`PINNED_MAX_IV`]: derived from production's
/// [`MIN_LEVEL`]/[`MAX_LEVEL`]/[`MAX_MON_MOVES`], a drift would carry the
/// rejected input and the accepted boundary along with it and leave the tests
/// green. Each is cross-checked against its constant exactly once, at the test
/// that uses it.
const PINNED_MIN_LEVEL: u8 = 1;
const PINNED_MAX_LEVEL: u8 = 100;
const PINNED_MAX_MON_MOVES: usize = 4;
const BULBASAUR: SpeciesId = SpeciesId(1);
const BULBASAUR_BASE_HP: u8 = 45;
const BULBASAUR_BASE_ATTACK: u8 = 49;
const TACKLE: MoveId = MoveId(33);
const TACKLE_BASE_PP: u8 = 35;
const SCRATCH: MoveId = MoveId(10);
const TENTACOOL: SpeciesId = SpeciesId(72);
const ZIGZAGOON: SpeciesId = SpeciesId(288);
const PICKUP: AbilityId = AbilityId(53);
/// The old-Unown compatibility hole (`src/data/pokemon/species_info.h:5`) and
/// the real species on either side of it. Pinned as literals rather than
/// derived from [`SPECIES_OLD_UNOWN_B`] and [`SPECIES_OLD_UNOWN_Z`]: a reserved
/// range that grew to swallow Celebi or Treecko would otherwise drag these
/// fixtures along with it and leave the test green.
const PINNED_OLD_UNOWN_B: u16 = 252;
const PINNED_OLD_UNOWN_Z: u16 = 276;
const OLD_UNOWN_INTERIOR: SpeciesId = SpeciesId(260);
const CELEBI: SpeciesId = SpeciesId(251);
const TREECKO: SpeciesId = SpeciesId(277);
const HARDY_PERSONALITY: u32 = 0x1234_5663;
/// `GetNatureFromPersonality` (`pokeemerald/src/pokemon.c:5498`) is
/// `personality % 25`, and nature id 3 is Adamant. Pinned as literals rather
/// than read back from [`Nature::id`], which would reduce the assertions below
/// to a round-trip through the same table they mean to pin.
const ADAMANT_NATURE_ID: u32 = 3;
const HARDY_NATURE_ID: u32 = 0;
const ADAMANT_PERSONALITY: u32 = ADAMANT_NATURE_ID;
const PERSONALITY_NATURE_CYCLE: u32 = 25;
const WRAPPED_ADAMANT_PERSONALITY: u32 = ADAMANT_PERSONALITY + PERSONALITY_NATURE_CYCLE;
const THREE_PP_UPS_ON_FIRST_SLOT: PpBonuses = PpBonuses::from_bits(0b0000_0011);
const THREE_PP_UPS_ON_LAST_SLOT: PpBonuses = PpBonuses::from_bits(0b1100_0000);
const TACKLE_MAX_PP_WITH_THREE_UPS: u8 = 56;

#[test]
fn calc_max_hp_matches_hand_computed_bulbasaur_at_level_5() {
    assert_eq!(
        calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, PINNED_MAX_IV, 0, 5),
        21
    );
}

#[test]
fn calc_max_hp_applies_ev_contribution_before_level_scaling() {
    assert_eq!(
        calc_max_hp(
            BULBASAUR,
            BULBASAUR_BASE_HP,
            PINNED_MAX_IV,
            MAX_EFFECTIVE_EV,
            5
        ),
        24
    );
    let no_evs = calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, PINNED_MAX_IV, 0, 5);
    assert_eq!(
        no_evs,
        calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, PINNED_MAX_IV, 3, 5),
        "EV division truncates before level scaling"
    );
    assert_eq!(
        no_evs,
        calc_max_hp(BULBASAUR, BULBASAUR_BASE_HP, PINNED_MAX_IV, 7, 5),
        "level scaling can truncate an EV contribution"
    );
}

#[test]
fn calc_max_hp_forces_shedinja_to_one_regardless_of_inputs() {
    assert_eq!(
        calc_max_hp(
            SPECIES_SHEDINJA,
            u8::MAX,
            PINNED_MAX_IV,
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
        PINNED_MAX_IV,
        0,
        5,
        Nature::Adamant,
        Stat::Attack,
    );
    assert_eq!(adamant_attack, 12);
    assert_eq!(
        calc_stat(
            BULBASAUR_BASE_ATTACK,
            PINNED_MAX_IV,
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
            PINNED_MAX_IV,
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
        calc_max_hp(BULBASAUR, bulbasaur.hp, PINNED_MAX_IV, 0, 5)
    );
    assert_eq!(
        stats.attack,
        calc_stat(
            bulbasaur.attack,
            PINNED_MAX_IV,
            0,
            5,
            Nature::Hardy,
            Stat::Attack
        )
    );
    assert_eq!(
        stats.speed,
        calc_stat(
            bulbasaur.speed,
            PINNED_MAX_IV,
            0,
            5,
            Nature::Hardy,
            Stat::Speed
        )
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
    // The one deliberate cross-check: the literal slot count against
    // production's constant, so a capacity that shrank fails here rather than
    // shrinking the "overfull" fixture to match.
    assert_eq!(MAX_MON_MOVES, PINNED_MAX_MON_MOVES);
    let overfull_count = PINNED_MAX_MON_MOVES + 1;
    assert_eq!(
        BattlePokemon::new(
            &dex,
            BULBASAUR,
            5,
            Ivs::default(),
            0,
            vec![TACKLE; overfull_count]
        ),
        Err(BattleError::InvalidMoveCount(overfull_count))
    );
    assert!(
        BattlePokemon::new(
            &dex,
            BULBASAUR,
            5,
            Ivs::default(),
            0,
            vec![TACKLE; PINNED_MAX_MON_MOVES]
        )
        .is_ok(),
        "a full four-slot moveset is legal"
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
    // The one deliberate cross-check: literal boundaries against production's
    // constants, so a range that moved fails here rather than moving the
    // rejected inputs and the accepted boundaries with it.
    assert_eq!((MIN_LEVEL, MAX_LEVEL), (PINNED_MIN_LEVEL, PINNED_MAX_LEVEL));
    let below_minimum = 0;
    let above_maximum = 101;
    assert_eq!(
        build(below_minimum),
        Err(BattleError::InvalidLevel(below_minimum))
    );
    assert_eq!(
        build(above_maximum),
        Err(BattleError::InvalidLevel(above_maximum))
    );
    assert_eq!(build(u8::MAX), Err(BattleError::InvalidLevel(u8::MAX)));
    assert!(build(PINNED_MIN_LEVEL).is_ok());
    assert!(build(PINNED_MAX_LEVEL).is_ok());
}

#[test]
fn new_rejects_ivs_above_the_five_bit_maximum() {
    let dex = Dex::new();
    let build = |ivs| BattlePokemon::new(&dex, BULBASAUR, 5, ivs, 0, vec![TACKLE]);
    for over in [
        Ivs {
            hp: FIRST_UNREPRESENTABLE_IV,
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
            speed: FIRST_UNREPRESENTABLE_IV,
            ..Ivs::default()
        }),
        Err(BattleError::InvalidIv(FIRST_UNREPRESENTABLE_IV))
    );
    assert!(build(MAX_IVS).is_ok(), "31 across the board is legal");
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
    // The one deliberate cross-check: the literal fixtures against production's
    // constants, so a reserved range that moved fails here rather than moving
    // the neighbours below along with it.
    assert_eq!(
        (SPECIES_OLD_UNOWN_B.0, SPECIES_OLD_UNOWN_Z.0),
        (PINNED_OLD_UNOWN_B, PINNED_OLD_UNOWN_Z)
    );
    for species in [
        SpeciesId(PINNED_OLD_UNOWN_B),
        OLD_UNOWN_INTERIOR,
        SpeciesId(PINNED_OLD_UNOWN_Z),
    ] {
        assert_eq!(
            BattlePokemon::new(&dex, species, 5, Ivs::default(), 0, vec![TACKLE]),
            Err(BattleError::PlaceholderSpecies),
            "reserved id {} must be refused",
            species.0
        );
    }
    for species in [CELEBI, TREECKO] {
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
    assert_eq!(build(HARDY_NATURE_ID).nature(), Nature::Hardy);
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
fn a_fresh_mon_carries_no_primary_status() {
    let dex = Dex::new();
    let mon = sample_mon(&dex);
    assert_eq!(mon.status1(), Status1::Healthy);
}

#[test]
fn speed_for_turn_order_quarters_the_stage_scaled_speed_only_when_paralysed() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    mon.stages_mut().speed = StatStage::new(2).unwrap();
    let stage_scaled = mon.effective_speed();
    assert_eq!(
        mon.speed_for_turn_order(),
        stage_scaled,
        "a healthy mon's turn-order speed is unmodified"
    );

    mon.set_status1(Status1::Paralysed);
    assert_eq!(
        mon.speed_for_turn_order(),
        stage_scaled / 4,
        "the quarter divides the *stage-scaled* speed, truncating independently"
    );
    assert_eq!(
        mon.effective_speed(),
        stage_scaled,
        "effective_speed itself never carries the paralysis modifier"
    );
}

#[test]
fn speed_for_turn_order_scales_the_stage_before_quartering_not_after() {
    // Bulbasaur (base Speed 45) at level 16 with a 0 Speed IV: (2*45+0)*16/100
    // = 14 (truncated from 14.4), +5 = 19 raw Speed -- chosen so the two
    // truncating divisions below give *different* answers depending on
    // order, the way `apply_uses_multiply_then_divide_not_a_fused_fraction`
    // (`crate::stat_stage`) pins multiply-before-divide.
    //
    // Quarter-then-stage would instead give 19/4 = 4 (from 4.75), then
    // 4*10/15 = 2 (from 2.67) -- a different, wrong answer.
    const WRONG_QUARTER_FIRST_ORDER: u32 = 2;

    let dex = Dex::new();
    let mut ivs = MAX_IVS;
    ivs.speed = 0;
    let mut mon =
        BattlePokemon::new(&dex, BULBASAUR, 16, ivs, HARDY_PERSONALITY, vec![TACKLE]).unwrap();
    assert_eq!(mon.stats().speed, 19, "fixture sanity: raw Speed is 19");
    mon.stages_mut().speed = StatStage::new(-1).unwrap();
    mon.set_status1(Status1::Paralysed);

    // Stage-then-quarter (upstream order): 19*10/15 = 12 (from 12.67), then
    // 12/4 = 3.
    assert_eq!(mon.effective_speed(), 12);
    assert_eq!(
        mon.speed_for_turn_order(),
        3,
        "quartering the *already stage-scaled* 12 gives 3"
    );

    assert_ne!(mon.speed_for_turn_order(), WRONG_QUARTER_FIRST_ORDER);
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
fn heal_cures_primary_status() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    mon.set_status1(Status1::Paralysed);
    mon.heal(&dex).unwrap();
    assert_eq!(
        mon.status1(),
        Status1::Healthy,
        "HealPlayerParty zeroes MON_DATA_STATUS in the same pass as HP and PP \
         (script_pokemon_util.c:30-58)"
    );
}

#[test]
fn clear_battle_scratch_resets_stages_and_volatiles_but_not_status1() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    mon.stages_mut().speed = StatStage::new(2).unwrap();
    mon.set_status1(Status1::Paralysed);
    mon.clear_battle_scratch();
    assert_eq!(mon.stages(), StatStages::default());
    assert_eq!(
        mon.status1(),
        Status1::Paralysed,
        "paralysis outlives an ordinary win -- only a faint's own cleanup or \
         a heal cures it, and this method is shared with the post-battle \
         return-to-overworld path where the battler did not faint"
    );
}

#[test]
fn ivs_validate_zero_through_max_iv() {
    assert!(Ivs::default().is_valid());
    assert!(MAX_IVS.is_valid());
    // The one deliberate cross-check: the literal fixture against production's
    // constant, so `MAX_IV` moving off 31 fails here.
    assert_eq!(MAX_IVS.as_array(), [MAX_IV; 6]);
    assert!(!Ivs {
        attack: FIRST_UNREPRESENTABLE_IV,
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

/// A yield below both caps is added to every named stat untouched, and
/// leaves the running total ready to accept the next award.
#[test]
fn gain_evs_adds_the_yield_to_each_named_stat() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    mon.gain_evs(EvYield {
        hp: 1,
        attack: 2,
        defense: 3,
        speed: 4,
        sp_attack: 5,
        sp_defense: 6,
    });
    assert_eq!(
        mon.evs(),
        Evs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        }
    );
}

/// `MonGainEVs` is cumulative across KOs, not a per-call snapshot.
#[test]
fn gain_evs_accumulates_across_calls() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let yield_ = EvYield {
        hp: 0,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 2,
        sp_defense: 0,
    };
    mon.gain_evs(yield_);
    mon.gain_evs(yield_);
    assert_eq!(mon.evs().sp_attack, 4);
}

/// `MAX_PER_STAT_EVS` (255): a stat already close to the cap only gains
/// enough to reach it exactly, never past it.
#[test]
fn gain_evs_caps_a_single_stat_at_max_per_stat_evs() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let yield_ = EvYield {
        hp: 0,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 200,
        sp_defense: 0,
    };
    mon.gain_evs(yield_);
    assert_eq!(
        mon.evs().sp_attack,
        200,
        "fixture sanity: the first award alone stays under both caps"
    );
    // A second 200-point award would total 400, well past the 255-point
    // per-stat cap (and still under the 510-point total cap, so this pins
    // the per-stat cap specifically).
    mon.gain_evs(yield_);
    assert_eq!(mon.evs().sp_attack, u8::try_from(MAX_PER_STAT_EVS).unwrap());

    // Already at the cap: a further award adds nothing.
    mon.gain_evs(yield_);
    assert_eq!(mon.evs().sp_attack, u8::try_from(MAX_PER_STAT_EVS).unwrap());
}

/// `MAX_TOTAL_EVS` (510): once the running total reaches it, upstream's own
/// `break` (`pokeemerald/src/pokemon.c:6005`-`:6006`) stops the whole loop --
/// a later stat gets **no** award at all, not a share of whatever headroom
/// is left (there is none once the total is exactly at the cap).
#[test]
fn gain_evs_stops_the_whole_loop_once_the_total_cap_is_reached() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    // HP to 255, Attack to 255: total 510, exactly the cap, with Speed
    // (later in stat order) still untouched.
    mon.gain_evs(EvYield {
        hp: u8::try_from(MAX_PER_STAT_EVS).unwrap(),
        attack: u8::try_from(MAX_PER_STAT_EVS).unwrap(),
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let full = mon.evs();
    assert_eq!(u16::from(full.hp) + u16::from(full.attack), MAX_TOTAL_EVS);

    mon.gain_evs(EvYield {
        hp: 0,
        attack: 0,
        defense: 0,
        speed: 3,
        sp_attack: 0,
        sp_defense: 0,
    });
    assert_eq!(
        mon.evs().speed,
        0,
        "the mon is already full, so a later stat gets nothing at all"
    );
}

/// The total cap binds a stat's own award even when that stat's *own*
/// per-stat headroom is nowhere close to exhausted -- upstream applies the
/// total cap first (`pokeemerald/src/pokemon.c:6051`-`:6059`), and the two
/// caps are independent bounds on the same award, so a stat starting at `0`
/// EVs (255 points of its own headroom) still gets narrowed down to
/// whatever the whole mon has left.
#[test]
fn gain_evs_applies_the_total_cap_even_with_per_stat_headroom_to_spare() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    // HP to 255 (full) and Attack to 254 -- 509 of the 510-point total cap,
    // one point short, with Defense still untouched at `0`.
    mon.gain_evs(EvYield {
        hp: 255,
        attack: 254,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let running = mon.evs();
    assert_eq!(
        u16::from(running.hp) + u16::from(running.attack),
        MAX_TOTAL_EVS - 1,
        "fixture sanity: one point of total headroom remains"
    );

    // Defense's own per-stat cap would allow all 5 of these points; only
    // one point of *total* headroom is left for the whole mon.
    mon.gain_evs(EvYield {
        hp: 0,
        attack: 0,
        defense: 5,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    assert_eq!(
        mon.evs().defense,
        1,
        "only the one remaining point of total headroom is awarded, not \
         the full per-stat-legal 5"
    );
    assert_eq!(
        u16::from(running.hp) + u16::from(running.attack) + u16::from(mon.evs().defense),
        MAX_TOTAL_EVS
    );
}

/// `gain_evs` moves only [`BattlePokemon::evs`]: the live stat cache stays
/// untouched until a level-up folds the gain in
/// ([`BattlePokemon::raise_level_to_experience`], module docs).
#[test]
fn gain_evs_does_not_disturb_the_live_stat_cache() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let before = mon.stats();
    mon.gain_evs(EvYield {
        hp: 3,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    assert_eq!(
        mon.stats(),
        before,
        "gain_evs alone must not recompute the cache"
    );
}

/// A level-up's HP growth
/// must be measured `0`-EV-old-max to `0`-EV-new-max, the same delta a
/// freshly built mon gets -- never against an EV-aware maximum the live
/// cache never held *before* this level-up. `party::hp_hidden_by_load`'s
/// whole rebase system depends on [`BattlePokemon::stats`] staying the
/// `0`-EV floor forever (module docs); recomputing it EV-aware here adds
/// too much current HP at the moment of the level-up itself, independent
/// of anything party.rs later does with the result.
#[test]
fn a_level_up_on_a_loaded_ev_trained_mon_grows_current_hp_by_the_zero_ev_delta() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex).with_evs(Evs {
        hp: 252,
        ..Evs::default()
    });
    mon.apply_damage(3);
    let before = mon.current_hp();

    let bulbasaur = dex.species(mon.species()).unwrap();
    let award = assets::experience_for_level(bulbasaur.growth_rate, 6).unwrap() - mon.experience();
    mon.apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    assert_eq!(mon.level(), 6, "fixture sanity: the mon levelled up");

    let zero_ev_old_max =
        compute_stats(mon.species(), bulbasaur, 5, mon.nature(), mon.ivs()).max_hp;
    let zero_ev_new_max =
        compute_stats(mon.species(), bulbasaur, 6, mon.nature(), mon.ivs()).max_hp;
    let real_new_max = compute_stats_with_evs(
        mon.species(),
        bulbasaur,
        6,
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    )
    .max_hp;
    assert!(
        real_new_max > zero_ev_new_max,
        "fixture sanity: the loaded HP EV really does raise the EV-aware \
         maximum above the 0-EV one, or this test cannot distinguish the \
         two deltas"
    );
    assert_eq!(
        mon.stats().max_hp,
        zero_ev_new_max,
        "the live cache stays the 0-EV formula through this level-up -- \
         only the save-time recompute in pokeemerald-rs::party is EV-aware \
         (module docs)"
    );
    assert_eq!(
        mon.current_hp(),
        before + (zero_ev_new_max - zero_ev_old_max),
        "the level-up must grow current HP by the 0-EV delta, not one \
         computed against an EV-aware maximum the live cache never held \
         before this level-up"
    );
}

/// A KO that awards EVs and crosses a
/// level in the same turn must leave [`BattlePokemon::evs`] carrying the
/// just-gained EVs by the time the save-time recompute
/// (`pokeemerald-rs::party::merge_into_save_pokemon`) reads them back --
/// `Cmd_getexp`'s own order, `MonGainEVs` before the
/// `CalculateMonStats`-triggering exp write
/// (`pokeemerald/src/battle_script_commands.c:3420`, module docs).
/// [`crate::battle::Battle::settle_win_reward`] is the real call site for
/// the ordering; `pokeemerald-rs::party`'s own tests pin the save-file
/// outcome end to end. This pins the [`BattlePokemon`]-level half: the
/// gain survives the level-up, and [`BattlePokemon::stats`] itself stays
/// the `0`-EV formula throughout (`raise_level_to_experience`'s own module
/// docs) -- the save-time recompute, not this live cache, is what carries
/// the EV-aware block.
#[test]
fn a_ko_that_awards_evs_and_crosses_a_level_keeps_the_gain_on_evs_not_the_live_cache() {
    let dex = Dex::new();
    // Level 99, one Sp. Attack EV short of the next `ev / 4` unit, so
    // Bulbasaur's own KO yield (`sp_attack: 1`) crosses that boundary
    // exactly.
    let mut mon = BattlePokemon::new(
        &dex,
        SpeciesId(1), // Bulbasaur
        99,
        Ivs::default(),
        0x1234_5663, // % 25 == 0, so the derived nature is neutral Hardy
        vec![MoveId(33)],
    )
    .unwrap()
    .with_evs(Evs {
        sp_attack: 3,
        ..Evs::default()
    });
    let bulbasaur = dex.species(mon.species()).unwrap();

    let award =
        assets::experience_for_level(bulbasaur.growth_rate, 100).unwrap() - mon.experience();
    mon.gain_evs(bulbasaur.ev_yield);
    assert_eq!(
        mon.evs().sp_attack,
        4,
        "fixture sanity: the KO's own yield crossed the ev/4 boundary"
    );
    let pending = mon.apply_experience(&dex, award).unwrap();
    assert!(
        pending.is_none(),
        "Bulbasaur's level 100 has no learnset entry"
    );
    assert_eq!(mon.level(), 100, "fixture sanity: the mon levelled up");

    // The gain is not lost across the level-up.
    assert_eq!(
        mon.evs().sp_attack,
        4,
        "the level-up must not reset or drop the KO's own EV gain"
    );

    // The information a save-time recompute needs is present and would
    // move the block: `compute_stats_with_evs` fed `mon.evs()` differs
    // from the `0`-EV formula at the same level.
    let ev_aware = compute_stats_with_evs(
        mon.species(),
        bulbasaur,
        mon.level(),
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    );
    let zero_ev = compute_stats(
        mon.species(),
        bulbasaur,
        mon.level(),
        mon.nature(),
        mon.ivs(),
    );
    assert_ne!(
        ev_aware, zero_ev,
        "fixture sanity: the gained EV really does move the block away \
         from the 0-EV formula, or a save-time recompute reading mon.evs() \
         could not observe this KO's gain"
    );

    // But the live cache itself never sees it -- only an external,
    // save-time recompute reading `mon.evs()` does (module docs).
    assert_eq!(
        mon.stats(),
        zero_ev,
        "BattlePokemon::stats stays the 0-EV formula through this \
         level-up; only pokeemerald-rs::party's own save-time recompute is \
         EV-aware"
    );
}
