//! The level-five starter grant: `ScriptGiveMon(starter, 5, ITEM_NONE, 0, 0, 0)`
//! (`pokeemerald/src/battle_setup.c:921-923`,
//! `pokeemerald/src/script_pokemon_util.c:68-72`) into a fresh party.
//!
//! The creation contract is `CreateMon`'s (`pokeemerald/src/pokemon.c:2215-2302`)
//! with a random personality and random IVs:
//!
//! 1. four consecutive 16-bit draws, with no nature, shiny, or OT-ID
//!    constraint: personality low half, personality high half, then the packed
//!    IV words (HP/Attack/Defense, then Speed/Special Attack/Special Defense);
//! 2. the player's trainer ID, the first seven bytes of the player's name, and
//!    the player's gender (one bit) as the original trainer;
//! 3. the species' base friendship, `ITEM_POKE_BALL`, `VERSION_EMERALD`, and
//!    the creation level as met level;
//! 4. the met location, which is the current map's region-map section -- state
//!    this module cannot see, so the caller supplies it;
//! 5. `GiveBoxMonInitialMoveset`'s moves at full PP.
//!
//! The runtime battler and the saved record come out of the same
//! [`to_save_pokemon`] encoding, so they cannot disagree; [`LoadedLead`] then
//! owns both. Not covered here, and left to their owners: the chooser, the
//! starter story variable, the battle launch, the Pokédex seen/caught flags
//! `ScriptGiveMon` sets, and `GiveMonToPlayer`'s party-full PC routing.

use assets::SpeciesId;
use battle::{build_pokemon_with_random_personality, initial_moveset, BattleError, BattleRng, Dex};
use engine::save::{PlayerGender, SaveBlock1, SaveBlock2, BOX_OT_NAME_LEN, SUBSTRUCTURE_LEN};

use super::{to_save_pokemon, LoadedLead, MISC_MET_DATA, MISC_MET_LOCATION, OT_GENDER_SHIFT};

/// `ScriptGiveMon`'s level for the starter (`battle_setup.c:922`).
const STARTER_LEVEL: u8 = 5;

/// One of the three Route 101 starters, with upstream's species ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Starter {
    Treecko,
    Torchic,
    Mudkip,
}

impl Starter {
    pub(crate) const ALL: [Self; 3] = [Self::Treecko, Self::Torchic, Self::Mudkip];

    pub(crate) const fn species(self) -> SpeciesId {
        match self {
            Self::Treecko => SpeciesId(277),
            Self::Torchic => SpeciesId(280),
            Self::Mudkip => SpeciesId(283),
        }
    }
}

/// Why the starter was not granted. Neither case draws RNG or writes the save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GrantError {
    /// The destination party already holds members; this grant files the
    /// first one and does not model `GiveMonToPlayer`'s later slots.
    PartyNotEmpty { count: u8 },
    /// The species or level could not be built.
    Battler(BattleError),
}

/// Creates `starter` at level five for the player in `identity`, files it as
/// the only member of `party`, and returns the lead that owns the live battler
/// and its backing record.
///
/// `met_location` is the numeric `MAPSEC_*` of the map the player stands in
/// (Route 101, `16`, at the genuine handout).
///
/// # Errors
///
/// Returns [`GrantError`] before drawing from `rng` or writing to `party`.
pub(crate) fn grant_starter(
    dex: &Dex,
    rng: &mut impl BattleRng,
    starter: Starter,
    met_location: u8,
    identity: &SaveBlock2,
    party: &mut SaveBlock1,
) -> Result<LoadedLead, GrantError> {
    if party.player_party_count != 0 {
        return Err(GrantError::PartyNotEmpty {
            count: party.player_party_count,
        });
    }
    let species = starter.species();
    let battler = build_pokemon_with_random_personality(
        dex,
        species,
        STARTER_LEVEL,
        initial_moveset(species, STARTER_LEVEL),
        rng,
    )
    .map_err(GrantError::Battler)?
    .with_original_trainer_id(u32::from_le_bytes(identity.player_trainer_id));

    let mut record = to_save_pokemon(dex, &battler);
    stamp_encounter_and_owner(&mut record, met_location, identity);

    party.player_party[0] = record;
    party.player_party_count = 1;
    Ok(LoadedLead::created(dex, battler, record))
}

/// `CreateBoxMon`'s OT name, OT gender, and met location stamps
/// (`pokeemerald/src/pokemon.c:2253-2263`).
fn stamp_encounter_and_owner(
    record: &mut engine::save::Pokemon,
    met_location: u8,
    identity: &SaveBlock2,
) {
    let mut ot_name = [0u8; BOX_OT_NAME_LEN];
    ot_name.copy_from_slice(&identity.player_name[..BOX_OT_NAME_LEN]);
    record.box_data.set_ot_name(ot_name);

    let mut substructures = record
        .box_data
        .substructures()
        .expect("a record just encoded by to_save_pokemon has a valid checksum");
    let misc: &mut [u8; SUBSTRUCTURE_LEN] = &mut substructures.misc;
    misc[MISC_MET_LOCATION] = met_location;
    let gender_bit = match identity.player_gender {
        PlayerGender::Male => 0,
        PlayerGender::Female => 1,
        // A one-bit field keeps only the low bit of the raw byte.
        PlayerGender::Other(raw) => u16::from(raw & 1),
    };
    let origins = u16::from_le_bytes([misc[MISC_MET_DATA.start], misc[MISC_MET_DATA.start + 1]]);
    misc[MISC_MET_DATA].copy_from_slice(&(origins | (gender_bit << OT_GENDER_SHIFT)).to_le_bytes());
    record.box_data.set_substructures(&substructures);
}

#[cfg(test)]
mod tests;
