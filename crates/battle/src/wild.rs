//! Wild Pokémon construction with the upstream RNG order: the party lead's
//! Cute Charm gender roll, nature (with the lead's Synchronize coin flip),
//! matching personality, then packed IVs
//! (`pokeemerald/src/wild_encounter.c:367-415`; `src/pokemon.c:2205-2346`).

use assets::{GenderRatio, LevelUpLearnsets, MoveId, SpeciesId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::gender::Gender;
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
/// Returns the first Pokémon validation, ability-admission, or move-execution error. The
/// rolled personality can select either ability slot, so a species with any
/// always-refused ability slot is refused.
pub fn ensure_wild_startable(dex: &Dex, species: SpeciesId, level: u8) -> Result<(), BattleError> {
    let moves = initial_moveset(species, level);
    BattlePokemon::validate(dex, species, level, &moves)?;
    crate::battle::admission::ensure_every_ability_slot_admissible(dex, species)?;
    for move_id in &moves {
        crate::battle::ensure_executable(dex, *move_id)?;
    }
    Ok(())
}

/// Chooses the wild Pokémon's nature as upstream's `PickWildMonNature` does
/// outside the Safari Zone (`pokeemerald/src/wild_encounter.c:367-376`).
///
/// A `Some` lead nature means the party's first Pokémon is a non-egg holder of
/// Synchronize: one coin draw (`Random() % 2`) is spent first, and a zero hands
/// back that nature with no further draw. Otherwise (the flip lost, or `None`)
/// the plain random-nature draw follows, so a lost flip costs two draws.
#[must_use]
pub fn pick_wild_nature(synchronize_nature: Option<Nature>, rng: &mut impl BattleRng) -> Nature {
    if let Some(lead_nature) = synchronize_nature {
        if rng.next_u16().is_multiple_of(2) {
            return lead_nature;
        }
    }
    roll_nature(rng)
}

/// Chooses a nature from one RNG draw using its numeric ID order.
#[must_use]
pub fn roll_nature(rng: &mut impl BattleRng) -> Nature {
    Nature::ALL[usize::from(rng.next_u16()) % Nature::ALL.len()]
}

/// Draws personalities until one has the requested nature.
#[must_use]
pub fn roll_personality_for_nature(nature: Nature, rng: &mut impl BattleRng) -> u32 {
    roll_personality_where(rng, |personality| {
        Nature::from_personality(personality) == nature
    })
}

/// Draws personalities until one has the requested nature and gender, as the
/// non-Unown arm of `CreateMonWithGenderNatureLetter` does
/// (`pokeemerald/src/pokemon.c:2335-2343`).
///
/// `gender` must be reachable under `ratio`, or this never returns.
fn roll_personality_for_nature_and_gender(
    nature: Nature,
    ratio: GenderRatio,
    gender: Gender,
    rng: &mut impl BattleRng,
) -> u32 {
    roll_personality_where(rng, |personality| {
        Nature::from_personality(personality) == nature
            && Gender::of_personality(ratio, personality) == gender
    })
}

fn roll_personality_where(rng: &mut impl BattleRng, is_accepted: impl Fn(u32) -> bool) -> u32 {
    loop {
        let personality = rng.next_u32();
        if is_accepted(personality) {
            return personality;
        }
    }
}

/// Decides whether a Cute Charm lead forces the wild Pokémon's gender, as
/// `CreateWildMon` does (`pokeemerald/src/wild_encounter.c:386-398`).
///
/// `cute_charm_gender` is the gender the lead hands the wild Pokémon. The
/// activation draw (`Random() % 3`) is spent only when it names male or female
/// and `ratio` permits both genders; two of its three outcomes force the
/// gender, and the third leaves the ordinary personality roll.
fn roll_cute_charm_gender(
    cute_charm_gender: Option<Gender>,
    ratio: GenderRatio,
    rng: &mut impl BattleRng,
) -> Option<Gender> {
    let forced_gender = cute_charm_gender.filter(|gender| *gender != Gender::Genderless)?;
    if !ratio.permits_both_genders() {
        return None;
    }
    (!rng.next_u16().is_multiple_of(3)).then_some(forced_gender)
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
/// Draws the Cute Charm gender roll, then its nature, matching personality,
/// and IVs in that order.
///
/// `synchronize_nature` is the lead's nature when the party's first Pokémon
/// has Synchronize and is not an egg, else `None`; see [`pick_wild_nature`].
/// `cute_charm_gender` is the gender the party's first Pokémon hands the wild
/// Pokémon when it is a non-egg Cute Charm holder (the opposite of its own,
/// see [`Gender::cute_charm_wild_gender`]), else `None`; a `Genderless` value
/// has no effect.
///
/// # Errors
///
/// Returns the first Pokémon validation error without consuming RNG.
pub fn build_wild_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    moves: Vec<MoveId>,
    synchronize_nature: Option<Nature>,
    cute_charm_gender: Option<Gender>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    BattlePokemon::validate(dex, species, level, &moves)?;
    let ratio = dex.species(species)?.gender_ratio;
    let forced_gender = roll_cute_charm_gender(cute_charm_gender, ratio, rng);
    let requested_nature = pick_wild_nature(synchronize_nature, rng);
    let personality = match forced_gender {
        Some(gender) => {
            roll_personality_for_nature_and_gender(requested_nature, ratio, gender, rng)
        }
        None => roll_personality_for_nature(requested_nature, rng),
    };
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
