//! `EFFECT_THAW_HIT`'s burn secondary: chance, silent guards, and the
//! admission screens (`battle_script_commands.c:2341-2379,2912-2930`).

use super::{
    ensure_admissible, is_thaw_hit_effect, spend_effect_chance_draw, SecondaryApplication,
    EFFECT_THAW_HIT,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::status1::Status1;
use assets::{AbilityId, MoveEffect, MoveId, SpeciesId};

const FLAME_WHEEL: MoveId = MoveId::FLAME_WHEEL;
const SACRED_FIRE: MoveId = MoveId::SACRED_FIRE;

const ZIGZAGOON: u16 = 288;
/// Mono Fire-type with Blaze.
const CHARMANDER: u16 = 4;
/// Water Veil in its second ability slot.
const GOLDEEN: u16 = 118;
const WURMPLE: u16 = 290;
const DUNSPARCE: u16 = 206;
const RALTS: u16 = 392;
const WATER_VEIL_SLOT: u8 = 1;

fn mon(dex: &Dex, species: u16) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        SpeciesId(species),
        10,
        Ivs {
            hp: 31,
            attack: 31,
            defense: 31,
            speed: 31,
            sp_attack: 31,
            sp_defense: 31,
        },
        0,
        vec![MoveId::TACKLE],
    )
    .unwrap()
}

fn water_veil_goldeen(dex: &Dex) -> BattlePokemon {
    let goldeen = mon(dex, GOLDEEN).with_ability_slot(WATER_VEIL_SLOT);
    assert_eq!(goldeen.ability(), AbilityId::WATER_VEIL);
    goldeen
}

fn resolve(
    move_id: MoveId,
    roll: u16,
    hit_had_effect: bool,
    defender: &BattlePokemon,
) -> Result<SecondaryApplication, BattleError> {
    let mut rng = SequenceRng::new([roll]);
    let result = spend_effect_chance_draw(&Dex::new(), move_id, hit_had_effect, defender, &mut rng);
    assert_eq!(rng.draws(), 1, "exactly one chance draw");
    result
}

#[test]
fn only_effect_thaw_hit_is_the_modelled_burn_trampoline() {
    let dex = Dex::new();
    assert_eq!(dex.move_data(FLAME_WHEEL).unwrap().effect, EFFECT_THAW_HIT);
    assert!(is_thaw_hit_effect(
        dex.move_data(SACRED_FIRE).unwrap().effect
    ));
    assert!(!is_thaw_hit_effect(MoveEffect::BURN_HIT));
    assert!(!is_thaw_hit_effect(MoveEffect::BLAZE_KICK));
}

#[test]
fn the_burn_chance_is_strict_and_spends_one_draw_either_way() {
    let dex = Dex::new();
    let defender = mon(&dex, ZIGZAGOON);
    for (move_id, roll, expected) in [
        (FLAME_WHEEL, 9, SecondaryApplication::Burn),
        (FLAME_WHEEL, 10, SecondaryApplication::None),
        (FLAME_WHEEL, 109, SecondaryApplication::Burn),
        (FLAME_WHEEL, 110, SecondaryApplication::None),
        (SACRED_FIRE, 49, SecondaryApplication::Burn),
        (SACRED_FIRE, 50, SecondaryApplication::None),
        (SACRED_FIRE, 149, SecondaryApplication::Burn),
        (SACRED_FIRE, 150, SecondaryApplication::None),
    ] {
        assert_eq!(
            resolve(move_id, roll, true, &defender),
            Ok(expected),
            "{move_id:?} roll {roll}"
        );
    }
    assert_eq!(
        resolve(FLAME_WHEEL, 0, false, &defender),
        Ok(SecondaryApplication::None),
        "an ineffective hit never carries the secondary"
    );
}

#[test]
fn fire_water_veil_shield_dust_and_an_existing_status_silently_block_the_burn() {
    let dex = Dex::new();
    let shield_dust = mon(&dex, WURMPLE);
    assert_eq!(shield_dust.ability(), AbilityId::SHIELD_DUST);
    let mut statused = mon(&dex, ZIGZAGOON);
    statused.set_status1(Status1::Paralysed);
    for (name, defender) in [
        ("Fire type", mon(&dex, CHARMANDER)),
        ("Water Veil", water_veil_goldeen(&dex)),
        ("Shield Dust", shield_dust),
        ("existing status", statused),
    ] {
        assert_eq!(
            resolve(FLAME_WHEEL, 0, true, &defender),
            Ok(SecondaryApplication::None),
            "{name}"
        );
    }
}

#[test]
fn a_burn_that_could_land_on_a_synchronize_holder_is_refused() {
    let dex = Dex::new();
    let attacker = mon(&dex, ZIGZAGOON);
    let ralts = mon(&dex, RALTS);
    assert_eq!(ralts.ability(), AbilityId::SYNCHRONIZE);
    for move_id in [FLAME_WHEEL, SACRED_FIRE] {
        assert_eq!(
            ensure_admissible(&dex, move_id, &attacker, &ralts),
            Err(BattleError::UnportedAbilityInteraction(
                AbilityId::SYNCHRONIZE
            ))
        );
    }
    let mut frozen = ralts.clone();
    frozen.set_status1(Status1::Frozen);
    assert_eq!(
        ensure_admissible(&dex, FLAME_WHEEL, &attacker, &frozen),
        Err(BattleError::UnportedAbilityInteraction(
            AbilityId::SYNCHRONIZE
        )),
        "a frozen holder can thaw before the move executes"
    );
    let mut paralysed = ralts;
    paralysed.set_status1(Status1::Paralysed);
    assert_eq!(
        ensure_admissible(&dex, FLAME_WHEEL, &attacker, &paralysed),
        Ok(()),
        "a permanent status already blocks the burn"
    );
}

#[test]
fn serene_grace_is_refused_only_where_its_doubled_chance_is_observable() {
    let dex = Dex::new();
    let attacker = mon(&dex, DUNSPARCE);
    assert_eq!(attacker.ability(), AbilityId::SERENE_GRACE);
    let refused = Err(BattleError::UnportedAbilityInteraction(
        AbilityId::SERENE_GRACE,
    ));
    for move_id in [FLAME_WHEEL, SACRED_FIRE] {
        assert_eq!(
            ensure_admissible(&dex, move_id, &attacker, &mon(&dex, ZIGZAGOON)),
            refused,
            "{move_id:?} could burn"
        );
        assert_eq!(
            ensure_admissible(&dex, move_id, &attacker, &mon(&dex, CHARMANDER)),
            Ok(()),
            "{move_id:?} against a Fire type is silent at any chance"
        );
        assert_eq!(
            ensure_admissible(&dex, move_id, &attacker, &mon(&dex, WURMPLE)),
            Ok(()),
            "{move_id:?} against Shield Dust is silent at any chance"
        );
    }
    let goldeen = water_veil_goldeen(&dex);
    assert_eq!(
        ensure_admissible(&dex, FLAME_WHEEL, &attacker, &goldeen),
        Ok(()),
        "Flame Wheel's 20% stays a silent discard against Water Veil"
    );
    assert_eq!(
        ensure_admissible(&dex, SACRED_FIRE, &attacker, &goldeen),
        refused,
        "Sacred Fire's doubled 100% is certain and prints Water Veil's message"
    );
}
