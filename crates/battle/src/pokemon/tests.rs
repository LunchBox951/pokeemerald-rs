//! Unit tests for [`super`]'s battler type (S-6) -- construction
//! invariants, the stat formulas, PP (including PP Up capacity, issue
//! #304), and healing.
//!
//! Split out of `pokemon.rs` when issue #304 gave that module two siblings
//! (`learn`, `pp_bonuses`): the tests are their own file for the same reason
//! `party.rs`'s are, so the type and its pins each read in one screen.

use super::{
    calc_max_hp, calc_stat, compute_stats, compute_stats_with_evs, BattlePokemon, Evs, Ivs,
    MoveSlot, PpBonuses, StatStages, MAX_IV, MAX_LEVEL, MIN_LEVEL, MOVE_NONE, SPECIES_NONE,
    SPECIES_OLD_UNOWN_B, SPECIES_OLD_UNOWN_Z, SPECIES_SHEDINJA,
};
use crate::damage::MoveCategory;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::{Nature, Stat};
use crate::stat_stage::StatStage;
use assets::{MoveId, SpeciesId, SpeciesTable};

/// Max Gen-3 individual values (stat rolls — *not* a cryptographic
/// initialization vector; see [`Ivs`]): every `31` below is `MAX_IV_MASK`.
const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

#[test]
fn calc_max_hp_matches_hand_computed_bulbasaur_at_level_5() {
    // Bulbasaur base HP 45, IV 31 (max), level 5, 0 EV:
    // n = 2*45+31+0/4 = 121; 121*5/100 = 6 (605/100 truncated); +5+10 = 21.
    assert_eq!(calc_max_hp(SpeciesId(1), 45, 31, 0, 5), 21);
}

#[test]
fn calc_max_hp_folds_ev_over_four_into_the_parenthesised_sum() {
    // Same Bulbasaur, 252 EV (the upstream single-stat cap): ev/4 = 63.
    // n = 2*45+31+63 = 184; 184*5/100 = 9 (920/100 truncated); +5+10 = 24.
    assert_eq!(calc_max_hp(SpeciesId(1), 45, 31, 252, 5), 24);
    // ev/4's own truncation happens before the level scaling, and the two
    // truncations bite in that order: 3 floor-divides to 0, so it cannot
    // reach the total at all. 7 floor-divides to 1 -- n = 122 -- but
    // 122 * 5 / 100 is still 6, so `* level / 100` absorbs that point and
    // the total is unchanged here too, for the second reason rather than
    // the first.
    assert_eq!(
        calc_max_hp(SpeciesId(1), 45, 31, 0, 5),
        calc_max_hp(SpeciesId(1), 45, 31, 3, 5)
    );
    assert_eq!(
        calc_max_hp(SpeciesId(1), 45, 31, 0, 5),
        calc_max_hp(SpeciesId(1), 45, 31, 7, 5)
    );
}

#[test]
fn calc_max_hp_forces_shedinja_to_one_regardless_of_inputs() {
    // Every input the ordinary formula reads -- base HP, IV, EV, level --
    // pushed to its upstream maximum; Shedinja's flat `1`
    // (`pokeemerald/src/pokemon.c:2845`-`:2848`) bypasses all of them.
    assert_eq!(calc_max_hp(SPECIES_SHEDINJA, 255, 31, 252, 100), 1);
}

#[test]
fn calc_stat_applies_the_nature_modifier_after_the_plus_five() {
    // Bulbasaur base Attack 49, IV 31, level 5, 0 EV, Adamant (+Attack):
    // n = 2*49+31+0/4 = 129; 129*5/100 = 6 (645/100); +5 = 11; *110/100 = 12
    // (1210/100 truncated).
    let n = calc_stat(49, 31, 0, 5, Nature::Adamant, Stat::Attack);
    assert_eq!(n, 12);
    // Same base/IV/level, neutral nature: no scaling, stays 11.
    assert_eq!(calc_stat(49, 31, 0, 5, Nature::Hardy, Stat::Attack), 11);
}

#[test]
fn calc_stat_folds_ev_over_four_into_the_parenthesised_sum() {
    // Same Bulbasaur Attack, 252 EV: ev/4 = 63.
    // n = 2*49+31+63 = 192; 192*5/100 = 9 (960/100); +5 = 14.
    assert_eq!(calc_stat(49, 31, 252, 5, Nature::Hardy, Stat::Attack), 14);
}

