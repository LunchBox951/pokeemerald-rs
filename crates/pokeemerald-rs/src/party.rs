//! `gPlayerParty` <-> `SaveBlock1::playerParty` (I-6, issue #232):
//! the encoder between the battle crate's [`battle::BattlePokemon`] and the
//! save model's [`engine::save::Pokemon`].
//!
//! Upstream keeps the live party in `gPlayerParty` and copies it into the
//! save block on every save and back out on every load --
//! `SavePlayerParty`/`LoadPlayerParty`, reached through
//! `CopyPartyAndObjectsToSave`/`CopyPartyAndObjectsFromSave`
//! (`pokeemerald/src/load_save.c:160-206`). There the two are the *same*
//! `struct Pokemon`, so the copy is a `memcpy` and there is nothing to
//! encode.
//!
//! This workspace splits them deliberately: `battle` owns already-computed
//! battle stats and depends on nothing (`crate::battle::wild`'s own docs),
//! while `engine::save::pokemon` owns Emerald's exact 100-byte layout with
//! its order-shuffled, XOR-encrypted substructures. Neither crate can see
//! the other, so the translation lives here, in the one crate that depends
//! on both `(oop-boundaries)`. This module *is* the port of that `memcpy`
//! -- see [`to_save_pokemon`] / [`from_save_pokemon`].
//!
//! # What survives the round trip, and what does not
//!
//! Round-trips exactly: species, level, **accumulated experience**,
//! personality (and so nature), the six IVs, the moveset with each slot's
//! remaining PP, current HP, and the original-trainer id.
//!
//! Deliberately *not* modelled, written as upstream's own default and
//! discarded on the way back:
//!
//! * **Effort values** -- every mon `battle` builds has `0` EVs (that
//!   crate's own module docs), so `PokemonSubstruct2` is written all-zero
//!   rather than inventing values, and [`from_save_pokemon`] does not read
//!   it. A saved mon with real EVs would come back with the stats a 0-EV
//!   mon has.
//! * **Held item, `ppBonuses`, contest condition, pokérus, met
//!   location/level/game, poké ball, OT gender, ribbons, markings,
//!   nickname, OT name, language, and non-volatile status** -- none has a
//!   typed home in `battle::BattlePokemon`. Each is written as
//!   `CreateMon`'s zero/`MAIL_NONE` default so the bytes are *valid*, not
//!   as an invented value.
//! * **Egg and ability-slot bits** -- `isEgg`/`abilityNum`
//!   (`PokemonSubstruct3`'s bits 30/31) stay clear: this port models no
//!   eggs and no abilities.
//!
//! One field is *derived* rather than carried, so the saved bytes stay
//! self-consistent with what upstream would have written:
//!
//! * **Friendship** -- the species' `base_friendship`, as `CreateBoxMon`
//!   sets it (`pokeemerald/src/pokemon.c:2270`).
//!
//! Experience used to be on that list, derived from the growth curve at
//! the mon's level. Since battles apply earned experience to the battler
//! itself ([`battle::BattlePokemon::apply_experience`], issue #237), the
//! sub-level progress between two thresholds is real state, so the encode
//! writes the mon's own total (upstream's `MON_DATA_EXP`) and the decode
//! restores it -- see [`from_save_pokemon`] for how inconsistent bytes are
//! reconciled.
//!
//! The stat block (`hp`/`max_hp`/`attack`/.../`special_defense`) is written
//! from [`battle::BattlePokemon::stats`], i.e. `CalculateMonStats`' output,
//! and *recomputed* on decode by [`battle::BattlePokemon::new`] rather than
//! trusted -- upstream recomputes it too, on every `CalculateMonStats` call
//! site. Only `hp` is read back, because current HP is the one stat that is
//! state rather than a function of species/level/IVs.

use battle::{BattlePokemon, Dex, Ivs, MAX_MON_MOVES};
use engine::save::{BoxPokemon, Pokemon, PokemonSubstructures, SUBSTRUCTURE_LEN};

/// `MAIL_NONE` (`pokeemerald/include/constants/items.h:446`): the party
/// block's "no mail held" sentinel, which `CreateMon` writes into every
/// freshly built mon (`src/pokemon.c:2201-2202`).
const MAIL_NONE: u8 = 0xFF;

