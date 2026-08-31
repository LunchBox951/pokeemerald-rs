use super::{
    build_pokemon_with_random_personality, build_wild_pokemon, ensure_wild_startable,
    initial_moveset, roll_ivs, roll_nature, roll_personality_for_nature,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::Nature;
use crate::pokemon::{Ivs, MAX_MON_MOVES, MOVE_NONE};
use crate::script_rng::SequenceRng;
use assets::{MoveId, SpeciesId};

const BULBASAUR: SpeciesId = SpeciesId(1);
const TREECKO: SpeciesId = SpeciesId(277);
const POOCHYENA: SpeciesId = SpeciesId(286);
const ZIGZAGOON: SpeciesId = SpeciesId(288);
const WURMPLE: SpeciesId = SpeciesId(290);
const SEEDOT: SpeciesId = SpeciesId(298);
const UNKNOWN_SPECIES: SpeciesId = SpeciesId(60_000);

const TACKLE: MoveId = MoveId(33);
const TAIL_WHIP: MoveId = MoveId(39);
const GROWL: MoveId = MoveId(45);
const STRING_SHOT: MoveId = MoveId(81);
const HARDEN: MoveId = MoveId(106);
const BIDE: MoveId = MoveId(117);
const HOWL: MoveId = MoveId(336);
const UNKNOWN_MOVE: MoveId = MoveId(60_000);

fn personality_draw(personality: u32) -> [u16; 2] {
    let [low_first, low_second, high_first, high_second] = personality.to_le_bytes();
    [
        u16::from_le_bytes([low_first, low_second]),
        u16::from_le_bytes([high_first, high_second]),
    ]
}

#[test]
fn roll_nature_is_modulo_the_nature_count_of_one_draw() {
    for (draw, expected) in [
        (u16::from(Nature::Hardy.id()), Nature::Hardy),
        (u16::from(Nature::Quirky.id()), Nature::Quirky),
        (
            u16::try_from(Nature::ALL.len()).expect("the nature count fits in u16"),
            Nature::Hardy,
        ),
    ] {
        let mut rng = SequenceRng::new([draw]);
        assert_eq!(roll_nature(&mut rng), expected);
        assert_eq!(rng.draws(), 1);
    }
}

#[test]
fn roll_personality_for_nature_stops_at_the_first_match() {
    let personality = u32::from(Nature::Hardy.id());
    let mut rng = SequenceRng::new(personality_draw(personality));
    assert_eq!(
        roll_personality_for_nature(Nature::Hardy, &mut rng),
        personality
    );
    assert_eq!(rng.draws(), 2);
}

#[test]
fn roll_personality_for_nature_rejects_until_a_match() {
    let rejected_personality = u32::from(Nature::Lonely.id());
    let accepted_personality = u32::from(Nature::Bold.id());
    let draws = [
        personality_draw(rejected_personality),
        personality_draw(accepted_personality),
    ];
    let mut rng = SequenceRng::new(draws.into_iter().flatten());
    assert_eq!(
        roll_personality_for_nature(Nature::Bold, &mut rng),
        accepted_personality
    );
    assert_eq!(rng.draws(), 4);
}

#[test]
fn roll_ivs_splits_two_draws_into_five_bit_fields() {
    let hp_31_attack_10_defense_21 = 0b0_10101_01010_11111;
    let speed_3_sp_attack_2_sp_defense_1 = 0b0_00001_00010_00011;
    let mut rng = SequenceRng::new([hp_31_attack_10_defense_21, speed_3_sp_attack_2_sp_defense_1]);
    assert_eq!(
        roll_ivs(&mut rng),
        Ivs {
            hp: 31,
            attack: 10,
            defense: 21,
            speed: 3,
            sp_attack: 2,
            sp_defense: 1,
        }
    );
    assert_eq!(rng.draws(), 2);
}

#[test]
fn roll_ivs_masks_to_five_bits_even_with_high_bits_set() {
    let mut rng = SequenceRng::new([u16::MAX, u16::MAX]);
    assert_eq!(
        roll_ivs(&mut rng),
        Ivs {
            hp: 31,
            attack: 31,
            defense: 31,
            speed: 31,
            sp_attack: 31,
            sp_defense: 31,
        }
    );
}

#[test]
fn build_wild_pokemon_draws_nature_then_personality_then_ivs_in_order() {
    let dex = Dex::new();
    let personality = u32::from(Nature::Quirky.id());
    let [personality_low, personality_high] = personality_draw(personality);
    let hp_only_iv_draw = 0x001F;
    let special_attack_only_iv_draw = 0x03E0;
    let mut rng = SequenceRng::new([
        u16::from(Nature::Quirky.id()),
        personality_low,
        personality_high,
        hp_only_iv_draw,
        special_attack_only_iv_draw,
    ]);
    let mon = build_wild_pokemon(&dex, BULBASAUR, 5, vec![TACKLE], &mut rng).unwrap();
    assert_eq!(mon.nature(), Nature::Quirky);
    assert_eq!(mon.personality(), personality);
    assert_eq!(
        mon.ivs(),
        Ivs {
            hp: 31,
            attack: 0,
            defense: 0,
            speed: 0,
            sp_attack: 31,
            sp_defense: 0,
        }
    );
    assert_eq!(rng.draws(), 5);
    assert_eq!(mon.level(), 5);
    assert_eq!(mon.species(), BULBASAUR);
    assert!(!mon.is_fainted());
}

#[test]
fn build_wild_pokemon_rejects_bad_inputs_before_drawing_anything() {
    let dex = Dex::new();
    for (level, moves, expected) in [
        (101, vec![TACKLE], BattleError::InvalidLevel(101)),
        (0, vec![TACKLE], BattleError::InvalidLevel(0)),
        (5, vec![], BattleError::InvalidMoveCount(0)),
        (5, vec![MOVE_NONE], BattleError::PlaceholderMove(0)),
        (
            5,
            vec![UNKNOWN_MOVE],
            BattleError::UnknownMove(UNKNOWN_MOVE),
        ),
    ] {
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            build_wild_pokemon(&dex, BULBASAUR, level, moves, &mut rng),
            Err(expected)
        );
        assert_eq!(rng.draws(), 0, "a rejected request must not draw");
    }
}

