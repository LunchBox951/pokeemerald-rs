//! Constructor-time admission of a battler's inherited status (issue #945).
//!
//! `AbilityBattleEffects(ABILITYEFFECT_IMMUNITY)` (`battle_util.c:2873-2951`)
//! cures a Paralysed Limber or Poisoned Immunity holder at move end; that
//! cure is unmodelled, so both constructors refuse such a participant, active
//! or benched, before any constructor RNG draw.

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattlePokemon, Dex, Status1};

const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);

fn mon(species: SpeciesId, slot: u8, status: Status1) -> BattlePokemon {
    let dex = Dex::new();
    let mut mon = max_iv_mon(&dex, species.0, 20, vec![MoveId::TACKLE]).with_ability_slot(slot);
    mon.set_status1(status);
    mon
}

fn healthy(species: SpeciesId) -> BattlePokemon {
    mon(species, 0, Status1::Healthy)
}

/// The two refused pairs: a Paralysed Persian (Limber) and a Poisoned
/// Snorlax (Immunity).
fn refused() -> [(BattlePokemon, AbilityId); 2] {
    [
        (
            mon(SpeciesId::PERSIAN, 0, Status1::Paralysed),
            AbilityId::LIMBER,
        ),
        (
            mon(SpeciesId::SNORLAX, 0, Status1::Poisoned),
            AbilityId::IMMUNITY,
        ),
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
    let (mut bad, _) = refused().into_iter().next().unwrap();
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
}

#[test]
fn the_selected_ability_slot_decides_not_the_species_pool() {
    // Snorlax slot 1 is Thick Fat, so a poisoned slot-1 Snorlax is admitted.
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
