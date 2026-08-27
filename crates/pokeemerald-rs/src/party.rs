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
//! -- see [`merge_into_save_pokemon`] / [`to_save_pokemon`] /
//! [`from_save_pokemon`].
//!
//! # What survives the round trip, and what does not
//!
//! Which fields survive depends on *which* encoder runs, and that is the
//! whole point of the pair (issue #344).
//!
//! [`to_save_pokemon`] has nothing but a battler to write from, so a field
//! with no home in [`battle::BattlePokemon`] can only be written as
//! `CreateMon`'s own default. That is honest for a mon this port has just
//! created and destructive for one it has just *loaded*: saving would hand
//! the file back a mon stripped of everything the battle model does not
//! model. [`merge_into_save_pokemon`] is the loaded case -- it starts from
//! the record the save was read out of and overlays only the fields the
//! battler is authoritative for, so an unmodelled field is carried by the
//! bytes rather than by the model. The overworld save path uses the merge;
//! the from-scratch encoder is what a mon with no backing record gets.
//!
//! Battle-authoritative, and so written by both encoders: species, level,
//! **accumulated experience**, **effort values** (issue #415), personality
//! (and so nature), the six IVs, the ability slot, the moveset with each
//! slot's remaining PP, the packed `ppBonuses` byte (issue #304), current
//! HP, and the original-trainer id. Each is either identity the model
//! carries verbatim or state that battling and levelling up mutate, which
//! makes the live model the only copy that can be right.
//!
//! `ppBonuses` is the one of those that is *byte*-exact rather than merely
//! value-exact, and deliberately so: it is written back exactly as it was
//! read, including bits belonging to a slot the moveset does not fill.
//! Upstream's own paths never set such a bit (a PP Up can only be used on a
//! move that exists), so this port can neither produce one nor be sure a
//! save that has one is wrong -- and quietly zeroing a byte of somebody's
//! save is not this encoder's call to make. See
//! [`battle::PpBonuses`] for the packing and
//! [`battle::BattlePokemon::max_pp`] for what it buys each slot.
//!
//! Deliberately *not* modelled by `battle`, and so **retained from the
//! backing record** by [`merge_into_save_pokemon`]. Only
//! [`to_save_pokemon`], which has no record to retain them from, writes
//! upstream's own default; [`from_save_pokemon`] reads none of them back:
//!
//! * **Contest condition** -- `PokemonSubstruct2`'s trailing six bytes
//!   (`cool`/`beauty`/`cute`/`smart`/`tough`/`sheen`) have no home in
//!   `battle::BattlePokemon`, so both encoders leave them exactly as the
//!   save already held (`to_save_pokemon` at their `CreateMon` default of
//!   `0`, `merge_into_save_pokemon` at whatever `base` held). Effort values,
//!   the substructure's other half, no longer belong on this list -- see the
//!   EVs discussion below.
//! * **Held item, accumulated friendship, pokérus, met
//!   location/level/game, poké ball, OT gender, ribbons, markings,
//!   nickname, OT name, language, mail, and non-volatile status** -- none
//!   has a typed home in `battle::BattlePokemon`, so each is exactly the
//!   byte the save already held.
//! * **The egg bit** -- `isEgg` (`PokemonSubstruct3`'s bit 30) shares its
//!   word with the IVs the model *does* carry, so the merge rewrites the
//!   word around it rather than through it. This port models no eggs and
//!   so must not clear somebody else's flag on the way past.
//!
//! Retention has one visible edge, named here rather than left to be
//! found. `HealPlayerParty` clears `MON_DATA_STATUS` along with the HP and
//! PP it restores (`pokeemerald/src/script_pokemon_util.c:53-57`), and this
//! port's own heals ([`battle::BattlePokemon::heal`], reached from the
//! white-out and from the first battle's conclusion) restore both of those
//! but have no status field to clear. The merge's side of that gap is
//! plain retention: a stored status word rides through every ordinary
//! save like any other unmodelled field. The white-out closes its side at
//! the call site -- after a successful heal it clears the retained
//! record's status word directly ([`crate::flow`]'s
//! `overworld_phase::white_out`), so an ordinary save keeps the word and
//! a white-out does not, which is upstream's split. The first battle's
//! conclusion needs no such clear: its lead can only be a record this
//! port itself wrote (the trigger is consumed before any save could carry
//! real EVs or a status there), and every port-written record has a zero
//! status word. Once `battle` models status, the merge overlays it like
//! any other battle-owned field and the call-site clear disappears.
//!
//! One more field *does* round-trip, added alongside
//! [`battle::BattlePokemon::ability`] (issue #322): **the ability slot** --
//! `abilityNum` (`PokemonSubstruct3`'s bit 31, the IV word's top bit) is
//! written from [`battle::BattlePokemon::ability_slot`] and read back into
//! [`battle::BattlePokemon::with_ability_slot`] rather than left to
//! [`battle::BattlePokemon::new`]'s personality-parity default, so a saved
//! mon whose stored slot disagrees with its personality (a legitimate
//! upstream state -- nothing re-derives `abilityNum` from personality after
//! `CreateBoxMon`) keeps the ability it actually has.
//!
//! One field is *derived* rather than carried when there is no record to
//! read it from, so a from-scratch mon's bytes stay self-consistent with
//! what upstream would have written:
//!
//! * **Friendship** -- the species' `base_friendship`, as `CreateBoxMon`
//!   sets it (`pokeemerald/src/pokemon.c:2256`). Re-deriving it is right
//!   for a mon being created and wrong for one being re-saved, whose
//!   friendship is everything walking, levelling and fainting have done to
//!   it since; [`merge_into_save_pokemon`] keeps the stored byte.
//!
//! Experience used to be on that list, derived from the growth curve at
//! the mon's level. Since battles apply earned experience to the battler
//! itself ([`battle::BattlePokemon::apply_experience`], issue #237), the
//! sub-level progress between two thresholds is real state, so the encode
//! writes the mon's own total (upstream's `MON_DATA_EXP`) and the decode
//! restores it -- see [`from_save_pokemon`] for how inconsistent bytes are
//! reconciled.
//!
//! Effort values joined that list at issue #415, the same way: `battle`
//! now has a field to carry them in
//! ([`battle::BattlePokemon::with_evs`] adopts a loaded record's own
//! bytes at construction; [`battle::BattlePokemon::gain_evs`] increments
//! them on every KO this session awards, `MonGainEVs`,
//! `pokeemerald/src/battle_script_commands.c:3420`), so both encoders now
//! write the live value into `PokemonSubstruct2`'s first six bytes rather
//! than leaving them as whatever `base` held -- see
//! [`evs_from_substruct2`] and its callers. Upstream's own `MonGainEVs`
//! writes those bytes on every KO regardless of whether a level-up
//! follows, so this is independent of the stat-block cache discussion
//! below, which still decides whether the *other* six (cached) stat bytes
//! move this save.
//!
//! The stat block (`max_hp`/`attack`/.../`special_defense`) is the one
//! group of fields that is neither purely battle-authoritative nor purely
//! retained, so the merge decides per save. Upstream stores it as a
//! *cache*: `CalculateMonStats` (`pokeemerald/src/pokemon.c:2823`)
//! recomputes it from species, level, IVs **and EVs** at each of its call
//! sites -- a level-up, an evolution, a vitamin, a Box withdrawal -- and
//! every reader in between, battle included, takes the stored numbers as
//! they are. Nothing on the load path recomputes them: `LoadPlayerParty`
//! copies the bytes and calls nothing (`src/load_save.c:170-178`).
//!
//! [`from_save_pokemon`] cannot honour that cache at load time:
//! [`battle::BattlePokemon::new`] always rebuilds the block from `0` EVs,
//! and [`battle::BattlePokemon::with_evs`] (issue #415) deliberately does
//! not recompute it either -- so a loaded EV-trained mon still *battles* as
//! a 0-EV one until the next level-up folds the adopted value in, in
//! battle or -- for a stored level/experience pair that disagrees -- right
//! here at load, through [`battle::BattlePokemon::reconcile_saved_experience`]'s
//! own share of that recompute (`battle`'s module docs). Only `hp` is read
//! back out of the record at load, because current HP is the one entry
//! that is state rather than a function of the rest.
//!
//! [`merge_into_save_pokemon`] must therefore not write that rebuilt block
//! back unconditionally: an EV-trained file merely loaded and re-saved
//! would be filed weaker until something upstream next runs
//! `CalculateMonStats` over it. So the six bytes are **retained** when the
//! stored record's species and level are still the battler's -- the block
//! is a function of species/level/IVs/EVs/nature, so sub-level experience
//! earned this session does not invalidate it (the experience word itself
//! is always overlaid). They are **recomputed** exactly when species or
//! level moved: a mon that levelled up this session must not be filed with
//! its old numbers under its new level, which is a record upstream could
//! never write. The recompute (issue #384) is
//! [`compute_levelled_up_stats`] -- [`battle::compute_stats_with_evs`],
//! `CALC_STAT`'s own arithmetic, `ev / 4` term and all -- fed `mon`'s IVs
//! and nature alongside the EVs `PokemonSubstruct2` holds. Those bytes are,
//! since issue #415, `mon`'s own [`battle::BattlePokemon::evs`] freshly
//! written back (the EVs discussion above) rather than whatever `base`
//! loaded with, so a KO that gained EVs and crossed a level this same turn
//! sees its own gain here -- the exact gap issue #415 exists to close. This
//! is still not the same value as [`battle::BattlePokemon::stats`]'s own
//! live cache, which stays the `0`-EV formula until an *in-battle*
//! level-up recomputes it (`battle`'s module docs): this save-time
//! recompute is independent of that cache and does not read it, though the
//! two agree whenever the level moved through an in-battle award, since
//! both then run the identical formula over the identical inputs. Either
//! way the six bytes this filed block replaces are exactly what upstream's
//! own `CalculateMonStats` would have produced from them, not a value
//! weaker than the record they came from.
//!
//! Exactly one byte pair escapes that retention: Shedinja's maximum HP
//! (issue #401), which `CalculateMonStats` pins to a flat `1`
//! (`pokeemerald/src/pokemon.c:2845`-`:2848`) no base HP, IV, EV or level
//! can move. A retained maximum is kept because it may carry an EV
//! contribution this port cannot rebuild; Shedinja's never can, so a stored
//! maximum that disagrees is stale bytes rather than data, and
//! [`normalize_retained_shedinja_max_hp`] rewrites that one entry (and
//! rebases the load-clamp offset by the points it removes) inside the
//! retained branch. The other five stay retained even for Shedinja: no
//! `CalculateMonStats` runs on the save path, so EVs gained without a level
//! cross must go on sitting outside the cache exactly as they do upstream.
//! The raw EV bytes themselves are a different question (the EVs
//! discussion above) -- `MonGainEVs` writes them on every KO independent
//! of whether a level-up follows, so they move in the retained branch too;
//! only the *cached stat block* waits for a `CalculateMonStats` call this
//! save path never makes on its own.
//!
//! Current HP is outside that choice: it is battle state, so the merge
//! always writes the battler's, retained block or not. It can never
//! contradict either maximum, because the model's own maximum is the 0-EV
//! one and EVs only ever add to it -- true of a retained maximum and,
//! since issue #384, of a freshly recomputed one too. The same translation
//! applies over *both*: [`from_save_pokemon`] clamps a stored `hp` above
//! the model's maximum down to it, hiding the `stored - model_max` points
//! the session never saw. That offset is measured once at load
//! ([`hp_hidden_by_load`]) and carried as session state beside the lead;
//! the merge adds it back onto the live number, capped at whichever
//! maximum it is filing -- the retained one, or the one the recompute just
//! produced -- and never resurrecting a fainted battler. It must be state
//! rather than re-derived: the start menu writes the merge's output back
//! into the slot it passes as the next save's `base`, so measuring an
//! already-translated record would drift -- saving twice files the same
//! bytes. Continue -> SAVE is therefore byte-exact at any stored `hp` --
//! filing the clamped number would mark a full-health lead damaged, the
//! corruption shape issue #344 exists to stop, whether or not the same
//! save also carries a level-up -- and damage subtracts absolutely, as
//! upstream's EV-aware arithmetic would. A recompute does not merely carry
//! the offset forward, though: `CALC_STAT`'s `ev / 4` term is scaled by
//! `* level / 100`, so the same retained EVs are worth a different number
//! of points at a different level, and the offset was measured against the
//! *old* maximum's gap over the `0`-EV floor. So the merge rebases it --
//! [`zero_ev_max_hp`] recovers that old gap from the record it is
//! overwriting, and the freshly recomputed block supplies the new one --
//! before translating by it, so the offset goes on describing exactly the
//! points still hidden under whichever maximum is being filed rather than a
//! count fixed at load and never revisited. Carrying it unchanged across a
//! level-up filed a full-health EV-trained lead as damaged by however much
//! the gap grew (issue #384's round-2 review: a Treecko stored 41/41 at
//! level 13 filed 43/44 at level 14); retiring it to zero, as an earlier
//! version of this fix did, re-hides real points the moment the next save
//! lands back on the retained branch.
//!
//! That rebase is *signed*. `CALC_STAT` truncates the EV-aware product and
//! the `0`-EV one independently, so the gap between them does not grow
//! monotonically with level: it shrinks at every transition where the
//! `ev / 4` term bought a point at the old level and buys none at the new
//! one (issue #384's round-4 review -- a Treecko with HP IV 1 and 12 HP EVs
//! has a one-point gap at level 12 and none at level 13). At such a
//! level-up the model's own `0`-EV max-HP delta, which is what
//! `battle` adds to the live `current_hp`, is one point *wider* than the
//! EV-aware delta upstream's `CalculateMonStats` would have applied, so the
//! offset has to go negative to file upstream's number -- a rebase that
//! saturated at zero filed a lead stored at 1 HP as 3 where upstream files
//! 2. A live battler still never files `0` (that would report a mon the
//! session is still playing as fainted), the same way the translation never
//! files above the block's own maximum. The residue (a live HP pinned at
//! either boundary mid-session loses points the wider upstream range would
//! have kept) closes for a session that levels up: issue #415 makes
//! [`battle::BattlePokemon::stats`] itself EV-aware from that level-up
//! onward (`battle`'s module docs), so the live maximum this translation
//! caps against is no longer the `0`-EV one for the rest of the battle. It
//! stays open for a loaded EV-trained mon that never levels up this
//! session: its live cache never leaves the `0`-EV formula, by design
//! (`battle`'s module docs), so the boundary this paragraph describes can
//! still bind for it.

