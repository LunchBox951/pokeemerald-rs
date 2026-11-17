//! The saved-record half of Pickup (`Cmd_pickup`,
//! `pokeemerald/src/battle_script_commands.c:9654-9726`): what the roll reads
//! of a party slot, and how a found item reaches a record the battle did not
//! carry.
//!
//! Upstream reads every slot straight from the saved party, by species and the
//! ability-slot bit, whether or not the member fought, so a reserve that never
//! left the party rolls as well. [`slot_facts`] reads a record the same way
//! rather than decoding a battler, which would also refuse a record whose
//! moveset the port cannot play.

use battle::{Dex, PickupSlotFacts};
use engine::save::{Pokemon, PokemonError};

use super::{
    read_u16, read_u32, ABILITY_SLOT_SHIFT, GROWTH_HELD_ITEM, GROWTH_SPECIES, IS_EGG_BIT,
    MISC_IV_WORD, SPECIES_NONE,
};

/// What the Pickup roll reads of `record`: `None` for a slot that has no
/// Pokémon, which draws nothing.
///
/// - The ability is the species' entry at the stored ability-slot bit
///   (`gSpeciesInfo[species].abilities[MON_DATA_ABILITY_NUM]`,
///   `pokeemerald/src/battle_script_commands.c:9694-9697`), so a second slot
///   of a single-ability species is [`assets::AbilityId::NONE`] and does not
///   fall back to the first.
/// - A secure-region egg flag, a Bad Egg header bit, or a secure region that
///   fails its checksum all read as `SPECIES_EGG`
///   (`MON_DATA_SPECIES_OR_EGG`, `pokeemerald/src/pokemon.c:3742-3747`,
///   `:3992-3996`); a failed checksum on an occupied slot is an egg too.
/// - An empty slot (species `0`) and a species the port has no data for read
///   as no Pokémon.
/// - Health plays no part: a fainted member rolls.
pub(crate) fn slot_facts(dex: &Dex, record: &Pokemon) -> Option<PickupSlotFacts> {
    let Ok(substructures) = record.box_data.substructures() else {
        // An occupied slot with a bad checksum is an egg; an empty one is
        // nothing. Neither can find an item, so neither is a candidate.
        return None;
    };
    let species = assets::SpeciesId(read_u16(&substructures.growth, GROWTH_SPECIES));
    if species.0 == SPECIES_NONE {
        return None;
    }
    let iv_word = read_u32(&substructures.misc, MISC_IV_WORD);
    let is_egg = record.box_data.is_bad_egg() || iv_word & IS_EGG_BIT != 0;
    let ability_slot = usize::from(iv_word >> ABILITY_SLOT_SHIFT != 0);
    let ability = dex.species(species).ok()?.abilities[ability_slot];
    Some(PickupSlotFacts {
        ability,
        is_egg,
        held_item: assets::items::ItemId(read_u16(&substructures.growth, GROWTH_HELD_ITEM)),
        level: record.level,
    })
}

/// Writes `item` as the held item of `record`
/// (`SetMonData(MON_DATA_HELD_ITEM)`, `pokeemerald/src/pokemon.c:4229-4231`),
/// leaving every other field as it was; the secure region is re-encrypted and
/// its checksum recomputed.
///
/// # Errors
///
/// Returns the checksum error of a record whose secure region is already
/// invalid; the record is left as it was.
pub(crate) fn set_record_held_item(
    record: &mut Pokemon,
    item: assets::items::ItemId,
) -> Result<(), PokemonError> {
    let mut substructures = record.box_data.substructures()?;
    substructures.growth[GROWTH_HELD_ITEM].copy_from_slice(&item.0.to_le_bytes());
    record.box_data.set_substructures(&substructures);
    Ok(())
}
