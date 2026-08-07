//! Wild-encounter Pokémon construction (S-6): the `CreateWildMon` RNG path.
//!
//! Upstream builds a wild mon in three RNG-observable steps, in this order
//! (`pokeemerald/src/wild_encounter.c:379`, `src/pokemon.c:2205`/`2298`):
//!
//! 1. [`roll_nature`] — `PickWildMonNature` (`wild_encounter.c:335`): one
//!    `Random()` draw picking a `NATURE_*` id.
//! 2. [`roll_personality_for_nature`] — `CreateMonWithNature`
//!    (`pokemon.c:2305`): a rejection loop, `Random32()` per attempt, until
//!    `GetNatureFromPersonality(personality) == nature`
//!    (`personality % NUM_NATURES`, `pokemon.c:5498`).
//! 3. [`roll_ivs`] — `CreateBoxMon`'s `USE_RANDOM_IVS` branch
//!    (`pokemon.c:2276`): exactly two `Random()` draws, five bits per stat.
//!
//! This crate does not depend on `engine` (`engine::save::pokemon::Pokemon`
//! is a save-file *serialization* boundary — encrypted substructures, no
//! computed stats — not a battle-ready representation) or on `engine::rng`
//! specifically: [`crate::damage::BattleRng`] already matches its shape, so
//! any `engine::rng::Rng` (or test double) plugs in directly
//! `(oop-boundaries, minimal-deps)`.
//!
//! Simplified out of this slice (all ability/mode-gated, `(behavioral-
//! fidelity)`'s "as far as the first-encounter species need"):
//! Safari Zone Pokéblock-weighted natures, the leading party mon's
//! Synchronize/Cute Charm influence on nature/gender, and the OT-id shiny
//! reroll loop (`OT_ID_RANDOM_NO_SHINY`) — a wild mon here always takes the
//! player's OT id with **no** extra `Random32()` draws, matching
//! `CreateMonWithNature`'s `OT_ID_PLAYER_ID` argument.
//!
//! [`build_wild_pokemon`] still takes the moveset from its caller rather than
//! deriving it, because a caller may want a fixed one (every test in this
//! crate does). [`initial_moveset`] is the derivation upstream actually
//! performs on the way — `GiveBoxMonInitialMoveset`
//! (`pokeemerald/src/pokemon.c:2991-3012`), which draws nothing — so an
//! integration layer that wants the *real* wild moveset for a species/level
//! can compose the two (issue #169's overworld handoff does exactly that).
//!
//! [`roll_personality_for_nature`]'s rejection loop is upstream's own
//! design — a real 32-bit LCG visits every residue class mod
//! [`crate::nature::Nature::ALL`]'s length `25` within its full period, so
//! the loop always terminates for a real generator; it is not artificially
//! bounded here, matching upstream having no bound either.
//!
//! [`build_pokemon_with_random_personality`] (issue #221) is the module's
//! second construction path, for a mon `CreateMon` builds with
//! `hasFixedPersonality == FALSE` directly rather than through
//! `CreateWildMon`/`CreateMonWithNature`'s nature-first sequence above — the
//! scripted `BATTLE_TYPE_FIRST_BATTLE` Zigzagoon is the only caller today.
//! Its own doc comment has the exact draw-order difference.

use assets::{LevelUpLearnsets, MoveId, SpeciesId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::Nature;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};

