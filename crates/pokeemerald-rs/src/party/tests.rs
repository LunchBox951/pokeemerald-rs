//! Unit tests for the [`super`] party encoder (I-6, issue #232).

use super::{
    clamp_i32, compute_levelled_up_stats, evs_from_substruct2, from_save_pokemon,
    hp_hidden_by_load, merge_into_save_pokemon, pack_ivs, to_save_pokemon, unpack_ivs,
    zero_ev_max_hp, PartyError, MAIL_NONE,
};
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::{BoxPokemon, Pokemon};

/// `SPECIES_TREECKO` at the provisional starter's own level, with
/// deliberately *distinct* IVs (so a packer that wrote one field twice, or
/// in the wrong order, fails) and a personality that is not zero (so the
/// substructure order and XOR key are both non-trivial).
fn a_battler() -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(277),
        12,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        0x1234_ABCD,
        battle::initial_moveset(assets::SpeciesId(277), 12),
    )
    .expect("Treecko with its level-12 learnset is in the dex")
    .with_original_trainer_id(0x89AB_CDEF)
}

#[test]
fn ivs_pack_into_five_bit_fields_in_declaration_order() {
    let ivs = Ivs {
        hp: 1,
        attack: 2,
        defense: 3,
        speed: 4,
        sp_attack: 5,
        sp_defense: 6,
    };
    let word = pack_ivs(ivs);
    assert_eq!(word & 0x1F, 1, "hpIV occupies bits 0..5");
    assert_eq!((word >> 5) & 0x1F, 2);
    assert_eq!((word >> 10) & 0x1F, 3);
    assert_eq!((word >> 15) & 0x1F, 4);
    assert_eq!((word >> 20) & 0x1F, 5);
    assert_eq!((word >> 25) & 0x1F, 6);
    assert_eq!(word >> 30, 0, "isEgg and abilityNum must stay clear");
    assert_eq!(unpack_ivs(word), ivs);
}

/// `evs_from_substruct2` pinned directly against
/// `pokeemerald/include/pokemon.h:117`-`:122`'s declaration order --
/// `hpEV`/`attackEV`/`defenseEV`/`speedEV`/`spAttackEV`/`spDefenseEV` --
/// with six distinct values so a byte landing in the wrong named field
/// fails here rather than surviving as an unobserved swap. This is
/// deliberately a direct, single-purpose check on the function itself:
/// [`super::compute_levelled_up_stats`]'s own callers exercise EVs only
/// after `CALC_STAT`'s `/ 4` term and `* level / 100` truncation have had a
/// chance to erase a small byte-level difference (issue #384's review) --
/// this test cannot be fooled that way.
#[test]
fn evs_from_substruct2_maps_each_byte_to_its_named_field() {
    let evs_and_condition: [u8; engine::save::SUBSTRUCTURE_LEN] =
        [10, 20, 30, 40, 50, 60, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        evs_from_substruct2(&evs_and_condition),
        battle::Evs {
            hp: 10,
            attack: 20,
            defense: 30,
            speed: 40,
            sp_attack: 50,
            sp_defense: 60,
        }
    );
}

/// A Shedinja lead at `level`, built the same way [`a_battler`] builds its
/// Treecko -- real dex, real learnset -- so the fixtures below exercise the
/// actual extracted species row rather than a synthetic one.
fn a_shedinja(level: u8) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(303),
        level,
        Ivs {
            hp: 31,
            attack: 1,
            defense: 2,
            speed: 3,
            sp_attack: 4,
            sp_defense: 5,
        },
        0x1234_ABCD,
        battle::initial_moveset(assets::SpeciesId(303), level),
    )
    .expect("Shedinja with its own learnset is representable")
}

/// Issue #401: `compute_levelled_up_stats` -- the EV-aware recompute
/// [`merge_into_save_pokemon`] runs whenever species or level moved this
/// session -- must carry Shedinja's flat `1` maximum HP exactly like the
/// `0`-EV formula does, not just reproduce the ordinary formula with real
/// EVs folded in. Fed a maximal HP EV (252, the single-stat cap) so a
/// regression that dropped the species check would file something far
/// larger than `1`.
#[test]
fn compute_levelled_up_stats_forces_shedinja_to_one_max_hp() {
    let dex = Dex::new();
    let mon = a_shedinja(50);
    let evs_and_condition: [u8; engine::save::SUBSTRUCTURE_LEN] =
        [252, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let stats = compute_levelled_up_stats(&dex, &mon, &evs_and_condition);
    assert_eq!(stats.max_hp, 1);
}

/// Issue #401: `zero_ev_max_hp` -- the `0`-EV floor
/// [`merge_into_save_pokemon`]'s recompute branch rebases the load-clamp
/// offset against -- must also be Shedinja's flat `1`, not the ordinary
/// formula's output at `0` EVs.
#[test]
fn zero_ev_max_hp_is_one_for_shedinja() {
    let dex = Dex::new();
    let mon = a_shedinja(50);
    assert_eq!(zero_ev_max_hp(&dex, 303, 50, &mon), 1);
}

/// Issue #401, end to end: a Shedinja lead that levels up this session
/// must still be filed (and rebuilt on the next load) at `1` max HP and `1`
/// current HP -- the same invariant [`compute_levelled_up_stats_forces_shedinja_to_one_max_hp`]
/// and [`zero_ev_max_hp_is_one_for_shedinja`] pin directly, now exercised
/// through the real [`merge_into_save_pokemon`] recompute branch a level
/// change triggers.
#[test]
fn a_levelled_up_shedinja_lead_saves_at_one_max_hp() {
    let dex = Dex::new();
    let lead = a_shedinja(20);
    let stored = to_save_pokemon(&dex, &lead);

    let mut levelled = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let next_level_experience =
        assets::experience_for_level(dex.species(assets::SpeciesId(303)).unwrap().growth_rate, 21)
            .unwrap();
    levelled
        .apply_experience(&dex, next_level_experience - levelled.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(levelled.level(), 21, "fixture sanity: the level moved");
    assert_eq!(levelled.stats().max_hp, 1);
    assert_eq!(levelled.current_hp(), 1);

    let mut offset = hp_hidden_by_load(&dex, &stored, &levelled);
    let merged = merge_into_save_pokemon(&dex, &levelled, &stored, &mut offset);
    assert_eq!(merged.max_hp, 1, "the merge recomputed a level-21 block");
    assert_eq!(
        merged.hp, 1,
        "a Shedinja lead is never filed above its one point"
    );
}

/// Issue #401's correctness review: a Shedinja record whose species and
/// level *don't* move this session normally takes
/// [`merge_into_save_pokemon`]'s retained fast path, which carries the
/// stored six stat bytes forward untouched (issue #384) -- correct for
/// every other species, whose retained maximum can be a real EV-derived
/// value this model cannot reconstruct. Shedinja has no such value: its
/// maximum is always the upstream-mandated flat `1`, so a stored block
/// that disagrees (a save written by a build that predates this fix, or a
/// hand-edited one) is never legitimate data to preserve. This pins that
/// the retained branch normalizes that one entry
/// ([`super::normalize_retained_shedinja_max_hp`]) instead of carrying it
/// forward forever, and that the rebased load-clamp offset leaves the next
/// save byte-exact.
#[test]
fn an_unchanged_shedinja_lead_self_heals_a_stale_stored_maximum() {
    let dex = Dex::new();
    let lead = a_shedinja(20);
    let mut stored = to_save_pokemon(&dex, &lead);
    // What the pre-#401 ordinary formula (or a hand edit) could have left
    // behind -- species and level are exactly what `lead` already holds, so
    // the fast path's own guard sees nothing session-side to invalidate it.
    stored.max_hp = 40;
    stored.hp = 40;

    let reloaded = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_eq!(
        reloaded.stats().max_hp,
        1,
        "fixture sanity: the live model is already correct regardless of \
         the stale stored bytes"
    );

    let mut offset = hp_hidden_by_load(&dex, &stored, &reloaded);
    let merged = merge_into_save_pokemon(&dex, &reloaded, &stored, &mut offset);
    assert_eq!(
        merged.max_hp, 1,
        "an unchanged-level Shedinja still normalizes a stale stored \
         maximum rather than carrying it forward unchanged"
    );
    assert_eq!(merged.hp, 1);
    assert_eq!(
        offset, 0,
        "the points the normalization removed leave the offset with them; \
         they are not real hidden HP under a maximum of 1"
    );

    // Saving twice must file the same bytes: the normalization is a
    // one-shot heal, not a per-save drift (module docs).
    let resaved = merge_into_save_pokemon(&dex, &reloaded, &merged, &mut offset);
    assert_eq!(resaved.max_hp, 1);
    assert_eq!(resaved.hp, 1);
    assert_eq!(offset, 0);
}

/// Issue #401, PR #447's review thread: the retained branch normalizes
/// Shedinja's maximum HP and *only* that. The other five cached bytes stay
/// retained even for Shedinja, because nothing this save path does is a
/// `CalculateMonStats` call -- upstream's `MonGainEVs`
/// (`pokeemerald/src/battle_script_commands.c:3420`) writes the EV bytes
/// and leaves the stat cache stale until a level-up, evolution, vitamin or
/// Box withdrawal recomputes it, and `SavePlayerParty`/`LoadPlayerParty`
/// (`pokeemerald/src/load_save.c:160-178`) call neither. An earlier round
/// of this fix excluded Shedinja from the fast path outright, which cashed
/// exactly that EV gain into the cache one save early
/// `(behavioral-fidelity)`.
#[test]
fn an_unchanged_shedinja_keeps_the_five_cached_stats_its_evs_have_outrun() {
    let dex = Dex::new();
    let lead = a_shedinja(20);
    let mut stored = to_save_pokemon(&dex, &lead);

    // EVs a battle awarded with no level cross behind them: substruct2's
    // bytes move, the six cached stats do not. 252 + 252 stays inside
    // upstream's 510 total.
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[1] = 252;
    substructures.evs_and_condition[2] = 252;
    stored.box_data.set_substructures(&substructures);
    // A stale maximum on top, so the one entry that *is* rewritten stands
    // out against the five that are not.
    stored.max_hp = 40;
    stored.hp = 40;

    let reloaded = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let ev_aware = compute_levelled_up_stats(&dex, &reloaded, &substructures.evs_and_condition);
    assert!(
        ev_aware.attack > u32::from(stored.attack),
        "fixture sanity: a fresh EV-aware recompute really would move the \
         cached Attack, so retaining it is an observable choice"
    );

    let mut offset = hp_hidden_by_load(&dex, &stored, &reloaded);
    let merged = merge_into_save_pokemon(&dex, &reloaded, &stored, &mut offset);

    assert_eq!(merged.max_hp, 1, "the invariant entry is normalized");
    assert_eq!(merged.hp, 1);
    for (stat, filed, retained) in [
        ("Attack", merged.attack, stored.attack),
        ("Defense", merged.defense, stored.defense),
        ("Speed", merged.speed, stored.speed),
        ("Sp. Attack", merged.special_attack, stored.special_attack),
        (
            "Sp. Defense",
            merged.special_defense,
            stored.special_defense,
        ),
    ] {
        assert_eq!(
            filed, retained,
            "{stat} is a cached byte upstream leaves alone until a \
             CalculateMonStats call this save path never makes"
        );
    }
}

/// The `isEgg`/`abilityNum` bits share the IV word; decoding must mask them
/// off rather than letting them bleed into Sp. Defense.
#[test]
fn the_egg_and_ability_bits_do_not_leak_into_the_ivs() {
    let ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };
    assert_eq!(unpack_ivs(pack_ivs(ivs) | 0xC000_0000), ivs);
}

/// The acceptance property this module exists for: a battler saved and
/// reloaded is the same battler, not one re-derived from a species default.
#[test]
fn a_battler_round_trips_through_the_save_layout() {
    let dex = Dex::new();
    let mut mon = a_battler();
    // Play with it first, so the round trip has real *state* to carry --
    // a full-HP, full-PP mon would round-trip through a serializer that
    // dropped both.
    mon.apply_damage(7);
    mon.deduct_pp(0).unwrap();
    mon.deduct_pp(0).unwrap();

    let saved = to_save_pokemon(&dex, &mon);
    let restored = from_save_pokemon(&dex, &saved).expect("what we just wrote must decode");

    assert_eq!(restored.species(), mon.species());
    assert_eq!(restored.level(), mon.level());
    assert_eq!(restored.personality(), mon.personality());
    assert_eq!(restored.original_trainer_id(), mon.original_trainer_id());
    assert_eq!(restored.nature(), mon.nature());
    assert_eq!(restored.ivs(), mon.ivs());
    assert_eq!(restored.stats(), mon.stats());
    assert_eq!(
        restored.current_hp(),
        mon.current_hp(),
        "damage taken before saving must survive the save"
    );
    assert_ne!(
        restored.current_hp(),
        restored.stats().max_hp,
        "the fixture must save a damaged mon, or full-HP restore would pass"
    );
    assert_eq!(restored.moves(), mon.moves(), "moves and PP, slot for slot");
}

/// Sub-level experience earned in battle is state, not a function of the
/// level (`BattlePokemon::apply_experience`, issue #237) -- a round trip
/// that re-derived the growth word from the level would silently reset
/// every battle's progress on save/reload.
#[test]
fn sub_level_experience_survives_the_round_trip() {
    let dex = Dex::new();
    let mut mon = a_battler();
    // Not enough to reach level 13, so the only observable difference is
    // the experience total itself -- and no level crossed means no learnset
    // walk, hence nothing to ask the player about.
    assert!(mon.apply_experience(&dex, 10).unwrap().is_none());
    let treecko = dex.species(mon.species()).unwrap();
    assert_eq!(
        mon.experience(),
        assets::experience_for_level(treecko.growth_rate, 12).unwrap() + 10,
        "the fixture must sit strictly between two thresholds, or a \
         level-derived re-encode would pass"
    );
    assert_eq!(mon.level(), 12);

    let restored = from_save_pokemon(&dex, &to_save_pokemon(&dex, &mon))
        .expect("what we just wrote must decode");
    assert_eq!(restored.experience(), mon.experience());
    assert_eq!(restored.level(), mon.level());
    assert_eq!(restored.stats(), mon.stats());
}

/// A move learned mid-battle by crossing a learnset level
/// (`battle::BattlePokemon::apply_experience`'s learnset walk, issue #252)
/// is moveset *state*, exactly like the hand-picked moves
/// [`a_battler_round_trips_through_the_save_layout`] already covers -- it
/// must survive the save round trip in its own right, not just as a side
/// effect of that more general assertion.
#[test]
fn a_move_learned_by_levelling_up_survives_the_round_trip() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(280), // SPECIES_TORCHIC
        15,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        0x1234_ABCD,
        vec![assets::MoveId(10), assets::MoveId(45)], // Scratch, Growl
    )
    .expect("Torchic with a two-move starting set is in the dex")
    .with_original_trainer_id(0x89AB_CDEF);

    let torchic = dex.species(mon.species()).unwrap();
    let level_16 = assets::experience_for_level(torchic.growth_rate, 16).unwrap();
    assert!(
        mon.apply_experience(&dex, level_16 - mon.experience())
            .unwrap()
            .is_none(),
        "two of the four slots are free, so Peck is learned without asking"
    );
    assert_eq!(
        mon.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![assets::MoveId(10), assets::MoveId(45), assets::MoveId(64)], // + Peck
        "fixture sanity: the level-up must actually have taught Peck \
         (MOVE_PECK, Torchic's level-16 learnset entry) before the round \
         trip can prove anything about it"
    );

    let restored = from_save_pokemon(&dex, &to_save_pokemon(&dex, &mon))
        .expect("what we just wrote must decode");
    assert_eq!(
        restored.moves(),
        mon.moves(),
        "the taught move -- and its freshly rolled PP -- survives the save \
         round trip like any other moveset slot"
    );
    assert_eq!(restored.level(), mon.level());
}

