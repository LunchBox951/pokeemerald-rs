//! Both constructors refuse move-end-cured status/ability pairs
//! (`battle_util.c:2873-2951`) and always-refused abilities (Truant, Intimidate, Speed Boost,
//! Color Change, Trace, Effect Spore, Flame Body, Cute Charm), before any RNG draw.

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattlePokemon, Dex, Status1};

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);

fn mon(species: SpeciesId, slot: u8, status: Status1) -> BattlePokemon {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, species, 20, vec![MoveId::TACKLE]).with_ability_slot(slot);
    mon.set_status1(status);
    mon
}

fn healthy(species: SpeciesId) -> BattlePokemon {
    mon(species, 0, Status1::Healthy)
}

fn refused() -> [(BattlePokemon, AbilityId); 24] {
    [
        (healthy(SpeciesId::MIGHTYENA), AbilityId::INTIMIDATE),
        (healthy(SpeciesId::EKANS), AbilityId::INTIMIDATE),
        (
            mon(SpeciesId::MAWILE, 1, Status1::Healthy),
            AbilityId::INTIMIDATE,
        ),
        (healthy(SpeciesId::SLAKOTH), AbilityId::TRUANT),
        (healthy(SpeciesId::SLAKING), AbilityId::TRUANT),
        (healthy(SpeciesId::NINJASK), AbilityId::SPEED_BOOST),
        (healthy(SpeciesId::YANMA), AbilityId::SPEED_BOOST),
        (healthy(SpeciesId::KECLEON), AbilityId::COLOR_CHANGE),
        (healthy(SpeciesId::PORYGON), AbilityId::TRACE),
        (healthy(SpeciesId::PORYGON2), AbilityId::TRACE),
        (mon(SpeciesId::RALTS, 1, Status1::Healthy), AbilityId::TRACE),
        (
            mon(SpeciesId::KIRLIA, 1, Status1::Healthy),
            AbilityId::TRACE,
        ),
        (
            mon(SpeciesId::GARDEVOIR, 1, Status1::Healthy),
            AbilityId::TRACE,
        ),
        (healthy(SpeciesId::PARAS), AbilityId::EFFECT_SPORE),
        (healthy(SpeciesId::PARASECT), AbilityId::EFFECT_SPORE),
        (healthy(SpeciesId::SHROOMISH), AbilityId::EFFECT_SPORE),
        (healthy(SpeciesId::BRELOOM), AbilityId::EFFECT_SPORE),
        (healthy(SpeciesId::MAGMAR), AbilityId::FLAME_BODY),
        (
            mon(SpeciesId::SLUGMA, 1, Status1::Healthy),
            AbilityId::FLAME_BODY,
        ),
        (healthy(SpeciesId::CLEFAIRY), AbilityId::CUTE_CHARM),
        (healthy(SpeciesId::SKITTY), AbilityId::CUTE_CHARM),
        (
            mon(SpeciesId::PERSIAN, 0, Status1::Paralysed),
            AbilityId::LIMBER,
        ),
        (
            mon(SpeciesId::SNORLAX, 0, Status1::Poisoned),
            AbilityId::IMMUNITY,
        ),
        {
            let mut spinda = healthy(SpeciesId::SPINDA);
            assert_eq!(spinda.ability(), AbilityId::OWN_TEMPO);
            spinda.volatiles_mut().set_confusion(3);
            (spinda, AbilityId::OWN_TEMPO)
        },
    ]
}

fn assert_refused_without_draws(
    result: &Result<Battle, BattleError>,
    ability: AbilityId,
    rng: &SequenceRng,
) {
    assert!(matches!(
        result,
        Err(BattleError::UnportedAbilityInteraction(found)) if *found == ability
    ));
    assert_eq!(rng.draws(), 0, "refusal precedes every constructor draw");
}

fn wild(
    player: BattlePokemon,
    reserves: Vec<BattlePokemon>,
    enemy: BattlePokemon,
    first_battle: bool,
    rng: &mut SequenceRng,
) -> Result<Battle, BattleError> {
    Battle::new_with_player_reserves(Dex::new(), player, reserves, enemy, first_battle, rng)
}

fn trainer(
    player: BattlePokemon,
    reserves: Vec<BattlePokemon>,
    party: Vec<BattlePokemon>,
    rng: &mut SequenceRng,
) -> Result<Battle, BattleError> {
    Battle::new_trainer_with_player_reserves(
        Dex::new(),
        player,
        reserves,
        MAY_ROUTE_103_MUDKIP,
        party,
        rng,
    )
}

#[test]
fn wild_constructor_refuses_an_active_player_before_drawing() {
    for first_battle in [false, true] {
        for (bad, ability) in refused() {
            let mut rng = SequenceRng::new([]);
            let result = wild(
                bad,
                vec![],
                healthy(SpeciesId::ZIGZAGOON),
                first_battle,
                &mut rng,
            );
            assert_refused_without_draws(&result, ability, &rng);
        }
    }
}

