//! EV gain, per-stat and total caps, stat-cache adoption, and the EV-aware
//! level-up recalculation.

use super::super::{
    compute_stats, compute_stats_with_evs, BattlePokemon, Evs, Ivs, Stats, MAX_PER_STAT_EVS,
    MAX_TOTAL_EVS, SPECIES_SHEDINJA,
};
use super::shared::sample_mon;
use super::shared::{BULBASAUR, HARDY_PERSONALITY};
use crate::dex::Dex;
use assets::{EvYield, MoveId};

const LEVEL_BEFORE_FINAL_LEVEL_UP: u8 = 99;
/// One more Sp. Attack EV crosses the `ev / 4` boundary, as Bulbasaur's KO yield does.
const SP_ATTACK_EVS_ONE_SHORT_OF_NEXT_STAT_POINT: u8 = 3;

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
    mon.gain_evs(yield_);
    assert_eq!(mon.evs().sp_attack, u8::try_from(MAX_PER_STAT_EVS).unwrap());

    mon.gain_evs(yield_);
    assert_eq!(
        mon.evs().sp_attack,
        u8::try_from(MAX_PER_STAT_EVS).unwrap(),
        "a stat already at the cap gains nothing"
    );
}

/// Upstream `MonGainEVs` breaks its stat loop at the total cap (`src/pokemon.c:6005-6006`).
#[test]
fn gain_evs_stops_the_whole_loop_once_the_total_cap_is_reached() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    mon.gain_evs(EvYield {
        hp: u8::try_from(MAX_PER_STAT_EVS).unwrap(),
        attack: u8::try_from(MAX_PER_STAT_EVS).unwrap(),
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let full = mon.evs();
    assert_eq!(
        u16::from(full.hp) + u16::from(full.attack),
        MAX_TOTAL_EVS,
        "fixture sanity: HP and Attack alone fill the total cap"
    );

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

/// Upstream narrows an award by total headroom before per-stat headroom
/// (`src/pokemon.c:6051-6059`).
#[test]
fn gain_evs_applies_the_total_cap_even_with_per_stat_headroom_to_spare() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
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

#[test]
fn gain_evs_alone_does_not_recompute_the_stat_cache() {
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

/// A loaded EV-trained mon: its cache is the EV-aware block, not the zero-EV
/// one, with three points of damage taken.
fn trained_bulbasaur(dex: &Dex) -> BattlePokemon {
    let evs = Evs {
        hp: 252,
        ..Evs::default()
    };
    let probe = sample_mon(dex);
    let bulbasaur = dex.species(probe.species()).unwrap();
    let cache = compute_stats_with_evs(
        probe.species(),
        bulbasaur,
        5,
        probe.nature(),
        probe.ivs(),
        evs,
    );
    assert_eq!(
        cache.max_hp, 22,
        "fixture sanity: the EV-aware level-5 maximum"
    );
    probe.with_cached_stats(cache, 19).with_evs(evs)
}

#[test]
fn a_level_up_on_a_loaded_ev_trained_mon_recalculates_from_its_evs_and_keeps_the_damage() {
    let dex = Dex::new();
    let mut mon = trained_bulbasaur(&dex);
    assert_eq!(
        mon.stats().max_hp,
        22,
        "the adopted cache is not recomputed"
    );

    level_to(&mut mon, &dex, 6);

    let bulbasaur = dex.species(mon.species()).unwrap();
    let ev_aware = compute_stats_with_evs(
        mon.species(),
        bulbasaur,
        6,
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    );
    assert_eq!(
        ev_aware.max_hp, 25,
        "fixture sanity: the EV-aware level-6 maximum"
    );
    assert_eq!(
        mon.stats(),
        ev_aware,
        "the whole cache is replaced from the EVs"
    );
    assert_eq!(mon.cached_stats(), ev_aware);
    assert_eq!(
        mon.current_hp(),
        22,
        "19 + (25 - 22): three points of damage survive, none are healed"
    );
}

/// Upstream `Cmd_getexp` gains EVs before the exp write (`src/battle_script_commands.c:3420`).
#[test]
fn a_ko_that_awards_evs_before_a_level_up_feeds_them_into_the_recalculated_cache() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(
        &dex,
        BULBASAUR,
        LEVEL_BEFORE_FINAL_LEVEL_UP,
        Ivs::default(),
        HARDY_PERSONALITY,
        vec![MoveId::TACKLE],
    )
    .unwrap()
    .with_evs(Evs {
        sp_attack: SP_ATTACK_EVS_ONE_SHORT_OF_NEXT_STAT_POINT,
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

    assert_eq!(
        mon.evs().sp_attack,
        4,
        "the level-up must not reset or drop the KO's own EV gain"
    );

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
    assert_eq!(
        ev_aware.sp_attack,
        zero_ev.sp_attack + 1,
        "fixture sanity: the KO's own point crosses the ev/4 boundary"
    );
    assert_eq!(
        mon.stats(),
        ev_aware,
        "the award lands before the level-up (`src/battle_script_commands.c:3420`), \
         so the recalculated cache already carries it"
    );
}

/// A trained-looking cache whose max HP deliberately lags the zero-EV formula
/// the level-up still recalculates with here: `(22, 11, 11, 11, 11, 11)`.
fn divergent_cache() -> Stats {
    Stats {
        max_hp: 22,
        attack: 11,
        defense: 12,
        speed: 13,
        sp_attack: 14,
        sp_defense: 15,
    }
}

fn level_to(mon: &mut BattlePokemon, dex: &Dex, level: u8) {
    let growth = dex.species(mon.species()).unwrap().growth_rate;
    mon.reconcile_saved_experience(assets::experience_for_level(growth, level).unwrap());
    assert_eq!(
        mon.level(),
        level,
        "fixture sanity: the mon reached level {level}"
    );
}

#[test]
fn with_cached_stats_adopts_the_block_and_hp_without_recomputing_from_evs() {
    let dex = Dex::new();
    let mon = sample_mon(&dex)
        .with_evs(Evs {
            hp: 252,
            attack: 252,
            ..Evs::default()
        })
        .with_cached_stats(divergent_cache(), 19);
    assert_eq!(
        mon.stats(),
        divergent_cache(),
        "the block is taken verbatim"
    );
    assert_eq!(mon.current_hp(), 19, "current HP is taken verbatim");
    assert_eq!(mon.evs().hp, 252, "adopting a cache leaves the EVs alone");
}

#[test]
fn with_cached_stats_adopts_a_fainted_hp_of_zero() {
    let dex = Dex::new();
    let mon = sample_mon(&dex).with_cached_stats(divergent_cache(), 0);
    assert_eq!(mon.current_hp(), 0);
    assert!(mon.is_fainted());
    assert_eq!(mon.stats(), divergent_cache());
}

#[test]
fn with_cached_stats_does_not_clamp_hp_to_the_cached_maximum() {
    let dex = Dex::new();
    let mon = sample_mon(&dex).with_cached_stats(divergent_cache(), 40);
    assert_eq!(mon.current_hp(), 40, "no normalization on adoption");
}

/// `CalculateMonStats` adds `newMaxHP - oldMaxHP` to a living mon
/// (`src/pokemon.c:2880-2891`), measured against the *cached* old maximum.
#[test]
fn a_level_up_moves_living_hp_by_the_signed_delta_against_the_cached_maximum() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex).with_cached_stats(divergent_cache(), 19);
    level_to(&mut mon, &dex, 6);
    let bulbasaur = dex.species(mon.species()).unwrap();
    let new_max = compute_stats(mon.species(), bulbasaur, 6, mon.nature(), mon.ivs()).max_hp;
    assert_eq!(new_max, 21, "fixture sanity: zero-EV level 6 maximum");
    assert_eq!(mon.stats().max_hp, new_max);
    assert_eq!(
        mon.current_hp(),
        18,
        "19 + (21 - 22): a drop in maximum is honoured, not floored at the old HP"
    );
}

/// Stock `CalculateMonStats` (`BUGFIX` off, `src/pokemon.c:2882-2895`) writes
/// `1 + (21 - 22) = 0` through the HP field unclamped.
#[test]
fn a_level_up_hp_delta_is_not_clamped_without_bugfix() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex).with_cached_stats(divergent_cache(), 1);
    level_to(&mut mon, &dex, 6);
    assert_eq!(
        mon.current_hp(),
        0,
        "stock upstream leaves 1 + (21 - 22) = 0"
    );
    assert!(mon.is_fainted());
}