/// `GiveBoxMonInitialMoveset` (`pokeemerald/src/pokemon.c:2991-3012`): the
/// moves a mon created at `level` starts with, in the order it learned them.
///
/// Walks `gLevelUpLearnsets[species]` (already extracted as
/// [`assets::LevelUpLearnsets`]) in table order, stopping at the first entry
/// above `level` — upstream compares the packed `moveLevel > (level << 9)`,
/// which is the same comparison on the decoded level. Each move is offered to
/// `GiveMoveToBoxMon` (`:2939-2955`), whose two outcomes are reproduced here
/// `(no-verbatim)`:
///
/// - a move the mon already knows is skipped (`MON_ALREADY_KNOWS_MOVE`) —
///   without costing a slot, which matters for the handful of species whose
///   learnset repeats a move;
/// - once all four slots are full (`MON_HAS_MAX_MOVES`), upstream calls
///   `DeleteFirstMoveAndGiveMoveToBoxMon` (`:3010`), which shifts slots 1..4
///   down and appends the new move — so a high-level mon keeps its *last*
///   four learnable moves, oldest dropped first.
///
/// Draws nothing: the whole function is table-driven, which is why it can sit
/// outside [`build_wild_pokemon`]'s RNG sequence without perturbing it.
///
/// An unknown `species` (outside the extracted table) yields an empty
/// moveset rather than a panic; [`BattlePokemon::new`] rejects that
/// downstream with [`BattleError::InvalidMoveCount`], so it fails closed at
/// the boundary that already validates movesets instead of inventing a
/// Struggle-only mon here.
#[must_use]
pub fn initial_moveset(species: SpeciesId, level: u8) -> Vec<MoveId> {
    let Some(learnset) = LevelUpLearnsets::new().get(species) else {
        return Vec::new();
    };
    let mut moves: Vec<MoveId> = Vec::with_capacity(MAX_MON_MOVES);
    for entry in learnset {
        if entry.level > level {
            break;
        }
        if moves.contains(&entry.move_id) {
            continue;
        }
        if moves.len() == MAX_MON_MOVES {
            moves.remove(0);
        }
        moves.push(entry.move_id);
    }
    moves
}

/// Whether the whole `CreateWildMon` → [`crate::Battle::new`] handoff would
/// accept a wild `(species, level)` — every check both make, composed, with
/// **no state built and no RNG drawn** (issue #207 review): the real
/// [`initial_moveset`] the wild side would know, [`BattlePokemon::validate`]'s
/// species/level/moveset screens, and [`crate::Battle::new`]'s
/// per-move executability screen (the same `ensure_executable` it runs,
/// since the wild rejection loop can land on any slot).
///
/// This is the *pre-flight* an integration layer can run over an encounter
/// table before enabling rolls on a map: upstream never rejects a wild
/// battle, so the only stream-faithful way to handle a moveset this engine
/// cannot execute yet is to find out **before** any encounter draw happens,
/// not after `CreateWildMon`'s five draws are already spent.
///
/// # Errors
///
/// Whatever the first failing screen reports — e.g.
/// [`BattleError::InvalidMoveCount`] for a species outside the learnset
/// table, or [`BattleError::UnsupportedMoveEffect`] /
/// [`BattleError::NonDamagingMove`] for a moveset the turn engine cannot
/// execute (a level-3 Seedot's Bide/Harden, as of this slice).
pub fn ensure_wild_startable(dex: &Dex, species: SpeciesId, level: u8) -> Result<(), BattleError> {
    let moves = initial_moveset(species, level);
    BattlePokemon::validate(dex, species, level, &moves)?;
    for move_id in &moves {
        crate::battle::ensure_executable(dex, *move_id)?;
    }
    Ok(())
}

/// `PickWildMonNature`'s v1 path (`pokeemerald/src/wild_encounter.c:335`):
/// `Random() % NUM_NATURES`. The Safari Zone and Synchronize branches ahead
/// of this in upstream are both ability/mode-gated (see the module docs) and
/// not reached for a plain first encounter.
///
/// # Panics
///
/// Never in practice: `value % 25` is always `0..25`, and
/// [`Nature::from_id`] accepts every id in that range.
#[must_use]
pub fn roll_nature(rng: &mut impl BattleRng) -> Nature {
    let id = (rng.next_u16() % 25) as u8;
    Nature::from_id(id).expect("value % 25 is always a valid NATURE_* id")
}

/// `CreateMonWithNature`'s personality loop (`pokeemerald/src/pokemon.c:2305`):
/// draw `Random32()` until its nature (`personality % NUM_NATURES`) matches
/// `nature`.
#[must_use]
pub fn roll_personality_for_nature(nature: Nature, rng: &mut impl BattleRng) -> u32 {
    loop {
        let personality = rng.next_u32();
        if Nature::from_personality(personality) == nature {
            return personality;
        }
    }
}