use battle::{BattlePokemon, Dex, Ivs, MAX_MON_MOVES};
use engine::save::{BoxPokemon, Pokemon, PokemonSubstructures, SUBSTRUCTURE_LEN};

/// `MAIL_NONE` (`pokeemerald/include/constants/items.h:446`): the party
/// block's "no mail held" sentinel, which `CreateMon` writes into every
/// freshly built mon (`src/pokemon.c:2201-2202`).
const MAIL_NONE: u8 = 0xFF;

/// `SPECIES_NONE` (`pokeemerald/include/constants/species.h:4`): the zero
/// species an empty party slot holds. [`merge_into_save_pokemon`] uses it
/// to tell a stored mon from a slot that never had one.
const SPECIES_NONE: u16 = 0;

/// `isEgg`, bit 30 of `PokemonSubstruct3`'s IV word
/// (`pokeemerald/include/pokemon.h:147`).
///
/// The one bit of that word this port neither models nor may disturb: the
/// IVs below it and `abilityNum` above it are both battle-authoritative,
/// so the merge has to rebuild the word around this bit (module docs).
const IS_EGG_BIT: u32 = 1 << 30;

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
/// upward, leaving bit 30 (`isEgg`) and bit 31 (`abilityNum`) clear -- the
/// ability bit is `OR`ed in separately by [`to_save_pokemon`], which is the
/// only caller that has a [`battle::BattlePokemon`] to read it from.
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

