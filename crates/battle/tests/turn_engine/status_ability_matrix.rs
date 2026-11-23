//! This Status1-only matrix admits every ability with every status at
//! construction: Limber (paralysis), Immunity (poison), Water Veil (burn) and
//! Magma Armor (freeze) hold the status they forbid until the move-end cure
//! clears it (`battle_util.c:2873-2951`; `status_immunity.rs` plays the
//! cure), and every other ability is admitted whether modelled or deferred.
//!
//! Upstream: Guts and Marvel Scale stat multipliers (`pokemon.c:3211-3214`),
//! Shed Skin (`battle_util.c:2620-2640`), Synchronize (`battle_util.c:2971-2986`),
//! Early Bird (`battle_util.c:2030-2037`), Insomnia/Vital Spirit/Water Veil/Magma
//! Armor (`battle_script_commands.c:2291-2393`), Flash Fire (`battle_util.c:2703-2705`),
//! Natural Cure (`battle_script_commands.c:9590-9607`).

use crate::common::{admission_script, max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattlePokemon, Dex, Status1};

const STATUSES: [Status1; 8] = [
    Status1::Healthy,
    Status1::Paralysed,
    Status1::Poisoned,
    Status1::BadPoisoned { counter: 0 },
    Status1::BadPoisoned { counter: 5 },
    Status1::BadPoisoned { counter: 15 },
    Status1::Burned,
    Status1::Frozen,
];

/// Columns: name, species, saved ability slot, and expected ability.
const MATRIX: [(&str, SpeciesId, u8, AbilityId); 13] = [
    ("guts", SpeciesId::RATTATA, 1, AbilityId::GUTS),
    (
        "marvel scale",
        SpeciesId::MILOTIC,
        0,
        AbilityId::MARVEL_SCALE,
    ),
    ("shed skin", SpeciesId::METAPOD, 0, AbilityId::SHED_SKIN),
    ("synchronize", SpeciesId::ABRA, 0, AbilityId::SYNCHRONIZE),
    ("limber", SpeciesId::PERSIAN, 0, AbilityId::LIMBER),
    ("immunity", SpeciesId::SNORLAX, 0, AbilityId::IMMUNITY),
    ("early bird", SpeciesId::DODUO, 1, AbilityId::EARLY_BIRD),
    ("insomnia", SpeciesId::DROWZEE, 0, AbilityId::INSOMNIA),
    (
        "vital spirit",
        SpeciesId::MANKEY,
        0,
        AbilityId::VITAL_SPIRIT,
    ),
    ("water veil", SpeciesId::GOLDEEN, 1, AbilityId::WATER_VEIL),
    ("magma armor", SpeciesId::SLUGMA, 0, AbilityId::MAGMA_ARMOR),
    ("flash fire", SpeciesId::VULPIX, 0, AbilityId::FLASH_FIRE),
    (
        "natural cure",
        SpeciesId::CHANSEY,
        0,
        AbilityId::NATURAL_CURE,
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
fn the_status1_matrix_admits_every_status_and_keeps_it_until_a_move_ends() {
    for (name, species, slot, ability) in MATRIX {
        for status in STATUSES {
            let mon = holder(species, slot, status);
            assert_eq!(mon.ability(), ability, "{name} fixture ability");
            let enemy = opponent();
            assert_ne!(
                mon.speed_for_turn_order(),
                enemy.speed_for_turn_order(),
                "{name} with {status:?} fixture Speeds must not tie"
            );
            let mut rng = SequenceRng::new(admission_script(false));
            let battle = Battle::new(Dex::new(), mon, enemy, false, &mut rng)
                .unwrap_or_else(|error| panic!("{name} with {status:?}: {error:?}"));
            assert_eq!(
                battle.player().status1(),
                status,
                "{name} with {status:?}: construction cures nothing"
            );
            rng.assert_exhausted();
        }
    }
}

#[test]
fn guts_and_marvel_scale_read_each_representable_status() {
    use battle::damage::MoveCategory::Physical;
    // Both abilities scale the raw stat by 150% with integer truncation
    // (`pokemon.c:3211-3214`).
    for status in [
        Status1::Paralysed,
        Status1::Poisoned,
        Status1::BadPoisoned { counter: 0 },
        Status1::BadPoisoned { counter: 15 },
        Status1::Burned,
        Status1::Frozen,
    ] {
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