/// `CreateBoxMon`'s `USE_RANDOM_IVS` branch (`pokeemerald/src/pokemon.c:2276`):
/// two `Random()` draws, each split into three 5-bit fields
/// (`value & MAX_IV_MASK`, `(value & (MAX_IV_MASK << 5)) >> 5`,
/// `(value & (MAX_IV_MASK << 10)) >> 10`) — HP/Attack/Defense from the first
/// draw, Speed/Sp. Attack/Sp. Defense from the second.
#[must_use]
pub fn roll_ivs(rng: &mut impl BattleRng) -> Ivs {
    const MASK: u16 = 0x1F;
    let first = rng.next_u16();
    let second = rng.next_u16();
    Ivs {
        hp: (first & MASK) as u8,
        attack: ((first >> 5) & MASK) as u8,
        defense: ((first >> 10) & MASK) as u8,
        speed: (second & MASK) as u8,
        sp_attack: ((second >> 5) & MASK) as u8,
        sp_defense: ((second >> 10) & MASK) as u8,
    }
}

/// Build a wild [`BattlePokemon`], drawing nature, personality, and IVs from
/// `rng` in upstream's exact order (see the module docs), then the moveset
/// the caller supplies (`GiveBoxMonInitialMoveset` is not modelled — see the
/// module docs).
///
/// Every caller-supplied input is checked **before the first draw**
/// ([`BattlePokemon::validate`]): a rejected request must leave the shared RNG
/// stream exactly as it found it, the same rule
/// [`crate::battle::Battle::new`] follows `(behavioral-fidelity)`. Only the
/// rolled fields are validated afterwards, and [`roll_ivs`] cannot produce an
/// out-of-range IV in the first place (it masks each to five bits).
///
/// # Errors
///
/// [`BattleError::InvalidLevel`] for a `level` outside `MIN_LEVEL..=MAX_LEVEL`
/// (`1..=100`), [`BattleError::InvalidMoveCount`] /
/// [`BattleError::PlaceholderMove`] for a moveset upstream cannot represent,
/// or [`BattleError::UnknownSpecies`] / [`BattleError::UnknownMove`] if
/// `species`/any of `moves` is not in `dex` — none of which draw.
pub fn build_wild_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    // Before `roll_nature`, not after: an out-of-range level rejected on the
    // way out of `BattlePokemon::new` would already have consumed the five
    // encounter draws.
    BattlePokemon::validate(dex, species, level, &moves)?;
    let nature = roll_nature(rng);
    let personality = roll_personality_for_nature(nature, rng);
    let ivs = roll_ivs(rng);
    // `BattlePokemon::new` re-derives the nature from `personality`; the
    // rejection loop above guarantees it comes out as `nature`.
    BattlePokemon::new(dex, species, level, ivs, personality, moves)
}

/// `CreateMon`'s free-nature personality path
/// (`pokeemerald/src/pokemon.c:2206`-`:2296`, the `hasFixedPersonality ==
/// FALSE` branch of `CreateBoxMon`): personality is a single unconditional
/// `Random32()` draw, with no forced nature ahead of it and therefore no
/// [`roll_personality_for_nature`]-style rejection loop. IVs still come from
/// the same `USE_RANDOM_IVS` two-draw path as [`build_wild_pokemon`]
/// ([`roll_ivs`], reused rather than reimplemented `(no-verbatim)`).
///
/// Distinct from [`build_wild_pokemon`]'s `CreateMonWithNature` path
/// (`PickWildMonNature` then a nature-matching rejection loop): upstream
/// itself uses two different construction functions for these two cases —
/// a wild encounter forces a nature ahead of personality; a mon built
/// through plain `CreateMon` (nothing wild-table-specific about it) does
/// not. The scripted `BATTLE_TYPE_FIRST_BATTLE` Zigzagoon is built this way
/// (`SetUpBattleVarsAndBirchZigzagoon`, `pokeemerald/src/battle_controllers.c:70`,
/// issue #221) — `CreateMon(&gEnemyParty[0], SPECIES_ZIGZAGOON, 2,
/// USE_RANDOM_IVS, 0, 0, OT_ID_PLAYER_ID, 0)` — but nothing here is
/// Zigzagoon-specific, so species/level/moves stay the caller's, the same
/// division of labour [`build_wild_pokemon`] follows
/// (`crates/pokeemerald-rs/src/flow/first_battle.rs` supplies them).
///
/// # Errors
///
/// See [`build_wild_pokemon`] — the same pre-draw validation over the same
/// error set.
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
