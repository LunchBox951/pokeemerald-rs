//! Conversion between battle-ready party members and Emerald save records.
//!
//! [`battle::BattlePokemon`] models the state used in battle, while
//! [`engine::save::Pokemon`] preserves Emerald's complete 100-byte party
//! record. Both encoders write the live species, experience, PP bonuses,
//! moves and PP, IVs, ability slot, level, and current HP. A new record uses
//! save-format defaults for everything else; [`merge_into_save_pokemon`]
//! overlays those fields onto the record that was loaded, retaining its
//! header, held item, friendship, EVs, contest condition, egg flag, encounter
//! data, ribbons, status, and mail.
//!
//! The saved stat block is a cache derived from species, level, IVs, EVs, and
//! nature. Emerald's save/load path copies it without recalculating it
//! (`pokeemerald/src/load_save.c:160-178`), so a merge retains the block while
//! species and level are unchanged. If either changes, the block is recomputed
//! from the battler and the retained EV bytes.
//!
//! The battle model currently computes stats with zero EVs. Loading an
//! EV-trained record can therefore clamp current HP to the model's lower
//! maximum. [`hp_hidden_by_load`] records the hidden points; a later merge
//! translates the live HP back into the retained or recomputed range and
//! rebases the offset when the EV-derived maximum changes.

use std::ops::Range;

use battle::{BattlePokemon, Dex, Ivs, MAX_MON_MOVES};
use engine::save::{BoxPokemon, Pokemon, PokemonSubstructures, SUBSTRUCTURE_LEN};

const MAIL_NONE: u8 = u8::MAX;
const SPECIES_NONE: u16 = 0;

const GROWTH_SPECIES: Range<usize> = 0..2;
const GROWTH_EXPERIENCE: Range<usize> = 4..8;
const GROWTH_PP_BONUSES: usize = 8;
const GROWTH_FRIENDSHIP: usize = 9;

const STAT_VALUE_COUNT: usize = 6;

const MISC_IV_WORD: Range<usize> = 4..8;
const IV_FIELD_WIDTH: usize = 5;
const IV_FIELD_MASK: u32 = 0x1F;
const IS_EGG_BIT: u32 = 1 << 30;
const ABILITY_SLOT_SHIFT: usize = 31;

const MOVE_ID_WIDTH: usize = size_of::<u16>();
const ATTACK_PP_OFFSET: usize = MAX_MON_MOVES * MOVE_ID_WIDTH;

/// Why a saved party member could not be converted into a battler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PartyError {
    /// The encrypted substructures failed checksum validation.
    Substructures(engine::save::PokemonError),
    /// The decoded species, level, or moveset was not battle-ready.
    Battler(battle::BattleError),
}

impl std::fmt::Display for PartyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Substructures(err) => write!(f, "saved party member: {err}"),
            Self::Battler(err) => write!(f, "saved party member: {err}"),
        }
    }
}

impl std::error::Error for PartyError {}

impl From<engine::save::PokemonError> for PartyError {
    fn from(err: engine::save::PokemonError) -> Self {
        Self::Substructures(err)
    }
}

impl From<battle::BattleError> for PartyError {
    fn from(err: battle::BattleError) -> Self {
        Self::Battler(err)
    }
}

/// Packs the six five-bit IVs, leaving the egg and ability flags clear.
fn pack_ivs(ivs: Ivs) -> u32 {
    ivs.as_array()
        .iter()
        .enumerate()
        .fold(0u32, |word, (index, value)| {
            word | (u32::from(*value) & IV_FIELD_MASK) << (index * IV_FIELD_WIDTH)
        })
}

fn unpack_ivs(word: u32) -> Ivs {
    let [hp, attack, defense, speed, sp_attack, sp_defense]: [u8; STAT_VALUE_COUNT] =
        std::array::from_fn(|index| {
            u8::try_from((word >> (index * IV_FIELD_WIDTH)) & IV_FIELD_MASK).unwrap_or(0)
        });
    Ivs {
        hp,
        attack,
        defense,
        speed,
        sp_attack,
        sp_defense,
    }
}

