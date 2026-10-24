//! Cute Charm wild-encounter tests.
//!
//! These pin the lead's Cute Charm gender forcing in `build_wild_pokemon`,
//! which follows `CreateWildMon` (`pokeemerald/src/wild_encounter.c:386-398`)
//! and the non-Unown arm of `CreateMonWithGenderNatureLetter`
//! (`pokeemerald/src/pokemon.c:2335-2343`): the activation roll and its draw
//! order against the Synchronize coin flip, the rejection loop that keeps both
//! the requested nature and gender, and the species and requests that spend no
//! activation draw. They reuse the fixtures and imports of the parent `wild`
//! test module.

use super::*;

const RATTATA: SpeciesId = SpeciesId(19);
const NIDORAN_FEMALE: SpeciesId = SpeciesId(29);
const NIDORAN_MALE: SpeciesId = SpeciesId(32);
const MAGNEMITE: SpeciesId = SpeciesId(81);

/// Rattata's ratio byte is 127, so a personality is female below a low byte of 127.
const RATTATA_FEMALE_PERSONALITY: u32 = 0;
const RATTATA_MALE_PERSONALITY: u32 = 150;
const HP_AND_SPECIAL_ATTACK_IV_DRAWS: [u16; 2] = [0x001F, 0x03E0];

fn build_rattata_with_cute_charm(
    cute_charm_gender: Option<Gender>,
    synchronize_nature: Option<Nature>,
    scripted_draws: impl IntoIterator<Item = u16>,
) -> (BattlePokemon, SequenceRng) {
    let mut rng = SequenceRng::new(scripted_draws);
    let mon = build_wild_pokemon(
        &Dex::new(),
        RATTATA,
        5,
        vec![TACKLE],
        synchronize_nature,
        cute_charm_gender,
        &mut rng,
    )
    .unwrap();
    (mon, rng)
}

fn gender_of(mon: &BattlePokemon) -> Gender {
    let ratio = Dex::new().species(mon.species()).unwrap().gender_ratio;
    Gender::of_personality(ratio, mon.personality())
}

fn personality_and_iv_draws(personality: u32) -> Vec<u16> {
    personality_draw(personality)
        .into_iter()
        .chain(HP_AND_SPECIAL_ATTACK_IV_DRAWS)
        .collect()
}

#[test]
fn a_cute_charm_roll_of_one_or_two_forces_the_requested_gender() {
    for activation_draw in [1, 2] {
        let nature_draw = u16::from(Nature::Hardy.id());
        let script = [activation_draw, nature_draw]
            .into_iter()
            .chain(personality_and_iv_draws(RATTATA_MALE_PERSONALITY));
        let (mon, rng) = build_rattata_with_cute_charm(Some(Gender::Male), None, script);
        assert_eq!(mon.personality(), RATTATA_MALE_PERSONALITY);
        assert_eq!(gender_of(&mon), Gender::Male);
        assert_eq!(rng.draws(), 6);
        rng.assert_exhausted();
    }
}

#[test]
fn a_forced_female_rejects_a_male_personality_of_the_right_nature() {
    let nature_draw = u16::from(Nature::Hardy.id());
    let script = [2, nature_draw]
        .into_iter()
        .chain(personality_draw(RATTATA_MALE_PERSONALITY))
        .chain(personality_and_iv_draws(RATTATA_FEMALE_PERSONALITY));
    let (mon, rng) = build_rattata_with_cute_charm(Some(Gender::Female), None, script);
    assert_eq!(mon.personality(), RATTATA_FEMALE_PERSONALITY);
    assert_eq!(gender_of(&mon), Gender::Female);
    assert_eq!(rng.draws(), 8);
    rng.assert_exhausted();
}

#[test]
fn a_forced_gender_still_rejects_a_personality_of_the_wrong_nature() {
    let wrong_nature_right_gender = 151;
    let right_nature_wrong_gender = RATTATA_FEMALE_PERSONALITY;
    let script = [1, u16::from(Nature::Hardy.id())]
        .into_iter()
        .chain(personality_draw(wrong_nature_right_gender))
        .chain(personality_draw(right_nature_wrong_gender))
        .chain(personality_and_iv_draws(RATTATA_MALE_PERSONALITY));
    let (mon, rng) = build_rattata_with_cute_charm(Some(Gender::Male), None, script);
    assert_eq!(mon.personality(), RATTATA_MALE_PERSONALITY);
    assert_eq!(mon.nature(), Nature::Hardy);
    assert_eq!(rng.draws(), 10);
    rng.assert_exhausted();
}

