//! Wild Pokémon construction with the upstream RNG order: nature, matching
//! personality, then packed IVs (`pokeemerald/src/wild_encounter.c:379`;
//! `src/pokemon.c:2205-2309`).

use assets::{LevelUpLearnsets, MoveId, SpeciesId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::Nature;
use crate::pokemon::{BattlePokemon, Ivs, MAX_IV, MAX_MON_MOVES};

/// Returns the unique moves learned by `level`, retaining the newest four in
/// learning order. An unknown species yields an empty moveset.
#[must_use]
pub fn initial_moveset(species: SpeciesId, level: u8) -> Vec<MoveId> {
    let Some(learnset) = LevelUpLearnsets::new().get(species) else {
        return Vec::new();
    };
    let mut moves: Vec<MoveId> = Vec::with_capacity(MAX_MON_MOVES);
    for entry in learnset {
        if entry.level() > level {
            break;
        }
        if moves.contains(&entry.move_id()) {
            continue;
        }
        if moves.len() == MAX_MON_MOVES {
            moves.remove(0);
        }
        moves.push(entry.move_id());
    }
    moves
}

/// Checks whether a wild Pokémon can enter battle without building it or
/// consuming RNG.
///
/// # Errors
///
/// Returns the first Pokémon validation or move-execution error.
pub fn ensure_wild_startable(dex: &Dex, species: SpeciesId, level: u8) -> Result<(), BattleError> {
    let moves = initial_moveset(species, level);
    BattlePokemon::validate(dex, species, level, &moves)?;
    for move_id in &moves {
        crate::battle::ensure_executable(dex, *move_id)?;
    }
    Ok(())
}

/// Chooses a nature from one RNG draw using its numeric ID order.
#[must_use]
pub fn roll_nature(rng: &mut impl BattleRng) -> Nature {
    Nature::ALL[usize::from(rng.next_u16()) % Nature::ALL.len()]
}

/// Draws personalities until one has the requested nature.
#[must_use]
pub fn roll_personality_for_nature(nature: Nature, rng: &mut impl BattleRng) -> u32 {
    loop {
        let personality = rng.next_u32();
        if Nature::from_personality(personality) == nature {
            return personality;
        }
    }
}

const IV_BITS_PER_STAT: u32 = 5;

fn unpack_three_ivs(draw: u16) -> [u8; 3] {
    let iv_mask = u16::from(MAX_IV);
    [
        u8::try_from(draw & iv_mask).expect("a masked IV fits in u8"),
        u8::try_from((draw >> IV_BITS_PER_STAT) & iv_mask).expect("a masked IV fits in u8"),
        u8::try_from((draw >> (IV_BITS_PER_STAT * 2)) & iv_mask).expect("a masked IV fits in u8"),
    ]
}

/// Draws HP, Attack, Defense, Speed, Special Attack, and Special Defense IVs.
#[must_use]
pub fn roll_ivs(rng: &mut impl BattleRng) -> Ivs {
    let [hp, attack, defense] = unpack_three_ivs(rng.next_u16());
    let [speed, sp_attack, sp_defense] = unpack_three_ivs(rng.next_u16());
    Ivs {
        hp,
        attack,
        defense,
        speed,
        sp_attack,
        sp_defense,
    }
}

/// Builds a wild Pokémon after validating every input without consuming RNG.
/// Draws its nature, matching personality, and IVs in that order.
///
/// # Errors
///
/// Returns the first Pokémon validation error without consuming RNG.
pub fn build_wild_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    BattlePokemon::validate(dex, species, level, &moves)?;
    let requested_nature = roll_nature(rng);
    let personality = roll_personality_for_nature(requested_nature, rng);
    let ivs = roll_ivs(rng);
    BattlePokemon::new(dex, species, level, ivs, personality, moves)
}

/// Builds a Pokémon with one unconstrained personality draw followed by its
/// IV draws, after validating every input without consuming RNG.
///
/// # Errors
///
/// Returns the first Pokémon validation error without consuming RNG.
pub fn build_pokemon_with_random_personality(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    BattlePokemon::validate(dex, species, level, &moves)?;
    let personality = rng.next_u32();
    let ivs = roll_ivs(rng);
    BattlePokemon::new(dex, species, level, ivs, personality, moves)
}

#[cfg(test)]
mod tests;