/// Why a saved party member could not be decoded into a battler
/// ([`from_save_pokemon`]).
///
/// Concrete per-crate-boundary enum `(oop-boundaries)`. Upstream cannot
/// fail here at all -- `LoadPlayerParty` is a struct copy over data the
/// sector checksum already validated -- so every variant is this port's
/// answer to bytes that pass the checksum but do not describe a mon any
/// battle code could run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PartyError {
    /// The boxed mon's encrypted substructures did not match their stored
    /// checksum. Upstream's own `GetBoxMonData` treats this as "bad egg"
    /// (`src/pokemon.c`'s `DecryptBoxMon` checksum path); this port refuses
    /// the decode instead of handing the battle engine a scrambled mon.
    Substructures(engine::save::PokemonError),
    /// The decoded species/level/moveset is not something
    /// [`battle::BattlePokemon::new`] accepts -- an unknown species, a
    /// level outside `1..=100`, or an empty moveset. Carries that
    /// constructor's own diagnosis.
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

/// Pack the six individual values into `PokemonSubstruct3`'s `/*0x04*/`
/// word (`pokeemerald/include/pokemon.h`): five bits each, in
/// HP/Attack/Defense/Speed/SpAttack/SpDefense declaration order from bit 0
/// upward, leaving bit 30 (`isEgg`) and bit 31 (`abilityNum`) clear.
///
/// The declaration order *is* the bit order: the ARM ABI lays consecutive
/// bitfields out from the least significant bit of the storage unit, and
/// this word is read back as a little-endian `u32`.
fn pack_ivs(ivs: Ivs) -> u32 {
    ivs.as_array()
        .iter()
        .enumerate()
        .fold(0u32, |word, (index, value)| {
            word | (u32::from(*value) & 0x1F) << (index * 5)
        })
}

/// [`pack_ivs`]' inverse: the six five-bit fields, in the same order.
fn unpack_ivs(word: u32) -> Ivs {
    let field = |index: u32| u8::try_from((word >> (index * 5)) & 0x1F).unwrap_or(0);
    Ivs {
        hp: field(0),
        attack: field(1),
        defense: field(2),
        speed: field(3),
        sp_attack: field(4),
        sp_defense: field(5),
    }
}

/// `SavePlayerParty`'s per-mon half (`src/load_save.c:160-168`): the battler
/// `mon`, as the exact 100-byte party value the save block stores.
///
/// The box header's XOR key is built from the Pokémon's retained original
/// trainer id (`personality ^ otId`), which can differ from the current
/// player's id for a traded Pokémon.
///
/// See the module docs for the complete list of fields this writes as a
/// default rather than carrying.
pub(crate) fn to_save_pokemon(dex: &Dex, mon: &BattlePokemon) -> Pokemon {
    let mut growth = [0u8; SUBSTRUCTURE_LEN];
    growth[0..2].copy_from_slice(&mon.species().0.to_le_bytes());
    // `heldItem` (`/*0x02*/`) stays `ITEM_NONE`.
    // The mon's own accumulated total (`MON_DATA_EXP`), never re-derived
    // from the level: sub-level progress earned in battle is state the
    // save must carry (module docs).
    growth[4..8].copy_from_slice(&mon.experience().to_le_bytes());
    let friendship = match dex.species(mon.species()) {
        Ok(base) => base.base_friendship,
        // Unreachable through `BattlePokemon`, whose constructor already
        // resolved this species against a dex -- but a species missing from
        // *this* dex must still produce writable bytes rather than a panic.
        Err(_) => 0,
    };
    // `ppBonuses` (`/*0x08*/`) stays 0: no PP Ups are modelled.
    growth[9] = friendship;

    let mut attacks = [0u8; SUBSTRUCTURE_LEN];
    for (index, slot) in mon.moves().iter().take(MAX_MON_MOVES).enumerate() {
        attacks[index * 2..index * 2 + 2].copy_from_slice(&slot.move_id.0.to_le_bytes());
        attacks[8 + index] = slot.pp;
    }

    let mut misc = [0u8; SUBSTRUCTURE_LEN];
    misc[4..8].copy_from_slice(&pack_ivs(mon.ivs()).to_le_bytes());

    let mut box_data = BoxPokemon::new(mon.personality(), mon.original_trainer_id());
    box_data.set_substructures(&PokemonSubstructures {
        growth,
        attacks,
        // `PokemonSubstruct2` -- EVs and contest condition, all zero.
        evs_and_condition: [0u8; SUBSTRUCTURE_LEN],
        misc,
    });

    let stats = mon.stats();
    Pokemon {
        box_data,
        // `MON_DATA_STATUS`: no non-volatile status is modelled.
        status: 0,
        level: mon.level(),
        mail: MAIL_NONE,
        hp: clamp_u16(mon.current_hp()),
        max_hp: clamp_u16(stats.max_hp),
        attack: clamp_u16(stats.attack),
        defense: clamp_u16(stats.defense),
        speed: clamp_u16(stats.speed),
        special_attack: clamp_u16(stats.sp_attack),
        special_defense: clamp_u16(stats.sp_defense),
    }
}

