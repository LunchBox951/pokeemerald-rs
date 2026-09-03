//! Conversion between battle-ready party members and Emerald save records.
//!
//! [`battle::BattlePokemon`] models the state used in battle, while
//! [`engine::save::Pokemon`] preserves Emerald's complete 100-byte party
//! record. Both encoders write the live species, experience, PP bonuses,
//! moves and PP, IVs, ability slot, level, and current HP. A new record uses
//! save-format defaults for everything else; [`merge_into_save_pokemon`]
//! overlays those fields onto the record that was loaded, retaining its
//! header, held item, friendship, contest condition, egg flag, encounter
//! data, ribbons, status, and mail. EVs are the battler's: the merge writes
//! them back from [`battle::BattlePokemon::evs`].
//!
//! Deliberately not modelled by `battle`, and so retained from the backing
//! record by [`merge_into_save_pokemon`]: held item, friendship, pokérus,
//! met data, poké ball, OT gender, ribbons, markings, nickname, OT name,
//! language, mail, non-volatile status, the egg bit, and contest condition
//! (`PokemonSubstruct2`'s trailing six bytes). [`to_save_pokemon`], which has
//! no record to retain them from, writes upstream's own `CreateMon` default
//! for each instead; friendship is derived from the species' base
//! friendship, matching `CreateBoxMon`.
//!
//! Effort values are the one battle-authoritative field with its own
//! adoption/gain contract -- see [`battle::pokemon::evs`] for that. Both
//! encoders write the live value into `PokemonSubstruct2`'s first six bytes
//! ([`evs_from_substruct2`], [`evs_to_substruct2`]) unconditionally, whether
//! or not the session's KOs also crossed a level.
//!
//! The saved stat block is a cache derived from species, level, IVs, EVs, and
//! nature. Emerald's save/load path copies it without recalculating it
//! (`pokeemerald/src/load_save.c:160-178`), so a merge retains the block while
//! species and level are unchanged; if either changes, the block is
//! recomputed EV-aware -- see [`compute_levelled_up_stats`] for which EVs
//! that recompute uses, and why. [`to_save_pokemon`] runs the same
//! species/level-moved test against
//! [`battle::BattlePokemon::created_at_level`] instead of a stored record's
//! byte, since it has no record of its own to compare against.
//!
//! Loading an EV-trained record can clamp current HP to the live model's
//! lower maximum ([`battle::pokemon::evs`] owns why that maximum never
//! moves). [`hp_hidden_by_load`] records the hidden points; a later merge
//! translates the live HP back into the retained or recomputed range and
//! rebases the offset (signed -- it can shrink across a level-up) when the
//! EV-derived maximum changes ([`overlay_current_hp_with_hidden_points`]).
//! Shedinja is the one exception the recompute cannot fix on the retained
//! branch: [`normalize_retained_shedinja_max_hp`] pins its maximum back to
//! `1` and rebases the offset by the points it removes.

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
    /// The record is an egg -- never a battler, upstream's own
    /// `SetBattlePartyIds` egg exclusion (`pokeemerald/src/battle_controllers.c:601-602`).
    Egg,
}

impl std::fmt::Display for PartyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Substructures(err) => write!(f, "saved party member: {err}"),
            Self::Battler(err) => write!(f, "saved party member: {err}"),
            Self::Egg => write!(f, "saved party member: is an egg"),
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

/// `PokemonSubstruct2`'s first six bytes, one whole byte per EV, in the same
/// HP/Attack/Defense/Speed/SpAttack/SpDefense order [`pack_ivs`]/
/// [`unpack_ivs`] use, unlike the IVs' packed five-bit fields. Used by
/// [`from_save_pokemon`] to seed [`battle::BattlePokemon::with_evs`] --
/// [`compute_levelled_up_stats`]'s own stat-block recompute reads
/// [`battle::BattlePokemon::evs_at_last_level_up`] instead, not these bytes
/// directly. [`evs_to_substruct2`] is the inverse.
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

/// [`evs_from_substruct2`]'s inverse: packs a [`battle::Evs`]
/// back into `PokemonSubstruct2`'s first six bytes, in the same order. Used
/// by both encoders to file the battler's own (possibly KO-incremented) EVs
/// -- [`to_save_pokemon`] for a mon with no backing record,
/// [`merge_into_save_pokemon`] for one there is.
fn evs_to_substruct2(evs: battle::Evs) -> [u8; 6] {
    evs.as_array()
}