/// A growth word at or past the next level's threshold reconciles the way
/// upstream's own `GetLevelFromMonExp` does: the level rises to match the
/// experience, rather than trusting a level/experience pair upstream could
/// never store.
#[test]
fn an_experience_total_past_the_next_threshold_levels_the_decoded_mon_up() {
    let dex = Dex::new();
    let mon = a_battler();
    let mut saved = to_save_pokemon(&dex, &mon);

    let treecko = dex.species(mon.species()).unwrap();
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let mut substructures = saved.box_data.substructures().unwrap();
    substructures.growth[4..8].copy_from_slice(&level_13.to_le_bytes());
    saved.box_data.set_substructures(&substructures);

    let restored = from_save_pokemon(&dex, &saved).expect("valid bytes must decode");
    assert_eq!(restored.level(), 13, "the level follows the experience");
    assert_eq!(restored.experience(), level_13);
}

/// The level reconciliation above derives the level *only*. Upstream's
/// load path (`CalculateMonStats` -> `GetLevelFromMonExp`,
/// `pokeemerald/src/pokemon.c`) copies the attacks substructure verbatim
/// and never runs `MonTryLearningNewMove`, so decoding an inconsistent
/// save must not teach the crossed levels' learnset moves -- merely
/// loading a save may never mutate its own authoritative moveset
/// (`BattlePokemon::reconcile_saved_experience` vs the in-battle
/// `apply_experience` walk, issue #252).
#[test]
fn decoding_an_inconsistent_save_levels_up_without_teaching_moves() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(280), // SPECIES_TORCHIC
        15,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        0x1234_ABCD,
        vec![assets::MoveId(10), assets::MoveId(45)], // Scratch, Growl
    )
    .expect("Torchic with a two-move starting set is in the dex")
    .with_original_trainer_id(0x89AB_CDEF);
    let mut saved = to_save_pokemon(&dex, &mon);

    // Level 16 is Torchic's Peck (`MOVE_PECK`, id 64) learnset entry --
    // the in-battle award in `a_move_learned_by_levelling_up_survives_the_
    // round_trip` proves crossing it teaches; this decode must not.
    let torchic = dex.species(mon.species()).unwrap();
    let level_16 = assets::experience_for_level(torchic.growth_rate, 16).unwrap();
    let mut substructures = saved.box_data.substructures().unwrap();
    substructures.growth[4..8].copy_from_slice(&level_16.to_le_bytes());
    saved.box_data.set_substructures(&substructures);

    let restored = from_save_pokemon(&dex, &saved).expect("valid bytes must decode");
    assert_eq!(
        restored.level(),
        16,
        "the level still follows the experience"
    );
    assert_eq!(
        restored
            .moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![assets::MoveId(10), assets::MoveId(45)],
        "but the moveset stays exactly the saved attacks substructure -- \
         no Peck: load is not a level-up"
    );
}

/// The saved bytes are upstream's, not an invented shape: the growth
/// substructure holds the species and the accumulated experience, the party
/// block holds `MAIL_NONE`, and the box header carries the OT id the key is
/// built from.
#[test]
fn the_saved_bytes_sit_at_upstream_offsets() {
    let dex = Dex::new();
    let mon = a_battler();
    let saved = to_save_pokemon(&dex, &mon);

    assert_eq!(saved.box_data.ot_id(), mon.original_trainer_id());
    assert_eq!(saved.level, 12);
    assert_eq!(saved.mail, MAIL_NONE);
    assert_eq!(saved.max_hp, u16::try_from(mon.stats().max_hp).unwrap());

    let substructures = saved.box_data.substructures().unwrap();
    assert_eq!(
        u16::from_le_bytes([substructures.growth[0], substructures.growth[1]]),
        mon.species().0
    );
    let treecko = dex.species(mon.species()).unwrap();
    assert_eq!(
        u32::from_le_bytes(substructures.growth[4..8].try_into().unwrap()),
        mon.experience(),
        "the growth word holds the mon's own accumulated experience"
    );
    assert_eq!(
        mon.experience(),
        assets::experience_for_level(treecko.growth_rate, 12).unwrap(),
        "which, for a freshly built mon, is the growth-curve seed \
         CreateBoxMon writes"
    );
    assert_eq!(substructures.growth[9], treecko.base_friendship);
    assert_eq!(
        substructures.evs_and_condition,
        [0; engine::save::SUBSTRUCTURE_LEN],
        "no EVs are modelled, so the EV substructure is written all-zero"
    );
    assert_eq!(
        u16::from_le_bytes([substructures.attacks[0], substructures.attacks[1]]),
        mon.moves()[0].move_id.0
    );
    assert_eq!(substructures.attacks[8], mon.moves()[0].pp);
}