#[test]
fn compute_stats_bundles_all_six_stats() {
    let dex = Dex::new();
    let bulbasaur = dex.species(SpeciesId(1)).unwrap();
    let stats = compute_stats(SpeciesId(1), bulbasaur, 5, Nature::Hardy, MAX_IVS);
    assert_eq!(
        stats.max_hp,
        calc_max_hp(SpeciesId(1), bulbasaur.hp, 31, 0, 5)
    );
    assert_eq!(
        stats.attack,
        calc_stat(bulbasaur.attack, 31, 0, 5, Nature::Hardy, Stat::Attack)
    );
    assert_eq!(
        stats.speed,
        calc_stat(bulbasaur.speed, 31, 0, 5, Nature::Hardy, Stat::Speed)
    );
}

#[test]
fn compute_stats_with_evs_matches_compute_stats_at_zero_evs() {
    // `compute_stats` is `compute_stats_with_evs` at `Evs::default()`
    // (module docs) -- pin the delegation directly.
    let dex = Dex::new();
    let bulbasaur = dex.species(SpeciesId(1)).unwrap();
    assert_eq!(
        compute_stats(SpeciesId(1), bulbasaur, 50, Nature::Adamant, MAX_IVS),
        compute_stats_with_evs(
            SpeciesId(1),
            bulbasaur,
            50,
            Nature::Adamant,
            MAX_IVS,
            Evs::default()
        )
    );
}

#[test]
fn compute_stats_with_evs_raises_every_stat_the_ev_bytes_train() {
    // A maximally EV-trained mon (252 in every stat, the upstream
    // single-stat cap) must file a strictly larger block than the same mon
    // at 0 EVs -- the `ev / 4` term CALC_STAT adds (module docs).
    let dex = Dex::new();
    let bulbasaur = dex.species(SpeciesId(1)).unwrap();
    let untrained = compute_stats_with_evs(
        SpeciesId(1),
        bulbasaur,
        50,
        Nature::Hardy,
        MAX_IVS,
        Evs::default(),
    );
    let trained_evs = Evs {
        hp: 252,
        attack: 252,
        defense: 252,
        speed: 252,
        sp_attack: 252,
        sp_defense: 252,
    };
    let trained = compute_stats_with_evs(
        SpeciesId(1),
        bulbasaur,
        50,
        Nature::Hardy,
        MAX_IVS,
        trained_evs,
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
    // Shedinja's own base HP is 1 in the extracted data, so this also pins
    // that the flat `1` isn't merely coincidental with the ordinary
    // formula's output at low inputs -- run it at a high level and full HP
    // EVs, where the ordinary formula would give something far larger.
    let dex = Dex::new();
    let shedinja = dex.species(SPECIES_SHEDINJA).unwrap();
    let trained_evs = Evs {
        hp: 252,
        ..Evs::default()
    };
    let stats = compute_stats_with_evs(
        SPECIES_SHEDINJA,
        shedinja,
        100,
        Nature::Hardy,
        MAX_IVS,
        trained_evs,
    );
    assert_eq!(stats.max_hp, 1);
}

fn sample_mon(dex: &Dex) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        SpeciesId(1), // Bulbasaur
        5,
        Ivs::default(),
        0x1234_5663,      // % 25 == 0, so the derived nature is neutral Hardy
        vec![MoveId(33)], // Tackle
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
            move_id: MoveId(33),
            pp: 35, // Tackle's base PP
        }]
    );
    assert_eq!(mon.move_at(0), Some(MoveId(33)));
    assert_eq!(
        mon.move_at(1),
        None,
        "an unknown slot is upstream MOVE_NONE"
    );
}

#[test]
fn new_rejects_a_moveset_that_upstream_cannot_represent() {
    let dex = Dex::new();
    // Empty: `struct BattlePokemon` always has four slots and a battler
    // with none of them filled never reaches the engine -- and an empty
    // moveset would make the wild opponent's rejection loop spin forever.
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 5, Ivs::default(), 0, vec![]),
        Err(BattleError::InvalidMoveCount(0))
    );
    // Overfull: MAX_MON_MOVES is 4 (`include/constants/global.h:82`).
    assert_eq!(
        BattlePokemon::new(
            &dex,
            SpeciesId(1),
            5,
            Ivs::default(),
            0,
            vec![MoveId(33); 5]
        ),
        Err(BattleError::InvalidMoveCount(5))
    );
}