#[test]
fn build_pokemon_with_random_personality_draws_only_personality_then_ivs() {
    let dex = Dex::new();
    let personality = u32::from(Nature::Lonely.id());
    let [personality_low, personality_high] = personality_draw(personality);
    let hp_only_iv_draw = 0x001F;
    let special_attack_only_iv_draw = 0x03E0;
    let mut rng = SequenceRng::new([
        personality_low,
        personality_high,
        hp_only_iv_draw,
        special_attack_only_iv_draw,
    ]);
    let mon =
        build_pokemon_with_random_personality(&dex, BULBASAUR, 5, vec![TACKLE], &mut rng).unwrap();
    assert_eq!(mon.personality(), personality);
    assert_eq!(
        mon.ivs(),
        Ivs {
            hp: 31,
            attack: 0,
            defense: 0,
            speed: 0,
            sp_attack: 31,
            sp_defense: 0,
        }
    );
    assert_eq!(rng.draws(), 4);
}

#[test]
fn build_pokemon_with_random_personality_rejects_bad_inputs_before_drawing_anything() {
    let dex = Dex::new();
    for (level, moves, expected) in [
        (101, vec![TACKLE], BattleError::InvalidLevel(101)),
        (0, vec![TACKLE], BattleError::InvalidLevel(0)),
        (5, vec![], BattleError::InvalidMoveCount(0)),
        (5, vec![MOVE_NONE], BattleError::PlaceholderMove(0)),
        (
            5,
            vec![UNKNOWN_MOVE],
            BattleError::UnknownMove(UNKNOWN_MOVE),
        ),
    ] {
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            build_pokemon_with_random_personality(&dex, BULBASAUR, level, moves, &mut rng),
            Err(expected)
        );
        assert_eq!(rng.draws(), 0, "a rejected request must not draw");
    }
}