/// Issue #415's own review: a fresh game's provisional starter has no
/// backing save record at all -- `SaveBlock1::player_party` starts empty
/// (`crate::new_game::init_save_blocks`) -- so a starter that gains EVs and
/// levels up in its first battle, before that first save ever runs, must
/// still be filed with `CalculateMonStats`'s own EV-aware stat block through
/// `to_save_pokemon` (a direct first save) and
/// `merge_into_save_pokemon`'s own no-backing-record fallback alike.
#[test]
fn to_save_pokemon_files_ev_aware_stats_after_a_level_up() {
    let dex = Dex::new();
    let mut mon = a_battler().with_evs(battle::Evs {
        hp: 252,
        attack: 252,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let species = dex.species(mon.species()).unwrap();
    let created_at_level = mon.created_at_level();

    // The in-battle level-up that makes the EV-aware recompute apply
    // (`to_save_pokemon`'s own doc comment): `Battle::settle_win_reward`
    // awards EVs before applying experience, so a KO that does both sees
    // its own gain here exactly as a real battle would.
    let next_level_experience =
        assets::experience_for_level(species.growth_rate, created_at_level + 1).unwrap();
    mon.apply_experience(&dex, next_level_experience - mon.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(
        mon.level(),
        created_at_level + 1,
        "fixture sanity: the level moved"
    );

    let zero_ev_max_hp = mon.stats().max_hp;
    let ev_aware = battle::compute_stats_with_evs(
        mon.species(),
        species,
        mon.level(),
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    );
    assert!(
        ev_aware.max_hp > zero_ev_max_hp,
        "fixture sanity: 252 HP EVs really do move CALC_STAT's own max HP \
         at this level, so retaining the live 0-EV cache would be an \
         observable regression"
    );
    assert_eq!(
        mon.current_hp(),
        zero_ev_max_hp,
        "fixture sanity: the level-up grew current HP by the 0-EV delta \
         alone (`battle`'s own module docs), so the mon is still at its own \
         (0-EV) full health"
    );

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        u32::from(saved.max_hp),
        ev_aware.max_hp,
        "a mon with no backing save record must be filed with its real \
         EV-aware stat block, not the live 0-EV cache"
    );
    assert_eq!(
        saved.hp, saved.max_hp,
        "a mon that is full health under the live 0-EV cache must still be \
         filed at full under the wider EV-aware maximum this encoder just \
         computed -- not damaged by the gap between the two floors"
    );

    // The exact path a fresh game's first save takes: no backing record at
    // all (`SaveBlock1::player_party[0]` starts at `Pokemon::default()`, an
    // empty `SPECIES_NONE` slot), so `merge_into_save_pokemon`'s
    // `backing_substructures` check fails and it falls back to
    // `to_save_pokemon` internally.
    let mut offset = 0;
    let merged = merge_into_save_pokemon(&dex, &mon, &Pokemon::default(), &mut offset);
    assert_eq!(
        u32::from(merged.max_hp),
        ev_aware.max_hp,
        "the fresh-game fallback path must match the direct encoder"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "the fallback path must file the same full-health record the \
         direct encoder does"
    );
    assert_eq!(
        offset,
        clamp_i32(ev_aware.max_hp.saturating_sub(zero_ev_max_hp)),
        "the fallback must seed hp_hidden_by_load with the gap the record \
         it just wrote opened over the live 0-EV floor, not leave it at 0 \
         -- otherwise the very next same-session save, taking the retained \
         fast path, would re-measure this same full-health lead against \
         the retained EV-aware maximum with no gap to translate by and \
         file it damaged"
    );

    // That next same-session save: species and level are unchanged, so
    // `merge_into_save_pokemon` takes the retained fast path against the
    // record `merged` just became, trusting the offset above rather than
    // re-deriving it. Saving twice must file the same bytes (module docs).
    let resaved = merge_into_save_pokemon(&dex, &mon, &merged, &mut offset);
    assert_eq!(resaved.max_hp, merged.max_hp);
    assert_eq!(
        resaved.hp, resaved.max_hp,
        "a second, unchanged-state save must still file the lead at full, \
         not flip it to damaged because the carried offset was lost"
    );
}

/// The counterpart the fix above must not overreach on (behavioral-fidelity
/// review): `MonGainEVs` only ever writes the EV bytes
/// (`pokeemerald/src/pokemon.c:5988`-`:6064`), and nothing recomputes the
/// cached stat block until an actual `CalculateMonStats` call, which the
/// battle controller makes only on a level-up
/// (`src/battle_controller_player.c:1247`-`:1264`). A mon that gained real
/// EVs but has not levelled up since `BattlePokemon::new` built it must
/// stay filed at the stale `0`-EV block that cache actually holds, not cash
/// the EVs in a save early.
#[test]
fn to_save_pokemon_keeps_the_stale_cache_when_no_level_up_happened_yet() {
    let dex = Dex::new();
    let mon = a_battler().with_evs(battle::Evs {
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
        "fixture sanity: the EVs really would move CALC_STAT's own max HP, \
         so filing the live 0-EV cache instead is an observable choice, not \
         a coincidence"
    );

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        u32::from(saved.max_hp),
        mon.stats().max_hp,
        "no upstream CalculateMonStats call has happened yet, so the filed \
         block must stay the live 0-EV one"
    );
    assert_eq!(
        saved.hp, saved.max_hp,
        "the live cache's own full health, filed unmodified"
    );

    let mut offset = 0;
    let merged = merge_into_save_pokemon(&dex, &mon, &Pokemon::default(), &mut offset);
    assert_eq!(u32::from(merged.max_hp), mon.stats().max_hp);
    assert_eq!(
        offset, 0,
        "no gap opened over the live floor, so nothing to carry forward"
    );
}

/// A trailing `MOVE_NONE` slot is an *empty* slot upstream, not a known
/// move -- a decoder that carried it through would build a battler
/// `BattlePokemon::new` refuses outright.
#[test]
fn empty_move_slots_are_dropped_rather_than_decoded_as_moves() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(277),
        5,
        Ivs::default(),
        0,
        vec![assets::MoveId(1)],
    )
    .unwrap();
    let saved = to_save_pokemon(&dex, &mon);
    let restored = from_save_pokemon(&dex, &saved).expect("a one-move mon must decode");
    assert_eq!(restored.moves().len(), 1);
}

/// A checksum-valid sector can still hold a mon whose *decrypted* region is
/// garbage. The decode must say so rather than hand the battle engine a
/// scrambled battler.
#[test]
fn a_corrupt_secure_region_is_reported_not_guessed_at() {
    let dex = Dex::new();
    let mut saved = to_save_pokemon(&dex, &a_battler());
    let mut bytes = saved.box_data.to_bytes();
    bytes[40] ^= 0x80;
    saved.box_data = BoxPokemon::from_bytes(bytes);

    assert!(matches!(
        from_save_pokemon(&dex, &saved),
        Err(PartyError::Substructures(_))
    ));
}

/// An all-zero party slot (`SPECIES_NONE`, no moves) is what an *empty*
/// party member is. Decoding one must fail closed rather than produce a
/// zero-stat battler that could then be sent into a fight.
#[test]
fn an_empty_party_slot_does_not_decode_into_a_battler() {
    let err = from_save_pokemon(&Dex::new(), &Pokemon::default())
        .expect_err("SPECIES_NONE is not a fightable mon");
    assert!(matches!(err, PartyError::Battler(_)), "{err}");
    assert!(err.to_string().starts_with("saved party member:"));
}

/// The save-data defect issue #304 fixes: `ppBonuses` used to be written as
/// `0`, so loading and saving a file silently spent every PP Up the player
/// had ever used. The byte now round-trips exactly, and the capacity it
/// encodes is real on the way back in.
#[test]
fn pp_ups_survive_the_round_trip_byte_for_byte() {
    let dex = Dex::new();
    // Three PP Ups on slot 0, one on slot 1 -- distinct per slot, so a
    // packer that wrote one field twice or shifted it wrong fails.
    let bonuses = battle::PpBonuses::from_bits(0b0000_0111);
    let mut mon = a_battler().with_pp_bonuses(&dex, bonuses).unwrap();
    let slot_0_max = mon.max_pp(&dex, 0).unwrap();
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;
    assert!(
        slot_0_max > base_pp,
        "fixture sanity: the upgraded slot must hold more than base PP"
    );
    // Spend a few, so the decode has to place remaining PP against the
    // *adjusted* maximum rather than against base PP.
    mon.deduct_pp(0).unwrap();
    mon.deduct_pp(0).unwrap();

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        saved.box_data.substructures().unwrap().growth[8],
        bonuses.bits(),
        "the growth substructure's /*0x08*/ byte is ppBonuses itself"
    );

    let restored = from_save_pokemon(&dex, &saved).expect("what we just wrote must decode");
    assert_eq!(restored.pp_bonuses(), bonuses);
    assert_eq!(restored.max_pp(&dex, 0).unwrap(), slot_0_max);
    assert_eq!(
        restored.max_pp(&dex, 1).unwrap(),
        mon.max_pp(&dex, 1).unwrap()
    );
    assert_eq!(
        restored.moves()[0].pp,
        slot_0_max - 2,
        "remaining PP is measured from the PP-Up-adjusted maximum"
    );
    assert_eq!(restored.moves(), mon.moves(), "moves and PP, slot for slot");

    let resaved = to_save_pokemon(&dex, &restored);
    assert_eq!(
        resaved.box_data.substructures().unwrap().growth[8],
        bonuses.bits(),
        "re-serialising must emit the identical byte, not zero"
    );
    assert_eq!(resaved, saved, "and the whole 100-byte value is unchanged");
}