/// `PokemonSubstruct2`'s first six bytes (`pokeemerald/include/pokemon.h:
/// 117`-`:122`) -- one whole byte per EV, in the same
/// HP/Attack/Defense/Speed/SpAttack/SpDefense order [`pack_ivs`]/
/// [`unpack_ivs`] use, unlike the IVs' packed five-bit fields. Used by
/// [`from_save_pokemon`] to seed [`battle::BattlePokemon::with_evs`] (issue
/// #415) and, before that, by [`merge_into_save_pokemon`]'s stat-block
/// recompute to size a levelled-up block -- [`compute_levelled_up_stats`].
/// [`evs_to_substruct2`] is the inverse.
fn evs_from_substruct2(evs_and_condition: &[u8; SUBSTRUCTURE_LEN]) -> battle::Evs {
    battle::Evs {
        hp: evs_and_condition[0],
        attack: evs_and_condition[1],
        defense: evs_and_condition[2],
        speed: evs_and_condition[3],
        sp_attack: evs_and_condition[4],
        sp_defense: evs_and_condition[5],
    }
}

/// [`evs_from_substruct2`]'s inverse (issue #415): pack a
/// [`battle::Evs`] back into `PokemonSubstruct2`'s first six bytes, in the
/// same order. Used by both encoders to file the battler's own (possibly
/// KO-incremented) EVs -- [`to_save_pokemon`] for a mon with no backing
/// record, [`merge_into_save_pokemon`] for one there is.
fn evs_to_substruct2(evs: battle::Evs) -> [u8; 6] {
    evs.as_array()
}

/// The stat block [`merge_into_save_pokemon`] files for a lead whose species
/// or level moved this session -- `CalculateMonStats`
/// (`pokeemerald/src/pokemon.c:2823`), fed `mon`'s own IVs and nature
/// alongside the EVs `evs_and_condition` retains, rather than
/// [`battle::BattlePokemon::stats`]'s `0`-EV cache (module docs, issue
/// #384). Falls back to that cache if `mon`'s species is not in `dex` --
/// unreachable through [`BattlePokemon`], whose constructor already
/// resolved the species against a dex, but a species missing from *this*
/// dex must still produce a stat block rather than panic (matching
/// [`to_save_pokemon`]'s own friendship fallback).
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

