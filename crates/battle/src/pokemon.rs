//! A Pokémon's owned battle state, stat calculation, HP, experience, and
//! move slots.
//!
//! [`BattlePokemon`] does not retain held items or non-volatile status.
//! Shedinja's 1-HP special case (issue #401) *is* modelled — see
//! [`calc_max_hp`].
//!
//! EVs are modelled only as far as issue #415 needs: every mon
//! [`BattlePokemon::new`] builds starts at `0` EVs, [`BattlePokemon::with_evs`]
//! adopts a loaded record's own retained bytes, and [`BattlePokemon::gain_evs`]
//! is `MonGainEVs` (`pokeemerald/src/pokemon.c:5988`), applied on every KO
//! before [`BattlePokemon::apply_experience`] — upstream's own order.
//! Neither the Pokérus nor the Macho Brace doubling applies: this crate
//! carries neither, so every award is upstream's un-multiplied base yield.
//! [`BattlePokemon::stats`] itself stays the `0`-EV formula through every
//! in-battle level-up regardless; only `pokeemerald-rs::party`'s save-time
//! recompute reads [`BattlePokemon::evs`] for the EV-aware block a save
//! needs. [`pp_bonuses`] owns packed PP Up state, while [`learn`] owns
//! level-up move decisions.

use assets::{experience_for_level, AbilityId, BaseStats, EvYield, MoveId, SpeciesId, Type};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::{Nature, Stat};
use crate::stat_stage::StatStage;
use crate::volatile::Volatiles;

pub mod learn;
pub mod pp_bonuses;

#[cfg(test)]
mod tests;

pub use learn::{LearnedMove, MoveLearnDecision, MoveLearnResolution, PendingMoveLearn};
pub use pp_bonuses::{calculate_pp_with_bonus, PpBonuses, MAX_PP_UPS};

/// Maximum number of moves a Pokémon can know.
pub const MAX_MON_MOVES: usize = 4;

/// Empty move-slot marker, which cannot be used as a known move.
pub const MOVE_NONE: MoveId = MoveId(0);

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

/// Largest effort value a single stat can hold (`MAX_PER_STAT_EVS`,
/// `pokeemerald/include/constants/pokemon.h:203`) — [`BattlePokemon::gain_evs`]'s
/// per-stat cap.
pub const MAX_PER_STAT_EVS: u16 = 255;

/// Largest sum of all six effort values (`MAX_TOTAL_EVS`,
/// `pokeemerald/include/constants/pokemon.h:204`) — [`BattlePokemon::gain_evs`]'s
/// whole-mon cap, applied before [`MAX_PER_STAT_EVS`] (upstream's own order).
pub const MAX_TOTAL_EVS: u16 = 510;

/// Individual values for all six stats, each in `0..=`[`MAX_IV`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Ivs {
    /// HP individual value.
    pub hp: u8,
    /// Attack individual value.
    pub attack: u8,
    /// Defense individual value.
    pub defense: u8,
    /// Speed individual value.
    pub speed: u8,
    /// Special Attack individual value.
    pub sp_attack: u8,
    /// Special Defense individual value.
    pub sp_defense: u8,
}

impl Ivs {
    /// Values in HP, Attack, Defense, Speed, Special Attack, Special Defense
    /// order.
    #[must_use]
    pub const fn as_array(self) -> [u8; 6] {
        [
            self.hp,
            self.attack,
            self.defense,
            self.speed,
            self.sp_attack,
            self.sp_defense,
        ]
    }

    const fn first_invalid(self) -> Option<u8> {
        let values = self.as_array();
        let mut index = 0;
        while index < values.len() {
            if values[index] > MAX_IV {
                return Some(values[index]);
            }
            index += 1;
        }
        None
    }

    /// Whether every value is in `0..=`[`MAX_IV`].
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.first_invalid().is_none()
    }
}

/// Stored effort values for all six stats.
///
/// Each byte accepts `0..=255`. The stat formula divides by four, so values
/// `252..=255` all provide the maximum contribution. [`BattlePokemon::evs`]'s
/// type (issue #415), and also the standalone value [`compute_stats_with_evs`]
/// takes for the one caller outside this crate with real EVs and no battler
/// of its own to attach them to — `party::merge_into_save_pokemon`,
/// recomputing a levelled-up stat block from a save record's own bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Evs {
    /// HP effort value.
    pub hp: u8,
    /// Attack effort value.
    pub attack: u8,
    /// Defense effort value.
    pub defense: u8,
    /// Speed effort value.
    pub speed: u8,
    /// Special Attack effort value.
    pub sp_attack: u8,
    /// Special Defense effort value.
    pub sp_defense: u8,
}