/// A byte upstream itself could never write -- PP Ups recorded against a
/// slot this mon has no move for -- is still carried through untouched.
/// Save data is not quietly rewritten because this port cannot explain it.
#[test]
fn pp_bonus_bits_for_unknown_slots_are_not_stripped() {
    let dex = Dex::new();
    let bonuses = battle::PpBonuses::from_bits(0b1111_1111);
    // A deliberately one-move mon, so three of the byte's four fields
    // belong to slots that hold no move at all.
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(277),
        12,
        Ivs::default(),
        0x1234_ABCD,
        vec![assets::MoveId(33)],
    )
    .unwrap()
    .with_pp_bonuses(&dex, bonuses)
    .unwrap();
    assert!(
        mon.moves().len() < battle::MAX_MON_MOVES,
        "fixture sanity: the fixture must leave at least one slot empty"
    );

    let saved = to_save_pokemon(&dex, &mon);
    let restored = from_save_pokemon(&dex, &saved).unwrap();

    assert_eq!(restored.pp_bonuses().bits(), 0b1111_1111);
    assert_eq!(
        to_save_pokemon(&dex, &restored)
            .box_data
            .substructures()
            .unwrap()
            .growth[8],
        0b1111_1111
    );
}

/// The white-out heal restores a saved mon to its *upgraded* maximum, not
/// to the move's base PP (`HealPlayerParty`'s own `CalculatePPWithBonus`).
#[test]
fn healing_a_restored_mon_refills_to_the_upgraded_maximum() {
    let dex = Dex::new();
    let bonuses = battle::PpBonuses::from_bits(0b0000_0011);
    let mut mon = a_battler().with_pp_bonuses(&dex, bonuses).unwrap();
    for _ in 0..5 {
        mon.deduct_pp(0).unwrap();
    }
    let saved = to_save_pokemon(&dex, &mon);

    let mut restored = from_save_pokemon(&dex, &saved).unwrap();
    restored.heal(&dex).unwrap();

    let base_pp = dex.move_data(restored.moves()[0].move_id).unwrap().pp;
    assert_eq!(restored.moves()[0].pp, restored.max_pp(&dex, 0).unwrap());
    assert!(
        restored.moves()[0].pp > base_pp,
        "a heal that stopped at base PP would strip the PP Ups again"
    );
}

/// The ability slot round-trips through the save's `abilityNum` bit
/// (`PokemonSubstruct3`'s bit 31, the misc IV word's top bit) rather than
/// being re-derived from personality on load -- a real save can hold a mon
/// whose stored slot disagrees with its personality parity (nothing
/// upstream re-derives `abilityNum` after `CreateBoxMon` writes it once),
/// and this port must not silently swap such a mon's ability on load
/// (issue #322).
///
/// `SPECIES_TENTACOOL` (`72`) is the dual-ability fixture already used by
/// `battle`'s own ability tests: slot 0 is Clear Body, slot 1 is Liquid
/// Ooze (`gSpeciesInfo`). An *even* personality selects slot 0 by default
/// ([`battle::BattlePokemon::new`]), so overriding to slot 1 here is
/// deliberately the disagreeing case.
#[test]
fn a_disagreeing_ability_slot_survives_the_save_round_trip() {
    const TENTACOOL: u16 = 72;
    const CLEAR_BODY: u16 = 29;
    const LIQUID_OOZE: u16 = 64;

    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(TENTACOOL),
        20,
        Ivs::default(),
        0x1234_ABCC, // even -- personality parity alone would pick slot 0
        vec![assets::MoveId(33)],
    )
    .expect("Tentacool is in the dex")
    .with_ability_slot(1);
    assert_eq!(
        mon.ability().0,
        LIQUID_OOZE,
        "fixture sanity: the override, not personality parity, decides"
    );
    assert_ne!(
        mon.ability().0,
        CLEAR_BODY,
        "fixture sanity: personality parity alone would have picked this"
    );

    let restored = from_save_pokemon(&dex, &to_save_pokemon(&dex, &mon))
        .expect("what we just wrote must decode");
    assert_eq!(restored.ability_slot(), 1);
    assert_eq!(
        restored.ability().0,
        LIQUID_OOZE,
        "the disagreeing slot survives the round trip instead of being \
         re-derived from the (even) personality"
    );
}

/// `ITEM_LEFTOVERS` (`pokeemerald/include/constants/items.h:230`) -- a real
/// held item, so the sentinel is a value upstream could actually store.
const SENTINEL_HELD_ITEM: u16 = 200;
/// `STATUS1_PARALYSIS` (`pokeemerald/include/constants/battle.h:120`).
const SENTINEL_STATUS: u32 = 1 << 6;
/// A party mail slot index, not `MAIL_NONE`: mail is held *by* the mon and
/// is lost with it (`GiveMailToMon`, `pokeemerald/src/mail_data.c:111`).
const SENTINEL_MAIL: u8 = 2;
/// Friendship well away from Treecko's `base_friendship` of 70, so the
/// re-derived value and the accumulated one cannot be confused.
const SENTINEL_FRIENDSHIP: u8 = 213;
/// `PokemonSubstruct2` whole: six EVs then the five contest conditions and
/// sheen (`pokeemerald/include/pokemon.h:115-129`), every byte distinct so a
/// merge that shifted the substructure fails rather than passes, and the six
/// EVs summing to exactly upstream's `510`-point party-wide cap (issue
/// #384's round-2 review -- the original fixture's `790` was a shape no
/// upstream save can hold). The `sp_attack`/`sp_defense` pair (`4`/`246`) is
/// deliberately spread far enough apart that their `/ 4` contributions
/// (`1`/`61`) still disagree after `CALC_STAT`'s `* level / 100` truncation
/// at the levels this file's fixtures use -- `evs_from_substruct2` swapping
/// those two fields must fail a test, not merely round-trip a byte
/// difference small enough for the level scaling to erase.
const SENTINEL_EVS_AND_CONDITION: [u8; engine::save::SUBSTRUCTURE_LEN] =
    [252, 6, 0, 2, 4, 246, 11, 22, 33, 44, 55, 66];
/// What `CalculateMonStats` added to the stored stat block for those EVs,
/// one addend per stat in `max_hp`/`attack`/`defense`/`speed`/`sp_attack`/
/// `sp_defense` order, each distinct so a merge that wrote the block back
/// shuffled fails rather than passes.
///
/// The exact numbers are not upstream's arithmetic and do not need to be:
/// this fixture exercises the *retained* branch, which never rebuilds an
/// EV contribution -- it keeps the stored six bytes exactly as they were
/// (module docs) -- so what the fixture needs is only that the stored
/// block is *not* the 0-EV block the model would otherwise recompute. That
/// makes retaining it observable -- and makes an unconditional overwrite
/// the permanent weakening issue #344's review caught. [`SENTINEL_EVS_AND_CONDITION`]'s
/// own bytes, not these, are what the *recompute* branch is EV-aware
/// against (issue #384) -- see `compute_levelled_up_stats` and the tests
/// that exercise it.
const SENTINEL_STAT_BONUS: [u16; 6] = [7, 15, 1, 15, 1, 2];

/// [`SENTINEL_EVS_AND_CONDITION`]'s first six bytes, named the way
/// `compute_levelled_up_stats` (via `evs_from_substruct2`) and
/// `battle::compute_stats_with_evs` both want them -- split out purely so
/// the recompute-branch tests that feed this through the formula by hand
/// (to check the merge's own output against it) do not have to repeat the
/// six-field literal, not because anything else reuses it.
fn sentinel_retained_evs() -> battle::Evs {
    battle::Evs {
        hp: SENTINEL_EVS_AND_CONDITION[0],
        attack: SENTINEL_EVS_AND_CONDITION[1],
        defense: SENTINEL_EVS_AND_CONDITION[2],
        speed: SENTINEL_EVS_AND_CONDITION[3],
        sp_attack: SENTINEL_EVS_AND_CONDITION[4],
        sp_defense: SENTINEL_EVS_AND_CONDITION[5],
    }
}

/// A party slot as a *save file* holds it: this encoder's own output for
/// [`a_battler`], then stamped with a sentinel in every field the battle
/// model does not carry.
///
/// This is the fixture issue #344 needs. The defect was not that these
/// bytes decoded wrong -- nothing decodes them -- but that re-saving a
/// loaded mon rebuilt the record from the battler alone and so wrote each
/// of them back as a zero.
fn a_stored_record() -> Pokemon {
    let mut record = to_save_pokemon(&Dex::new(), &a_battler());

    let mut substructures = record.box_data.substructures().unwrap();
    substructures.growth[2..4].copy_from_slice(&SENTINEL_HELD_ITEM.to_le_bytes());
    substructures.growth[9] = SENTINEL_FRIENDSHIP;
    substructures.evs_and_condition = SENTINEL_EVS_AND_CONDITION;
    // `PokemonSubstruct3`'s pre-IV bytes: pokérus, met location, and the
    // packed met level/game/ball/OT gender word.
    substructures.misc[0] = 0x24;
    substructures.misc[1] = 0x59;
    substructures.misc[2..4].copy_from_slice(&0xB2C5u16.to_le_bytes());
    // The ribbon word (`/*0x08*/`).
    substructures.misc[8..12].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    record.box_data.set_substructures(&substructures);

    // The box header's own deferred bytes, which `BoxPokemon` retains
    // verbatim: nickname, language, OT name, and markings.
    let mut bytes = record.box_data.to_bytes();
    bytes[8..18].copy_from_slice(&[0xBB; 10]);
    bytes[18] = 5;
    bytes[20..27].copy_from_slice(&[0xCC; 7]);
    bytes[27] = 0b0000_1010;
    record.box_data = BoxPokemon::from_bytes(bytes);

    record.status = SENTINEL_STATUS;
    record.mail = SENTINEL_MAIL;

    // The cached stat block an EV-trained mon carries: the numbers this
    // port recomputes, raised by the EV contribution it cannot. `hp` is
    // left alone -- it is the mon's *current* HP, and leaving it at the
    // 0-EV maximum keeps the fixture a mon this port can hold exactly
    // (`from_save_pokemon` would otherwise clamp it down to that maximum,
    // which is the modelling gap rather than the merge).
    record.max_hp += SENTINEL_STAT_BONUS[0];
    record.attack += SENTINEL_STAT_BONUS[1];
    record.defense += SENTINEL_STAT_BONUS[2];
    record.speed += SENTINEL_STAT_BONUS[3];
    record.special_attack += SENTINEL_STAT_BONUS[4];
    record.special_defense += SENTINEL_STAT_BONUS[5];
    record
}

