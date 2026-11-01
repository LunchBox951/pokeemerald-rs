//! A Pokémon's owned battle state: identity, HP, status, and move slots.
//!
//! [`BattlePokemon`] carries its held item ([`BattlePokemon::held_item`]); a
//! battle admits it only when the item's hold effect is none
//! ([`crate::ensure_held_item_playable`]). It carries primary status
//! ([`Status1`]) distinct from [`Volatiles`] — see that type's own docs for
//! what is and is not modelled. Shedinja's 1-HP special case *is* modelled —
//! see [`stats`].
//!
//! [`stats`] owns the stat formula and the standalone [`Ivs`] and [`Stats`]
//! value types. `combat_stats` owns the battle-time stat reads, and
//! `experience` owns the level-from-experience walk.
//!
//! [`evs`] owns the whole EV surface: adoption, KO gains, and the one line
//! [`BattlePokemon::stats`] refuses to cross because of them.
//! [`pp_bonuses`] owns packed PP Up state, while [`learn`] owns level-up
//! move decisions.
//!
//! What stays here is the aggregate itself: the [`BattlePokemon`] fields, its
//! construction and validation, and its small accessors and HP, status, and PP
//! mutators. Those share one private field set, so the remaining size is the
//! struct's own surface rather than a second concept.

use assets::items::ItemId;
use assets::{experience_for_level, AbilityId, BaseStats, MoveId, SpeciesId, Type};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::Nature;
use crate::stat_stage::StatStage;
use crate::status1::Status1;
use crate::volatile::Volatiles;

mod combat_stats;
pub mod evs;
mod experience;
pub mod learn;
pub mod pp_bonuses;
pub mod stats;

#[cfg(test)]
mod tests;

pub use evs::{Evs, MAX_PER_STAT_EVS, MAX_TOTAL_EVS};
pub use learn::{LearnedMove, MoveLearnDecision, MoveLearnResolution, PendingMoveLearn};
pub use pp_bonuses::{calculate_pp_with_bonus, PpBonuses, MAX_PP_UPS};
use stats::effective_battle_base_stats;
#[cfg(test)]
use stats::{calc_max_hp, calc_stat};
pub use stats::{compute_stats, compute_stats_with_evs, Ivs, Stats};

/// Maximum number of moves a Pokémon can know.
pub const MAX_MON_MOVES: usize = 4;

/// Empty move-slot marker, which cannot be used as a known move.
pub const MOVE_NONE: MoveId = MoveId::NONE;

/// Reserved empty-species marker.
pub const SPECIES_NONE: SpeciesId = SpeciesId(0);

/// First reserved old-Unown compatibility species.
///
/// The inclusive range through [`SPECIES_OLD_UNOWN_Z`] contains dummy stat
/// rows rather than obtainable species (`src/data/pokemon/species_info.h:5`).
pub const SPECIES_OLD_UNOWN_B: SpeciesId = SpeciesId(252);

/// Last reserved old-Unown compatibility species.
pub const SPECIES_OLD_UNOWN_Z: SpeciesId = SpeciesId(276);

/// Shedinja's species identifier.
pub const SPECIES_SHEDINJA: SpeciesId = SpeciesId(303);

/// Minimum valid Pokémon level.
pub const MIN_LEVEL: u8 = 1;

/// Maximum valid Pokémon level.
pub const MAX_LEVEL: u8 = 100;

/// Maximum individual value for one stat.
pub const MAX_IV: u8 = 31;

/// The five combat, accuracy, and evasion stages for one battler.
///
/// Every stage starts at [`StatStage::NEUTRAL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatStages {
    /// Attack stage.
    pub attack: StatStage,
    /// Defense stage.
    pub defense: StatStage,
    /// Speed stage.
    pub speed: StatStage,
    /// Special Attack stage.
    pub sp_attack: StatStage,
    /// Special Defense stage.
    pub sp_defense: StatStage,
    /// Accuracy stage.
    pub accuracy: StatStage,
    /// Evasion stage.
    pub evasion: StatStage,
}