#[test]
fn a_cute_charm_roll_of_zero_leaves_the_ordinary_personality_roll() {
    let nature_draw = u16::from(Nature::Hardy.id());
    let script = [0, nature_draw]
        .into_iter()
        .chain(personality_and_iv_draws(RATTATA_FEMALE_PERSONALITY));
    let (mon, rng) = build_rattata_with_cute_charm(Some(Gender::Male), None, script);
    assert_eq!(mon.personality(), RATTATA_FEMALE_PERSONALITY);
    assert_eq!(gender_of(&mon), Gender::Female);
    assert_eq!(rng.draws(), 6);
    rng.assert_exhausted();
}

#[test]
fn no_cute_charm_holder_spends_no_activation_draw() {
    let nature_draw = u16::from(Nature::Hardy.id());
    let script = [nature_draw]
        .into_iter()
        .chain(personality_and_iv_draws(RATTATA_FEMALE_PERSONALITY));
    let (mon, rng) = build_rattata_with_cute_charm(None, None, script);
    assert_eq!(mon.personality(), RATTATA_FEMALE_PERSONALITY);
    assert_eq!(rng.draws(), 5);
    rng.assert_exhausted();
}

#[test]
fn a_genderless_cute_charm_request_has_no_effect_on_a_mixed_species() {
    let nature_draw = u16::from(Nature::Hardy.id());
    let script = [nature_draw]
        .into_iter()
        .chain(personality_and_iv_draws(RATTATA_FEMALE_PERSONALITY));
    let (mon, rng) = build_rattata_with_cute_charm(Some(Gender::Genderless), None, script);
    assert_eq!(mon.personality(), RATTATA_FEMALE_PERSONALITY);
    assert_eq!(rng.draws(), 5);
    rng.assert_exhausted();
}

#[test]
fn gender_locked_and_genderless_species_take_no_activation_draw() {
    for (species, locked_gender) in [
        (NIDORAN_MALE, Gender::Male),
        (NIDORAN_FEMALE, Gender::Female),
        (MAGNEMITE, Gender::Genderless),
    ] {
        let nature_draw = u16::from(Nature::Hardy.id());
        let mut rng =
            SequenceRng::new([nature_draw].into_iter().chain(personality_and_iv_draws(0)));
        let mon = build_wild_pokemon(
            &Dex::new(),
            species,
            5,
            vec![TACKLE],
            None,
            Some(Gender::Male),
            &mut rng,
        )
        .unwrap();
        assert_eq!(gender_of(&mon), locked_gender, "species {species:?}");
        assert_eq!(rng.draws(), 5, "species {species:?}");
        rng.assert_exhausted();
    }
}

#[test]
fn the_cute_charm_roll_comes_before_the_synchronize_coin_flip() {
    let quirky_personality = 174;
    assert_eq!(Nature::from_personality(quirky_personality), Nature::Quirky);
    assert_eq!(
        Gender::of_personality(assets::GenderRatio::HALF_FEMALE, quirky_personality),
        Gender::Male
    );
    let winning_flip = [1, 0].into_iter();
    let (mon, rng) = build_rattata_with_cute_charm(
        Some(Gender::Male),
        Some(Nature::Quirky),
        winning_flip.chain(personality_and_iv_draws(quirky_personality)),
    );
    assert_eq!(mon.personality(), quirky_personality);
    assert_eq!(rng.draws(), 6);
    rng.assert_exhausted();

    let adamant_male_personality = 153;
    assert_eq!(
        Nature::from_personality(adamant_male_personality),
        Nature::Adamant
    );
    let losing_flip_then_adamant = [1, 1, u16::from(Nature::Adamant.id())];
    let (mon, rng) = build_rattata_with_cute_charm(
        Some(Gender::Male),
        Some(Nature::Quirky),
        losing_flip_then_adamant
            .into_iter()
            .chain(personality_and_iv_draws(adamant_male_personality)),
    );
    assert_eq!(mon.personality(), adamant_male_personality);
    assert_eq!(rng.draws(), 7);
    rng.assert_exhausted();
}

#[test]
fn a_rejected_request_draws_nothing_even_with_a_cute_charm_lead() {
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        build_wild_pokemon(
            &Dex::new(),
            RATTATA,
            0,
            vec![TACKLE],
            None,
            Some(Gender::Male),
            &mut rng,
        ),
        Err(BattleError::InvalidLevel(0))
    );
    assert_eq!(rng.draws(), 0);
}