#[test]
fn new_rejects_move_none_placeholder_slots() {
    let dex = Dex::new();
    // MOVE_NONE is the *empty slot* marker, never a known move:
    // `CheckMoveLimitations` rules it out (`battle_util.c:1098`) and the
    // wild rejection loop retries past it
    // (`battle_controller_opponent.c:1599`-`:1601`).
    assert_eq!(
        BattlePokemon::new(
            &dex,
            SpeciesId(1),
            5,
            Ivs::default(),
            0,
            vec![MOVE_NONE, MoveId(33)]
        ),
        Err(BattleError::PlaceholderMove(0))
    );
    // An all-placeholder moveset passes the non-empty count check, so the
    // placeholder check is what actually rejects it.
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 5, Ivs::default(), 0, vec![MOVE_NONE]),
        Err(BattleError::PlaceholderMove(0))
    );
}

#[test]
fn new_rejects_levels_outside_the_upstream_range() {
    let dex = Dex::new();
    let build = |level| {
        BattlePokemon::new(
            &dex,
            SpeciesId(1),
            level,
            Ivs::default(),
            0,
            vec![MoveId(33)],
        )
    };
    // MIN_LEVEL..=MAX_LEVEL is 1..=100 (`include/constants/pokemon.h:145`-`:146`).
    assert_eq!(build(0), Err(BattleError::InvalidLevel(0)));
    assert_eq!(build(101), Err(BattleError::InvalidLevel(101)));
    assert_eq!(build(255), Err(BattleError::InvalidLevel(255)));
    assert!(build(MIN_LEVEL).is_ok());
    assert!(build(MAX_LEVEL).is_ok());
}

#[test]
fn new_rejects_ivs_above_the_five_bit_maximum() {
    let dex = Dex::new();
    let build = |ivs| BattlePokemon::new(&dex, SpeciesId(1), 5, ivs, 0, vec![MoveId(33)]);
    // Upstream stores each IV in five bits (MAX_IV_MASK = 31,
    // `include/constants/pokemon.h:201`), so 32+ is unrepresentable.
    for over in [
        Ivs {
            hp: 32,
            ..Ivs::default()
        },
        Ivs {
            sp_defense: 255,
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
    assert!(build(MAX_IVS).is_ok(), "31 across the board is legal");
}

#[test]
fn new_reports_unknown_species_and_moves() {
    let dex = Dex::new();
    let bad_species = SpeciesId(SpeciesTable::LEN_U16);
    assert_eq!(
        BattlePokemon::new(&dex, bad_species, 5, Ivs::default(), 0, vec![MoveId(33)]),
        Err(BattleError::UnknownSpecies(bad_species))
    );

    let bad_move = MoveId(60_000);
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 5, Ivs::default(), 0, vec![bad_move]),
        Err(BattleError::UnknownMove(bad_move))
    );
}

#[test]
fn new_rejects_the_species_none_placeholder() {
    let dex = Dex::new();
    // Slot 0 of `gSpeciesInfo` exists but is the all-zero SPECIES_NONE
    // placeholder: addressable is not the same as real, so construction
    // refuses it rather than building a fightable mon from zeroes.
    assert_eq!(
        BattlePokemon::new(&dex, SPECIES_NONE, 5, Ivs::default(), 0, vec![MoveId(33)]),
        Err(BattleError::PlaceholderSpecies)
    );
}

#[test]
fn new_rejects_the_old_unown_reserved_range_but_not_its_neighbours() {
    let dex = Dex::new();
    // 252..=276 are the Gen-2 compatibility holes carrying the dummy
    // OLD_UNOWN_SPECIES_INFO row; the ids on either side are Celebi
    // (251) and Treecko (277), which must keep working.
    for species in [SPECIES_OLD_UNOWN_B, SpeciesId(260), SPECIES_OLD_UNOWN_Z] {
        assert_eq!(
            BattlePokemon::new(&dex, species, 5, Ivs::default(), 0, vec![MoveId(33)]),
            Err(BattleError::PlaceholderSpecies),
            "reserved id {} must be refused",
            species.0
        );
    }
    for species in [SpeciesId(251), SpeciesId(277)] {
        assert!(
            BattlePokemon::new(&dex, species, 5, Ivs::default(), 0, vec![MoveId(33)]).is_ok(),
            "real neighbour id {} must construct",
            species.0
        );
    }
}

