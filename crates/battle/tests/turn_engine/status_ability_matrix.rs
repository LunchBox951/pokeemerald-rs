//! The Status1-reading ability disposition, pinned through the constructors
//! (issue #945). Only the Limber and Immunity cures are refused; every other
//! status-reading ability is either modelled elsewhere or deferred, and is
//! admitted here.
//!
//! Modelled: Guts and Marvel Scale (`pokemon.c:3211-3214`, stat accessors),
//! Shed Skin (`battle_util.c:2620-2640`, residual), Synchronize on paralysis
//! and poison (`battle_util.c:2971-2986`). Refused: Limber and Immunity cures
//! (`battle_util.c:2873-2951`). Deferred because their status or switch
//! behaviour is absent: Early Bird (`:2030-2037`), Insomnia, Vital Spirit,
//! Water Veil, Magma Armor (`battle_script_commands.c:2291-2393`), Flash Fire
//! (`battle_util.c:2703-2705`), Natural Cure (`battle_script_commands.c:9590-9607`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattlePokemon, Dex, Status1};

const STATUSES: [Status1; 3] = [Status1::Healthy, Status1::Paralysed, Status1::Poisoned];

/// Species, saved ability slot, expected ability, and the one status whose
/// constructor admission is refused, if any.
const MATRIX: [(&str, SpeciesId, u8, AbilityId, Option<Status1>); 13] = [
    ("guts", SpeciesId::RATTATA, 1, AbilityId::GUTS, None),
    (
        "marvel scale",
        SpeciesId::MILOTIC,
        0,
        AbilityId::MARVEL_SCALE,
        None,
    ),
    (
        "shed skin",
        SpeciesId::METAPOD,
        0,
        AbilityId::SHED_SKIN,
        None,
    ),
    (
        "synchronize",
        SpeciesId::ABRA,
        0,
        AbilityId::SYNCHRONIZE,
        None,
    ),
    (
        "limber",
        SpeciesId::PERSIAN,
        0,
        AbilityId::LIMBER,
        Some(Status1::Paralysed),
    ),
    (
        "immunity",
        SpeciesId::SNORLAX,
        0,
        AbilityId::IMMUNITY,
        Some(Status1::Poisoned),
    ),
    (
        "early bird",
        SpeciesId::DODUO,
        1,
        AbilityId::EARLY_BIRD,
        None,
    ),
    ("insomnia", SpeciesId::DROWZEE, 0, AbilityId::INSOMNIA, None),
    (
        "vital spirit",
        SpeciesId::MANKEY,
        0,
        AbilityId::VITAL_SPIRIT,
        None,
    ),
    (
        "water veil",
        SpeciesId::GOLDEEN,
        1,
        AbilityId::WATER_VEIL,
        None,
    ),
    (
        "magma armor",
        SpeciesId::SLUGMA,
        0,
        AbilityId::MAGMA_ARMOR,
        None,
    ),
    (
        "flash fire",
        SpeciesId::VULPIX,
        0,
        AbilityId::FLASH_FIRE,
        None,
    ),
    (
        "natural cure",
        SpeciesId::CHANSEY,
        0,
        AbilityId::NATURAL_CURE,
        None,
    ),
];

fn holder(species: SpeciesId, slot: u8, status: Status1) -> BattlePokemon {
    let mut mon =
        max_iv_mon(&Dex::new(), species.0, 20, vec![MoveId::TACKLE]).with_ability_slot(slot);
    mon.set_status1(status);
    mon
}

fn opponent() -> BattlePokemon {
    max_iv_mon(
        &Dex::new(),
        SpeciesId::ZIGZAGOON.0,
        20,
        vec![MoveId::TACKLE],
    )
}

#[test]
fn the_constructor_refuses_exactly_the_limber_and_immunity_cures() {
    for (name, species, slot, ability, refused) in MATRIX {
        for status in STATUSES {
            let mon = holder(species, slot, status);
            assert_eq!(mon.ability(), ability, "{name} fixture ability");
            let mut rng = SequenceRng::new([0, 0]);
            let result = Battle::new(Dex::new(), mon, opponent(), false, &mut rng);
            if refused == Some(status) {
                assert!(
                    matches!(
                        result,
                        Err(BattleError::UnportedAbilityInteraction(found)) if found == ability
                    ),
                    "{name} with {status:?} must be refused"
                );
                assert_eq!(rng.draws(), 0, "{name} refusal precedes every draw");
            } else {
                assert!(result.is_ok(), "{name} with {status:?} must be admitted");
                assert!(rng.draws() >= 1, "{name} with {status:?} initialises");
            }
        }
    }
}

#[test]
fn guts_and_marvel_scale_read_each_representable_status() {
    use battle::damage::MoveCategory::Physical;
    for status in [Status1::Paralysed, Status1::Poisoned] {
        let healthy_guts = holder(SpeciesId::RATTATA, 1, Status1::Healthy);
        let sick_guts = holder(SpeciesId::RATTATA, 1, status);
        let base = healthy_guts.attacking_stat(Physical).0;
        assert_eq!(sick_guts.attacking_stat(Physical).0, base * 150 / 100);

        let healthy_scale = holder(SpeciesId::MILOTIC, 0, Status1::Healthy);
        let sick_scale = holder(SpeciesId::MILOTIC, 0, status);
        let base = healthy_scale.defending_stat(Physical).0;
        assert_eq!(sick_scale.defending_stat(Physical).0, base * 150 / 100);
    }
}
