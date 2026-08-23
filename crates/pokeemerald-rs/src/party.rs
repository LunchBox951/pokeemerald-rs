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
//! **accumulated experience**, personality (and so nature), the six IVs,
//! the ability slot, the moveset with each slot's remaining PP, the packed
//! `ppBonuses` byte (issue #304), current HP, and the original-trainer id.
//! Each is either identity the model carries verbatim or state that
//! battling and levelling up mutate, which makes the live model the only
//! copy that can be right.
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
//! * **Effort values and contest condition** -- every mon `battle` builds
//!   has `0` EVs (that crate's own module docs), so `PokemonSubstruct2` is
//!   passed through whole rather than re-derived. A saved mon with real EVs
//!   still *battles* as a 0-EV mon, because the decode cannot rebuild what
//!   the model has no field for, but the bytes stay on disk for the model
//!   that eventually will.
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
//! [`from_save_pokemon`] cannot honour that cache, because
//! [`battle::BattlePokemon::new`] rebuilds the block from the `0` EVs this
//! port models, so a loaded EV-trained mon *battles* as a 0-EV one until
//! `battle` carries EVs. Only `hp` is read back out of the record, because
//! current HP is the one entry that is state rather than a function of the
//! rest.
//!
//! [`merge_into_save_pokemon`] must therefore not write that rebuilt block
//! back unconditionally: an EV-trained file merely loaded and re-saved
//! would be filed weaker until something upstream next runs
//! `CalculateMonStats` over it. So the six bytes are **retained** when the
//! stored record's species and level are still the battler's -- the block
//! is a function of species/level/IVs/EVs/nature, so sub-level experience
//! earned this session does not invalidate it (the experience word itself
//! is always overlaid). They are **overwritten** from
//! [`battle::BattlePokemon::stats`] exactly when species or level moved: a
//! mon that levelled up this session must not be filed with its old
//! numbers under its new level, which is a record upstream could never
//! write. The overwritten block is the 0-EV one, which is a derived field
//! going stale rather than state being lost -- upstream rebuilds it from
//! the retained EVs at its own next `CalculateMonStats`.
//!
//! Current HP is outside that choice: it is battle state, so the merge
//! always writes the battler's, retained block or not. It can never
//! contradict a retained maximum, because the model's own maximum is the
//! 0-EV one and EVs only ever add to it. One translation applies over a
//! *retained* block: [`from_save_pokemon`] clamps a stored `hp` above the
//! model's maximum down to it, hiding the `stored - model_max` points the
//! session never saw. That offset is measured once at load
//! ([`hp_hidden_by_load`]) and carried as session state beside the lead;
//! the merge adds it back onto the live number, capped at the retained
//! `max_hp` and never resurrecting a fainted battler. It must be state
//! rather than re-derived: the start menu writes the merge's output back
//! into the slot it passes as the next save's `base`, so measuring an
//! already-translated record would drift -- saving twice files the same
//! bytes. Continue -> SAVE is therefore byte-exact at any stored `hp` --
//! filing the clamped number would mark a full-health lead damaged, the
//! corruption shape issue #344 exists to stop -- and damage subtracts
//! absolutely, as upstream's EV-aware arithmetic would. The residue (a
//! live HP pinned at either boundary mid-session loses points the wider
//! upstream range would have kept) closes when `battle` carries EVs.

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

    let mut box_data = BoxPokemon::new(mon.personality(), mon.original_trainer_id());
    box_data.set_substructures(&PokemonSubstructures {
        growth,
        attacks,
        // `PokemonSubstruct2` -- EVs and contest condition, all zero.
        evs_and_condition: [0u8; SUBSTRUCTURE_LEN],
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
    overlay_battle_stats(&mut record, mon);
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
/// derived stat block -- from the battler.
///
/// [`to_save_pokemon`] always runs this: a record built from scratch has
/// no cached block to keep, so every entry must come from the model.
/// [`merge_into_save_pokemon`] runs it only when the session changed what
/// the block is a function of, and writes [`overlay_current_hp`] alone
/// otherwise -- see the module docs for why re-deriving a retained block
/// would file an EV-trained save permanently weaker.
fn overlay_battle_stats(record: &mut Pokemon, mon: &BattlePokemon) {
    let stats = mon.stats();
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
/// Safe to write over a retained stat block: the battler's HP is at most
/// its own maximum, and that maximum is computed from the `0` EVs this
/// port models, so it cannot exceed the maximum a retained block holds.
fn overlay_current_hp(record: &mut Pokemon, mon: &BattlePokemon) {
    record.hp = clamp_u16(mon.current_hp());
}

/// [`overlay_current_hp`] for a *retained* stat block, undoing
/// [`from_save_pokemon`]'s load clamp (module docs): the points of the
/// stored `hp` above the model's maximum were hidden from the session, so
/// they are added back onto the live number, capped at the retained
/// `max_hp`. A Continue -> SAVE round trip is byte-exact at any stored
/// `hp`, and in-session damage subtracts absolutely from the stored value
/// rather than from its clamp. A fainted battler stays fainted: `0` is
/// the session's own outcome, not a clamp artifact.
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
    hp_hidden_by_load: u16,
) {
    let live = clamp_u16(mon.current_hp());
    record.hp = if live == 0 {
        0
    } else {
        live.saturating_add(hp_hidden_by_load).min(record.max_hp)
    };
}

/// The current-HP points [`from_save_pokemon`]'s clamp hid from the
/// session: the stored `hp` above the decoded battler's own (0-EV)
/// maximum. Measured once, when the record is decoded, and carried beside
/// the lead until [`merge_into_save_pokemon`] adds it back; zero whenever
/// the stored value fits the model's range.
pub(crate) fn hp_hidden_by_load(stored: &Pokemon, lead: &BattlePokemon) -> u16 {
    stored.hp.saturating_sub(clamp_u16(lead.stats().max_hp))
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
/// enter that guard. Current HP is always the battler's, translated
/// back across the load clamp when the block is retained (module docs).
/// The module docs give the reasoning; [`overlay_battle_stats`] and
/// [`overlay_current_hp`] are the two writes.
///
/// `hp_hidden_by_load` is the session offset [`hp_hidden_by_load`] measured
/// at load; the merge owns rebasing it. Both the recompute branch and the
/// from-scratch fallback zero it, because the record they write has the
/// model's own `max_hp` and hides nothing -- carrying the old offset into
/// the next save's retained-block branch would silently heal the lead by
/// its stale value.
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
    hp_hidden_by_load: &mut u16,
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

    // `PokemonSubstruct2` -- EVs and contest condition -- is untouched: the
    // battle model has no field that could contradict it.

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
        // including the EV contribution this port cannot rebuild. Only
        // current HP, which is state, comes from the battler -- translated
        // back across the load clamp by the offset measured at load time
        // (module docs).
        overlay_current_hp_over_retained_block(&mut merged, mon, *hp_hidden_by_load);
    } else {
        // Species or level moved this session, so the cached block is a
        // function of inputs that no longer hold and upstream would have
        // recomputed it (`CalculateMonStats`). Sub-level experience is
        // deliberately excluded from the guard above. The record's stat
        // block is the model's own from here on, so no stored points stay
        // hidden behind the load clamp: a carried offset would heal the
        // next retained-block save by exactly its stale value.
        *hp_hidden_by_load = 0;
        overlay_battle_stats(&mut merged, mon);
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
) -> Result<PokemonSubstructures, &'static str> {
    if base.box_data.personality() != mon.personality()
        || base.box_data.ot_id() != mon.original_trainer_id()
    {
        return Err("holds a different Pokémon (personality or OT id)");
    }
    let substructures = base
        .box_data
        .substructures()
        .map_err(|_| "failed its own checksum")?;
    let species = u16::from_le_bytes([substructures.growth[0], substructures.growth[1]]);
    if species == SPECIES_NONE {
        return Err("is empty");
    }
    Ok(substructures)
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
    .with_pp_bonuses(dex, pp_bonuses)?;

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

#[cfg(test)]
mod tests;