#[test]
fn nature_is_derived_from_the_personality_value() {
    let dex = Dex::new();
    let build = |personality| {
        BattlePokemon::new(
            &dex,
            SpeciesId(1),
            5,
            MAX_IVS,
            personality,
            vec![MoveId(33)],
        )
        .unwrap()
    };
    // GetNatureFromPersonality (`pokemon.c:5498`): personality % 25.
    // Nature id 3 is Adamant (+Atk), so a mon built at personality 3
    // carries Adamant *and* Adamant-modified stats — a contradictory
    // nature/personality pair is unrepresentable by construction.
    let adamant = build(3);
    assert_eq!(adamant.nature(), Nature::Adamant);
    let bulbasaur = dex.species(SpeciesId(1)).unwrap();
    assert_eq!(
        adamant.stats(),
        compute_stats(SpeciesId(1), bulbasaur, 5, Nature::Adamant, MAX_IVS)
    );
    // 28 % 25 == 3 wraps to the same nature.
    assert_eq!(build(28).nature(), Nature::Adamant);
    assert_eq!(build(0).nature(), Nature::Hardy);
}

#[test]
fn ability_is_derived_from_the_personality_parity() {
    let dex = Dex::new();
    let build = |species: u16, personality: u32| {
        BattlePokemon::new(
            &dex,
            SpeciesId(species),
            5,
            MAX_IVS,
            personality,
            vec![MoveId(33)],
        )
        .unwrap()
    };
    // Tentacool carries two abilities, so bit 0 selects the slot
    // (`CreateBoxMon`, `src/pokemon.c:2296`-`:2300`): Clear Body at 29,
    // Liquid Ooze at 64 (`gSpeciesInfo`).
    assert_eq!(build(72, 0x88).ability().0, 29);
    assert_eq!(build(72, 0x89).ability().0, 64);
    // A lone-ability species ignores parity entirely — and stores a
    // clear slot bit, exactly the `abilityNum` `CreateBoxMon` leaves
    // unwritten for it (`:2296`-`:2300`), so the serialized save bit
    // matches Emerald's: Zigzagoon is Pickup in slot 0 on both
    // parities.
    assert_eq!(build(288, 0).ability().0, 53);
    assert_eq!(build(288, 1).ability().0, 53);
    assert_eq!(build(288, 1).ability_slot(), 0);
    // The dual-ability odd-parity mon, by contrast, stores slot 1.
    assert_eq!(build(72, 0x89).ability_slot(), 1);
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

/// Issue #401: a constructed Shedinja starts at the flat `1` HP
/// `CalculateMonStats` forces (`pokeemerald/src/pokemon.c:2845`-`:2848`),
/// not the tens of HP the ordinary formula would give Shedinja's own base
/// HP (1), IV, and level-20 scaling.
#[test]
fn new_gives_shedinja_one_max_hp_regardless_of_level_or_ivs() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        SPECIES_SHEDINJA,
        20,
        MAX_IVS,
        0,
        vec![MoveId(10)], // Scratch, Shedinja's level-1 learnset entry
    )
    .expect("Shedinja at level 20 with Scratch is representable");
    assert_eq!(mon.stats().max_hp, 1);
    assert_eq!(mon.current_hp(), 1);
    assert!(!mon.is_fainted());
}

/// Issue #401: the shared level-recalculation path
/// (`raise_level_to_experience`, reached here through
/// `reconcile_saved_experience` exactly as the save decoder reaches it)
/// must keep Shedinja's max HP pinned at `1` across a level change, not
/// just at construction.
#[test]
fn reconciling_experience_keeps_shedinja_at_one_max_hp_across_a_level_up() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(&dex, SPECIES_SHEDINJA, 20, MAX_IVS, 0, vec![MoveId(10)])
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

    // Drain the slot through the only mutation the type offers: the
    // moveset itself is not reachable for writing (`oop-boundaries`).
    for _ in 0..(starting_pp - 1) {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 0);
    assert_eq!(mon.deduct_pp(0), Err(BattleError::NoPpRemaining(0)));
}