/// The save-data defect issue #344 fixes: loading a save reduced slot 0 to
/// a battler and saving rebuilt the record from that battler alone, so
/// every field with no home in the model came back a zero. Re-saving a
/// loaded mon now overlays the battler onto its own stored record.
#[test]
fn re_saving_a_loaded_mon_keeps_every_field_the_battle_model_does_not_carry() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    // Play with it, so the merge is the interesting case rather than an
    // accidental byte-for-byte re-emit.
    lead.apply_damage(9);
    lead.deduct_pp(0).unwrap();

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    let before = stored.box_data.substructures().unwrap();
    let after = merged
        .box_data
        .substructures()
        .expect("the merge must leave the checksum valid");
    assert_eq!(
        &after.growth[2..4],
        &before.growth[2..4],
        "heldItem is the save's"
    );
    assert_eq!(
        after.growth[9], SENTINEL_FRIENDSHIP,
        "accumulated friendship is the save's, not the species' base value"
    );
    assert_ne!(
        after.growth[9],
        dex.species(lead.species()).unwrap().base_friendship,
        "fixture sanity: a re-derived friendship would differ from this"
    );
    assert_eq!(
        after.evs_and_condition, SENTINEL_EVS_AND_CONDITION,
        "EVs and contest condition are the save's, whole"
    );
    assert_eq!(
        &after.misc[0..4],
        &before.misc[0..4],
        "pokérus and the met/ball/OT-gender bytes are the save's"
    );
    assert_eq!(
        &after.misc[8..12],
        &before.misc[8..12],
        "the ribbon word is the save's"
    );
    assert_eq!(
        merged.status, SENTINEL_STATUS,
        "non-volatile status is the save's"
    );
    assert_eq!(merged.mail, SENTINEL_MAIL, "the mail slot is the save's");
    assert_eq!(
        merged.box_data.to_bytes()[8..28],
        stored.box_data.to_bytes()[8..28],
        "nickname, language, OT name and markings are the save's"
    );
    assert_eq!(merged.box_data.personality(), stored.box_data.personality());
    assert_eq!(merged.box_data.ot_id(), stored.box_data.ot_id());

    // The cached stat block is retained on the same terms, because nothing
    // it is a function of moved: damage and spent PP are not inputs to
    // `CalculateMonStats`.
    assert_eq!(
        [
            merged.max_hp,
            merged.attack,
            merged.defense,
            merged.speed,
            merged.special_attack,
            merged.special_defense,
        ],
        [
            stored.max_hp,
            stored.attack,
            stored.defense,
            stored.speed,
            stored.special_attack,
            stored.special_defense,
        ],
        "the EV-trained stat block is the save's, not the 0-EV block this \
         port recomputes"
    );
    assert_ne!(
        merged.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "fixture sanity: recomputing the block really would have moved it"
    );
    assert_eq!(
        merged.hp,
        u16::try_from(lead.current_hp()).unwrap(),
        "current HP is battle state, so it is the battler's either way"
    );
    assert!(
        merged.hp <= merged.max_hp,
        "and cannot contradict a retained maximum: the model's own maximum \
         is the 0-EV one, and EVs only add"
    );
}

/// Sub-level experience is the common case a battle leaves behind, and it
/// is not an input to `CalculateMonStats`: the EV-trained block must
/// survive it (module docs -- the guard tests species and level, not the
/// experience word, which is overlaid regardless).
#[test]
fn sub_level_experience_does_not_flatten_the_retained_stat_block() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");

    let level_13 =
        assets::experience_for_level(dex.species(lead.species()).unwrap().growth_rate, 13).unwrap();
    let _ = lead
        .apply_experience(&dex, level_13 - 1 - lead.experience())
        .expect("an award short of the threshold is in range");
    assert_eq!(lead.level(), 12, "fixture sanity: no level was crossed");
    assert_ne!(
        lead.experience(),
        u32::from_le_bytes(
            stored.box_data.substructures().unwrap().growth[4..8]
                .try_into()
                .unwrap()
        ),
        "fixture sanity: the experience word really moved"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );
    let after = merged.box_data.substructures().unwrap();
    assert_eq!(
        u32::from_le_bytes(after.growth[4..8].try_into().unwrap()),
        lead.experience(),
        "the awarded experience is filed"
    );
    assert_eq!(
        [merged.max_hp, merged.attack, merged.defense],
        [stored.max_hp, stored.attack, stored.defense],
        "and the EV-trained block is retained: sub-level experience is not \
         an input to the stat formula"
    );
}

/// The strongest form of the retention rule, and what the review of issue
/// #344 asked for: a lead that a session merely loaded and saved again,
/// without touching it, must write its record back *byte for byte*.
///
/// Offsets 88..=99 -- the six cached stat bytes -- were the ones that moved
/// before the merge made that block conditional, because it re-derived
/// them from the 0-EV model. Nothing on either side would put them back:
/// upstream's load path runs no `CalculateMonStats` (`super`'s module
/// docs), so an EV-trained file was filed permanently weaker by the act of
/// being loaded and saved.
#[test]
fn re_saving_an_untouched_lead_writes_the_record_back_byte_for_byte() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_ne!(
        stored.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "fixture sanity: the stored block carries an EV contribution the \
         model cannot rebuild, so a re-derived block would differ"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    let (merged_bytes, stored_bytes) = (merged.to_bytes(), stored.to_bytes());
    let moved: Vec<usize> = (0..merged_bytes.len())
        .filter(|index| merged_bytes[*index] != stored_bytes[*index])
        .collect();
    assert_eq!(
        moved,
        Vec::<usize>::new(),
        "an untouched lead must re-save as the same 100 bytes"
    );

    // And it stays that way: a player who continues and saves repeatedly
    // without battling cannot drift the record one byte per session.
    let reloaded = from_save_pokemon(&dex, &merged).expect("the re-saved record must decode");
    let again = merge_into_save_pokemon(
        &dex,
        &reloaded,
        &merged,
        &mut hp_hidden_by_load(&dex, &merged, &reloaded),
    );
    assert_eq!(again.to_bytes(), stored.to_bytes());
}