/// The `0`-EV model's own maximum HP for `species` at `level`, `mon`'s
/// nature and IVs (neither of which the session can move) fed through the
/// same `CALC_STAT` formula [`compute_levelled_up_stats`] uses -- but at
/// `0` EVs, matching [`battle::BattlePokemon::stats`]'s own cache rather
/// than the record's retained ones.
///
/// [`merge_into_save_pokemon`]'s recompute branch uses this to rebase the
/// load-clamp offset: the offset was measured at load against the gap
/// between a *retained* maximum and this same `0`-EV floor, at whatever
/// species/level the record held then (module docs, issue #384's round-2
/// review). Moving the offset forward to a freshly recomputed maximum means
/// first undoing that old gap and then adding the new one -- both against
/// this floor, not against the battler's own cache, which already sits at
/// the *new* level and would compare a level-13 gap to a level-14 one.
///
/// Falls back to `mon.stats().max_hp` when `species` is not in `dex` --
/// unreachable in practice (this file models no evolution, so `species` is
/// always `mon.species()`, which [`BattlePokemon`]'s constructor already
/// resolved), but the same defensive fallback [`compute_levelled_up_stats`]
/// uses rather than a panic.
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
    // `ppBonuses` (`/*0x08*/`): the mon's own packed byte, written back
    // whole (module docs) -- issue #304, before which this stayed 0 and a
    // load/save cycle silently stripped a save's PP Ups.
    growth[8] = mon.pp_bonuses().bits();
    growth[9] = friendship;

    let attacks = encode_attacks(mon);

    let mut misc = [0u8; SUBSTRUCTURE_LEN];
    // `abilityNum` (`PokemonSubstruct3`'s bit 31, module docs) shares the IV
    // word's top bit with `isEgg` (bit 30, left clear -- no eggs modelled).
    // `ability_slot` is already masked to one bit
    // ([`battle::BattlePokemon::with_ability_slot`]), so the shift never
    // collides with the packed IVs below it.
    let iv_word = pack_ivs(mon.ivs()) | (u32::from(mon.ability_slot()) << 31);
    misc[4..8].copy_from_slice(&iv_word.to_le_bytes());

    // `PokemonSubstruct2` -- the mon's own EVs (issue #415; `0` for a mon
    // nothing has ever called `with_evs`/`gain_evs` on, matching `CreateMon`'s
    // own default) in the first six bytes, contest condition zero in the
    // trailing six (module docs).
    let mut evs_and_condition = [0u8; SUBSTRUCTURE_LEN];
    evs_and_condition[0..6].copy_from_slice(&evs_to_substruct2(mon.evs()));

    let mut box_data = BoxPokemon::new(mon.personality(), mon.original_trainer_id());
    box_data.set_substructures(&PokemonSubstructures {
        growth,
        attacks,
        evs_and_condition,
        misc,
    });

    let mut record = Pokemon {
        box_data,
        // `MON_DATA_STATUS`: no non-volatile status is modelled, and a mon
        // being created has none to lose.
        status: 0,
        mail: MAIL_NONE,
        ..Pokemon::default()
    };
    overlay_battle_stats(&mut record, mon, mon.stats());
    record
}

/// `PokemonSubstruct1` (`pokeemerald/include/pokemon.h:109-113`): the four
/// move ids followed by the four remaining-PP bytes.
///
/// All twelve bytes are the battler's, so even
/// [`merge_into_save_pokemon`] writes the substructure whole and leaves
/// the slots past the moveset `MOVE_NONE` with zero PP. That is not a hole
/// in the retention rule: upstream shifts the surviving moves down when one
/// is replaced (`DeleteFirstMoveAndGiveMoveToMon`,
/// `pokeemerald/src/pokemon.c:3046-3071`), so a gap with a real move behind
/// it is a shape upstream cannot store, and [`from_save_pokemon`] stops at
/// the first gap in any case -- there is nothing there to preserve.
fn encode_attacks(mon: &BattlePokemon) -> [u8; SUBSTRUCTURE_LEN] {
    let mut attacks = [0u8; SUBSTRUCTURE_LEN];
    for (index, slot) in mon.moves().iter().take(MAX_MON_MOVES).enumerate() {
        attacks[index * 2..index * 2 + 2].copy_from_slice(&slot.move_id.0.to_le_bytes());
        attacks[8 + index] = slot.pp;
    }
    attacks
}

/// The party record's unencrypted tail -- level, current HP, and the
/// derived stat block -- from `stats`.
///
/// [`to_save_pokemon`] always runs this, fed the battler's own `0`-EV
/// [`battle::BattlePokemon::stats`]: a record built from scratch has no
/// cached block to keep, so every entry must come from the model, and the
/// plain `current_hp` this writes into `record.hp` is exactly right --
/// there is no load clamp to translate back across. [`merge_into_save_pokemon`]'s
/// recompute branch runs it too, fed [`compute_levelled_up_stats`], only
/// when the session changed what the block is a function of -- but there
/// `record.hp` is not the last word: the caller re-overlays
/// [`overlay_current_hp_over_retained_block`] on top, so the load-clamp
/// offset still lands against the maximum just recomputed here (module
/// docs, issue #384). The retained branch skips this function altogether,
/// running [`overlay_current_hp_over_retained_block`] alone against the
/// block it kept.
fn overlay_battle_stats(record: &mut Pokemon, mon: &BattlePokemon, stats: battle::Stats) {
    record.level = mon.level();
    record.max_hp = clamp_u16(stats.max_hp);
    record.attack = clamp_u16(stats.attack);
    record.defense = clamp_u16(stats.defense);
    record.speed = clamp_u16(stats.speed);
    record.special_attack = clamp_u16(stats.sp_attack);
    record.special_defense = clamp_u16(stats.sp_defense);
    overlay_current_hp(record, mon);
}