impl Default for StatStages {
    fn default() -> Self {
        Self {
            attack: StatStage::NEUTRAL,
            defense: StatStage::NEUTRAL,
            speed: StatStage::NEUTRAL,
            sp_attack: StatStage::NEUTRAL,
            sp_defense: StatStage::NEUTRAL,
            accuracy: StatStage::NEUTRAL,
            evasion: StatStage::NEUTRAL,
        }
    }
}

/// One known move and its remaining PP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveSlot {
    /// The known move.
    pub move_id: MoveId,
    /// Remaining PP, bounded by [`BattlePokemon::max_pp`].
    pub pp: u8,
}

const fn is_placeholder_species(species: SpeciesId) -> bool {
    species.0 == SPECIES_NONE.0
        || (species.0 >= SPECIES_OLD_UNOWN_B.0 && species.0 <= SPECIES_OLD_UNOWN_Z.0)
}

const ABILITY_SLOT_MASK: u8 = 1;

fn initial_ability_slot(base_stats: &BaseStats, personality: u32) -> u8 {
    let has_secondary_ability = base_stats.abilities[1].0 != 0;
    u8::from(has_secondary_ability && personality & u32::from(ABILITY_SLOT_MASK) != 0)
}

/// The ability [`BattlePokemon::new`] selects for `personality`, without building the Pokémon.
pub(crate) fn initial_ability(base_stats: &BaseStats, personality: u32) -> AbilityId {
    let [first, second] = base_stats.abilities;
    if initial_ability_slot(base_stats, personality) == 0 {
        first
    } else {
        second
    }
}

/// A single battler's owned state.
///
/// Construction guarantees a valid species, level, IV set, and non-empty
/// moveset of at most [`MAX_MON_MOVES`] real moves. Methods preserve those
/// invariants while changing battle state.
#[derive(Debug, Clone)]
pub struct BattlePokemon {
    species: SpeciesId,
    level: u8,
    /// The construction level, fixed for this instance's lifetime; the save
    /// encoders compare it with [`BattlePokemon::level`] to detect a session
    /// level-up.
    ///
    /// Excluded from [`PartialEq`] (below): provenance, not battle identity.
    created_at_level: u8,
    nature: Nature,
    ivs: Ivs,
    personality: u32,
    ability_slot: u8,
    original_trainer_id: u32,
    /// The held item, [`ItemId::NONE`] when empty. Part of the mon's identity,
    /// so it survives fainting and write-back; which items a battle accepts is
    /// decided by [`crate::battle::held_item`].
    held_item: ItemId,
    types: [Type; 2],
    base_stats: BaseStats,
    experience: u32,
    stats: Stats,
    current_hp: u32,
    /// This mon's effort values -- see the [`evs`] module docs for the full
    /// adoption/gain lifecycle and what carrying them does, and does not,
    /// change.
    evs: Evs,
    /// [`Self::evs`] as of the last crossed level-up -- the set upstream's
    /// `CalculateMonStats` cached at that event, which the save-time
    /// recompute reads instead of the live [`Self::evs`]. [`Evs::default`]
    /// until a level-up happens.
    ///
    /// Excluded from [`PartialEq`] (below): provenance, not battle identity.
    evs_at_last_level_up: Evs,
    moves: Vec<MoveSlot>,
    pp_bonuses: PpBonuses,
    stages: StatStages,
    volatiles: Volatiles,
    status1: Status1,
    pending_move_learn: Option<PendingMoveLearn>,
}

