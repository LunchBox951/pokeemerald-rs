//! `AdjustFriendship`'s walking event (`FRIENDSHIP_EVENT_WALKING`,
//! `pokeemerald/src/pokemon.c:5914-5985`) on a saved party record, and the
//! valid-species test (`IsMonValidSpecies`, `field_poison.c:20-27`) both the
//! walking gain and the field-poison drain share.
//!
//! The walking gain works on the saved [`Pokemon`] rather than a loaded
//! battler: friendship is a growth byte the battler does not carry, and a
//! valid saved species qualifies whatever its HP or moveset. Writes go
//! through [`BoxPokemon::set_substructures`] so the secure checksum follows.
//!
//! Not modelled: the other friendship events, the battle-facility skip
//! (`ShouldSkipFriendshipChange`), and the `+1` for a met location equal to
//! the current region map section -- the saved byte is numeric and the
//! current map's section is only a symbolic name in `assets`.

use engine::rng::RandomSource;
use engine::save::Pokemon;

use super::{
    read_u16, read_u32, GROWTH_FRIENDSHIP, GROWTH_SPECIES, IS_EGG_BIT, MISC_IV_WORD, MISC_MET_DATA,
    POKE_BALL_SHIFT,
};

/// `SPECIES_EGG` (`pokeemerald/include/constants/species.h`), the id
/// `MON_DATA_SPECIES_OR_EGG` reports for an egg and which a record may also
/// carry as its growth species.
const SPECIES_EGG: u16 = 412;
/// `sFriendshipEventModifiers[FRIENDSHIP_EVENT_WALKING]`: `{ 1, 1, 1 }` in
/// every friendship tier (`pokemon.c:2100`).
const WALKING_GAIN: u8 = 1;
/// `ITEM_LUXURY_BALL` (`pokeemerald/include/constants/items.h:18`).
const BALL_LUXURY: u16 = assets::items::ItemId::LUXURY_BALL.0;
/// The poké ball field is 4 bits wide (`pokemon.h:131-149`).
const POKE_BALL_MASK: u16 = 0xF;

/// `IsMonValidSpecies` (`pokeemerald/src/field_poison.c:20-27`): a species
/// that is neither `SPECIES_NONE` nor `SPECIES_EGG`, read as
/// `MON_DATA_SPECIES_OR_EGG` does. A record whose secure checksum fails is a
/// Bad Egg, so it is not valid either.
pub(crate) fn has_valid_species(record: &Pokemon) -> bool {
    if record.box_data.is_bad_egg() {
        return false;
    }
    let Ok(substructures) = record.box_data.substructures() else {
        return false;
    };
    let species = read_u16(&substructures.growth, GROWTH_SPECIES);
    let iv_word = read_u32(&substructures.misc, MISC_IV_WORD);
    species != 0 && species != SPECIES_EGG && iv_word & IS_EGG_BIT == 0
}

/// `AdjustFriendship(mon, FRIENDSHIP_EVENT_WALKING)`
/// (`pokeemerald/src/pokemon.c:5914-5985`) for one saved party slot.
///
/// An empty or egg slot returns without a draw (`:5938`). Otherwise one draw
/// is taken and an odd one returns (`:5948-5953`); an even one adds
/// [`WALKING_GAIN`], plus one for a Luxury Ball (`:5971-5974`), clamped to
/// 255. The draw is taken even for a mon already at 255.
pub(crate) fn adjust_friendship_walking(record: &mut Pokemon, rng: &mut impl RandomSource) {
    if !has_valid_species(record) {
        return;
    }
    if rng.next_u16() & 1 != 0 {
        return;
    }
    let mut substructures = record
        .box_data
        .substructures()
        .expect("has_valid_species checked the checksum");
    let origins = read_u16(&substructures.misc, MISC_MET_DATA);
    let luxury = (origins >> POKE_BALL_SHIFT) & POKE_BALL_MASK == BALL_LUXURY;
    let gain = WALKING_GAIN + u8::from(luxury);
    let friendship = &mut substructures.growth[GROWTH_FRIENDSHIP];
    *friendship = friendship.saturating_add(gain);
    record.box_data.set_substructures(&substructures);
}

#[cfg(test)]
mod tests;