fn evs_from_substruct2(evs_and_condition: &[u8; SUBSTRUCTURE_LEN]) -> battle::Evs {
    let [hp, attack, defense, speed, sp_attack, sp_defense, ..] = *evs_and_condition;
    battle::Evs {
        hp,
        attack,
        defense,
        speed,
        sp_attack,
        sp_defense,
    }
}

/// Recomputes a changed stat cache with the save record's retained EVs.
///
/// A dex mismatch falls back to the battler's existing cache instead of
/// making save conversion panic.
fn compute_levelled_up_stats(
    dex: &Dex,
    mon: &BattlePokemon,
    evs_and_condition: &[u8; SUBSTRUCTURE_LEN],
) -> battle::Stats {
    match dex.species(mon.species()) {
        Ok(base) => battle::compute_stats_with_evs(
            mon.species(),
            base,
            mon.level(),
            mon.nature(),
            mon.ivs(),
            evs_from_substruct2(evs_and_condition),
        ),
        Err(_) => mon.stats(),
    }
}

fn zero_ev_max_hp(dex: &Dex, species: u16, level: u8, mon: &BattlePokemon) -> u32 {
    match dex.species(assets::SpeciesId(species)) {
        Ok(base) => {
            battle::compute_stats_with_evs(
                assets::SpeciesId(species),
                base,
                level,
                mon.nature(),
                mon.ivs(),
                battle::Evs::default(),
            )
            .max_hp
        }
        Err(_) => mon.stats().max_hp,
    }
}

/// Creates a complete save record from a battler and save-format defaults.
pub(crate) fn to_save_pokemon(dex: &Dex, mon: &BattlePokemon) -> Pokemon {
    let mut growth = [0u8; SUBSTRUCTURE_LEN];
    growth[GROWTH_SPECIES].copy_from_slice(&mon.species().0.to_le_bytes());
    growth[GROWTH_EXPERIENCE].copy_from_slice(&mon.experience().to_le_bytes());
    growth[GROWTH_PP_BONUSES] = mon.pp_bonuses().bits();
    growth[GROWTH_FRIENDSHIP] = dex
        .species(mon.species())
        .map_or(0, |species| species.base_friendship);

    let mut misc = [0u8; SUBSTRUCTURE_LEN];
    let iv_word = pack_ivs(mon.ivs()) | (u32::from(mon.ability_slot()) << ABILITY_SLOT_SHIFT);
    misc[MISC_IV_WORD].copy_from_slice(&iv_word.to_le_bytes());

    let mut box_data = BoxPokemon::new(mon.personality(), mon.original_trainer_id());
    box_data.set_substructures(&PokemonSubstructures {
        growth,
        attacks: encode_attacks(mon),
        evs_and_condition: [0u8; SUBSTRUCTURE_LEN],
        misc,
    });

    let mut record = Pokemon {
        box_data,
        mail: MAIL_NONE,
        ..Pokemon::default()
    };
    overlay_battle_stats(&mut record, mon, mon.stats());
    record
}

fn encode_attacks(mon: &BattlePokemon) -> [u8; SUBSTRUCTURE_LEN] {
    let mut attacks = [0u8; SUBSTRUCTURE_LEN];
    let (move_ids, remaining_pp) = attacks.split_at_mut(ATTACK_PP_OFFSET);
    for ((move_id, pp), slot) in move_ids
        .chunks_exact_mut(MOVE_ID_WIDTH)
        .zip(remaining_pp)
        .zip(mon.moves())
    {
        move_id.copy_from_slice(&slot.move_id.0.to_le_bytes());
        *pp = slot.pp;
    }
    attacks
}

fn overlay_battle_stats(record: &mut Pokemon, mon: &BattlePokemon, stats: battle::Stats) {
    record.level = mon.level();
    record.hp = clamp_u16(mon.current_hp());
    record.max_hp = clamp_u16(stats.max_hp);
    record.attack = clamp_u16(stats.attack);
    record.defense = clamp_u16(stats.defense);
    record.speed = clamp_u16(stats.speed);
    record.special_attack = clamp_u16(stats.sp_attack);
    record.special_defense = clamp_u16(stats.sp_defense);
}