// Manual, rather than derived, so `created_at_level` and
// `evs_at_last_level_up` (their own doc comments) can stay out of it without
// a wrapper type.
impl PartialEq for BattlePokemon {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            species,
            level,
            created_at_level: _,
            nature,
            ivs,
            personality,
            ability_slot,
            original_trainer_id,
            held_item,
            types,
            base_stats,
            experience,
            stats,
            current_hp,
            evs,
            evs_at_last_level_up: _,
            moves,
            pp_bonuses,
            stages,
            volatiles,
            status1,
            pending_move_learn,
        } = self;
        *species == other.species
            && *level == other.level
            && *nature == other.nature
            && *ivs == other.ivs
            && *personality == other.personality
            && *ability_slot == other.ability_slot
            && *original_trainer_id == other.original_trainer_id
            && *held_item == other.held_item
            && *types == other.types
            && *base_stats == other.base_stats
            && *experience == other.experience
            && *stats == other.stats
            && *current_hp == other.current_hp
            && *evs == other.evs
            && *moves == other.moves
            && *pp_bonuses == other.pp_bonuses
            && *stages == other.stages
            && *volatiles == other.volatiles
            && *status1 == other.status1
            && *pending_move_learn == other.pending_move_learn
    }
}

impl Eq for BattlePokemon {}

impl BattlePokemon {
    /// Validates inputs that do not depend on generated personality or IVs.
    ///
    /// A valid request has a level in [`MIN_LEVEL`]`..=`[`MAX_LEVEL`], one to
    /// [`MAX_MON_MOVES`] known moves with no [`MOVE_NONE`] entries, and a real
    /// species outside the reserved [`SPECIES_NONE`] and old-Unown ranges.
    /// Wild construction calls this before drawing random values so invalid
    /// requests do not advance the shared RNG stream.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::InvalidLevel`],
    /// [`BattleError::InvalidMoveCount`], [`BattleError::PlaceholderMove`],
    /// [`BattleError::PlaceholderSpecies`], [`BattleError::UnknownSpecies`],
    /// or [`BattleError::UnknownMove`] for the corresponding invalid input.
    pub fn validate(
        dex: &Dex,
        species: SpeciesId,
        level: u8,
        moves: &[MoveId],
    ) -> Result<(), BattleError> {
        if !(MIN_LEVEL..=MAX_LEVEL).contains(&level) {
            return Err(BattleError::InvalidLevel(level));
        }
        let invalid_move_count = moves.is_empty() || moves.len() > MAX_MON_MOVES;
        if invalid_move_count {
            return Err(BattleError::InvalidMoveCount(moves.len()));
        }
        if let Some(index) = moves.iter().position(|move_id| *move_id == MOVE_NONE) {
            return Err(BattleError::PlaceholderMove(index));
        }
        if is_placeholder_species(species) {
            return Err(BattleError::PlaceholderSpecies);
        }
        dex.species(species)?;
        for move_id in moves {
            dex.move_data(*move_id)?;
        }
        Ok(())
    }

    /// Builds a full-HP battler with base PP and neutral battle scratch.
    ///
    /// Nature comes from [`Nature::from_personality`]. Ability slot defaults
    /// to personality parity for species with two abilities and zero for
    /// species with one ability. Every IV must be in `0..=`[`MAX_IV`]; other
    /// input requirements are documented by [`BattlePokemon::validate`].
    ///
    /// # Errors
    ///
    /// Returns the errors documented by [`BattlePokemon::validate`], or
    /// [`BattleError::InvalidIv`] when any IV exceeds [`MAX_IV`].
    pub fn new(
        dex: &Dex,
        species: SpeciesId,
        level: u8,
        ivs: Ivs,
        personality: u32,
        moves: Vec<MoveId>,
    ) -> Result<Self, BattleError> {
        Self::validate(dex, species, level, &moves)?;
        let nature = Nature::from_personality(personality);
        if let Some(invalid_iv) = ivs.first_invalid() {
            return Err(BattleError::InvalidIv(invalid_iv));
        }
        let base = dex.species(species)?;
        let experience = experience_for_level(base.growth_rate, level)
            .map_err(|_| BattleError::InvalidLevel(level))?;
        let stats = compute_stats(
            species,
            &effective_battle_base_stats(species, base),
            level,
            nature,
            ivs,
        );
        let mut slots = Vec::with_capacity(moves.len());
        for move_id in moves {
            let pp = dex.move_data(move_id)?.pp;
            slots.push(MoveSlot { move_id, pp });
        }
        Ok(Self {
            species,
            level,
            created_at_level: level,
            nature,
            ivs,
            personality,
            ability_slot: initial_ability_slot(base, personality),
            original_trainer_id: 0,
            held_item: ItemId::NONE,
            types: base.types,
            base_stats: *base,
            experience,
            stats,
            current_hp: stats.max_hp,
            // `CreateBoxMon` zeroes the box before writing the fields it
            // sets, and the EV bytes are not one of them: a freshly built
            // mon has no effort values. A saved one restores its own bytes
            // through [`BattlePokemon::with_evs`].
            evs: Evs::default(),
            // No level-up has happened yet -- see the field's own doc.
            evs_at_last_level_up: Evs::default(),
            moves: slots,
            pp_bonuses: PpBonuses::NONE,
            stages: StatStages::default(),
            volatiles: Volatiles::default(),
            status1: Status1::default(),
            pending_move_learn: None,
        })
    }