/// Current HP, which both encoders write unconditionally: it is the one
/// entry of the record's tail that is battle *state* rather than a
/// function of species/level/IVs/EVs, so the live battler is always the
/// only copy that can be right (module docs).
///
/// Safe to write over any stat block [`overlay_battle_stats`] computes,
/// retained or recomputed: the battler's HP is at most its own maximum, and
/// that maximum is computed from the `0` EVs this port models, so it cannot
/// exceed either a retained block's maximum or a freshly recomputed
/// (EV-aware) one, both of which only ever add to the `0`-EV formula. In
/// [`to_save_pokemon`] this is the last word on `record.hp`; in
/// [`merge_into_save_pokemon`]'s recompute branch it is a provisional value
/// [`overlay_current_hp_over_retained_block`] immediately overwrites with
/// the load-clamp translation (module docs).
fn overlay_current_hp(record: &mut Pokemon, mon: &BattlePokemon) {
    record.hp = clamp_u16(mon.current_hp());
}

/// [`overlay_current_hp`] undoing [`from_save_pokemon`]'s load clamp
/// (module docs): the points of the stored `hp` above the model's maximum
/// were hidden from the session, so they are added back onto the live
/// number, capped at `record.max_hp`. Used both for a *retained* stat block
/// and, since issue #384, for a freshly *recomputed* one -- the caller
/// always writes `record.max_hp` first, so the cap is whichever block is
/// actually being filed. A Continue -> SAVE round trip is byte-exact at any
/// stored `hp`, and in-session damage subtracts absolutely from the stored
/// value rather than from its clamp. A fainted battler stays fainted: `0`
/// is the session's own outcome, not a clamp artifact.
///
/// The offset is signed, and goes negative after a level-up whose EV gap
/// shrank ([`merge_into_save_pokemon`]): there the model's live HP moved by
/// a *wider* delta than upstream's own EV-aware block would have, so filing
/// upstream's number means subtracting rather than adding. The fainted
/// guard's converse holds over that subtraction -- a live battler must not
/// file `0`, which would mark a mon the session is still playing as
/// fainted -- so the translated value floors at `1`, the mirror of the cap
/// that keeps it under the block's own maximum.
///
/// The offset is the caller's, measured once at load ([`hp_hidden_by_load`])
/// and carried as session state, *not* re-derived from `record.hp` here:
/// the start menu writes this function's output back into the slot it will
/// pass as the next save's `base`, so a re-derivation would measure an
/// already-translated value and drift -- saving twice must file the same
/// bytes.
fn overlay_current_hp_over_retained_block(
    record: &mut Pokemon,
    mon: &BattlePokemon,
    hp_hidden_by_load: i32,
) {
    let live = clamp_u16(mon.current_hp());
    record.hp = if live == 0 {
        0
    } else {
        let translated = i64::from(live) + i64::from(hp_hidden_by_load);
        u16::try_from(translated.max(1).min(i64::from(record.max_hp))).unwrap_or(record.max_hp)
    };
}

/// The one entry of a *retained* stat block that is still rewritten
/// (issue #401): Shedinja's maximum HP, which `CalculateMonStats` pins to a
/// flat `1` (`pokeemerald/src/pokemon.c:2845`-`:2848`) whatever its base HP,
/// IV, EV and level say. Every other retained byte can be a real EV-derived
/// value this port cannot reconstruct, so it is carried forward untouched
/// (module docs, issue #384); Shedinja's maximum can never be one, so a
/// stored block that disagrees -- a save written by a build predating this
/// fix, or a hand-edited one -- is normalized rather than carried forward
/// forever.
///
/// Only the maximum. The other five bytes stay retained even for Shedinja,
/// because nothing on this path is a `CalculateMonStats` call: upstream's
/// `MonGainEVs` (`pokeemerald/src/battle_script_commands.c:3420`) updates
/// the EV bytes and leaves the cache stale until a level-up, evolution,
/// vitamin or Box withdrawal recomputes it, and `SavePlayerParty` /
/// `LoadPlayerParty` (`pokeemerald/src/load_save.c:160-178`) call neither.
/// Refreshing all six here would cash an EV gain that crossed no level into
/// the stat cache early, for Shedinja alone `(behavioral-fidelity)`.
///
/// The load-clamp offset is rebased by the points the normalization
/// removes, exactly as [`merge_into_save_pokemon`]'s recompute branch
/// rebases across a level change: this branch runs only when species and
/// level are unchanged, so [`battle::BattlePokemon::stats`]'s own maximum
/// *is* the `0`-EV floor the offset was measured against
/// ([`hp_hidden_by_load`], [`zero_ev_max_hp`]), and the gap over it
/// disappears the moment the filed maximum becomes that floor.
fn normalize_retained_shedinja_max_hp(
    record: &mut Pokemon,
    mon: &BattlePokemon,
    hp_hidden_by_load: &mut i32,
) {
    if mon.species() != battle::SPECIES_SHEDINJA {
        return;
    }
    let invariant_max_hp = mon.stats().max_hp;
    let stale_points = clamp_i32(u32::from(record.max_hp).saturating_sub(invariant_max_hp));
    record.max_hp = clamp_u16(invariant_max_hp);
    *hp_hidden_by_load = hp_hidden_by_load.saturating_sub(stale_points);
}

