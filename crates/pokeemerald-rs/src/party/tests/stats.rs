use super::super::{from_save_pokemon, merge_into_save_pokemon, to_save_pokemon, LoadedLead};
use super::common::{treecko_fixture, FIXTURE_PERSONALITY};
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::Pokemon;

const ATTACK_EV_INDEX: usize = 1;

const DEFENSE_EV_INDEX: usize = 2;

const MAX_EFFECTIVE_EV: u8 = 252;

fn shedinja_fixture(level: u8) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        battle::SPECIES_SHEDINJA,
        level,
        Ivs {
            hp: 31,
            attack: 1,
            defense: 2,
            speed: 3,
            sp_attack: 4,
            sp_defense: 5,
        },
        FIXTURE_PERSONALITY,
        battle::initial_moveset(battle::SPECIES_SHEDINJA, level),
    )
    .expect("Shedinja with its own learnset is representable")
}

fn award_next_level(lead: &mut LoadedLead, dex: &Dex) {
    let growth = dex.species(lead.battler().species()).unwrap().growth_rate;
    let next = assets::experience_for_level(growth, lead.battler().level() + 1).unwrap();
    let award = next - lead.battler().experience();
    lead.battler_mut()
        .apply_experience(dex, award)
        .expect("no move-learn prompt is pending");
}

#[test]
fn a_levelled_up_shedinja_lead_saves_at_one_max_hp() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let stored = to_save_pokemon(&dex, &lead);

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    award_next_level(&mut loaded, &dex);
    assert_eq!(
        loaded.battler().level(),
        21,
        "fixture sanity: the level moved"
    );
    assert_eq!(loaded.battler().stats().max_hp, 1);
    assert_eq!(loaded.battler().current_hp(), 1);

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(
        (merged.hp, merged.max_hp),
        (1, 1),
        "`CalculateMonStats` pins a living Shedinja to one point (`pokemon.c:2866-2871`)"
    );
    assert_eq!(merged.level, 21);
}

#[test]
fn a_fainted_shedinja_lead_stays_at_zero_through_a_level_up_and_a_save() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let mut stored = to_save_pokemon(&dex, &lead);
    stored.hp = 0;

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert!(loaded.battler().is_fainted(), "fixture sanity");
    award_next_level(&mut loaded, &dex);

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(
        (merged.hp, merged.max_hp, merged.level),
        (0, 1, 21),
        "a fainted Shedinja returns before the HP write (`pokemon.c:2866-2871`)"
    );
}

#[test]
fn a_shedinja_record_is_adopted_and_saved_verbatim_when_nothing_levels_up() {
    let dex = Dex::new();
    let stored = to_save_pokemon(&dex, &shedinja_fixture(20));
    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        (
            loaded.battler().current_hp(),
            loaded.battler().stats().max_hp
        ),
        (1, 1)
    );

    assert_eq!(loaded.merge_and_save(&dex).to_bytes(), stored.to_bytes());
}

#[test]
/// Upstream's EV award writes EV bytes without recomputing the five cached
/// stats, so they stay legitimately stale until a level-up.
fn an_unchanged_shedinja_keeps_the_five_cached_stats_its_evs_have_outrun() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let mut stored = to_save_pokemon(&dex, &lead);

    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[ATTACK_EV_INDEX] = MAX_EFFECTIVE_EV;
    substructures.evs_and_condition[DEFENSE_EV_INDEX] = MAX_EFFECTIVE_EV;
    stored.box_data.set_substructures(&substructures);

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    let species = dex.species(battle::SPECIES_SHEDINJA).unwrap();
    let ev_aware = battle::compute_stats_with_evs(
        loaded.battler().species(),
        species,
        loaded.battler().level(),
        loaded.battler().nature(),
        loaded.battler().ivs(),
        loaded.battler().evs(),
    );
    assert!(
        ev_aware.attack > u32::from(stored.attack),
        "fixture sanity: a fresh EV-aware recompute really would move the \
         cached Attack, so retaining it is an observable choice"
    );
    let merged = loaded.merge_and_save(&dex);

    assert_eq!(merged.to_bytes(), stored.to_bytes());
    assert_eq!((merged.max_hp, merged.hp), (1, 1));
    assert_eq!(merged.attack, stored.attack);
    assert_eq!(merged.defense, stored.defense);
}