    /// Applies saved PP Ups and refills every known move to its adjusted
    /// maximum.
    ///
    /// # Errors
    ///
    /// Returns the lookup error for any known move missing from `dex`.
    pub fn with_pp_bonuses(mut self, dex: &Dex, bonuses: PpBonuses) -> Result<Self, BattleError> {
        self.pp_bonuses = bonuses;
        for index in 0..self.moves.len() {
            let full = self.max_pp(dex, index)?;
            self.moves[index].pp = full;
        }
        Ok(self)
    }

    /// Species identifier.
    #[must_use]
    pub const fn species(&self) -> SpeciesId {
        self.species
    }

    /// Current level in [`MIN_LEVEL`]`..=`[`MAX_LEVEL`].
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }

    /// The level [`BattlePokemon::new`] built this instance at — see
    /// [`Self::created_at_level`]'s own field doc.
    #[must_use]
    pub const fn created_at_level(&self) -> u8 {
        self.created_at_level
    }

    /// Accumulated experience on this species' growth curve.
    #[must_use]
    pub const fn experience(&self) -> u32 {
        self.experience
    }

    /// Nature derived from the personality value.
    #[must_use]
    pub const fn nature(&self) -> Nature {
        self.nature
    }

    /// Individual values.
    #[must_use]
    pub const fn ivs(&self) -> Ivs {
        self.ivs
    }

    /// Personality value used to derive nature and the initial ability slot.
    #[must_use]
    pub const fn personality(&self) -> u32 {
        self.personality
    }

    /// Ability selected by [`BattlePokemon::ability_slot`].
    ///
    /// Species without a secondary ability always use their primary ability.
    #[must_use]
    pub const fn ability(&self) -> AbilityId {
        let [first, second] = self.base_stats.abilities;
        if second.0 == 0 || self.ability_slot == 0 {
            first
        } else {
            second
        }
    }

    /// Stored ability slot, either zero or one.
    #[must_use]
    pub const fn ability_slot(&self) -> u8 {
        self.ability_slot
    }

    /// Both of this species' ability slots, regardless of which one this
    /// Pokemon has. `[second]` is [`AbilityId`]`(0)` for a single-ability
    /// species.
    ///
    /// Trainer AI's ability guess for an unrevealed target reads both slots
    /// rather than [`BattlePokemon::ability`] (`src/battle_ai_script_commands.c:
    /// 1350-1391`).
    #[must_use]
    pub(crate) const fn ability_slots(&self) -> [AbilityId; 2] {
        self.base_stats.abilities
    }

    /// Restores a saved ability slot, masking the input to one bit.
    #[must_use]
    pub const fn with_ability_slot(mut self, ability_slot: u8) -> Self {
        self.ability_slot = ability_slot & ABILITY_SLOT_MASK;
        self
    }

    /// The trainer id of the Pokémon's original trainer.
    ///
    /// This identity belongs to the Pokémon rather than its current owner;
    /// traded Pokémon therefore retain it when they move between saves.
    #[must_use]
    pub const fn original_trainer_id(&self) -> u32 {
        self.original_trainer_id
    }

    /// Assign the original-trainer identity at the boundary that creates or
    /// restores this Pokémon.
    #[must_use]
    pub const fn with_original_trainer_id(mut self, original_trainer_id: u32) -> Self {
        self.original_trainer_id = original_trainer_id;
        self
    }

    /// The item this Pokémon holds, [`ItemId::NONE`] when empty.
    #[must_use]
    pub const fn held_item(&self) -> ItemId {
        self.held_item
    }

    /// Give this Pokémon `held_item` at the boundary that creates or restores
    /// it, mirroring `MON_DATA_HELD_ITEM` written after `CreateMon`
    /// (`battle_main.c:2035-2065`).
    #[must_use]
    pub const fn with_held_item(mut self, held_item: ItemId) -> Self {
        self.held_item = held_item;
        self
    }

    /// Species types captured at construction.
    #[must_use]
    pub const fn types(&self) -> [Type; 2] {
        self.types
    }

    /// Computed battle stats.
    #[must_use]
    pub const fn stats(&self) -> Stats {
        self.stats
    }

    /// Current HP (`0` means fainted).
    #[must_use]
    pub const fn current_hp(&self) -> u32 {
        self.current_hp
    }

    /// Known moves and their remaining PP in slot order.
    ///
    /// The slice is non-empty, contains no [`MOVE_NONE`], and has at most
    /// [`MAX_MON_MOVES`] entries.
    #[must_use]
    pub fn moves(&self) -> &[MoveSlot] {
        &self.moves
    }

    /// Move in `index`, or `None` when that slot is empty.
    #[must_use]
    pub fn move_at(&self, index: usize) -> Option<MoveId> {
        self.moves.get(index).map(|slot| slot.move_id)
    }

    /// Packed PP Up state, including bits for currently empty move slots.
    #[must_use]
    pub const fn pp_bonuses(&self) -> PpBonuses {
        self.pp_bonuses
    }

    /// Maximum PP for `index`, including PP Ups assigned to that slot.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::InvalidMoveSlot`] when `index` is empty, or the
    /// lookup error for a known move missing from `dex`.
    pub fn max_pp(&self, dex: &Dex, index: usize) -> Result<u8, BattleError> {
        let slot = self
            .moves
            .get(index)
            .ok_or(BattleError::InvalidMoveSlot(index))?;
        let base_pp = dex.move_data(slot.move_id)?.pp;
        Ok(calculate_pp_with_bonus(base_pp, self.pp_bonuses, index))
    }

    /// In-battle stat stages.
    #[must_use]
    pub const fn stages(&self) -> StatStages {
        self.stages
    }

    /// Mutable access to the independently constrained stat stages.
    pub fn stages_mut(&mut self) -> &mut StatStages {
        &mut self.stages
    }

    /// Whether current HP is zero.
    #[must_use]
    pub const fn is_fainted(&self) -> bool {
        self.current_hp == 0
    }

    /// Subtracts HP, saturating at zero.
    pub fn apply_damage(&mut self, amount: u32) {
        self.current_hp = self.current_hp.saturating_sub(amount);
    }

    /// Adds HP, saturating on overflow and clamping at maximum HP.
    ///
    /// This can revive a fainted battler because upstream's HP-gain branch has
    /// no fainted guard (`src/battle_script_commands.c:1896-1900`).
    pub fn heal_hp(&mut self, amount: u32) {
        self.current_hp = self
            .current_hp
            .saturating_add(amount)
            .min(self.stats.max_hp);
    }

    /// Current volatile conditions.
    #[must_use]
    pub const fn volatiles(&self) -> Volatiles {
        self.volatiles
    }

    /// Mutable access to independently valid volatile conditions.
    pub const fn volatiles_mut(&mut self) -> &mut Volatiles {
        &mut self.volatiles
    }

    /// Current primary status.
    #[must_use]
    pub const fn status1(&self) -> Status1 {
        self.status1
    }

    /// Overwrites the primary status. `Asleep { turns: 0 }` is stored as
    /// [`Status1::Healthy`]: upstream's cleared sleep bits are a healthy battler.
    pub const fn set_status1(&mut self, status1: Status1) {
        self.status1 = status1.normalised();
    }

    /// Resets stat stages and volatile conditions before returning this
    /// battler to persistent party state.
    ///
    /// [`BattlePokemon::status1`] is deliberately left untouched here,
    /// unlike stages and volatiles: `crates/pokeemerald-rs`'s
    /// `finalize_battle_turn` calls this on the surviving lead mon after
    /// *every* battle outcome, including an ordinary win, and upstream
    /// paralysis or poison outlives a win precisely because nothing but a
    /// faint, a heal, or a cure clears either — only
    /// `Cmd_cleareffectsonfaint`'s `hp == 0`
    /// branch zeroes `status1` (`battle_script_commands.c:3063`-`:3077`), a
    /// narrower precondition than "this battler is leaving battle" that this
    /// one shared method cannot express for both of its callers at once.
    /// [`BattlePokemon::heal`] is the faithful cure path for a survivor; a
    /// corpse's own `hp == 0` clear is its faint-settlement caller's own
    /// explicit [`BattlePokemon::set_status1`] call, right alongside this
    /// method (`crate::battle::execute::Battle::settle_faint`, the
    /// double-faint arm of `crate::battle::execute::pipelines`).
    pub fn clear_battle_scratch(&mut self) {
        self.stages = StatStages::default();
        self.volatiles = Volatiles::default();
    }

    /// Deducts one PP from a move slot.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::InvalidMoveSlot`] when `index` is empty, or
    /// [`BattleError::NoPpRemaining`] when the slot is exhausted.
    pub fn deduct_pp(&mut self, index: usize) -> Result<(), BattleError> {
        self.deduct_pp_by(index, 1)
    }

    /// Deducts `amount` PP from a move slot, saturating at zero rather than
    /// underflowing (`battle_script_commands.c:1234`-`:1237`).
    ///
    /// The caller computes `amount`, incrementing it when the move's target
    /// has Pressure (`battle_script_commands.c:1205`-`:1228`); this method
    /// only applies the reduction.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::InvalidMoveSlot`] when `index` is empty, or
    /// [`BattleError::NoPpRemaining`] when the slot is already exhausted.
    pub(crate) fn deduct_pp_by(&mut self, index: usize, amount: u8) -> Result<(), BattleError> {
        let slot = self
            .moves
            .get_mut(index)
            .ok_or(BattleError::InvalidMoveSlot(index))?;
        if slot.pp == 0 {
            return Err(BattleError::NoPpRemaining(index));
        }
        slot.pp = slot.pp.saturating_sub(amount);
        Ok(())
    }

    /// Restores maximum HP, each move's PP Up-adjusted maximum, and cures
    /// primary status.
    ///
    /// `HealPlayerParty` zeroes `MON_DATA_STATUS` in the same per-mon pass
    /// that restores HP and PP (`script_pokemon_util.c:30`-`:58`), so
    /// [`BattlePokemon::status1`] is cleared here too rather than left to
    /// survive the free heal a caller uses this for.
    ///
    /// # Errors
    ///
    /// Returns the lookup error for any known move missing from `dex`.
    pub fn heal(&mut self, dex: &Dex) -> Result<(), BattleError> {
        self.current_hp = self.stats.max_hp;
        for index in 0..self.moves.len() {
            let full = self.max_pp(dex, index)?;
            self.moves[index].pp = full;
        }
        self.status1 = Status1::default();
        Ok(())
    }
}