/// `LoadPlayerParty`'s per-mon half (`src/load_save.c:170-178`): the battler
/// a saved party value describes.
///
/// Stats are recomputed from species/level/nature/IVs by
/// [`battle::BattlePokemon::new`] (module docs), then accumulated
/// experience, current HP, and each move slot's PP are wound back to the
/// saved values through the mutations that preserve that type's invariants
/// ([`battle::BattlePokemon::apply_experience`] /
/// [`battle::BattlePokemon::apply_damage`] /
/// [`battle::BattlePokemon::deduct_pp`]) -- never by reaching past them.
///
/// # Errors
///
/// [`PartyError::Substructures`] if the encrypted region fails its
/// checksum; [`PartyError::Battler`] if the decoded species/level/moves are
/// not a battler `battle` can build (see that variant's docs).
pub(crate) fn from_save_pokemon(dex: &Dex, saved: &Pokemon) -> Result<BattlePokemon, PartyError> {
    let substructures = saved.box_data.substructures()?;
    let species = assets::SpeciesId(u16::from_le_bytes([
        substructures.growth[0],
        substructures.growth[1],
    ]));
    let ivs = unpack_ivs(u32::from_le_bytes([
        substructures.misc[4],
        substructures.misc[5],
        substructures.misc[6],
        substructures.misc[7],
    ]));
    // `MOVE_NONE` marks an unfilled slot upstream (`battle::MOVE_NONE`'s own
    // docs), so the moveset stops at the first empty one rather than
    // carrying placeholders into a battler that forbids them.
    let move_ids: Vec<assets::MoveId> = (0..MAX_MON_MOVES)
        .map(|index| {
            assets::MoveId(u16::from_le_bytes([
                substructures.attacks[index * 2],
                substructures.attacks[index * 2 + 1],
            ]))
        })
        .take_while(|move_id| *move_id != battle::MOVE_NONE)
        .collect();
    let known_moves = move_ids.len();

    let mut mon = BattlePokemon::new(
        dex,
        species,
        saved.level,
        ivs,
        saved.box_data.personality(),
        move_ids,
    )?
    .with_original_trainer_id(saved.box_data.ot_id());

    // `BattlePokemon::new` seeds experience at the level's own threshold;
    // the saved total (`MON_DATA_EXP`) also carries the sub-level progress
    // earned in battle, so apply the difference. Inconsistent bytes
    // reconcile the way upstream's own level derivation does
    // (`GetLevelFromMonExp`, reached from `CalculateMonStats`,
    // `pokeemerald/src/pokemon.c`): a total at or past the next level's
    // threshold levels the mon up to match it, and a total *below* the
    // saved level's own floor (unwritable by [`to_save_pokemon`]) stays at
    // the floor rather than representing a level/experience pair upstream
    // could never store.
    let saved_experience = u32::from_le_bytes([
        substructures.growth[4],
        substructures.growth[5],
        substructures.growth[6],
        substructures.growth[7],
    ]);
    mon.apply_experience(saved_experience.saturating_sub(mon.experience()));

    // Full HP is what `BattlePokemon::new` starts at; the save's own `hp`
    // is the state to restore. A saved value above the recomputed maximum
    // (an EV-carrying mon this port cannot rebuild -- module docs) clamps
    // to full rather than underflowing the subtraction.
    let max_hp = mon.stats().max_hp;
    mon.apply_damage(max_hp.saturating_sub(u32::from(saved.hp)));

    for index in 0..known_moves {
        let saved_pp = substructures.attacks[8 + index];
        // Slots start at the move's own base PP; spend the difference. A
        // saved value *above* base PP (PP Ups, not modelled) leaves the
        // slot full instead of adding capacity that does not exist here.
        let full_pp = mon.moves()[index].pp;
        for _ in 0..full_pp.saturating_sub(saved_pp) {
            mon.deduct_pp(index)?;
        }
    }

    Ok(mon)
}

/// Narrow a computed stat to the `u16` the save block stores, saturating
/// rather than wrapping. Unreachable in practice -- `CalculateMonStats`'
/// output for a level-100 mon is far under 1,000 -- but a wrap here would
/// silently mint a different mon.
fn clamp_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests;