impl Evs {
    /// Values in HP, Attack, Defense, Speed, Special Attack, Special Defense
    /// order — the same order [`Ivs::as_array`] uses.
    #[must_use]
    pub const fn as_array(self) -> [u8; 6] {
        [
            self.hp,
            self.attack,
            self.defense,
            self.speed,
            self.sp_attack,
            self.sp_defense,
        ]
    }
}

/// Computed battle stats before applying [`StatStages`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stats {
    /// Maximum HP.
    pub max_hp: u32,
    /// Attack.
    pub attack: u32,
    /// Defense.
    pub defense: u32,
    /// Speed.
    pub speed: u32,
    /// Special Attack.
    pub sp_attack: u32,
    /// Special Defense.
    pub sp_defense: u32,
}

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

const STAT_FORMULA_SCALE: u32 = 100;
const EFFORT_VALUES_PER_STAT_POINT: u32 = 4;
const NON_HP_STAT_OFFSET: u32 = 5;
const HP_STAT_OFFSET: u32 = 10;
const SHEDINJA_MAX_HP: u32 = 1;

fn calc_stat(
    base: u8,
    individual_value: u8,
    effort_value: u8,
    level: u8,
    nature: Nature,
    stat: Stat,
) -> u32 {
    let effort_contribution = u32::from(effort_value) / EFFORT_VALUES_PER_STAT_POINT;
    let scaled_stat = (2 * u32::from(base) + u32::from(individual_value) + effort_contribution)
        * u32::from(level)
        / STAT_FORMULA_SCALE;
    nature.modify_stat(stat, scaled_stat + NON_HP_STAT_OFFSET)
}

fn calc_max_hp(
    species: SpeciesId,
    base: u8,
    individual_value: u8,
    effort_value: u8,
    level: u8,
) -> u32 {
    if species == SPECIES_SHEDINJA {
        // Shedinja bypasses every ordinary HP input (`src/pokemon.c:2845-2848`).
        return SHEDINJA_MAX_HP;
    }
    let effort_contribution = u32::from(effort_value) / EFFORT_VALUES_PER_STAT_POINT;
    let scaled_stat = (2 * u32::from(base) + u32::from(individual_value) + effort_contribution)
        * u32::from(level)
        / STAT_FORMULA_SCALE;
    scaled_stat + u32::from(level) + HP_STAT_OFFSET
}

/// Computes battle stats with zero EVs.
#[must_use]
pub fn compute_stats(
    species: SpeciesId,
    base: &BaseStats,
    level: u8,
    nature: Nature,
    ivs: Ivs,
) -> Stats {
    compute_stats_with_evs(species, base, level, nature, ivs, Evs::default())
}