#[test]
fn rolled_ivs_are_always_within_the_upstream_range() {
    for value in [0u16, 1, 0x5A5A, u16::MAX] {
        let mut rng = SequenceRng::new([value, !value]);
        assert!(roll_ivs(&mut rng).is_valid());
    }
}

#[test]
fn route_101_wild_movesets_match_their_level_up_learnsets() {
    for (species, level, expected) in [
        (POOCHYENA, 2, vec![TACKLE]),
        (POOCHYENA, 3, vec![TACKLE]),
        (POOCHYENA, 4, vec![TACKLE]),
        (POOCHYENA, 5, vec![TACKLE, HOWL]),
        (ZIGZAGOON, 2, vec![TACKLE, GROWL]),
        (ZIGZAGOON, 3, vec![TACKLE, GROWL]),
        (WURMPLE, 2, vec![TACKLE, STRING_SHOT]),
        (WURMPLE, 3, vec![TACKLE, STRING_SHOT]),
    ] {
        assert_eq!(
            initial_moveset(species, level),
            expected,
            "species {} at level {level}",
            species.0
        );
    }
}

#[test]
fn a_higher_level_mon_knows_the_moves_it_has_reached_in_learning_order() {
    let at_five = initial_moveset(ZIGZAGOON, 5);
    let at_nine = initial_moveset(ZIGZAGOON, 9);
    assert_eq!(at_five, vec![TACKLE, GROWL, TAIL_WHIP]);
    assert!(
        at_nine.starts_with(&at_five),
        "learning order must be preserved: {at_nine:?} does not extend {at_five:?}"
    );
}

#[test]
fn a_full_moveset_drops_the_oldest_move_first() {
    let learnsets = assets::LevelUpLearnsets::new();
    for species in [POOCHYENA, ZIGZAGOON, WURMPLE, BULBASAUR, TREECKO] {
        let moves = initial_moveset(species, 100);
        assert!(
            moves.len() <= MAX_MON_MOVES,
            "species {} kept {} moves",
            species.0,
            moves.len()
        );
        let mut deduped = moves.clone();
        deduped.dedup();
        assert_eq!(deduped, moves, "species {} repeated a move", species.0);

        let learnset = learnsets
            .get(species)
            .expect("species is in the extracted learnset table");
        let mut all: Vec<MoveId> = Vec::new();
        for entry in learnset {
            if entry.level <= 100 && !all.contains(&entry.move_id) {
                all.push(entry.move_id);
            }
        }
        assert_eq!(
            moves,
            all[all.len().saturating_sub(MAX_MON_MOVES)..].to_vec()
        );
    }
}

#[test]
fn ensure_wild_startable_accepts_route_101_mons_and_rejects_a_bide_seedot() {
    let dex = Dex::new();
    for species in [WURMPLE, POOCHYENA, ZIGZAGOON] {
        for level in 2..=3 {
            assert_eq!(
                ensure_wild_startable(&dex, species, level),
                Ok(()),
                "species {} at level {level} must be startable",
                species.0
            );
        }
    }
    assert_eq!(initial_moveset(SEEDOT, 3), vec![BIDE, HARDEN]);
    assert_eq!(crate::battle::ensure_executable(&dex, HARDEN), Ok(()));
    assert_eq!(dex.move_data(BIDE).unwrap().power, 1);
    assert_eq!(
        ensure_wild_startable(&dex, SEEDOT, 3),
        Err(BattleError::UnsupportedMoveEffect(BIDE)),
    );
}

#[test]
fn an_unknown_species_yields_no_moves_and_is_rejected_downstream() {
    assert!(initial_moveset(UNKNOWN_SPECIES, 5).is_empty());
    let dex = Dex::new();
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        build_wild_pokemon(&dex, UNKNOWN_SPECIES, 5, Vec::new(), &mut rng),
        Err(BattleError::InvalidMoveCount(0))
    );
    assert_eq!(rng.draws(), 0);
}
