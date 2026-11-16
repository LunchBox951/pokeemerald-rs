//! Party slot 0's ability handoff to wild level selection (`ChooseWildMonLevel`),
//! driven through the engine chooser with a scripted draw sequence.

use super::super::{slot_zero_wild_level_ability, to_save_pokemon};
use assets::{AbilityId, MoveId, SpeciesId, WildPokemon};
use battle::{BattlePokemon, Dex, Ivs};
use engine::overworld::wild_encounter::choose_wild_mon_level;
use engine::rng::RandomSource;
use engine::save::Pokemon;

const TOGEPI: SpeciesId = SpeciesId(175);
const MANKEY: SpeciesId = SpeciesId(56);
const ABSOL: SpeciesId = SpeciesId(376);
const MEW: SpeciesId = SpeciesId(151);

const HEADER_FLAGS_BYTE: usize = 19;
const HEADER_BAD_EGG_BIT: u8 = 1 << 0;
const HEADER_EGG_BIT: u8 = 1 << 2;

struct Scripted<I>(I);

impl<I: ExactSizeIterator<Item = u16>> RandomSource for Scripted<I> {
    fn next_u16(&mut self) -> u16 {
        self.0.next().expect("the script covers every draw")
    }
}

fn battler(species: SpeciesId) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        50,
        Ivs::default(),
        0,
        vec![MoveId::POUND],
    )
    .unwrap()
}

fn saved(species: SpeciesId) -> Pokemon {
    to_save_pokemon(&Dex::new(), &battler(species))
}

fn ordinary_party_mate() -> Pokemon {
    to_save_pokemon(&Dex::new(), &crate::new_game::provisional_starter())
}

/// The level and the number of draws left unspent for a band rolled behind `party`.
fn level_and_unspent_draws(
    party: &[Pokemon],
    (min_level, max_level): (u8, u8),
    script: &[u16],
) -> (u8, usize) {
    let mon = WildPokemon {
        min_level,
        max_level,
        species: SpeciesId(1),
    };
    let mut rng = Scripted(script.iter().copied());
    let ability = slot_zero_wild_level_ability(&Dex::new(), party);
    let level = choose_wild_mon_level(&mon, ability, &mut rng);
    (level, rng.0.len())
}

#[test]
fn each_bias_ability_in_slot_zero_spends_a_coin_and_either_outcome_applies() {
    for species in [TOGEPI, MANKEY, ABSOL] {
        let party = [saved(species)];
        assert_eq!(level_and_unspent_draws(&party, (10, 13), &[2, 0]), (13, 0));
        assert_eq!(level_and_unspent_draws(&party, (10, 13), &[2, 1]), (11, 0));
        assert_eq!(level_and_unspent_draws(&party, (13, 10), &[2, 1]), (11, 0));
        assert_eq!(
            level_and_unspent_draws(&party, (7, 7), &[65535, 1]),
            (7, 0),
            "a one-level band still spends the coin"
        );
    }
}

#[test]
fn a_fainted_slot_zero_behind_another_lead_still_biases() {
    let mut togepi = battler(TOGEPI);
    togepi.apply_damage(u32::MAX);
    let party = [to_save_pokemon(&Dex::new(), &togepi), ordinary_party_mate()];
    assert_eq!(level_and_unspent_draws(&party, (10, 13), &[2, 0]), (13, 0));
}

#[test]
fn an_egg_an_empty_party_a_later_slot_or_another_ability_spends_no_coin() {
    for flag in [HEADER_EGG_BIT, HEADER_BAD_EGG_BIT] {
        let mut bytes = saved(TOGEPI).to_bytes();
        bytes[HEADER_FLAGS_BYTE] |= flag;
        let egg = Pokemon::from_bytes(bytes);
        assert_eq!(
            level_and_unspent_draws(&[egg, ordinary_party_mate()], (10, 13), &[2, 0]),
            (12, 1)
        );
    }
    assert_eq!(level_and_unspent_draws(&[], (10, 13), &[2, 0]), (12, 1));
    assert_eq!(
        level_and_unspent_draws(&[ordinary_party_mate(), saved(TOGEPI)], (10, 13), &[2, 0]),
        (12, 1)
    );
    assert_eq!(
        level_and_unspent_draws(&[saved(MEW)], (10, 13), &[2, 0]),
        (12, 1)
    );
}

#[test]
fn slot_zero_reports_its_own_ability() {
    let dex = Dex::new();
    assert_eq!(
        slot_zero_wild_level_ability(&dex, &[saved(ABSOL)]),
        Some(AbilityId::PRESSURE)
    );
    assert_eq!(
        slot_zero_wild_level_ability(&dex, &[saved(MEW)]),
        Some(AbilityId::SYNCHRONIZE)
    );
}