/// The other half of the boundary: what the session *did* change has to
/// land in the merged record, or retention would just be a stale save.
#[test]
#[allow(clippy::too_many_lines)] // one continuous level-up-and-merge scenario; splitting would re-derive the same fixture state
fn re_saving_a_loaded_mon_overlays_what_the_session_changed() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");

    let treecko = dex.species(lead.species()).unwrap();
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let _ = lead
        .apply_experience(&dex, level_13 - lead.experience())
        .expect("a level-13 award is in range");
    assert_eq!(lead.level(), 13, "fixture sanity: the mon levelled up");
    lead.apply_damage(11);
    lead.deduct_pp(1).unwrap();
    lead.deduct_pp(1).unwrap();

    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);
    let after = merged.box_data.substructures().unwrap();

    assert_eq!(
        u32::from_le_bytes(after.growth[4..8].try_into().unwrap()),
        lead.experience(),
        "the growth word carries the experience the battle awarded"
    );
    assert_eq!(merged.level, 13, "and the level that came with it");
    // Not a plain pass-through of `lead.current_hp()`: the fixture's own
    // load-clamp offset was `0` (the stored `hp` never exceeded the level-12
    // `0`-EV floor), but `SENTINEL_STAT_BONUS`'s artificial level-12 gap (7)
    // is smaller than the *real* level-13 EV-aware gap the recompute
    // derives from `SENTINEL_EVS_AND_CONDITION` (8) -- so this level-up
    // still moves the offset by the difference, exactly as it would for a
    // real save whose retained gap actually is `CalculateMonStats`' own
    // output (module docs, issue #384's round-2 review). Reproduced here
    // from `battle::compute_stats_with_evs` rather than reached into
    // `super::zero_ev_max_hp`, so this test pins the property rather than
    // the implementation.
    let recompute = |level: u8, evs: battle::Evs| {
        battle::compute_stats_with_evs(
            lead.species(),
            treecko,
            level,
            lead.nature(),
            lead.ivs(),
            evs,
        )
    };
    let old_floor = recompute(stored.level, battle::Evs::default()).max_hp;
    let gap_old = u32::from(stored.max_hp) - old_floor;
    let gap_new = u32::from(merged.max_hp) - lead.stats().max_hp;
    let rebased_offset =
        u16::try_from(gap_new - gap_old).expect("the fixture's EVs keep this well under u16::MAX");
    assert_eq!(
        merged.hp,
        u16::try_from(lead.current_hp()).unwrap() + rebased_offset,
        "the level-up moved the EV-aware gap, so the filed hp carries that \
         movement even though nothing was clamped at load"
    );
    assert_ne!(merged.hp, stored.hp, "fixture sanity: the damage is real");

    // The recomputed block is EV-aware -- fed the fixture's own retained
    // `SENTINEL_EVS_AND_CONDITION` bytes through `CalculateMonStats`'
    // formula, not the battler's `0`-EV `lead.stats()` cache (issue #384,
    // and still true post-#415: only this save-time recompute is EV-aware,
    // the live cache stays `0`-EV for the whole battle -- see
    // `BattlePokemon::raise_level_to_experience`'s own module docs).
    let expected = recompute(lead.level(), sentinel_retained_evs());
    assert_eq!(
        merged.max_hp,
        u16::try_from(expected.max_hp).unwrap(),
        "a level-up moved what the cached block is a function of, so the \
         block is recomputed -- EV-aware, from the record's own retained \
         EV bytes, rather than left disagreeing with the level above it \
         (module docs)"
    );
    assert_ne!(
        merged.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "fixture sanity: the retained hp EV (252) really does raise the \
         filed block above the battler's own 0-EV cache"
    );
    assert_ne!(
        merged.max_hp, stored.max_hp,
        "fixture sanity: the retained block would have been the level-12 one"
    );
    assert_eq!(
        [
            merged.attack,
            merged.defense,
            merged.speed,
            merged.special_attack,
            merged.special_defense,
        ],
        [
            u16::try_from(expected.attack).unwrap(),
            u16::try_from(expected.defense).unwrap(),
            u16::try_from(expected.speed).unwrap(),
            u16::try_from(expected.sp_attack).unwrap(),
            u16::try_from(expected.sp_defense).unwrap(),
        ],
        "the whole block, not just the maximum HP"
    );
    assert_eq!(
        after.attacks,
        super::encode_attacks(&lead),
        "moves and per-slot PP, slot for slot"
    );
    assert_ne!(
        &after.attacks[8..12],
        &stored.box_data.substructures().unwrap().attacks[8..12],
        "fixture sanity: the spent PP is real"
    );
    assert_eq!(
        after.evs_and_condition[0..6],
        SENTINEL_EVS_AND_CONDITION[0..6],
        "issue #415: the record's own retained EVs round-trip back out \
         unchanged -- nothing in this session called `gain_evs`"
    );

    // Not quite the same battler back out: `merged.hp` now carries the
    // rebased offset's extra point, which the `0`-EV model can represent
    // (it is still under `lead.stats().max_hp`), so the decode restores it
    // rather than re-clamping it away -- [`battle::BattlePokemon::heal_hp`]
    // is exactly that same "add, capped at maximum" arithmetic
    // ([`overlay_current_hp_over_retained_block`]'s own doc comment).
    let mut expected_reloaded = lead.clone();
    expected_reloaded.heal_hp(u32::from(rebased_offset));
    let reloaded = from_save_pokemon(&dex, &merged).expect("the merge must decode again");
    assert_eq!(
        reloaded, expected_reloaded,
        "and back out as the same battler, plus the rebased offset's point"
    );
}

/// The identity gate. Personality and the OT id are the substructure XOR
/// key *and* the mon's identity, so a slot holding a different Pokémon must
/// be rebuilt rather than overlaid -- grafting one mon's moveset onto
/// another's ribbons and met data would be worse than the zeroing the merge
/// exists to fix.
#[test]
fn a_slot_holding_a_different_pokemon_is_rebuilt_rather_than_overlaid() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");

    let swapped_personality = BattlePokemon::new(
        &dex,
        lead.species(),
        lead.level(),
        lead.ivs(),
        lead.personality() ^ 0x0F0F_0F0F,
        lead.moves().iter().map(|slot| slot.move_id).collect(),
    )
    .unwrap()
    .with_original_trainer_id(lead.original_trainer_id());
    assert_eq!(
        merge_into_save_pokemon(&dex, &swapped_personality, &stored, &mut 0),
        to_save_pokemon(&dex, &swapped_personality),
        "a different personality is a different mon"
    );

    let traded_away = lead.clone().with_original_trainer_id(0x0BAD_0BAD);
    assert_eq!(
        merge_into_save_pokemon(&dex, &traded_away, &stored, &mut 0),
        to_save_pokemon(&dex, &traded_away),
        "so is a different original trainer -- it is half the XOR key"
    );
}

/// A new game's slot 0 is `SPECIES_NONE` and has no retained bytes at all,
/// so the first save of a fresh file must build the record from scratch
/// instead of overlaying onto an empty slot.
#[test]
fn an_empty_slot_is_built_from_scratch() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(277),
        5,
        Ivs::default(),
        0,
        vec![assets::MoveId(1)],
    )
    .unwrap();
    let empty = Pokemon::default();
    assert_eq!(
        empty.box_data.personality(),
        mon.personality(),
        "fixture sanity: the identity gate alone would let this through, so \
         the species check is what decides"
    );
    let built = merge_into_save_pokemon(&dex, &mon, &empty, &mut 0);
    assert_eq!(built, to_save_pokemon(&dex, &mon));
    assert_eq!(built.mail, MAIL_NONE, "an empty slot has no mail to keep");
}

/// `isEgg` and `abilityNum` share the IV word, and the model owns exactly
/// one of them. The merge must rewrite the word around the egg bit rather
/// than through it (module docs).
#[test]
fn the_merge_rewrites_the_iv_word_around_the_egg_bit() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    let mut substructures = stored.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap());
    substructures.misc[4..8].copy_from_slice(&(iv_word | super::IS_EGG_BIT).to_le_bytes());
    stored.box_data.set_substructures(&substructures);

    let lead = from_save_pokemon(&dex, &stored)
        .expect("the fixture must decode")
        .with_ability_slot(1);
    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    let merged_word = u32::from_le_bytes(
        merged.box_data.substructures().unwrap().misc[4..8]
            .try_into()
            .unwrap(),
    );
    assert_eq!(
        merged_word & super::IS_EGG_BIT,
        super::IS_EGG_BIT,
        "the egg bit this port does not model stays exactly as it was"
    );
    assert_eq!(merged_word >> 31, 1, "abilityNum is the battler's");
    assert_eq!(unpack_ivs(merged_word), lead.ivs());
}

/// Issue #344's review, second round: an EV-trained lead saved at *full*
/// health has `hp == max_hp` above the model's 0-EV maximum, and
/// [`from_save_pokemon`] clamps the live copy down to the model's full.
/// The merge must translate that clamp back out rather than file the
/// clamped number: Continue -> SAVE of a full-health mon stays a no-op
/// instead of marking the mon damaged.
#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    stored.hp = stored.max_hp;
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored full must exceed the model's maximum, \
         or the load clamp never fires"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

/// The same clamp with the stored current HP strictly *between* the
/// model's maximum and the retained one: the load pins the live copy at
/// the model's full, and the merge files the stored byte back rather than
/// either boundary.
#[test]
fn continue_then_save_keeps_an_over_model_max_current_hp() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    stored.hp = stored.max_hp - 3;
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored value must sit above the model's \
         maximum, or the load clamp never fires"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

/// The clamp translation under battle damage: the stored points above the
/// model's maximum were hidden from the session, so damage taken must
/// subtract from the *stored* value, not from its clamp -- upstream's
/// arithmetic is absolute. Stored `model_max + 5` taking 10 damage files
/// `stored - 10`, not `model_max - 10`.
#[test]
fn battle_damage_on_a_clamped_load_subtracts_from_the_stored_hp() {
    const DAMAGE: u32 = 10;
    const HIDDEN: u16 = 5;

    let dex = Dex::new();
    let mut stored = a_stored_record();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    assert!(
        stored.hp < stored.max_hp,
        "fixture sanity: the stored hp must sit below the retained maximum"
    );
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    lead.apply_damage(DAMAGE);

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    assert_eq!(merged.hp, stored.hp - u16::try_from(DAMAGE).unwrap());
}

/// Issue #384's review: the load-clamp offset is a fact about the
/// battler's own `current_hp`, not about which branch the merge takes --
/// the battler's HP is capped at the `0`-EV model's maximum either way, so
/// a level-up that makes the merge recompute the block still needs the
/// offset translated back on, now against the freshly recomputed maximum
/// rather than the retained block's. The merge must therefore neither
/// retire the offset nor carry it forward unchanged: retiring it (as an
/// earlier version of this fix did) both dropped real hidden points from
/// the record this write files and made the *next* save -- once this
/// record's own species and level land it on the retained branch -- forget
/// them outright; carrying it unrebased (round 2 of this issue's review)
/// is wrong whenever the gap between the EV-aware maximum and the `0`-EV
/// floor is not the same size at the old level as at the new one, which
/// this fixture's *artificial* retained bonus ([`SENTINEL_STAT_BONUS`], not
/// a real `CalculateMonStats` output) deliberately is not. Saving twice
/// with no gameplay in between must still file the same bytes, whatever
/// the rebased offset comes out to be.
#[test]
fn a_stat_block_recompute_still_translates_the_load_clamp_offset() {
    const HIDDEN: u16 = 5;
    const DAMAGE: u32 = 10;

    let dex = Dex::new();
    let mut stored = a_stored_record();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_eq!(
        offset,
        i32::from(HIDDEN),
        "fixture sanity: the load clamp must fire"
    );

    lead.apply_damage(DAMAGE);
    let treecko = dex.species(lead.species()).unwrap();
    let next_level = assets::experience_for_level(treecko.growth_rate, lead.level() + 1).unwrap();
    lead.apply_experience(&dex, next_level - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the level must move"
    );

    // The offset the merge must file: rebased by how much the gap between
    // the EV-aware maximum and the `0`-EV floor moved between `stored`'s
    // level and `lead`'s new one, not carried across unchanged --
    // `zero_ev_max_hp`'s own formula (module docs), reproduced here from
    // the fixture's own inputs rather than reached into as a private
    // function, so this test still pins the *property* rather than the
    // implementation.
    let old_floor = battle::compute_stats_with_evs(
        lead.species(),
        treecko,
        stored.level,
        lead.nature(),
        lead.ivs(),
        battle::Evs::default(),
    )
    .max_hp;
    let gap_old = u32::from(stored.max_hp) - old_floor;

    let first = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);
    let gap_new = u32::from(first.max_hp) - lead.stats().max_hp;
    let expected_offset = i64::from(HIDDEN) + i64::from(gap_new) - i64::from(gap_old);
    assert_eq!(
        i64::from(offset),
        expected_offset,
        "the recompute rebases the offset by how the gap moved, rather than \
         zeroing it (which would drop the session's own hidden points) or \
         carrying it unrebased (which mis-sizes it once the gap is not the \
         same at the old level as at the new one)"
    );
    let live = i64::from(u16::try_from(lead.current_hp()).unwrap());
    assert_eq!(
        i64::from(first.hp),
        (live + i64::from(offset)).min(i64::from(first.max_hp)),
        "current HP crosses the same load clamp the retained branch \
         applies, now against the block just recomputed for the new level"
    );

    let second = merge_into_save_pokemon(&dex, &lead, &first, &mut offset);
    assert_eq!(
        second.to_bytes(),
        first.to_bytes(),
        "an immediate re-save, now on the retained branch, must file the \
         same bytes the recompute branch just wrote"
    );
}