/// The current-HP points [`from_save_pokemon`]'s clamp hid from the
/// session: `stored.hp` above the `0`-EV floor **at `stored.level`** --
/// [`zero_ev_max_hp`], not [`battle::BattlePokemon::stats`]'s own cache.
/// Measured once, when the record is decoded, and carried beside the lead
/// until [`merge_into_save_pokemon`] adds it back; zero whenever the stored
/// value fits that floor.
///
/// Never negative here -- the clamp can only hide points, never invent them
/// -- but signed for the caller's sake: [`merge_into_save_pokemon`]'s
/// rebase moves the offset below zero wherever a level-up shrinks the EV
/// gap, so the session state this seeds has to be able to hold that.
///
/// `stored.level` matters, not `lead.level()`: [`from_save_pokemon`] can
/// hand back a battler whose level has already moved past the byte this
/// same `stored` holds -- [`BattlePokemon::reconcile_saved_experience`]
/// raises the level to match a growth word the level byte contradicts
/// (issue #384's round-3 review). [`merge_into_save_pokemon`]'s recompute
/// branch rebases this offset by comparing an old floor taken at
/// `base.level` (the record's own stored byte, still `stored.level` here)
/// against a new one at `mon.level()`; that rebasing is only sound if the
/// offset itself was measured at `base.level` too, so this must not measure
/// against whatever level the battler has already reconciled to. Using
/// [`battle::BattlePokemon::stats`]'s cache here -- which tracks the
/// battler's *current* level -- silently mixed the two, filing a record
/// whose level byte disagreed with its experience weaker than upstream's
/// own `CalculateMonStats`, which derives the level from experience before
/// it ever computes a stat block.
pub(crate) fn hp_hidden_by_load(dex: &Dex, stored: &Pokemon, lead: &BattlePokemon) -> i32 {
    let floor = zero_ev_max_hp(dex, lead.species().0, stored.level, lead);
    i32::from(stored.hp.saturating_sub(clamp_u16(floor)))
}

