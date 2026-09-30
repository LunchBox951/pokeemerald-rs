//! Pins [`BattlePokemon`] construction and validation, its derived
//! nature/ability/stat accessors, turn-order speed, and its HP/status/PP
//! lifecycle: damage, heal, faint, and battle-scratch reset.

use super::super::{
    compute_stats, BattlePokemon, Ivs, MoveSlot, StatStages, MAX_IV, MAX_LEVEL, MAX_MON_MOVES,
    MIN_LEVEL, MOVE_NONE, SPECIES_NONE, SPECIES_OLD_UNOWN_B, SPECIES_OLD_UNOWN_Z, SPECIES_SHEDINJA,
};
use super::shared::{
    sample_mon, BULBASAUR, FIRST_UNREPRESENTABLE_IV, HARDY_PERSONALITY, MAX_IVS, TACKLE,
};
use crate::ability::LIQUID_OOZE;
use crate::damage::MoveCategory;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::Nature;
use crate::stat_change::CLEAR_BODY;
use crate::stat_stage::StatStage;
use crate::status1::Status1;
use assets::{AbilityId, MoveId, SpeciesId, SpeciesTable};

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
/// `GetNatureFromPersonality` (`pokeemerald/src/pokemon.c:5498`) is
/// `personality % 25`, and nature id 3 is Adamant. Pinned as literals rather
/// than read back from [`Nature::id`], which would reduce the assertions below
/// to a round-trip through the same table they mean to pin.
const ADAMANT_NATURE_ID: u32 = 3;
const HARDY_NATURE_ID: u32 = 0;
const ADAMANT_PERSONALITY: u32 = ADAMANT_NATURE_ID;
const PERSONALITY_NATURE_CYCLE: u32 = 25;
const WRAPPED_ADAMANT_PERSONALITY: u32 = ADAMANT_PERSONALITY + PERSONALITY_NATURE_CYCLE;
/// Upstream's level range (`pokeemerald/include/constants/pokemon.h:145`-`:146`)
/// and move-slot count (`include/constants/global.h:82`), pinned as literals
/// for the same reason as [`PINNED_MAX_IV`](super::shared::PINNED_MAX_IV):
/// derived from production's [`MIN_LEVEL`]/[`MAX_LEVEL`]/[`MAX_MON_MOVES`], a
/// drift would carry the rejected input and the accepted boundary along with
/// it and leave the tests green. Each is cross-checked against its constant
/// exactly once, at the test that uses it.
const PINNED_MIN_LEVEL: u8 = 1;
const PINNED_MAX_LEVEL: u8 = 100;
const PINNED_MAX_MON_MOVES: usize = 4;
/// `SPECIES_MACHOP`: Guts in its primary ability slot.
const MACHOP: SpeciesId = SpeciesId(66);
/// `SPECIES_MILOTIC`: Marvel Scale in its primary (and only) ability slot.
const MILOTIC: SpeciesId = SpeciesId(329);
/// `SPECIES_REMORAID`: Hustle in its primary (and only) ability slot.
const REMORAID: SpeciesId = SpeciesId(223);

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
fn attacking_stat_raises_a_hustle_holders_physical_attack() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(&dex, REMORAID, 5, MAX_IVS, 0, vec![TACKLE]).unwrap();
    assert_eq!(mon.ability(), AbilityId::HUSTLE);
    let raw_attack = mon.stats().attack;
    let boosted_attack = 150 * raw_attack / 100;

    assert_eq!(
        mon.attacking_stat(MoveCategory::Physical),
        (boosted_attack, StatStage::NEUTRAL),
        "a Hustle holder's physical Attack is raised 150%, unconditional on status"
    );
    assert_eq!(
        mon.attacking_stat(MoveCategory::Special),
        (mon.stats().sp_attack, StatStage::NEUTRAL),
        "Hustle never touches Special Attack"
    );
}

#[test]
fn attacking_stat_raises_only_a_statused_guts_holders_physical_attack() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(&dex, MACHOP, 5, MAX_IVS, 0, vec![TACKLE]).unwrap();
    assert_eq!(mon.ability(), AbilityId::GUTS);
    let raw_attack = mon.stats().attack;
    let boosted_attack = 150 * raw_attack / 100;

    assert_eq!(
        mon.attacking_stat(MoveCategory::Physical),
        (raw_attack, StatStage::NEUTRAL),
        "a healthy Guts holder's physical Attack is unmodified"
    );
    assert_eq!(
        mon.attacking_stat(MoveCategory::Special),
        (mon.stats().sp_attack, StatStage::NEUTRAL),
        "Guts never touches Special Attack, healthy or not"
    );

    for status in [Status1::Paralysed, Status1::Poisoned] {
        mon.set_status1(status);
        assert_eq!(
            mon.attacking_stat(MoveCategory::Physical),
            (boosted_attack, StatStage::NEUTRAL),
            "{status:?}"
        );
        assert_eq!(
            mon.attacking_stat(MoveCategory::Special),
            (mon.stats().sp_attack, StatStage::NEUTRAL),
            "{status:?}: Special Attack still never changes"
        );
    }
}

#[test]
fn defending_stat_raises_only_a_statused_marvel_scale_holders_physical_defense() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(&dex, MILOTIC, 5, MAX_IVS, 0, vec![TACKLE]).unwrap();
    assert_eq!(mon.ability(), AbilityId::MARVEL_SCALE);
    let raw_defense = mon.stats().defense;
    let boosted_defense = 150 * raw_defense / 100;

    assert_eq!(
        mon.defending_stat(MoveCategory::Physical),
        (raw_defense, StatStage::NEUTRAL),
        "a healthy Marvel Scale holder's physical Defense is unmodified"
    );
    assert_eq!(
        mon.defending_stat(MoveCategory::Special),
        (mon.stats().sp_defense, StatStage::NEUTRAL),
        "Marvel Scale never touches Special Defense, healthy or not"
    );

    for status in [Status1::Paralysed, Status1::Poisoned] {
        mon.set_status1(status);
        assert_eq!(
            mon.defending_stat(MoveCategory::Physical),
            (boosted_defense, StatStage::NEUTRAL),
            "{status:?}"
        );
        assert_eq!(
            mon.defending_stat(MoveCategory::Special),
            (mon.stats().sp_defense, StatStage::NEUTRAL),
            "{status:?}: Special Defense still never changes"
        );
    }
}

#[test]
fn a_statused_but_unrelated_ability_leaves_both_accessors_unmodified() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(&dex, BULBASAUR, 5, MAX_IVS, 0, vec![TACKLE]).unwrap();
    assert_ne!(mon.ability(), AbilityId::GUTS);
    assert_ne!(mon.ability(), AbilityId::MARVEL_SCALE);
    mon.set_status1(Status1::Paralysed);
    assert_eq!(
        mon.attacking_stat(MoveCategory::Physical),
        (mon.stats().attack, StatStage::NEUTRAL)
    );
    assert_eq!(
        mon.defending_stat(MoveCategory::Physical),
        (mon.stats().defense, StatStage::NEUTRAL)
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
    mon.volatiles_mut().set_confusion(3);
    mon.set_status1(Status1::Paralysed);
    mon.clear_battle_scratch();
    assert_eq!(mon.stages(), StatStages::default());
    assert!(
        !mon.volatiles().confused(),
        "confusion is battle-only scratch, cleared with every other volatile"
    );
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