/// [`overlay_current_hp_over_retained_block`]'s fainted guard
/// (`live == 0` files `0` rather than `live.saturating_add(hp_hidden_by_load)`)
/// pinned through the recompute branch specifically, not just the retained
/// one this file's other tests exercise: a lead that faints and *then*
/// levels up this session (so the merge recomputes the block) still has a
/// real load-clamp offset sitting beside it, and that offset must not add
/// itself onto a dead battler's `0` and file it alive.
#[test]
fn a_fainted_lead_stays_fainted_through_a_stat_block_recompute() {
    const HIDDEN: u16 = 5;

    let dex = Dex::new();
    let mut stored = a_stored_record();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_eq!(
        offset,
        i32::from(HIDDEN),
        "fixture sanity: the load clamp must fire"
    );

    let treecko = dex.species(lead.species()).unwrap();
    let next_level = assets::experience_for_level(treecko.growth_rate, lead.level() + 1).unwrap();
    lead.apply_experience(&dex, next_level - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the level must move, so the merge takes the \
         recompute branch"
    );

    lead.apply_damage(u32::MAX);
    assert!(lead.is_fainted(), "fixture sanity: the lead must faint");

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    assert_eq!(
        merged.hp, 0,
        "a fainted lead files 0 even under a freshly recomputed block with \
         real hidden points behind it -- the load-clamp offset must never \
         resurrect it"
    );
}

/// Issue #384's review: a lead that loaded at full health and levels up
/// this session must still be filed at full under the freshly recomputed
/// (EV-aware) maximum. The recompute branch that zeroed the load-clamp
/// offset instead filed the model's own plain, untranslated `current_hp`
/// -- capped at the weaker `0`-EV maximum regardless of what the record's
/// EVs are worth -- so a lead stored at full came back marked damaged by
/// however many points it never lost, exactly the corruption shape issue
/// #344 exists to stop.
///
/// This crosses `13` -> `14`, not `12` -> `13`: `CALC_STAT`'s `ev / 4` term
/// is scaled by `* level / 100`, so the *gap* between the EV-aware maximum
/// and the `0`-EV floor grows with level, and a level pair whose gap
/// happens to be the same size on both sides (as `12` -> `13` is for this
/// fixture's EVs) cannot tell a merge that rebases that gap apart from one
/// that just carries the load-clamp offset forward unchanged -- round 2 of
/// this issue's review caught exactly that (a Treecko stored `41`/`41` at
/// level 13 filed `43`/`44` at level 14 under the unrebased offset).
///
/// The stored block here is the *real* `CalculateMonStats` output for the
/// record's own retained EVs at the stored level, not the arbitrary
/// [`SENTINEL_STAT_BONUS`] every other fixture in this file uses, so the
/// record is internally consistent the way an upstream file -- which only
/// ever holds real cache values -- always is.
#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full_after_levelling_up() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    stored.level = 13;
    let treecko = dex.species(assets::SpeciesId(277)).unwrap();
    let retained_evs = sentinel_retained_evs();
    let stored_lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let old_ev_aware = battle::compute_stats_with_evs(
        stored_lead.species(),
        treecko,
        stored_lead.level(),
        stored_lead.nature(),
        stored_lead.ivs(),
        retained_evs,
    );
    stored.max_hp = u16::try_from(old_ev_aware.max_hp).unwrap();
    stored.hp = stored.max_hp;

    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored full must exceed the model's 0-EV \
         maximum, or the load clamp never fires"
    );
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_ne!(offset, 0, "fixture sanity: the load clamp must fire");

    let next_level = assets::experience_for_level(treecko.growth_rate, lead.level() + 1).unwrap();
    lead.apply_experience(&dex, next_level - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the level must move"
    );

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    let new_ev_aware = battle::compute_stats_with_evs(
        lead.species(),
        treecko,
        lead.level(),
        lead.nature(),
        lead.ivs(),
        retained_evs,
    );
    assert_eq!(
        merged.max_hp,
        u16::try_from(new_ev_aware.max_hp).unwrap(),
        "fixture sanity: the recomputed block is the level-14 EV-aware one"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "a full-health lead that levels up must still be filed at full \
         under the newly recomputed maximum, not at the model's own \
         weaker 0-EV current_hp"
    );
}

/// Issue #384's round-3 review: the fixture above crosses a level by an
/// explicit in-battle award; this one crosses it the other way a save can
/// -- a stored `level` byte that its own growth word's experience
/// contradicts. [`from_save_pokemon`] reconciles the level up to match the
/// experience (`BattlePokemon::reconcile_saved_experience`) before
/// [`hp_hidden_by_load`] ever runs, so an offset measured against the
/// *reconciled* level rather than the record's own stored byte -- what
/// [`merge_into_save_pokemon`]'s recompute branch rebases against
/// (`base.level`) -- filed this scenario weaker than upstream, whose
/// `CalculateMonStats` derives the level from experience before it ever
/// computes a stat block (`pokeemerald/src/pokemon.c:2840`).
#[test]
fn an_inconsistent_level_byte_still_files_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    stored.level = 13;
    let treecko = dex.species(assets::SpeciesId(277)).unwrap();
    let retained_evs = sentinel_retained_evs();
    // Same personality and IVs `a_stored_record`'s own fixture carries --
    // both are level-independent, so this throwaway battler's `nature()`/
    // `ivs()` stand in for the fixture's without an extra decode.
    let fixture = a_battler();
    let ev_aware_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    stored.max_hp = u16::try_from(ev_aware_at_13.max_hp).unwrap();
    stored.hp = stored.max_hp;

    // The growth word says level 14, contradicting the `level` byte just
    // set above -- upstream's own `GetLevelFromMonExp` reconciles this on
    // load, and so does `from_save_pokemon`, before any offset is measured.
    // One level, not a larger jump: the model's `0`-EV maximum crosses the
    // fixture's own stored (EV-aware) maximum somewhere past this point --
    // once it does, `from_save_pokemon`'s own clamp (`apply_damage`,
    // against the reconciled level's `0`-EV floor, not the record's own
    // stored one) pins `current_hp` there, a residual gap this crate's own
    // docs already name (`battle`'s live cache stays `0`-EV for the whole
    // battle, module docs) and not the mismatched-offset defect this
    // fixture targets.
    let level_14 = assets::experience_for_level(treecko.growth_rate, 14).unwrap();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.growth[4..8].copy_from_slice(&level_14.to_le_bytes());
    stored.box_data.set_substructures(&substructures);

    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_eq!(lead.level(), 14, "fixture sanity: the level reconciled up");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the stored byte still disagrees with the level \
         the mon actually holds"
    );

    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    let ev_aware_at_14 = battle::compute_stats_with_evs(
        lead.species(),
        treecko,
        lead.level(),
        lead.nature(),
        lead.ivs(),
        retained_evs,
    );
    assert_eq!(
        merged.max_hp,
        u16::try_from(ev_aware_at_14.max_hp).unwrap(),
        "fixture sanity: the merge recomputed the level-14 EV-aware block"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "a record whose level byte contradicts its experience word still \
         files a full-health lead at full -- matching upstream's own \
         CalculateMonStats, not damaged by an offset measured against the \
         reconciled level instead of the record's own stored byte"
    );
}