/// `SavePlayerParty`'s per-mon half for a mon that came *out* of a save
/// (`src/load_save.c:160-168`): `base`, the record it was decoded from,
/// with the battler's own fields overlaid onto it (issue #344).
///
/// Upstream can assign the whole structure because its live mon and its
/// stored mon are the same 100-byte type. This port's live mon is a
/// [`battle::BattlePokemon`], a deliberate subset of that type, so copying
/// "the whole thing" here means copying the part the battler owns onto the
/// part it does not -- held item, EVs, contest condition, friendship,
/// status, mail, met data, ribbons and header metadata all stay as the save
/// wrote them. Building the record from scratch instead is what erased
/// them (issue #344): every field with no model to come from came back a
/// zero.
///
/// The cached stat block is the one group that is retained *conditionally*
/// -- kept when species and level are unchanged, recomputed when the
/// session moved either one. Sub-level experience deliberately does not
/// enter that guard. Current HP is always the battler's, translated back
/// across the load clamp in *both* cases -- against the retained `max_hp`
/// when the block is kept, against the freshly recomputed one when it is
/// not (module docs, issue #384). [`overlay_battle_stats`] and
/// [`overlay_current_hp_over_retained_block`] are the writes.
///
/// `hp_hidden_by_load` is the session offset [`hp_hidden_by_load`] measured
/// at load; the merge owns rebasing it. Only the from-scratch fallback
/// above zeroes it: a record built with no backing substructures at all has
/// no clamp history to carry, and [`to_save_pokemon`] never reads the
/// offset back. The retained branch translates by it unchanged, because the
/// maximum it is filed against did not move. The recompute branch below
/// cannot: the offset was measured against `base`'s own retained maximum,
/// at `base.level`, and `CALC_STAT`'s `ev / 4` term is scaled by
/// `* level / 100`, so the same retained EVs are worth a different number
/// of points once the level the block is filed under has moved. So the
/// recompute branch first rebases the offset -- [`zero_ev_max_hp`] recovers
/// `base`'s own gap over the `0`-EV floor at `base.level`, the freshly
/// recomputed block supplies the gap at `mon.level()`, and the offset moves
/// by the difference, in either direction (the gap shrinks wherever
/// `CALC_STAT`'s independent truncations make the `ev / 4` term worth a
/// point at the old level and none at the new one, which is why the offset
/// is signed) -- and only then runs the same translation the
/// retained branch does, now against the just-recomputed (EV-aware, issue
/// #384) maximum rather than the retained block's. The battler's own
/// `current_hp` is still capped at the `0`-EV model's maximum regardless of
/// which branch runs, so a mon that loaded with real EVs needs the
/// translation either way. Carrying the offset forward unrebased, as an
/// earlier round of this fix did, filed a full-health EV-trained lead that
/// levelled up this session as damaged whenever the gap grew -- exactly the
/// corruption shape issue #344 exists to stop -- and zeroing it outright,
/// as an earlier version still did, would also have un-hidden those points
/// on the very next save, once this record's own species and level put it
/// on the retained branch above.
///
/// Falls back to [`to_save_pokemon`] when `base` is not this battler's own
/// record -- see [`backing_substructures`] for what disqualifies it. The
/// substructures go back through
/// [`engine::save::BoxPokemon::set_substructures`], which re-shuffles them
/// into personality order, re-encrypts them, and rewrites the checksum, so
/// the merged record is as valid as the one it was read from.
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

    // Whether the cached stat block is still the battler's own, decided
    // *before* the growth substructure below is overwritten with the
    // model's species (module docs). Species and level are the block's
    // only inputs the session can move -- IVs, EVs and nature all ride the
    // record -- so sub-level experience does not enter the test. Level is
    // checked against the record's own byte rather than inferred:
    // [`from_save_pokemon`] reconciles a stored level that its experience
    // contradicts, and the reconciled mon needs the block that matches the
    // level actually being written.
    //
    // Shedinja does not leave that guard (issue #401): only its *maximum
    // HP* is species-invariant, and the retained branch normalizes that one
    // entry on its own ([`normalize_retained_shedinja_max_hp`]). Excluding
    // Shedinja from the fast path outright would file the other five bytes
    // from a fresh EV-aware recompute
    // on an ordinary save that moved neither species nor level -- a
    // `CalculateMonStats` upstream never runs there (`MonGainEVs`,
    // `src/battle_script_commands.c:3420`, updates the EV bytes and leaves
    // the cache alone until a level-up, evolution, vitamin or Box
    // withdrawal), so an EV gain without a level cross would be cashed into
    // the stat cache early for Shedinja and only for Shedinja.
    let stored_species = u16::from_le_bytes([substructures.growth[0], substructures.growth[1]]);
    let stat_block_is_still_the_battlers =
        stored_species == mon.species().0 && base.level == mon.level();

    // `PokemonSubstruct0`: species, experience and `ppBonuses` are the
    // battler's; `heldItem` (`/*0x02*/`), `friendship` (`/*0x09*/`) and the
    // trailing filler are the save's.
    substructures.growth[0..2].copy_from_slice(&mon.species().0.to_le_bytes());
    substructures.growth[4..8].copy_from_slice(&mon.experience().to_le_bytes());
    substructures.growth[8] = mon.pp_bonuses().bits();

    substructures.attacks = encode_attacks(mon);

    // `PokemonSubstruct2`'s first six bytes -- the battler's own EVs (issue
    // #415), written unconditionally: upstream's `MonGainEVs`
    // (`pokeemerald/src/battle_script_commands.c:3420`) writes them on
    // every KO regardless of whether a level-up follows, independent of the
    // stat-block branch below. Contest condition, the substructure's other
    // half, has no home in `battle::BattlePokemon` and stays exactly as
    // `base` held it (module docs). This write runs *before* the recompute
    // branch below, which reads these same bytes back out
    // ([`compute_levelled_up_stats`]) to size a levelled-up block, so a KO
    // that gained EVs and crossed a level this same turn sees its own gain
    // there rather than the stale bytes `base` loaded with.
    substructures.evs_and_condition[0..6].copy_from_slice(&evs_to_substruct2(mon.evs()));

    // `PokemonSubstruct3`: only the IV word is rewritten, and only around
    // `isEgg`. `pokerus`, the met data, the ball, `otGender` and every
    // ribbon are the save's.
    let iv_word = u32::from_le_bytes([
        substructures.misc[4],
        substructures.misc[5],
        substructures.misc[6],
        substructures.misc[7],
    ]);
    let merged_ivs =
        pack_ivs(mon.ivs()) | (iv_word & IS_EGG_BIT) | (u32::from(mon.ability_slot()) << 31);
    substructures.misc[4..8].copy_from_slice(&merged_ivs.to_le_bytes());

    // The box header comes along whole -- nickname, language, OT name,
    // markings and the `hasSpecies`/`isBadEgg` bits are all bytes this port
    // does not model and so cannot re-derive.
    let mut merged = *base;
    merged.box_data.set_substructures(&substructures);
    if stat_block_is_still_the_battlers {
        // The save's own six stat bytes stay exactly as they were --
        // including the EV contribution this port cannot rebuild -- bar
        // Shedinja's maximum HP, the one entry no EV can move (issue #401).
        // Only current HP, which is state, comes from the battler --
        // translated back across the load clamp by the offset measured at
        // load time (module docs).
        normalize_retained_shedinja_max_hp(&mut merged, mon, hp_hidden_by_load);
        overlay_current_hp_over_retained_block(&mut merged, mon, *hp_hidden_by_load);
    } else {
        // Species or level moved this session, so the cached block is a
        // function of inputs that no longer hold and upstream would have
        // recomputed it (`CalculateMonStats`). Sub-level experience is
        // deliberately excluded from the guard above. The recomputed block
        // is fed the record's own retained EVs (`compute_levelled_up_stats`,
        // module docs, issue #384) rather than the battler's `0`-EV cache,
        // so the maximum this write files is upstream's own EV-aware one,
        // not a weaker cache the model built from `0` EVs.
        //
        // `overlay_battle_stats` writes a plain, untranslated `current_hp`
        // into `record.hp` as its last step; that is wrong here exactly as
        // it would be for the retained branch above, because the battler's
        // own `current_hp` is still capped at the `0`-EV model's maximum
        // regardless of which branch runs. So the same load-clamp
        // translation the retained branch applies runs again here, now
        // against the maximum `overlay_battle_stats` just recomputed rather
        // than the retained block's. The offset itself is *not* reset: it
        // still describes real hidden points under the new maximum, and
        // zeroing it would un-hide them on the very next save, once this
        // record's own species and level put it on the retained branch
        // above.
        //
        // Nor is it carried unchanged: it was measured against `base`'s own
        // gap over the `0`-EV floor at `base.level`, and `CALC_STAT`'s
        // `ev / 4` term is scaled by `* level / 100`, so that gap is not the
        // one the just-recomputed block has at `mon.level()`. Rebase by the
        // difference before translating -- `zero_ev_max_hp` recovers the
        // old floor from `base` and `stored_species`/`base.level` (the
        // record this write is overwriting), and `mon.stats().max_hp` is
        // already the new floor, at `mon.level()`, now that
        // `overlay_battle_stats` has run (module docs).
        //
        // The difference is signed. `CALC_STAT` truncates the EV-aware and
        // `0`-EV products independently, so the gap does not climb
        // monotonically with level: it shrinks wherever the EV term bought
        // a point at the old level and buys none at the new one, and there
        // the model's own `0`-EV level-up delta is *wider* than upstream's
        // EV-aware one. An offset that could not go below zero filed that
        // extra point (issue #384's round-4 review), so both the offset and
        // this arithmetic are `i32` -- the gaps themselves stay clamped at
        // `0`, since a record whose stored maximum sits below its own
        // `0`-EV floor is inconsistent bytes rather than a negative EV
        // contribution.
        let stats = compute_levelled_up_stats(dex, mon, &substructures.evs_and_condition);
        overlay_battle_stats(&mut merged, mon, stats);
        let old_floor = zero_ev_max_hp(dex, stored_species, base.level, mon);
        let gap_old = clamp_i32(u32::from(base.max_hp).saturating_sub(old_floor));
        let gap_new = clamp_i32(u32::from(merged.max_hp).saturating_sub(mon.stats().max_hp));
        *hp_hidden_by_load = hp_hidden_by_load
            .saturating_add(gap_new)
            .saturating_sub(gap_old);
        overlay_current_hp_over_retained_block(&mut merged, mon, *hp_hidden_by_load);
    }
    merged
}

