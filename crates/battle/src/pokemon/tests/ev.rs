//! Pins EV storage (gain, per-stat and total caps) and EV-aware level-up:
//! the live stat cache stays the 0-EV formula while [`Evs`] carries the
//! gain forward.

use super::super::{
    compute_stats, compute_stats_with_evs, BattlePokemon, Evs, Ivs, MAX_PER_STAT_EVS, MAX_TOTAL_EVS,
};
use super::shared::sample_mon;
use crate::dex::Dex;
use assets::{EvYield, MoveId, SpeciesId};

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
/// [`crate::battle::Battle::settle_enemy_reward`] is the real call site for
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
        vec![MoveId::TACKLE],
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