/// Restores HP hidden by the zero-EV load clamp without reviving a fainted mon.
fn overlay_current_hp_with_hidden_points(
    record: &mut Pokemon,
    mon: &BattlePokemon,
    hp_hidden_by_load: i32,
) {
    let live_hp = clamp_u16(mon.current_hp());
    record.hp = if live_hp == 0 {
        0
    } else {
        let translated_hp = i64::from(live_hp) + i64::from(hp_hidden_by_load);
        u16::try_from(translated_hp.max(1).min(i64::from(record.max_hp))).unwrap_or(record.max_hp)
    };
}

/// Enforces Shedinja's invariant maximum without refreshing its retained stats.
///
/// `CalculateMonStats` pins Shedinja's maximum HP to one
/// (`pokeemerald/src/pokemon.c:2845-2848`), but save/load does not recalculate
/// the other five cached stats.
fn normalize_retained_shedinja_max_hp(
    record: &mut Pokemon,
    mon: &BattlePokemon,
    hp_hidden_by_load: &mut i32,
) {
    if mon.species() != battle::SPECIES_SHEDINJA {
        return;
    }
    let invariant_max_hp = mon.stats().max_hp;
    let removed_points = clamp_i32(u32::from(record.max_hp).saturating_sub(invariant_max_hp));
    record.max_hp = clamp_u16(invariant_max_hp);
    *hp_hidden_by_load = hp_hidden_by_load.saturating_sub(removed_points);
}

/// Measures current-HP points hidden when a saved mon entered the zero-EV model.
///
/// The floor uses the record's stored level because experience reconciliation
/// may already have changed the battler's level.
pub(crate) fn hp_hidden_by_load(dex: &Dex, stored: &Pokemon, lead: &BattlePokemon) -> i32 {
    let stored_level_floor = zero_ev_max_hp(dex, lead.species().0, stored.level, lead);
    i32::from(stored.hp.saturating_sub(clamp_u16(stored_level_floor)))
}

/// Overlays a battler onto the save record from which it was loaded.
///
/// A record with a different identity, an empty species, or an invalid secure
/// checksum cannot safely retain bytes and is rebuilt from defaults.
pub(crate) fn merge_into_save_pokemon(
    dex: &Dex,
    mon: &BattlePokemon,
    base: &Pokemon,
    hp_hidden_by_load: &mut i32,
) -> Pokemon {
    let mut substructures = match backing_substructures(mon, base) {
        Ok(substructures) => substructures,
        Err(reason) => {
            eprintln!(
                "save: party slot 0 {reason} -- writing a record built from the battler \
                 alone, so every field the battle model does not carry takes CreateMon's \
                 own default"
            );
            *hp_hidden_by_load = 0;
            return to_save_pokemon(dex, mon);
        }
    };

    let stored_species = read_u16(&substructures.growth, GROWTH_SPECIES);
    let retained_stat_block_is_current =
        stored_species == mon.species().0 && base.level == mon.level();

    substructures.growth[GROWTH_SPECIES].copy_from_slice(&mon.species().0.to_le_bytes());
    substructures.growth[GROWTH_EXPERIENCE].copy_from_slice(&mon.experience().to_le_bytes());
    substructures.growth[GROWTH_PP_BONUSES] = mon.pp_bonuses().bits();
    substructures.attacks = encode_attacks(mon);

    let retained_egg_flag = read_u32(&substructures.misc, MISC_IV_WORD) & IS_EGG_BIT;
    let merged_iv_word = pack_ivs(mon.ivs())
        | retained_egg_flag
        | (u32::from(mon.ability_slot()) << ABILITY_SLOT_SHIFT);
    substructures.misc[MISC_IV_WORD].copy_from_slice(&merged_iv_word.to_le_bytes());

    let mut merged = *base;
    merged.box_data.set_substructures(&substructures);

    if retained_stat_block_is_current {
        normalize_retained_shedinja_max_hp(&mut merged, mon, hp_hidden_by_load);
    } else {
        let old_zero_ev_max_hp = zero_ev_max_hp(dex, stored_species, base.level, mon);
        let old_ev_bonus = clamp_i32(u32::from(base.max_hp).saturating_sub(old_zero_ev_max_hp));

        let recomputed_stats =
            compute_levelled_up_stats(dex, mon, &substructures.evs_and_condition);
        overlay_battle_stats(&mut merged, mon, recomputed_stats);
        let new_ev_bonus = clamp_i32(u32::from(merged.max_hp).saturating_sub(mon.stats().max_hp));

        *hp_hidden_by_load = hp_hidden_by_load
            .saturating_add(new_ev_bonus)
            .saturating_sub(old_ev_bonus);
    }

    overlay_current_hp_with_hidden_points(&mut merged, mon, *hp_hidden_by_load);
    merged
}