/// A negative signed result wraps through the 16-bit HP field (the Pomeg
/// glitch), as stock upstream stores it.
#[test]
fn a_negative_level_up_hp_result_wraps_through_the_sixteen_bit_hp_field() {
    let dex = Dex::new();
    let cache = Stats {
        max_hp: 30,
        ..divergent_cache()
    };
    let mut mon = sample_mon(&dex).with_cached_stats(cache, 2);
    level_to(&mut mon, &dex, 6);
    assert_eq!(
        mon.current_hp(),
        65_529,
        "2 + (21 - 30) = -7 stored through a u16"
    );
}

/// `CalculateMonStats` (`src/pokemon.c:2875-2878`): zero HP with a zero old
/// maximum is an uninitialised mon, which takes the new maximum.
#[test]
fn zero_hp_with_zero_old_max_takes_the_new_maximum() {
    let dex = Dex::new();
    let zero = Stats {
        max_hp: 0,
        ..divergent_cache()
    };
    let mut mon = sample_mon(&dex).with_cached_stats(zero, 0);
    level_to(&mut mon, &dex, 6);
    assert_eq!(mon.current_hp(), mon.stats().max_hp);
    assert_eq!(mon.current_hp(), 21);
}

/// `src/pokemon.c:2866-2871`: a Shedinja with a zero old maximum is also
/// initialised to one, even at zero HP.
#[test]
fn a_shedinja_with_a_zero_old_max_is_initialised_to_one_hp() {
    let dex = Dex::new();
    let zero = Stats {
        max_hp: 0,
        ..divergent_cache()
    };
    let mut mon = shedinja(&dex).with_cached_stats(zero, 0);
    level_to(&mut mon, &dex, 6);
    assert_eq!((mon.current_hp(), mon.stats().max_hp), (1, 1));
}

