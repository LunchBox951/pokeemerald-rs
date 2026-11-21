use assets::{MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Ivs};
use engine::rng::RandomSource;
use engine::save::Pokemon;

use super::{adjust_friendship_walking, has_valid_species};
use crate::party::{GROWTH_FRIENDSHIP, MISC_MET_DATA, POKE_BALL_SHIFT};

const LUXURY_BALL: u16 = 11;
const POKE_BALL: u16 = 4;
const SPECIES_EGG: u16 = 412;

/// Plays back fixed draws and records how many were taken.
struct Scripted {
    draws: Vec<u16>,
    taken: usize,
}

impl Scripted {
    fn new(draws: &[u16]) -> Self {
        Self {
            draws: draws.to_vec(),
            taken: 0,
        }
    }
}

impl RandomSource for Scripted {
    fn next_u16(&mut self) -> u16 {
        let draw = self.draws[self.taken];
        self.taken += 1;
        draw
    }
}

/// A level-5 Treecko through the production encoder, with `friendship` and
/// `ball` written into its growth and origins words.
pub(crate) fn record(friendship: u8, ball: u16) -> Pokemon {
    let dex = Dex::new();
    let ivs = Ivs {
        hp: 15,
        attack: 15,
        defense: 15,
        speed: 15,
        sp_attack: 15,
        sp_defense: 15,
    };
    let mon = BattlePokemon::new(&dex, SpeciesId(277), 5, ivs, 7, vec![MoveId::SLASH])
        .expect("Treecko and Slash are in the dex");
    let mut record = crate::party::to_save_pokemon(&dex, &mon);
    let mut substructures = record.box_data.substructures().unwrap();
    substructures.growth[GROWTH_FRIENDSHIP] = friendship;
    let origins = u16::from_le_bytes(substructures.misc[MISC_MET_DATA].try_into().unwrap());
    let origins = (origins & !(0xF << POKE_BALL_SHIFT)) | (ball << POKE_BALL_SHIFT);
    substructures.misc[MISC_MET_DATA].copy_from_slice(&origins.to_le_bytes());
    record.box_data.set_substructures(&substructures);
    record
}

pub(crate) fn friendship_of(record: &Pokemon) -> u8 {
    record.box_data.substructures().unwrap().growth[GROWTH_FRIENDSHIP]
}

fn walk(record: &mut Pokemon, draw: u16) -> usize {
    let mut rng = Scripted::new(&[draw]);
    adjust_friendship_walking(record, &mut rng);
    rng.taken
}

/// An even draw adds one (`sFriendshipEventModifiers`' walking row is
/// `{ 1, 1, 1 }` in every tier, `pokemon.c:2100`), with no tier dropping it.
#[test]
fn an_even_draw_adds_one_in_every_friendship_tier() {
    for (before, after) in [
        (0, 1),
        (70, 71),
        (99, 100),
        (100, 101),
        (199, 200),
        (200, 201),
    ] {
        let mut mon = record(before, POKE_BALL);
        assert_eq!(walk(&mut mon, 0), 1);
        assert_eq!(friendship_of(&mon), after);
    }
}

#[test]
fn an_odd_draw_changes_nothing() {
    for ball in [POKE_BALL, LUXURY_BALL] {
        let mut mon = record(70, ball);
        let before = mon;
        assert_eq!(walk(&mut mon, 1), 1);
        assert_eq!(mon, before);
    }
}

/// A Luxury Ball adds one more (`pokemon.c:5971-5973`).
#[test]
fn a_luxury_ball_adds_a_second_point() {
    let mut mon = record(70, LUXURY_BALL);
    walk(&mut mon, 2);
    assert_eq!(friendship_of(&mon), 72);
}

/// The result is clamped to 255 (`MAX_FRIENDSHIP`, `:5979-5980`) and the draw
/// is still taken at the cap.
#[test]
fn friendship_clamps_at_255_and_still_draws() {
    let mut luxury = record(254, LUXURY_BALL);
    assert_eq!(walk(&mut luxury, 0), 1);
    assert_eq!(friendship_of(&luxury), 255);
    let mut capped = record(255, POKE_BALL);
    assert_eq!(walk(&mut capped, 0), 1);
    assert_eq!(friendship_of(&capped), 255);
}

/// The write goes through the checksummed setter and touches only the one byte.
#[test]
fn only_the_friendship_byte_changes_and_the_checksum_follows() {
    let mut mon = record(70, POKE_BALL);
    let before = mon.box_data.substructures().unwrap();
    walk(&mut mon, 0);
    let mut after = mon.box_data.substructures().expect("checksum recomputed");
    assert_eq!(after.growth[GROWTH_FRIENDSHIP], 71);
    after.growth[GROWTH_FRIENDSHIP] = 70;
    assert_eq!(after, before);
}

/// An empty slot, a secure-egg-bit egg, a `SPECIES_EGG` record, and a Bad Egg
/// return before drawing (`pokemon.c:5938`).
#[test]
fn ineligible_slots_draw_nothing() {
    let mut empty = Pokemon::default();
    assert_eq!(walk(&mut empty, 0), 0);
    assert_eq!(empty, Pokemon::default());

    let mut egg = record(70, POKE_BALL);
    let mut substructures = egg.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap());
    substructures.misc[4..8].copy_from_slice(&(iv_word | 1 << 30).to_le_bytes());
    egg.box_data.set_substructures(&substructures);

    let mut species_egg = record(70, POKE_BALL);
    let mut substructures = species_egg.box_data.substructures().unwrap();
    substructures.growth[0..2].copy_from_slice(&SPECIES_EGG.to_le_bytes());
    species_egg.box_data.set_substructures(&substructures);

    let mut bad_egg = record(70, POKE_BALL);
    let mut bytes = bad_egg.box_data.to_bytes();
    *bytes.last_mut().unwrap() ^= 0xFF;
    bad_egg.box_data = engine::save::BoxPokemon::from_bytes(bytes);
    assert!(bad_egg.box_data.substructures().is_err());

    for mut mon in [egg, species_egg, bad_egg] {
        assert!(!has_valid_species(&mon));
        let before = mon;
        assert_eq!(walk(&mut mon, 0), 0);
        assert_eq!(mon, before);
    }
}

/// A fainted valid member still draws: only the species gates it.
#[test]
fn a_fainted_member_still_draws() {
    let mut mon = record(70, POKE_BALL);
    mon.hp = 0;
    assert_eq!(walk(&mut mon, 0), 1);
    assert_eq!(friendship_of(&mon), 71);
}