fn backing_substructures(
    mon: &BattlePokemon,
    base: &Pokemon,
) -> Result<PokemonSubstructures, NotTheBattlersRecord> {
    if base.box_data.personality() != mon.personality()
        || base.box_data.ot_id() != mon.original_trainer_id()
    {
        return Err(NotTheBattlersRecord::DifferentPokemon);
    }
    let substructures = base
        .box_data
        .substructures()
        .map_err(|_| NotTheBattlersRecord::ChecksumFailed)?;
    if read_u16(&substructures.growth, GROWTH_SPECIES) == SPECIES_NONE {
        return Err(NotTheBattlersRecord::Empty);
    }
    Ok(substructures)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotTheBattlersRecord {
    DifferentPokemon,
    ChecksumFailed,
    Empty,
}

impl core::fmt::Display for NotTheBattlersRecord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::DifferentPokemon => "holds a different Pokémon (personality or OT id)",
            Self::ChecksumFailed => "failed its own checksum",
            Self::Empty => "is empty",
        })
    }
}

/// Converts a validated save record into a battle-ready party member.
///
/// # Errors
///
/// Returns [`PartyError`] when the secure checksum or battler fields are invalid.
pub(crate) fn from_save_pokemon(dex: &Dex, saved: &Pokemon) -> Result<BattlePokemon, PartyError> {
    let substructures = saved.box_data.substructures()?;
    let species = assets::SpeciesId(read_u16(&substructures.growth, GROWTH_SPECIES));
    let iv_word = read_u32(&substructures.misc, MISC_IV_WORD);
    let ability_slot = u8::from(iv_word >> ABILITY_SLOT_SHIFT != 0);

    let (move_ids, remaining_pp) = substructures.attacks.split_at(ATTACK_PP_OFFSET);
    let move_ids: Vec<assets::MoveId> = move_ids
        .chunks_exact(MOVE_ID_WIDTH)
        .map(|bytes| assets::MoveId(u16::from_le_bytes([bytes[0], bytes[1]])))
        .take_while(|move_id| *move_id != battle::MOVE_NONE)
        .collect();
    let known_moves = move_ids.len();

    let pp_bonuses = battle::PpBonuses::from_bits(substructures.growth[GROWTH_PP_BONUSES]);
    let mut mon = BattlePokemon::new(
        dex,
        species,
        saved.level,
        unpack_ivs(iv_word),
        saved.box_data.personality(),
        move_ids,
    )?
    .with_original_trainer_id(saved.box_data.ot_id())
    .with_ability_slot(ability_slot)
    .with_pp_bonuses(dex, pp_bonuses)?;

    let saved_experience = read_u32(&substructures.growth, GROWTH_EXPERIENCE);
    // Match `CalculateMonStats`: derive level from experience without invoking
    // the separate move-learning path (`pokeemerald/src/pokemon.c:2823-2843`).
    mon.reconcile_saved_experience(saved_experience);

    let saved_damage = mon.stats().max_hp.saturating_sub(u32::from(saved.hp));
    mon.apply_damage(saved_damage);

    for (index, saved_pp) in remaining_pp.iter().copied().take(known_moves).enumerate() {
        let pp_spent = mon.moves()[index].pp.saturating_sub(saved_pp);
        for _ in 0..pp_spent {
            mon.deduct_pp(index)?;
        }
    }

    Ok(mon)
}

fn read_u16(bytes: &[u8], range: Range<usize>) -> u16 {
    let field = bytes[range]
        .try_into()
        .expect("a u16 save field has exactly two bytes");
    u16::from_le_bytes(field)
}

fn read_u32(bytes: &[u8], range: Range<usize>) -> u32 {
    let field = bytes[range]
        .try_into()
        .expect("a u32 save field has exactly four bytes");
    u32::from_le_bytes(field)
}

fn clamp_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn clamp_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests;