#[test]
fn a_fainted_mon_stays_at_zero_hp_through_a_level_up() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    mon.apply_damage(mon.current_hp());
    assert!(mon.is_fainted(), "fixture sanity");
    level_to(&mut mon, &dex, 6);
    assert_eq!(
        mon.current_hp(),
        0,
        "`pokemon.c:2893` returns before writing HP"
    );
    assert!(mon.stats().max_hp > 19, "the stat block itself still grew");
}

#[test]
fn a_full_health_mon_gains_the_whole_maximum_delta() {
    let dex = Dex::new();
    let mut mon = sample_mon(&dex);
    let before = mon.stats().max_hp;
    assert_eq!(mon.current_hp(), before, "fixture sanity: full health");
    level_to(&mut mon, &dex, 6);
    assert_eq!(mon.current_hp(), mon.stats().max_hp);
}

fn shedinja(dex: &Dex) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        SPECIES_SHEDINJA,
        5,
        Ivs::default(),
        HARDY_PERSONALITY,
        vec![MoveId::TACKLE],
    )
    .unwrap()
}

/// `src/pokemon.c:2866-2871`: a living Shedinja is rewritten to exactly one HP.
#[test]
fn a_living_shedinja_stays_at_one_hp_through_a_level_up() {
    let dex = Dex::new();
    let mut mon = shedinja(&dex);
    assert_eq!((mon.current_hp(), mon.stats().max_hp), (1, 1));
    level_to(&mut mon, &dex, 6);
    assert_eq!((mon.current_hp(), mon.stats().max_hp), (1, 1));
}

#[test]
fn a_fainted_shedinja_stays_at_zero_hp_through_a_level_up() {
    let dex = Dex::new();
    let mut mon = shedinja(&dex);
    mon.apply_damage(1);
    assert!(mon.is_fainted(), "fixture sanity");
    level_to(&mut mon, &dex, 6);
    assert_eq!((mon.current_hp(), mon.stats().max_hp), (0, 1));
}