/// Issue #384's round-4 review: the gap between the EV-aware maximum and
/// the `0`-EV floor does not only *grow* with level. `CALC_STAT` truncates
/// `(n + ev / 4) * level / 100` and `n * level / 100` independently, so at
/// some real level transitions the EV term buys a point at the old level
/// and none at the new one, and the gap *shrinks*. The rebase must be able
/// to run backwards there: a damaged lead's live HP moved by the model's
/// own `0`-EV level-up delta, which is one point wider than the EV-aware
/// delta upstream would have applied, so a rebase that cannot go below
/// zero files that extra point.
///
/// This fixture's numbers: Treecko, HP IV `1`, so the `0`-EV `n` is `81`;
/// HP EV `12`, so `ev / 4` is `3` and the EV-aware `n` is `84`. At level
/// 12 that is `floor(1008/100) - floor(972/100) = 10 - 9 = 1` point of
/// gap; at level 13 it is `floor(1092/100) - floor(1053/100) = 10 - 10 =
/// 0`. A lead stored at `1` HP therefore loads with no hidden points at
/// all (`1` is far below the floor), gains the model's `+2` on the
/// level-up where upstream's own EV-aware block gains `+1`, and must be
/// filed at upstream's `2` rather than the model's `3`.
#[test]
fn a_shrinking_ev_gap_files_upstreams_own_level_up_delta() {
    let dex = Dex::new();
    let treecko = dex.species(assets::SpeciesId(277)).unwrap();
    let fixture = a_battler();
    let retained_evs = battle::Evs {
        hp: 12,
        ..sentinel_retained_evs()
    };

    let mut stored = a_stored_record();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[0] = retained_evs.hp;
    stored.box_data.set_substructures(&substructures);

    let ev_aware_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    let ev_aware_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    let floor_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        battle::Evs::default(),
    );
    let floor_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        battle::Evs::default(),
    );
    assert_eq!(
        ev_aware_at_12.max_hp - floor_at_12.max_hp,
        1,
        "fixture sanity: the level-12 gap is one point"
    );
    assert_eq!(
        ev_aware_at_13.max_hp - floor_at_13.max_hp,
        0,
        "fixture sanity: the level-13 gap is none -- the gap shrinks, which \
         is the whole point of this fixture"
    );

    stored.level = 12;
    stored.max_hp = u16::try_from(ev_aware_at_12.max_hp).unwrap();
    stored.hp = 1;

    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_eq!(
        offset, 0,
        "fixture sanity: a stored 1 HP is far below the 0-EV floor, so the \
         load clamp hides nothing"
    );

    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    lead.apply_experience(&dex, level_13 - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(lead.level(), 13, "fixture sanity: the level must move");
    assert_eq!(
        lead.current_hp(),
        1 + (floor_at_13.max_hp - floor_at_12.max_hp),
        "fixture sanity: the live battler gained the 0-EV delta, which is \
         the wider one"
    );

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    assert_eq!(
        merged.max_hp,
        u16::try_from(ev_aware_at_13.max_hp).unwrap(),
        "fixture sanity: the merge recomputed the level-13 EV-aware block"
    );
    assert_eq!(
        u32::from(merged.hp),
        1 + (ev_aware_at_13.max_hp - ev_aware_at_12.max_hp),
        "a level-up files upstream's own EV-aware max-HP delta onto the \
         stored current HP, even where that delta is narrower than the \
         model's 0-EV one -- the rebase has to subtract the point the \
         shrinking gap took back"
    );
}

/// The converse of the fainted guard, over the negative offset the fixture
/// above produces: a battler the session is still playing must never be
/// filed at `0`. Subtracting a shrunken gap from a live HP already down at
/// `1` would do exactly that -- and a record that says `0` is a fainted
/// lead on the next load, which the model on screen is not. The
/// translation floors at `1`, the mirror of the cap that keeps it under the
/// block's own maximum; the point of divergence from upstream (whose own
/// EV-aware copy really would be at `0` here) is the same residue the
/// module docs name, and it closes when `battle` carries EVs.
#[test]
fn a_live_lead_never_files_as_fainted_when_the_ev_gap_shrinks() {
    let dex = Dex::new();
    let treecko = dex.species(assets::SpeciesId(277)).unwrap();
    let fixture = a_battler();
    let retained_evs = battle::Evs {
        hp: 12,
        ..sentinel_retained_evs()
    };

    let mut stored = a_stored_record();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[0] = retained_evs.hp;
    stored.box_data.set_substructures(&substructures);
    let ev_aware_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    stored.level = 12;
    stored.max_hp = u16::try_from(ev_aware_at_12.max_hp).unwrap();
    stored.hp = 1;

    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    lead.apply_experience(&dex, level_13 - lead.experience())
        .expect("no move-learn prompt is pending");
    // Back down to a single point, with the rebase about to take one away.
    lead.apply_damage(lead.current_hp() - 1);
    assert_eq!(lead.current_hp(), 1, "fixture sanity: one point left");
    assert!(!lead.is_fainted(), "fixture sanity: and still standing");

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    assert_eq!(offset, -1, "fixture sanity: the rebase went negative");
    assert_eq!(
        merged.hp, 1,
        "a live battler files at least 1 -- a 0 here would come back from \
         the next load as a fainted lead the session never fainted"
    );
}

/// Issue #415, end to end: a KO that both awards EVs and crosses a level
/// this same turn must file *both* -- the newly gained EV byte, and a stat
/// block computed with it rather than the record's stale, pre-KO one.
/// Regression for the issue's own defect report: "A KO that grants a level
/// and crosses an `ev/4` boundary therefore files lower stats than
/// upstream, and the newly earned EVs are lost."
#[test]
fn a_ko_that_crosses_a_level_and_an_ev_slash_4_boundary_saves_both() {
    let dex = Dex::new();
    let lead = a_battler(); // Treecko, level 12.
    let treecko = dex.species(lead.species()).unwrap();

    // Attack EV starts one short of the next `ev / 4` unit (3 -> floor 0).
    let mut stored = to_save_pokemon(&dex, &lead);
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[1] = 3;
    stored.box_data.set_substructures(&substructures);

    let mut battler = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_eq!(
        battler.evs().attack,
        3,
        "fixture sanity: the loaded EV round-trips"
    );

    // The KO: `BattlePokemon::gain_evs` before `apply_experience` --
    // `Battle::settle_win_reward`'s own order (module docs) -- against a
    // real species' real yield (Poochyena, species 286, Attack yield 1),
    // crossing the `ev / 4` boundary (3 -> 4 -> floor 1).
    let poochyena = dex.species(assets::SpeciesId(286)).unwrap();
    assert_eq!(
        poochyena.ev_yield.attack, 1,
        "fixture sanity: Poochyena's real upstream Attack yield"
    );
    battler.gain_evs(poochyena.ev_yield);
    assert_eq!(
        battler.evs().attack,
        4,
        "fixture sanity: the ev/4 boundary is crossed"
    );

    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    battler
        .apply_experience(&dex, level_13 - battler.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(
        battler.level(),
        13,
        "fixture sanity: the same KO also crossed a level"
    );

    let mut offset = hp_hidden_by_load(&dex, &stored, &battler);
    let merged = merge_into_save_pokemon(&dex, &battler, &stored, &mut offset);
    let after = merged.box_data.substructures().unwrap();

    assert_eq!(
        after.evs_and_condition[1], 4,
        "the KO's own EV gain is not lost -- it is filed, not the stale \
         pre-KO byte"
    );

    let filed_with_the_gain = battle::compute_stats_with_evs(
        battler.species(),
        treecko,
        13,
        battler.nature(),
        battler.ivs(),
        battle::Evs {
            attack: 4,
            ..battle::Evs::default()
        },
    );
    let filed_without_the_gain = battle::compute_stats_with_evs(
        battler.species(),
        treecko,
        13,
        battler.nature(),
        battler.ivs(),
        battle::Evs {
            attack: 3,
            ..battle::Evs::default()
        },
    );
    assert_ne!(
        filed_with_the_gain.attack, filed_without_the_gain.attack,
        "fixture sanity: the ev/4 boundary crossing really does move the \
         formula's own output, or the assertion below would be vacuous"
    );
    assert_eq!(
        merged.attack,
        u16::try_from(filed_with_the_gain.attack).unwrap(),
        "the level-up save carries the battle's own EV yield -- not the \
         weaker pre-KO stat block issue #415 exists to fix"
    );
}

/// Slice-review finding (behavioral-fidelity), issue #415: an in-battle
/// level-up must not leave [`battle::BattlePokemon::stats`] EV-aware for
/// the rest of the session. `hp_hidden_by_load`'s whole rebase system
/// assumes the live model's own maximum is *always* the `0`-EV floor
/// ([`zero_ev_max_hp`]) -- if a level-up instead recomputes it EV-aware,
/// the retained branch's later merge adds the hidden-EV offset on top of a
/// `current_hp` that is already real, double-counting it and silently
/// healing away damage the session actually took.
#[test]
fn a_retained_branch_after_an_in_battle_level_up_does_not_double_count_the_hidden_ev_gap() {
    let dex = Dex::new();
    let lead = a_battler(); // Treecko, level 12, 0 EVs.
    let treecko = dex.species(lead.species()).unwrap();

    // A real HP EV investment, as if trained in an earlier session -- HP
    // specifically, since `hp_hidden_by_load`/`zero_ev_max_hp` measure the
    // gap over the `0`-EV *max_hp* floor, which only the HP EV moves.
    let mut stored = to_save_pokemon(&dex, &lead);
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[0] = 252;
    stored.box_data.set_substructures(&substructures);

    let mut battler = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_eq!(battler.evs().hp, 252, "fixture sanity: the EV round-trips");

    // Level up in-battle -- no KO EV gain this time, isolating the
    // level-up path from the award path.
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    battler
        .apply_experience(&dex, level_13 - battler.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(battler.level(), 13, "fixture sanity: the mon levelled up");

    // Save once, so the stored record catches up to the new level -- an
    // ordinary mid-session save.
    let mut offset = hp_hidden_by_load(&dex, &stored, &battler);
    let saved_once = merge_into_save_pokemon(&dex, &battler, &stored, &mut offset);
    assert_eq!(
        saved_once.level, 13,
        "fixture sanity: the stored record now matches the new level"
    );

    // A later KO gains a few more EVs without crossing another level -- the
    // retained branch's own territory (module docs). The stored record's
    // own maximum (real, EV-aware, from the save above) sits above the
    // `0`-EV floor at level 13, so the hidden-offset measurement below is
    // nonzero.
    battler.gain_evs(assets::EvYield {
        hp: 3,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let mut offset2 = hp_hidden_by_load(&dex, &saved_once, &battler);
    assert_ne!(
        offset2, 0,
        "fixture sanity: the retained maximum really is above the 0-EV \
         floor, or the double-count this test targets could not manifest"
    );

    // Real damage taken in a subsequent battle, after the second save's
    // own snapshot was measured.
    battler.apply_damage(10);

    let merged = merge_into_save_pokemon(&dex, &battler, &saved_once, &mut offset2);
    assert_eq!(
        merged.hp,
        merged.max_hp - 10,
        "the 10 points of real damage must survive the save -- not be \
         silently healed by adding the hidden EV gap on top of a \
         current_hp that is already real"
    );
}