/// The decrypted substructures of a `base` that really is this battler's
/// own stored record, or the reason there is nothing
/// [`merge_into_save_pokemon`] may overlay onto -- phrased to complete
/// that function's log line, since a save that silently stops retaining is
/// exactly the failure issue #344 is about.
///
/// Personality and the original-trainer id are both the mon's identity and
/// its substructure XOR key, so a disagreement means the slot holds a
/// *different* Pokémon: overlaying would graft this battler's moveset onto
/// that one's ribbons and met data, which is worse than the zeroing this
/// merge exists to fix. A `SPECIES_NONE` slot never held a mon and so has
/// no bytes worth keeping, and a checksum failure means the retained bytes
/// cannot be read at all. Each case wants a record built from scratch.
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
    let species = u16::from_le_bytes([substructures.growth[0], substructures.growth[1]]);
    if species == SPECIES_NONE {
        return Err(NotTheBattlersRecord::Empty);
    }
    Ok(substructures)
}

/// Why a slot's bytes are not the battler's own record to overlay onto --
/// [`backing_substructures`]'s three disqualifiers, formatted only at
/// [`merge_into_save_pokemon`]'s logging boundary, where each variant
/// completes the "party slot 0 ..." sentence.
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

/// `LoadPlayerParty`'s per-mon half (`src/load_save.c:170-178`): the battler
/// a saved party value describes.
///
/// Stats are recomputed from species/level/nature/IVs, at `0` EVs, by
/// [`battle::BattlePokemon::new`] (module docs); the record's own EVs
/// ([`evs_from_substruct2`]) are adopted right after, through
/// [`battle::BattlePokemon::with_evs`] (issue #415), which does not disturb
/// that `0`-EV stat block on its own (`battle`'s module docs). Accumulated
/// experience is wound back to the saved value next -- through
/// [`battle::BattlePokemon::reconcile_saved_experience`], which can itself
/// raise the level (and, since the adopted EVs are already in place by
/// then, recompute the stat block EV-aware) for a stored level/experience
/// pair that disagrees, exactly the way an in-battle level-up would.
/// Current HP and each move slot's PP are then wound back to the saved
/// values through the remaining mutations that preserve that type's
/// invariants ([`battle::BattlePokemon::apply_damage`] /
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
    let iv_word = u32::from_le_bytes([
        substructures.misc[4],
        substructures.misc[5],
        substructures.misc[6],
        substructures.misc[7],
    ]);
    let ivs = unpack_ivs(iv_word);
    // `abilityNum`, the IV word's top bit (module docs) -- read back rather
    // than re-derived from personality, so a saved slot that disagrees with
    // personality parity (a legitimate upstream state) survives the round
    // trip.
    let ability_slot = u8::from(iv_word >> 31 != 0);
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

    // `MON_DATA_PP_BONUSES` (`/*0x08*/`), adopted before any PP is wound
    // back: it is what each slot's *capacity* is, so the spend below counts
    // down from the PP-Up-adjusted maximum rather than from base PP. Every
    // byte is legal (two-bit fields cannot overflow), so there is nothing to
    // screen -- `battle::PpBonuses`' own docs.
    let pp_bonuses = battle::PpBonuses::from_bits(substructures.growth[8]);

    // `PokemonSubstruct2`'s first six bytes -- this record's own EVs (issue
    // #415), adopted before `reconcile_saved_experience` below, which shares
    // its stat recompute with the in-battle award and so must see the same
    // EVs a battle-time recompute would (`battle::BattlePokemon::with_evs`'s
    // own docs).
    let evs = evs_from_substruct2(&substructures.evs_and_condition);

    let mut mon = BattlePokemon::new(
        dex,
        species,
        saved.level,
        ivs,
        saved.box_data.personality(),
        move_ids,
    )?
    .with_original_trainer_id(saved.box_data.ot_id())
    .with_ability_slot(ability_slot)
    .with_pp_bonuses(dex, pp_bonuses)?
    .with_evs(evs);

    // `BattlePokemon::new` seeds experience at the level's own threshold;
    // the saved total (`MON_DATA_EXP`) also carries the sub-level progress
    // earned in battle, so adopt the stored total. Inconsistent bytes
    // reconcile the way upstream's own load path does
    // (`GetLevelFromMonExp`, reached from `CalculateMonStats`,
    // `pokeemerald/src/pokemon.c`): a total at or past the next level's
    // threshold levels the mon up to match it, and a total *below* the
    // saved level's own floor (unwritable by [`to_save_pokemon`]) stays at
    // the floor rather than representing a level/experience pair upstream
    // could never store. Crucially the load path *only* derives the level:
    // upstream copies the attacks substructure verbatim and never runs
    // `MonTryLearningNewMove` on load, so the crossed levels' learnset
    // moves are NOT taught here -- `reconcile_saved_experience` exists so
    // this decode cannot mutate the save's own authoritative moveset.
    // `apply_experience`'s learnset walk (issue #252) belongs to
    // `Cmd_getexp`'s in-battle award alone.
    let saved_experience = u32::from_le_bytes([
        substructures.growth[4],
        substructures.growth[5],
        substructures.growth[6],
        substructures.growth[7],
    ]);
    mon.reconcile_saved_experience(saved_experience);

    // Full HP is what `BattlePokemon::new` starts at; the save's own `hp`
    // is the state to restore. A saved value above the recomputed maximum
    // (an EV-carrying mon this port cannot rebuild -- module docs) clamps
    // to full rather than underflowing the subtraction.
    let max_hp = mon.stats().max_hp;
    mon.apply_damage(max_hp.saturating_sub(u32::from(saved.hp)));

    for index in 0..known_moves {
        let saved_pp = substructures.attacks[8 + index];
        // `with_pp_bonuses` left every slot full at its PP-Up-adjusted
        // maximum (`CalculatePPWithBonus`), so the difference to spend is
        // measured from that maximum. A saved value *above* it -- which
        // upstream cannot write, since `MON_DATA_PP1` is only ever set from
        // that same formula -- leaves the slot full rather than underflowing
        // the subtraction.
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

/// The same narrowing for the signed side of the load-clamp offset
/// ([`merge_into_save_pokemon`]'s rebase): an EV gap is a difference of two
/// `u16` stat entries, so it always fits, but a wrap would turn a gap that
/// grew into one that shrank.
fn clamp_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests;
