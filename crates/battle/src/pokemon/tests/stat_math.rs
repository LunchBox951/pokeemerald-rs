//! Pins the free stat-calculation functions: max HP (including Shedinja's
//! forced one-HP special case), a single stat, and the bundled six-stat
//! computation with and without EVs.

use super::super::{
    calc_max_hp, calc_stat, compute_stats, compute_stats_with_evs, Evs, MAX_LEVEL, SPECIES_SHEDINJA,
};
use super::shared::{BULBASAUR, MAX_IVS, PINNED_MAX_IV};
use crate::dex::Dex;
use crate::nature::{Nature, Stat};

const BULBASAUR_BASE_HP: u8 = 45;
const BULBASAUR_BASE_ATTACK: u8 = 49;
const MAX_EFFECTIVE_EV: u8 = 252;
const MAX_EFFECTIVE_EVS: Evs = Evs {
    hp: MAX_EFFECTIVE_EV,
    attack: MAX_EFFECTIVE_EV,
    defense: MAX_EFFECTIVE_EV,
    speed: MAX_EFFECTIVE_EV,
    sp_attack: MAX_EFFECTIVE_EV,
    sp_defense: MAX_EFFECTIVE_EV,
};

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
