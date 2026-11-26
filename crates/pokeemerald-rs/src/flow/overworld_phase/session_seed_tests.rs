//! A session's RNG seed: the trainer ID's low half and the
//! field stream's start come from the seed the caller names, not from one
//! fixed value, for both a new game and a continue.

use super::test_support::ONE_F;
use super::OverworldPhase;
use crate::new_game::{self, RngSeedSource};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;

const SEED_A: u32 = 0x1234;
const SEED_B: u32 = 0x4321;

fn new_game_phase_seeded(seed: u32) -> OverworldPhase {
    OverworldPhase::new(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        PlayerState::new((4, 6), 3, Direction::West),
        None,
        crate::pack_source::PackSource::Runtime,
        new_game::NewGameOptions::DEFAULT,
        new_game::NewGameIdentity::DEFAULT,
        seed,
    )
}

fn continued_phase_seeded(seed: u32) -> OverworldPhase {
    let (block1, block2) = new_game::init_save_blocks_for_new_game();
    OverworldPhase::from_saved_seeded(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        block1,
        block2,
        seed,
    )
}

/// Upstream stores the seed as `sTrainerId` (the id's low half) and draws
/// the high half once after it (`pokeemerald/src/main.c:208-214`,
/// `src/new_game.c:84-88`); the phase keeps the advanced stream.
#[test]
fn a_new_game_takes_its_trainer_id_and_stream_from_the_named_seed() {
    let phase = new_game_phase_seeded(SEED_A);

    let mut expected = Rng::new(SEED_A);
    let high = expected.next_u16();
    let trainer_id = (u32::from(high) << 16) | SEED_A;
    assert_eq!(phase.save2.player_trainer_id, trainer_id.to_le_bytes());
    assert_eq!(phase.rng.state(), expected.state());
}

#[test]
fn new_games_with_different_seeds_get_different_trainer_ids_and_streams() {
    let first = new_game_phase_seeded(SEED_A);
    let second = new_game_phase_seeded(SEED_B);

    assert_ne!(
        first.save2.player_trainer_id,
        second.save2.player_trainer_id
    );
    assert_ne!(first.rng.state(), second.rng.state());
    assert_ne!(
        first.save2.player_trainer_id, [0; 4],
        "a seeded game is not the all-zero trainer id"
    );
}

#[test]
fn new_games_from_the_entropy_source_do_not_share_one_trainer_id() {
    let first = new_game_phase_seeded(RngSeedSource::Entropy.draw());
    let differs = (0..32).any(|_| {
        let other = new_game_phase_seeded(RngSeedSource::Entropy.draw());
        other.save2.player_trainer_id != first.save2.player_trainer_id
    });
    assert!(differs, "32 more new games all repeated one trainer id");
}

#[test]
fn the_fixed_source_reproduces_the_deterministic_new_game() {
    let fixed = new_game_phase_seeded(RngSeedSource::FIXED.draw());
    let pinned = new_game_phase_seeded(new_game::NEW_GAME_RNG_SEED);
    assert_eq!(
        fixed.save2.player_trainer_id,
        pinned.save2.player_trainer_id
    );
    assert_eq!(fixed.rng.state(), pinned.rng.state());
}

#[test]
fn a_continue_starts_its_field_stream_from_the_named_seed() {
    assert_eq!(continued_phase_seeded(SEED_A).rng.state(), SEED_A);
    assert_eq!(continued_phase_seeded(SEED_B).rng.state(), SEED_B);
}

#[test]
fn the_deterministic_continue_helper_still_pins_the_fixed_seed() {
    let (block1, block2) = new_game::init_save_blocks_for_new_game();
    let phase = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        block1,
        block2,
    );
    assert_eq!(phase.rng.state(), new_game::NEW_GAME_RNG_SEED);
}