#[test]
fn wild_constructor_refuses_an_active_enemy_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let result = wild(healthy(SpeciesId::ZIGZAGOON), vec![], bad, false, &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn wild_constructor_refuses_a_later_player_reserve_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let reserves = vec![healthy(SpeciesId::TREECKO), bad];
        let result = wild(
            healthy(SpeciesId::MUDKIP),
            reserves,
            healthy(SpeciesId::ZIGZAGOON),
            false,
            &mut rng,
        );
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_an_active_player_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let result = trainer(bad, vec![], vec![healthy(SpeciesId::TREECKO)], &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_the_active_enemy_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let result = trainer(healthy(SpeciesId::MUDKIP), vec![], vec![bad], &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_a_later_bench_member_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let party = vec![
            healthy(SpeciesId::TREECKO),
            healthy(SpeciesId::TREECKO),
            bad,
        ];
        let result = trainer(healthy(SpeciesId::MUDKIP), vec![], party, &mut rng);
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn trainer_constructor_refuses_a_later_player_reserve_before_drawing() {
    for (bad, ability) in refused() {
        let mut rng = SequenceRng::new([]);
        let reserves = vec![healthy(SpeciesId::TORCHIC), bad];
        let result = trainer(
            healthy(SpeciesId::MUDKIP),
            reserves,
            vec![healthy(SpeciesId::TREECKO)],
            &mut rng,
        );
        assert_refused_without_draws(&result, ability, &rng);
    }
}

#[test]
fn a_fainted_reserve_is_admitted_because_it_can_never_enter() {
    for (index, (mut bad, _)) in refused().into_iter().enumerate() {
        bad.apply_damage(u32::MAX);
        assert!(bad.is_fainted());
        let mut rng = SequenceRng::new([0, 0]);
        wild(
            healthy(SpeciesId::MUDKIP),
            vec![bad],
            healthy(SpeciesId::ZIGZAGOON),
            false,
            &mut rng,
        )
        .unwrap();
        let (mut bad, _) = refused().into_iter().nth(index).unwrap();
        bad.apply_damage(u32::MAX);
        let mut rng = SequenceRng::new([0, 0]);
        trainer(
            healthy(SpeciesId::MUDKIP),
            vec![],
            vec![healthy(SpeciesId::ZIGZAGOON), bad],
            &mut rng,
        )
        .unwrap();
    }
}

#[test]
fn confusion_is_refused_only_for_own_tempo() {
    let mut zigzagoon = healthy(SpeciesId::ZIGZAGOON);
    zigzagoon.volatiles_mut().set_confusion(3);
    let mut rng = SequenceRng::new([0, 0]);
    let battle = wild(
        zigzagoon,
        vec![],
        healthy(SpeciesId::ZIGZAGOON),
        false,
        &mut rng,
    )
    .unwrap();
    assert!(battle.player().volatiles().confused());

    let mut rng = SequenceRng::new([0, 0]);
    wild(
        healthy(SpeciesId::SPINDA),
        vec![],
        healthy(SpeciesId::ZIGZAGOON),
        false,
        &mut rng,
    )
    .unwrap();
}

#[test]
fn an_always_refused_ability_is_refused_whatever_the_status() {
    for (species, ability) in [
        (SpeciesId::MIGHTYENA, AbilityId::INTIMIDATE),
        (SpeciesId::NINJASK, AbilityId::SPEED_BOOST),
        (SpeciesId::KECLEON, AbilityId::COLOR_CHANGE),
        (SpeciesId::PORYGON, AbilityId::TRACE),
        (SpeciesId::SHROOMISH, AbilityId::EFFECT_SPORE),
        (SpeciesId::MAGMAR, AbilityId::FLAME_BODY),
        (SpeciesId::CLEFAIRY, AbilityId::CUTE_CHARM),
    ] {
        for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
            always_refused_whatever(species, ability, status);
        }
    }
}

fn always_refused_whatever(species: SpeciesId, ability: AbilityId, status: Status1) {
    let mut rng = SequenceRng::new([]);
    let result = wild(
        mon(species, 0, status),
        vec![],
        healthy(SpeciesId::ZIGZAGOON),
        false,
        &mut rng,
    );
    assert_refused_without_draws(&result, ability, &rng);
    let mut rng = SequenceRng::new([]);
    let result = trainer(
        mon(species, 0, status),
        vec![],
        vec![healthy(SpeciesId::ZIGZAGOON)],
        &mut rng,
    );
    assert_refused_without_draws(&result, ability, &rng);
}

#[test]
fn the_selected_ability_slot_decides_not_the_species_pool() {
    for (species, slot, ability) in [
        (SpeciesId::EKANS, 1, AbilityId::SHED_SKIN),
        (SpeciesId::MAWILE, 0, AbilityId::HYPER_CUTTER),
        (SpeciesId::YANMA, 1, AbilityId::COMPOUND_EYES),
        (SpeciesId::RALTS, 0, AbilityId::SYNCHRONIZE),
        (SpeciesId::KIRLIA, 0, AbilityId::SYNCHRONIZE),
        (SpeciesId::GARDEVOIR, 0, AbilityId::SYNCHRONIZE),
    ] {
        let holder = mon(species, slot, Status1::Healthy);
        assert_eq!(holder.ability(), ability);
        let mut rng = SequenceRng::new([0, 0]);
        wild(
            holder,
            vec![],
            healthy(SpeciesId::ZIGZAGOON),
            false,
            &mut rng,
        )
        .unwrap();
        assert!(rng.draws() >= 1);
    }

    let snorlax = mon(SpeciesId::SNORLAX, 1, Status1::Poisoned);
    assert_eq!(snorlax.ability(), AbilityId::THICK_FAT);
    let mut rng = SequenceRng::new([0, 0]);
    wild(
        snorlax,
        vec![],
        healthy(SpeciesId::ZIGZAGOON),
        false,
        &mut rng,
    )
    .unwrap();
    assert!(rng.draws() >= 1);
}