/// `HealPlayerParty` (`pokeemerald/src/script_pokemon_util.c:30-59`):
/// full HP, and every move's PP restored to its base value.
#[test]
fn heal_restores_hp_and_every_moves_pp() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let max_hp = mon.stats().max_hp;
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;

    mon.apply_damage(max_hp); // faint it
    mon.deduct_pp(0).unwrap();
    assert!(mon.is_fainted());
    assert!(mon.moves()[0].pp < base_pp);

    mon.heal(&dex).unwrap();
    assert_eq!(mon.current_hp(), max_hp);
    assert!(!mon.is_fainted());
    assert_eq!(mon.moves()[0].pp, base_pp);
}

/// A mon already at full HP/PP is unaffected -- `heal` is idempotent,
/// matching `HealPlayerParty` running against an already-healthy party
/// (upstream never gates the call on need).
#[test]
fn heal_is_a_no_op_on_an_already_full_mon() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let before = mon.clone();
    mon.heal(&dex).unwrap();
    assert_eq!(mon, before);
}

#[test]
fn ivs_report_their_upstream_five_bit_range() {
    // Gen-3 stat rolls, not cryptographic initialization vectors.
    assert!(Ivs::default().is_valid());
    assert!(MAX_IVS.is_valid());
    assert_eq!(MAX_IVS.as_array(), [MAX_IV; 6]);
    assert!(!Ivs {
        attack: MAX_IV + 1,
        ..Ivs::default()
    }
    .is_valid());
}

/// A freshly built mon has no PP Ups: `CreateBoxMon` never writes
/// `ppBonuses`, so every slot's capacity is the move's own base PP.
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

/// `CalculatePPWithBonus` through the mon: adopting a saved `ppBonuses`
/// byte raises the slot's capacity *and* fills it, which is the state a
/// freshly decoded save is in before its own spent PP is wound back.
#[test]
fn adopting_pp_bonuses_raises_and_fills_the_slot() {
    let dex = Dex::new();
    let base_pp = sample_mon(&dex).moves()[0].pp; // Tackle: 35.
    let mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, PpBonuses::from_bits(0b0000_0011))
        .unwrap();

    assert_eq!(mon.pp_bonuses().get(0), 3);
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), 56, "35 + 35 * 20 * 3 / 100");
    assert_eq!(mon.moves()[0].pp, 56);
    assert!(mon.moves()[0].pp > base_pp);
}

/// The whole byte survives, including bits belonging to slots this mon has
/// no move for — the save encoder writes back exactly what it read.
#[test]
fn pp_bonus_bits_for_unfilled_slots_are_carried_untouched() {
    let dex = Dex::new();
    // Slot 3 upgraded on a one-move mon: unreachable through upstream's own
    // paths, representable in bytes, and never silently rewritten here.
    let mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, PpBonuses::from_bits(0b1100_0000))
        .unwrap();
    assert_eq!(mon.pp_bonuses().bits(), 0b1100_0000);
    assert_eq!(mon.max_pp(&dex, 0).unwrap(), mon.moves()[0].pp);
}

/// `HealPlayerParty` restores PP with `CalculatePPWithBonus`
/// (`pokeemerald/src/script_pokemon_util.c:47`), so a PP-Up-carrying slot
/// heals to the *upgraded* maximum, not to the move's base PP.
#[test]
fn heal_restores_pp_to_the_pp_up_adjusted_maximum() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, PpBonuses::from_bits(0b0000_0011))
        .unwrap();
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;

    for _ in 0..20 {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 36);

    mon.heal(&dex).unwrap();
    assert_eq!(mon.moves()[0].pp, 56);
    assert!(
        mon.moves()[0].pp > base_pp,
        "healing to base PP would silently strip the PP Ups"
    );
}

/// The upgraded capacity is real, not cosmetic: the slot spends every one
/// of those PP before it runs out.
#[test]
fn an_upgraded_slot_spends_its_whole_upgraded_capacity() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex)
        .with_pp_bonuses(&dex, PpBonuses::from_bits(0b0000_0011))
        .unwrap();
    for _ in 0..56 {
        mon.deduct_pp(0).unwrap();
    }
    assert_eq!(mon.moves()[0].pp, 0);
    assert_eq!(mon.deduct_pp(0), Err(BattleError::NoPpRemaining(0)));
}