/// Recomputes a changed stat cache EV-aware, fed `mon`'s own IVs and nature
/// alongside `evs`, for a lead whose species or level moved since the
/// reference point each caller compares against.
///
/// `evs` must be [`battle::BattlePokemon::evs_at_last_level_up`], not the
/// live [`battle::BattlePokemon::evs`]: upstream's `CalculateMonStats` only
/// runs inside the level-up sequence that moved `mon` here in the first
/// place, so a KO's `MonGainEVs` bytes gained *after* that level-up must not
/// retroactively inflate the block it cached (regression:
/// `save_time_recompute_uses_the_evs_the_level_up_saw_not_later_gains`).
///
/// A dex mismatch falls back to the battler's existing `0`-EV cache instead
/// of making save conversion panic.
fn compute_levelled_up_stats(dex: &Dex, mon: &BattlePokemon, evs: battle::Evs) -> battle::Stats {
    match dex.species(mon.species()) {
        Ok(base) => battle::compute_stats_with_evs(
            mon.species(),
            base,
            mon.level(),
            mon.nature(),
            mon.ivs(),
            evs,
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

    // `PokemonSubstruct2` -- the mon's own EVs (`0` for a mon
    // nothing has ever called `with_evs`/`gain_evs` on, matching `CreateMon`'s
    // own default) in the first six bytes, contest condition zero in the
    // trailing six (module docs).
    let mut evs_and_condition = [0u8; SUBSTRUCTURE_LEN];
    evs_and_condition[0..6].copy_from_slice(&evs_to_substruct2(mon.evs()));

    let mut box_data = BoxPokemon::new(mon.personality(), mon.original_trainer_id());
    box_data.set_substructures(&PokemonSubstructures {
        growth,
        attacks: encode_attacks(mon),
        evs_and_condition,
        misc,
    });

    let mut record = Pokemon {
        box_data,
        mail: MAIL_NONE,
        ..Pokemon::default()
    };
    // Recomputed only when `mon` has levelled up since `BattlePokemon::new`
    // built it; see `compute_levelled_up_stats`'s own doc for which EVs
    // that recompute uses, and why.
    let stats = if mon.level() == mon.created_at_level() {
        mon.stats()
    } else {
        compute_levelled_up_stats(dex, mon, mon.evs_at_last_level_up())
    };
    overlay_battle_stats(&mut record, mon, stats);
    // Translate `record.hp` across whatever gap the branch above opened -- a
    // no-op when it did not (`stats == mon.stats()` there), so safe to run
    // unconditionally.
    let gap = clamp_i32(stats.max_hp.saturating_sub(mon.stats().max_hp));
    overlay_current_hp_with_hidden_points(&mut record, mon, gap);
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

/// The party record's unencrypted tail -- level, current HP, and the derived
/// stat block -- from `stats`, either [`BattlePokemon::stats`]'s live `0`-EV
/// cache or [`compute_levelled_up_stats`]'s EV-aware recompute. The plain
/// `current_hp` this writes into `record.hp` is never the last word in
/// either caller: both re-overlay [`overlay_current_hp_with_hidden_points`]
/// on top, so the load-clamp translation lands against whichever maximum was
/// just filed here.
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

/// Restores HP hidden by the zero-EV load clamp without reviving a fainted
/// mon: the points of a stored `hp` above the model's maximum were hidden
/// from the session, so they are added back onto the live number, capped at
/// `record.max_hp`, floored at `1` for a live battler. Used for both a
/// *retained* stat block and a freshly *recomputed* one -- the
/// caller always writes `record.max_hp` first, so the cap is whichever block
/// is actually being filed.
///
/// The offset is signed (negative after a level-up whose EV gap shrank),
/// measured once per session -- at load ([`hp_hidden_by_load`]) or fresh by
/// [`to_save_pokemon`] -- and carried, never re-derived here: saving twice
/// must file the same bytes.
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
/// Writes only the fields the battler owns; every retained field stays as
/// the save wrote it, except EVs, which the merge writes back from
/// [`battle::BattlePokemon::evs`].
///
/// The cached stat block is kept when species and level are unchanged and
/// recomputed EV-aware otherwise ([`compute_levelled_up_stats`]); current
/// HP is translated across the load clamp against whichever block is
/// filed, with the session offset rebased on the recompute branch
/// ([`zero_ev_max_hp`]). A record that cannot safely retain bytes falls
/// back to [`to_save_pokemon`] ([`backing_substructures`]), seeding the
/// offset from the fresh gap it opens.
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
            let record = to_save_pokemon(dex, mon);
            // Carried forward, not zeroed (this function's own doc comment
            // covers why). Read back off `record` rather than recomputed,
            // matching exactly what `to_save_pokemon` just filed.
            *hp_hidden_by_load =
                clamp_i32(u32::from(record.max_hp).saturating_sub(mon.stats().max_hp));
            return record;
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

    // `PokemonSubstruct2`'s first six bytes -- the battler's own live EVs,
    // written unconditionally regardless of the stat-block branch below (see
    // `compute_levelled_up_stats`'s own doc for why the two differ). Contest
    // condition, the substructure's other half, has no home in
    // `battle::BattlePokemon` and stays exactly as `base` held it.
    substructures.evs_and_condition[0..6].copy_from_slice(&evs_to_substruct2(mon.evs()));

    let mut merged = *base;
    merged.box_data.set_substructures(&substructures);

    if retained_stat_block_is_current {
        normalize_retained_shedinja_max_hp(&mut merged, mon, hp_hidden_by_load);
    } else {
        let old_zero_ev_max_hp = zero_ev_max_hp(dex, stored_species, base.level, mon);
        let old_ev_bonus = clamp_i32(u32::from(base.max_hp).saturating_sub(old_zero_ev_max_hp));

        let recomputed_stats = compute_levelled_up_stats(dex, mon, mon.evs_at_last_level_up());
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

/// Converts a validated save record into a battle-ready party member --
/// `LoadPlayerParty`'s per-mon half (`src/load_save.c:170-178`).
///
/// Stats are recomputed from species/level/nature/IVs, at `0` EVs, by
/// [`battle::BattlePokemon::new`]; the record's own EVs
/// ([`evs_from_substruct2`]) are adopted right after, through
/// [`battle::BattlePokemon::with_evs`], which does not disturb that `0`-EV
/// stat block (see [`battle::pokemon::evs`] for why). Accumulated experience
/// is wound back to the saved value through
/// [`battle::BattlePokemon::reconcile_saved_experience`];
/// current HP and each move slot's PP are then wound back to the saved
/// values through the remaining mutations that preserve that type's
/// invariants ([`battle::BattlePokemon::apply_damage`] /
/// [`battle::BattlePokemon::deduct_pp`]) -- never by reaching past them.
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

    // Adopted before any PP is wound back: it is what each slot's *capacity*
    // is, so the spend below counts down from the PP-Up-adjusted maximum
    // rather than from base PP. Every byte is legal (two-bit fields cannot
    // overflow), so there is nothing to screen -- `battle::PpBonuses`' own
    // docs.
    let pp_bonuses = battle::PpBonuses::from_bits(substructures.growth[GROWTH_PP_BONUSES]);

    // `PokemonSubstruct2`'s first six bytes -- this record's own EVs,
    // adopted through `with_evs` below so the battler carries them
    // for the merge's own save-time recompute to read back later; adopting
    // them does not disturb the `0`-EV stat block `BattlePokemon::new` just
    // built, so the order relative to `reconcile_saved_experience` below
    // does not matter.
    let evs = evs_from_substruct2(&substructures.evs_and_condition);

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
    .with_pp_bonuses(dex, pp_bonuses)?
    .with_evs(evs);

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

/// Whether `record`'s secure-region IV word carries Emerald's egg flag,
/// checked ahead of decode since [`from_save_pokemon`] itself does not
/// reject an egg.
fn record_is_egg(record: &Pokemon) -> bool {
    record
        .box_data
        .substructures()
        .is_ok_and(|substructures| read_u32(&substructures.misc, MISC_IV_WORD) & IS_EGG_BIT != 0)
}

/// Selects which saved slot is the active battler on continue: the first
/// non-egg, non-fainted slot in `party` -- `SetBattlePartyIds`'s
/// player-side scan (`pokeemerald/src/battle_controllers.c:585-606`,
/// called by `InitBattleControllers` at `:97`). Falls back to slot 0's own
/// decode when nothing qualifies, fainted or not -- but never to an egg
/// (`PartyError::Egg`): an egg is not a battler upstream either, so an
/// egg-only party fails closed the same way an all-fainted one does.
///
/// # Errors
///
/// Returns slot 0's [`PartyError`] when nothing qualifies and slot 0 is
/// itself unusable, whether that is a decode failure or an egg.
///
/// # Panics
///
/// Panics if `party` is empty; callers only pass a nonzero-count slice.
pub(crate) fn select_active_battler(
    dex: &Dex,
    party: &[Pokemon],
) -> Result<(usize, BattlePokemon), PartyError> {
    for (slot, record) in party.iter().enumerate() {
        if record_is_egg(record) {
            continue;
        }
        match from_save_pokemon(dex, record) {
            Ok(mon) if !mon.is_fainted() => return Ok((slot, mon)),
            Ok(_) => {}
            // Logged, not just discarded: a later slot winning the scan
            // must not hide that an earlier one's bytes would not decode.
            Err(err) => eprintln!("continue: slot {slot}'s record {err} -- skipped"),
        }
    }
    if record_is_egg(&party[0]) {
        return Err(PartyError::Egg);
    }
    from_save_pokemon(dex, &party[0]).map(|mon| (0, mon))
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