/// A fresh game's provisional starter has no backing save record at all
/// (`SaveBlock1::player_party` starts empty), so a starter that gains EVs
/// and levels up in its first battle, before that first save ever runs, is
/// filed with the cache `CalculateMonStats` just wrote, through
/// `to_save_pokemon` (a direct first save) and `merge_into_save_pokemon`'s
/// no-backing-record fallback alike.
#[test]
fn to_save_pokemon_files_the_level_up_cache_with_no_backing_record() {
    let dex = Dex::new();
    let mut mon = treecko_fixture().with_evs(battle::Evs {
        hp: 252,
        attack: 252,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let species = dex.species(mon.species()).unwrap();
    let created_at_level = mon.created_at_level();
    assert_eq!(
        (mon.stats().max_hp, mon.current_hp()),
        (31, 31),
        "fixture sanity: adopting EVs does not recalculate the cache"
    );

    let next_level_experience =
        assets::experience_for_level(species.growth_rate, created_at_level + 1).unwrap();
    mon.apply_experience(&dex, next_level_experience - mon.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(
        mon.level(),
        created_at_level + 1,
        "fixture sanity: the level moved"
    );
    let ev_aware = battle::compute_stats_with_evs(
        mon.species(),
        species,
        mon.level(),
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    );
    assert_eq!(
        (ev_aware.max_hp, ev_aware.attack),
        (41, 25),
        "fixture sanity"
    );
    assert_eq!(mon.stats(), ev_aware);
    assert_eq!(
        mon.current_hp(),
        41,
        "31 + (41 - 31): full health stays full"
    );

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        (u32::from(saved.max_hp), u32::from(saved.attack)),
        (ev_aware.max_hp, ev_aware.attack)
    );
    assert_eq!(saved.hp, saved.max_hp);

    // The exact path a fresh game's first save takes: no backing record at
    // all, so `merge_into_save_pokemon`'s `backing_substructures` check fails
    // and it falls back to `to_save_pokemon` internally.
    let merged = merge_into_save_pokemon(&dex, &mon, &Pokemon::default());
    assert_eq!(merged.to_bytes(), saved.to_bytes());

    let resaved = merge_into_save_pokemon(&dex, &mon, &merged);
    assert_eq!(
        resaved.to_bytes(),
        merged.to_bytes(),
        "a second, unchanged-state save files the same bytes"
    );
    assert_eq!(from_save_pokemon(&dex, &resaved).unwrap(), mon);
}

/// `MonGainEVs` only ever writes the EV bytes, and nothing recalculates the
/// cached stat block until an actual `CalculateMonStats` call, which the
/// battle controller makes only on a level-up. A mon that gained real EVs but
/// has not levelled up since `BattlePokemon::new` built it files the block
/// its cache actually holds, not the EVs cashed in early.
#[test]
fn to_save_pokemon_keeps_the_stale_cache_when_no_level_up_happened_yet() {
    let dex = Dex::new();
    let mon = treecko_fixture().with_evs(battle::Evs {
        hp: 252,
        ..battle::Evs::default()
    });
    assert_eq!(
        mon.level(),
        mon.created_at_level(),
        "fixture sanity: no level-up happened"
    );

    let ev_aware = battle::compute_stats_with_evs(
        mon.species(),
        dex.species(mon.species()).unwrap(),
        mon.level(),
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    );
    assert!(
        ev_aware.max_hp > mon.stats().max_hp,
        "fixture sanity: the EVs really would move CALC_STAT's own max HP"
    );

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!((saved.hp, saved.max_hp), (31, 31));

    let merged = merge_into_save_pokemon(&dex, &mon, &Pokemon::default());
    assert_eq!((merged.hp, merged.max_hp), (31, 31));
}

/// `GetDeoxysStat` (`pokemon.c:2698-2712`) is reached only through
/// `GetMonData` (`pokemon.c:3660-3684`); the stored `CalculateMonStats` block
/// stays on the raw Normal Forme row.
#[test]
fn to_save_pokemon_files_raw_deoxys_stats_for_an_unchanged_level() {
    let dex = Dex::new();
    let ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(410),
        50,
        ivs,
        0, // personality 0 is the neutral Hardy nature
        vec![assets::MoveId::SCRATCH],
    )
    .expect("Deoxys at level 50 with Scratch is representable");
    assert_eq!(
        mon.stats().speed,
        200,
        "fixture sanity: battle reads Speed Forme"
    );
    assert_eq!(mon.level(), mon.created_at_level());

    let saved = to_save_pokemon(&dex, &mon);
    // Normal Forme base 50/150/50/150/150/50, level 50, IV 31, zero EVs.
    assert_eq!(
        (
            saved.max_hp,
            saved.attack,
            saved.defense,
            saved.speed,
            saved.special_attack,
            saved.special_defense
        ),
        (125, 170, 70, 170, 170, 70)
    );
}

/// A loaded Deoxys keeps its stored Normal Forme block verbatim -- through a
/// stale cache, a level-up recalculation, and later EV gains -- while battle
/// reads the Speed Forme view.
#[test]
fn a_loaded_deoxys_files_its_normal_forme_block_through_level_up_and_ev_gains() {
    let dex = Dex::new();
    let ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };
    let fresh = BattlePokemon::new(
        &dex,
        assets::SpeciesId(410),
        50,
        ivs,
        0,
        vec![assets::MoveId::SCRATCH],
    )
    .unwrap();
    let mut stored = to_save_pokemon(&dex, &fresh);
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[ATTACK_EV_INDEX] = MAX_EFFECTIVE_EV;
    stored.box_data.set_substructures(&substructures);
    let stored_block = (
        stored.max_hp,
        stored.attack,
        stored.defense,
        stored.speed,
        stored.special_attack,
        stored.special_defense,
    );
    assert_eq!(stored_block, (125, 170, 70, 170, 170, 70));

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.battler().stats().attack,
        147,
        "battle reads Speed Forme with the saved Attack EVs"
    );
    assert_eq!(
        loaded.merge_and_save(&dex).to_bytes(),
        stored.to_bytes(),
        "the Speed Forme view never reaches the stored block"
    );

    award_next_level(&mut loaded, &dex);
    let leveled = loaded.merge_and_save(&dex);
    assert_eq!(
        (
            leveled.max_hp,
            leveled.attack,
            leveled.defense,
            leveled.speed,
            leveled.special_attack,
            leveled.special_defense
        ),
        (127, 205, 71, 173, 173, 71),
        "level 51 Normal Forme at 252 Attack EVs"
    );

    loaded.battler_mut().gain_evs(assets::EvYield {
        hp: 0,
        attack: 0,
        defense: 3,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    assert_eq!(
        loaded.merge_and_save(&dex).to_bytes()[..],
        {
            let mut expected = leveled;
            let mut sub = expected.box_data.substructures().unwrap();
            sub.evs_and_condition[DEFENSE_EV_INDEX] = 3;
            expected.box_data.set_substructures(&sub);
            expected.to_bytes()
        }[..],
        "a later EV gain files its byte and leaves the block alone"
    );
}