/// Computes battle stats with explicit EVs.
#[must_use]
pub fn compute_stats_with_evs(
    species: SpeciesId,
    base: &BaseStats,
    level: u8,
    nature: Nature,
    ivs: Ivs,
    evs: Evs,
) -> Stats {
    Stats {
        max_hp: calc_max_hp(species, base.hp, ivs.hp, evs.hp, level),
        attack: calc_stat(
            base.attack,
            ivs.attack,
            evs.attack,
            level,
            nature,
            Stat::Attack,
        ),
        defense: calc_stat(
            base.defense,
            ivs.defense,
            evs.defense,
            level,
            nature,
            Stat::Defense,
        ),
        speed: calc_stat(base.speed, ivs.speed, evs.speed, level, nature, Stat::Speed),
        sp_attack: calc_stat(
            base.sp_attack,
            ivs.sp_attack,
            evs.sp_attack,
            level,
            nature,
            Stat::SpAttack,
        ),
        sp_defense: calc_stat(
            base.sp_defense,
            ivs.sp_defense,
            evs.sp_defense,
            level,
            nature,
            Stat::SpDefense,
        ),
    }
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

/// A single battler's owned state.
///
/// Construction guarantees a valid species, level, IV set, and non-empty
/// moveset of at most [`MAX_MON_MOVES`] real moves. Methods preserve those
/// invariants while changing battle state.
#[derive(Debug, Clone)]
pub struct BattlePokemon {
    species: SpeciesId,
    level: u8,
    /// [`BattlePokemon::new`]'s own `level` argument, fixed for this
    /// instance's lifetime (issue #415) — the one signal
    /// `pokeemerald-rs::party`'s save encoders have for whether a level-up
    /// happened since this value was last known-good, since
    /// [`BattlePokemon::level`] alone cannot say so (it already tracks
    /// [`BattlePokemon::experience`] at every point past construction,
    /// crossed or not). See `party::to_save_pokemon` for what reads this.
    ///
    /// Excluded from [`PartialEq`] (below): construction-provenance
    /// bookkeeping, not part of the Pokémon's own battle identity.
    created_at_level: u8,
    nature: Nature,
    ivs: Ivs,
    personality: u32,
    ability_slot: u8,
    original_trainer_id: u32,
    types: [Type; 2],
    base_stats: BaseStats,
    experience: u32,
    stats: Stats,
    current_hp: u32,
    /// This mon's effort values (issue #415) — `0` for every mon
    /// [`BattlePokemon::new`] builds, a loaded record's own retained bytes
    /// for one restored through [`BattlePokemon::with_evs`], incremented in
    /// place by [`BattlePokemon::gain_evs`] on every KO. See the module docs
    /// for what carrying this value does, and does not, change.
    evs: Evs,
    moves: Vec<MoveSlot>,
    pp_bonuses: PpBonuses,
    stages: StatStages,
    volatiles: Volatiles,
    pending_move_learn: Option<PendingMoveLearn>,
}

// Manual, rather than derived, so `created_at_level` (this field's own doc
// comment) can stay out of it without a wrapper type.
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
            types,
            base_stats,
            experience,
            stats,
            current_hp,
            evs,
            moves,
            pp_bonuses,
            stages,
            volatiles,
            pending_move_learn,
        } = self;
        *species == other.species
            && *level == other.level
            && *nature == other.nature
            && *ivs == other.ivs
            && *personality == other.personality
            && *ability_slot == other.ability_slot
            && *original_trainer_id == other.original_trainer_id
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
        let stats = compute_stats(species, base, level, nature, ivs);
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
            types: base.types,
            base_stats: *base,
            experience,
            stats,
            current_hp: stats.max_hp,
            // `CreateBoxMon` zeroes the box before writing the fields it
            // sets, and the EV bytes are not one of them: a freshly built
            // mon has no effort values. A saved one restores its own bytes
            // through [`BattlePokemon::with_evs`] (issue #415).
            evs: Evs::default(),
            moves: slots,
            pp_bonuses: PpBonuses::NONE,
            stages: StatStages::default(),
            volatiles: Volatiles::default(),
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

    /// Restores a saved ability slot, masking the input to one bit.
    #[must_use]
    pub const fn with_ability_slot(mut self, ability_slot: u8) -> Self {
        self.ability_slot = ability_slot & ABILITY_SLOT_MASK;
        self
    }

    /// This mon's effort values (issue #415) — see the field's own docs and
    /// the module docs for what carrying this value does, and does not,
    /// change.
    #[must_use]
    pub const fn evs(&self) -> Evs {
        self.evs
    }

    /// Adopts a saved record's own EV bytes at the boundary that restores
    /// this Pokémon — the same position as
    /// [`BattlePokemon::with_original_trainer_id`], immediately after
    /// [`BattlePokemon::new`], which otherwise leaves every mon at `0` EVs.
    ///
    /// Deliberately does **not** recompute [`BattlePokemon::stats`]: the live
    /// stat cache stays the `0`-EV formula for the whole battle, through
    /// every level-up (see [`BattlePokemon::raise_level_to_experience`]) —
    /// the same posture [`BattlePokemon::with_ability_slot`] and
    /// [`BattlePokemon::with_pp_bonuses`] take toward fields upstream's load
    /// path never recomputes either. Every byte is accepted: upstream's EV
    /// fields are unconstrained `u8`s — [`MAX_PER_STAT_EVS`] and
    /// [`MAX_TOTAL_EVS`] bound only what [`BattlePokemon::gain_evs`] writes,
    /// not what a hand-edited or pre-#415 save can already hold.
    #[must_use]
    pub const fn with_evs(mut self, evs: Evs) -> Self {
        self.evs = evs;
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

    /// Resets stat stages and volatile conditions before returning this
    /// battler to persistent party state.
    pub fn clear_battle_scratch(&mut self) {
        self.stages = StatStages::default();
        self.volatiles = Volatiles::default();
    }

    /// `MonGainEVs` (`pokeemerald/src/pokemon.c:5988`-`:6064`), restricted to
    /// this crate's own scope (issue #415): adds `ev_yield`'s per-stat award
    /// to [`BattlePokemon::evs`], capping the running total at
    /// [`MAX_TOTAL_EVS`] before each stat's own value at
    /// [`MAX_PER_STAT_EVS`] — upstream's own order. The loop stops entirely,
    /// not just for the current stat, once the running total reaches
    /// [`MAX_TOTAL_EVS`], exactly as upstream's `break` does. Neither the
    /// Pokérus nor the Macho Brace doubling applies: this crate carries
    /// neither, so every award is upstream's un-multiplied base yield.
    ///
    /// Called from [`crate::battle::Battle::settle_win_reward`] on every KO,
    /// **before** [`BattlePokemon::apply_experience`] — upstream's own order
    /// (`Cmd_getexp` case 2's `MonGainEVs` call precedes case 3's stat
    /// recompute). [`BattlePokemon::stats`] never reads the result (see
    /// [`BattlePokemon::raise_level_to_experience`]); what the ordering buys
    /// is that [`BattlePokemon::evs`] already carries this KO's gain by the
    /// time `pokeemerald-rs::party`'s save-time recompute reads it back.
    pub fn gain_evs(&mut self, ev_yield: EvYield) {
        let yields = [
            ev_yield.hp,
            ev_yield.attack,
            ev_yield.defense,
            ev_yield.speed,
            ev_yield.sp_attack,
            ev_yield.sp_defense,
        ];
        let mut evs = self.evs.as_array();
        let mut total: u16 = evs.iter().copied().map(u16::from).sum();
        for (ev, stat_yield) in evs.iter_mut().zip(yields) {
            if total >= MAX_TOTAL_EVS {
                break;
            }
            let mut increase = u16::from(stat_yield);
            if total + increase > MAX_TOTAL_EVS {
                increase = MAX_TOTAL_EVS - total;
            }
            if u16::from(*ev) + increase > MAX_PER_STAT_EVS {
                increase = MAX_PER_STAT_EVS - u16::from(*ev);
            }
            *ev = u8::try_from(u16::from(*ev) + increase).unwrap_or(u8::MAX);
            total += increase;
        }
        self.evs = Evs {
            hp: evs[0],
            attack: evs[1],
            defense: evs[2],
            speed: evs[3],
            sp_attack: evs[4],
            sp_defense: evs[5],
        };
    }

    /// Applies earned experience one level threshold at a time, capping both
    /// level and total experience at [`MAX_LEVEL`]. Each crossed level
    /// recalculates stats, preserves damage taken, and teaches that level's
    /// complete learnset without filtering unsupported moves — a move this
    /// crate cannot execute yet is still learned, exactly like upstream (see
    /// [`BattlePokemon::walk_level_learnset`]). A full moveset pauses the
    /// award and returns a [`PendingMoveLearn`]; the caller must pass its
    /// decision to [`BattlePokemon::resolve_move_learn`] before applying more
    /// experience. [`BattlePokemon::evs`] is not changed here —
    /// [`BattlePokemon::gain_evs`] already folds a KO's award in before this
    /// runs (issue #415); friendship is still not part of this battle model
    /// and does not change.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::MoveLearnPending`] without mutation when a move
    /// decision is already pending — a second walk would overwrite the open
    /// question and drop its unconsumed remainder.
    #[must_use = "a full moveset pauses the level-up walk for a player \
                  decision the mon now carries \
                  (`BattlePokemon::pending_move_learn`); ignoring the \
                  report means never asking the player"]
    pub fn apply_experience(
        &mut self,
        dex: &Dex,
        amount: u32,
    ) -> Result<Option<PendingMoveLearn>, BattleError> {
        if let Some(pending) = self.pending_move_learn {
            return Err(BattleError::MoveLearnPending(pending.move_id()));
        }
        self.pending_move_learn = self.advance_experience(dex, amount);
        Ok(self.pending_move_learn)
    }

    /// Reconciles saved experience without teaching crossed-level moves.
    ///
    /// Experience is clamped between the current value and the level-100
    /// threshold. The level can rise to match it but never decreases.
    pub fn reconcile_saved_experience(&mut self, total: u32) {
        let max_experience =
            experience_for_level(self.base_stats.growth_rate, MAX_LEVEL).unwrap_or(u32::MAX);
        self.experience = self.experience.max(total.min(max_experience));
        self.raise_level_to_experience();
    }

    /// Raises the level (and stats, preserving damage taken) to match the
    /// current experience total, returning `Some((old_level, new_level))`
    /// when at least one threshold was crossed. Shared by the in-battle
    /// award ([`BattlePokemon::apply_experience`], which then teaches the
    /// crossed learnset moves) and the save decoder
    /// ([`BattlePokemon::reconcile_saved_experience`], which must not).
    ///
    /// The stat recompute stays [`compute_stats`], not
    /// [`compute_stats_with_evs`] fed [`BattlePokemon::evs`], even for a mon
    /// carrying adopted EVs ([`BattlePokemon::with_evs`]):
    /// `pokeemerald-rs::party`'s load-clamp rebase depends on
    /// [`BattlePokemon::stats`] staying the `0`-EV floor for the whole battle
    /// to measure how far a save-file maximum sits above it. Feeding real EVs
    /// in here only after a level-up would add the hidden EV gap on top of an
    /// already-real `current_hp`, silently healing away damage taken.
    /// [`BattlePokemon::gain_evs`]'s own award still reaches the save file —
    /// `pokeemerald-rs::party` reads [`BattlePokemon::evs`] directly for its
    /// own save-time recompute.
    fn raise_level_to_experience(&mut self) -> Option<(u8, u8)> {
        let mut new_level = self.level;
        while new_level < MAX_LEVEL {
            let next_level = new_level + 1;
            let threshold =
                experience_for_level(self.base_stats.growth_rate, next_level).unwrap_or(u32::MAX);
            if self.experience < threshold {
                break;
            }
            new_level = next_level;
        }
        if new_level == self.level {
            return None;
        }

        let old_level = self.level;
        let old_max_hp = self.stats.max_hp;
        self.level = new_level;
        self.stats = compute_stats(
            self.species,
            &self.base_stats,
            self.level,
            self.nature,
            self.ivs,
        );
        self.current_hp = self
            .current_hp
            .saturating_add(self.stats.max_hp.saturating_sub(old_max_hp));
        Some((old_level, new_level))
    }

    /// Attacking stat and stage for the move category.
    #[must_use]
    pub fn attacking_stat(&self, category: crate::damage::MoveCategory) -> (u32, StatStage) {
        match category {
            crate::damage::MoveCategory::Physical => (self.stats.attack, self.stages.attack),
            crate::damage::MoveCategory::Special => (self.stats.sp_attack, self.stages.sp_attack),
        }
    }

    /// Defending stat and stage for the move category.
    #[must_use]
    pub fn defending_stat(&self, category: crate::damage::MoveCategory) -> (u32, StatStage) {
        match category {
            crate::damage::MoveCategory::Physical => (self.stats.defense, self.stages.defense),
            crate::damage::MoveCategory::Special => (self.stats.sp_defense, self.stages.sp_defense),
        }
    }

    /// Speed after applying its stat stage.
    ///
    /// Weather, ability, item, and paralysis modifiers are not applied here.
    #[must_use]
    pub const fn effective_speed(&self) -> u32 {
        self.stages.speed.apply(self.stats.speed)
    }

    /// Deducts one PP from a move slot.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::InvalidMoveSlot`] when `index` is empty, or
    /// [`BattleError::NoPpRemaining`] when the slot is exhausted.
    pub fn deduct_pp(&mut self, index: usize) -> Result<(), BattleError> {
        let slot = self
            .moves
            .get_mut(index)
            .ok_or(BattleError::InvalidMoveSlot(index))?;
        if slot.pp == 0 {
            return Err(BattleError::NoPpRemaining(index));
        }
        slot.pp -= 1;
        Ok(())
    }

    /// Restores maximum HP and each move's PP Up-adjusted maximum.
    ///
    /// Non-volatile status is not changed because [`BattlePokemon`] does not
    /// model it.
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
        Ok(())
    }
}
