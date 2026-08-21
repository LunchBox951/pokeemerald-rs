//! A single Pokémon's in-battle representation (S-6): computed battle
//! stats, current HP, moveset PP, and stat stages.
//!
//! [`BattlePokemon`] is this crate's owned answer to upstream's
//! `struct Pokemon` + `struct BattlePokemon` (`pokeemerald/include/pokemon.h`,
//! `include/battle.h`) as far as issue #159's turn engine needs them: it
//! carries already-computed stats (via [`compute_stats`], the
//! `CalculateMonStats`/`CALC_STAT` formula, `pokeemerald/src/pokemon.c:2814`)
//! rather than the encrypted save-file byte layout
//! ([`engine::save::pokemon::Pokemon`] owns that boundary; this crate does
//! not depend on `engine` — see [`crate::wild`] for why).
//!
//! Out of scope for this slice: EV tracking (every mon this crate builds has
//! `0` EVs — matching a freshly caught wild mon and an EV-less starting
//! player mon, both realistic for a first encounter), held items,
//! non-volatile status conditions, and the Shedinja 1-HP special case in
//! `CalculateMonStats`. Abilities are out of scope too, with one exception:
//! [`BattlePokemon::ability`] (issues #321/#322), consumed by
//! [`crate::stat_change`]'s ability guards (Clear Body, White Smoke, Keen
//! Eye, Hyper Cutter) and by [`crate::ability`]'s two damage-formula
//! checks (Overgrow's pinch boost, Liquid Ooze's drain inversion) — see
//! those modules' docs for why the ability system stops there.
//!
//! Two concerns live in sibling modules rather than here, each because it is
//! its own concept `(oop-boundaries)`: [`pp_bonuses`] owns the packed
//! `ppBonuses` byte and `CalculatePPWithBonus`, and [`learn`] owns the
//! level-up learnset walk and the player decision a full moveset pauses for.

use assets::{experience_for_level, AbilityId, BaseStats, MoveId, SpeciesId, Type};

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

/// `MAX_MON_MOVES` (`pokeemerald/include/constants/global.h:82`): the most
/// moves a Pokémon can know at once.
pub const MAX_MON_MOVES: usize = 4;

/// `MOVE_NONE` (`pokeemerald/include/constants/moves.h:4`): the placeholder
/// occupying an *unfilled* `gBattleMons[].moves[]` slot, not a move a battler
/// can ever use. `CheckMoveLimitations` marks such a slot unselectable
/// (`MOVE_LIMITATION_ZEROMOVE`, `pokeemerald/src/battle_util.c:1098`) and the
/// wild opponent's own rejection loop retries while its pick lands on one
/// (`src/battle_controller_opponent.c:1599`-`:1601`), so a known move is never
/// `MOVE_NONE` — which is why [`BattlePokemon::new`] refuses one.
pub const MOVE_NONE: MoveId = MoveId(0);

/// `SPECIES_NONE` (`pokeemerald/include/constants/species.h:4`): the reserved
/// zero id whose `gSpeciesInfo` row is an all-zero placeholder, not a real
/// species. Upstream never builds a mon from it, so [`BattlePokemon::new`]
/// refuses it ([`BattleError::PlaceholderSpecies`]) rather than turning the
/// empty row's zero base stats into a fightable battler.
pub const SPECIES_NONE: SpeciesId = SpeciesId(0);

/// `SPECIES_OLD_UNOWN_B` (`pokeemerald/include/constants/species.h:257`):
/// first id of the reserved Gen-2 compatibility hole between Celebi and
/// Treecko. The 25 old-Unown rows (`..=`[`SPECIES_OLD_UNOWN_Z`]) hold the
/// leftover dummy stat block (`OLD_UNOWN_SPECIES_INFO`,
/// `src/data/pokemon/species_info.h:5`), and no upstream path — encounter
/// table, gift, or trade — ever produces one, so [`BattlePokemon::new`]
/// refuses the whole range like [`SPECIES_NONE`].
pub const SPECIES_OLD_UNOWN_B: SpeciesId = SpeciesId(252);

/// `SPECIES_OLD_UNOWN_Z` (`pokeemerald/include/constants/species.h:281`):
/// last id of the reserved old-Unown range — see [`SPECIES_OLD_UNOWN_B`].
pub const SPECIES_OLD_UNOWN_Z: SpeciesId = SpeciesId(276);

/// `MIN_LEVEL` (`pokeemerald/include/constants/pokemon.h:145`).
pub const MIN_LEVEL: u8 = 1;

/// `MAX_LEVEL` (`pokeemerald/include/constants/pokemon.h:146`).
pub const MAX_LEVEL: u8 = 100;

/// `MAX_IV_MASK` (`pokeemerald/include/constants/pokemon.h:201`): the largest
/// value any single individual value can take.
pub const MAX_IV: u8 = 31;

/// The six Pokémon **individual values** (`0..=`[`MAX_IV`]) rolled for a
/// Pokémon (`MAX_IV_MASK`, `pokeemerald/include/constants/pokemon.h:201`).
///
/// "IV" here is the Pokémon Gen-3 game-mechanics term — a per-stat genetic
/// roll feeding [`compute_stats`] — and has **nothing to do with a
/// cryptographic initialization vector**. Nothing in this crate is
/// cryptographic; the `31`s that appear around this type are the game's
/// maximum stat roll, not key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Ivs {
    /// HP IV.
    pub hp: u8,
    /// Attack IV.
    pub attack: u8,
    /// Defense IV.
    pub defense: u8,
    /// Speed IV.
    pub speed: u8,
    /// Sp. Attack IV.
    pub sp_attack: u8,
    /// Sp. Defense IV.
    pub sp_defense: u8,
}

impl Ivs {
    /// The six values in `CreateBoxMon`'s own draw order
    /// (`pokeemerald/src/pokemon.c:2276`: HP/Attack/Defense from the first
    /// draw, Speed/Sp. Attack/Sp. Defense from the second).
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

    /// Whether every value is within the upstream `0..=`[`MAX_IV`] range.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        let [hp, attack, defense, speed, sp_attack, sp_defense] = self.as_array();
        hp <= MAX_IV
            && attack <= MAX_IV
            && defense <= MAX_IV
            && speed <= MAX_IV
            && sp_attack <= MAX_IV
            && sp_defense <= MAX_IV
    }
}

/// A Pokémon's final computed battle stats — the `CalculateMonStats` output
/// (`pokeemerald/src/pokemon.c:2823`), pre-stage (stat stages are tracked
/// separately, see [`StatStages`], and applied by the damage/turn-order
/// formulas that consume these values).
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
    /// Sp. Attack.
    pub sp_attack: u32,
    /// Sp. Defense.
    pub sp_defense: u32,
}

/// The seven in-battle stat stages (`gBattleMons[].statStages`,
/// `pokeemerald/include/battle.h`): the five [`Stat`]s plus accuracy and
/// evasion (which [`Nature`] never modifies, so they live outside
/// [`crate::nature::Stat`]).
///
/// All start [`StatStage::NEUTRAL`] (upstream `DEFAULT_STAT_STAGE`). No move
/// in this slice's v1 path set changes them (`Growl`/`Tail Whip`-style
/// stat-changing effects are deferred — see the crate root docs), but the
/// damage/accuracy/crit formulas already thread these through, so a caller
/// can construct a non-neutral [`BattlePokemon`] directly to exercise that
/// path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatStages {
    /// Attack stage.
    pub attack: StatStage,
    /// Defense stage.
    pub defense: StatStage,
    /// Speed stage.
    pub speed: StatStage,
    /// Sp. Attack stage.
    pub sp_attack: StatStage,
    /// Sp. Defense stage.
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
    /// PP remaining. A freshly learned move starts at the move's own base PP
    /// (`gBattleMoves[move].pp`); the slot's *capacity* is
    /// [`BattlePokemon::max_pp`], which adds the PP Ups recorded in the
    /// mon's [`PpBonuses`] — capacity belongs to the mon, exactly as
    /// upstream's single `ppBonuses` byte does (see [`pp_bonuses`]).
    pub pp: u8,
}

/// `CALC_STAT`'s core (`pokeemerald/src/pokemon.c:2814`-`:2821`), whose
/// arithmetic line is exactly
/// `s32 n = (((2 * baseStat + iv + ev / 4) * level) / 100) + 5;` (`:2817`),
/// followed by `ModifyStatByNature` (`:2819`) — [`Nature::modify_stat`].
///
/// The `ev / 4` term is **absent from the expression below** rather than
/// written as a constant `0`: every mon this slice builds has `0` EVs (see
/// the module docs), and `ev / 4` is its own integer division whose result
/// (`0`) is added *before* the `* level` and `/ 100` steps, so dropping it
/// cannot change the value or the division order `(behavioral-fidelity)`.
/// Whenever EV tracking arrives, the term has to come back at that exact
/// position — inside the parenthesised sum, not folded into `n`.
fn calc_stat(base: u8, individual_value: u8, level: u8, nature: Nature, stat: Stat) -> u32 {
    let n = (2 * u32::from(base) + u32::from(individual_value)) * u32::from(level) / 100 + 5;
    nature.modify_stat(stat, n)
}

/// The max-HP half of `CalculateMonStats` (`pokeemerald/src/pokemon.c:2851`):
/// `(((2*base + iv) * level) / 100) + level + 10`. HP is never nature-modified
/// (`ModifyStatByNature` special-cases `statIndex <= STAT_HP`). The Shedinja
/// 1-HP special case (`species == SPECIES_SHEDINJA`) is not modelled.
fn calc_max_hp(base: u8, individual_value: u8, level: u8) -> u32 {
    let n = 2 * u32::from(base) + u32::from(individual_value);
    (n * u32::from(level)) / 100 + u32::from(level) + 10
}

/// Compute a Pokémon's final battle stats from its base stats, level,
/// nature, and IVs — `CalculateMonStats` (`pokeemerald/src/pokemon.c:2823`)
/// with EVs fixed at `0` (see the module docs).
#[must_use]
pub fn compute_stats(base: &BaseStats, level: u8, nature: Nature, ivs: Ivs) -> Stats {
    Stats {
        max_hp: calc_max_hp(base.hp, ivs.hp, level),
        attack: calc_stat(base.attack, ivs.attack, level, nature, Stat::Attack),
        defense: calc_stat(base.defense, ivs.defense, level, nature, Stat::Defense),
        speed: calc_stat(base.speed, ivs.speed, level, nature, Stat::Speed),
        sp_attack: calc_stat(base.sp_attack, ivs.sp_attack, level, nature, Stat::SpAttack),
        sp_defense: calc_stat(
            base.sp_defense,
            ivs.sp_defense,
            level,
            nature,
            Stat::SpDefense,
        ),
    }
}

/// A single battler's owned in-battle state `(oop-boundaries)`.
///
/// Every field is private and reached through a method: the constructor is
/// the only way in, and it is what enforces the invariants the battle engine
/// relies on — a level in [`MIN_LEVEL`]`..=`[`MAX_LEVEL`], IVs in
/// `0..=`[`MAX_IV`], and a moveset of `1..=`[`MAX_MON_MOVES`] real (never
/// [`MOVE_NONE`]) moves. In-battle mutation is limited to the operations that
/// preserve them: [`BattlePokemon::apply_damage`],
/// [`BattlePokemon::apply_experience`], [`BattlePokemon::resolve_move_learn`]
/// (which swaps one slot for a move the learnset named, never widening the
/// moveset past [`MAX_MON_MOVES`]),
/// [`BattlePokemon::deduct_pp`], and [`BattlePokemon::stages_mut`] (a
/// [`StatStage`] is itself a constrained type, so no invariant of *this* type
/// can be broken through it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattlePokemon {
    species: SpeciesId,
    level: u8,
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
    moves: Vec<MoveSlot>,
    /// `mon->ppBonuses` / `gBattleMons[].ppBonuses`
    /// (`pokeemerald/include/pokemon.h:104`, `:288`): how many PP Ups each
    /// slot carries. One byte for the whole mon, not a per-slot field, for
    /// the reasons [`pp_bonuses`] gives.
    pp_bonuses: PpBonuses,
    stages: StatStages,
    volatiles: Volatiles,
}

impl BattlePokemon {
    /// Every check [`BattlePokemon::new`] makes that does **not** depend on
    /// the nature/personality/IVs — i.e. everything checkable from
    /// caller-supplied inputs alone, before any of those are rolled.
    ///
    /// Exposed so a builder that *generates* the rolled fields
    /// ([`crate::wild::build_wild_pokemon`]) can reject bad inputs before its
    /// first `Random()` call: a rejected request must not advance the shared
    /// RNG stream `(behavioral-fidelity)`.
    ///
    /// # Errors
    ///
    /// [`BattleError::InvalidLevel`], [`BattleError::InvalidMoveCount`],
    /// [`BattleError::PlaceholderMove`], [`BattleError::PlaceholderSpecies`],
    /// [`BattleError::UnknownSpecies`], or [`BattleError::UnknownMove`] — see
    /// [`BattlePokemon::new`] for what each means. (IV range is the one check
    /// missing here, since IVs may not exist yet at the point this is called.)
    pub fn validate(
        dex: &Dex,
        species: SpeciesId,
        level: u8,
        moves: &[MoveId],
    ) -> Result<(), BattleError> {
        if !(MIN_LEVEL..=MAX_LEVEL).contains(&level) {
            return Err(BattleError::InvalidLevel(level));
        }
        if moves.is_empty() || moves.len() > MAX_MON_MOVES {
            return Err(BattleError::InvalidMoveCount(moves.len()));
        }
        if let Some(index) = moves.iter().position(|m| *m == MOVE_NONE) {
            return Err(BattleError::PlaceholderMove(index));
        }
        if species == SPECIES_NONE
            || (SPECIES_OLD_UNOWN_B.0..=SPECIES_OLD_UNOWN_Z.0).contains(&species.0)
        {
            return Err(BattleError::PlaceholderSpecies);
        }
        dex.species(species)?;
        for move_id in moves {
            dex.move_data(*move_id)?;
        }
        Ok(())
    }

    /// Build a full-HP battler at the given species/level/IVs/moves.
    ///
    /// The nature is **derived from `personality`**
    /// ([`Nature::from_personality`], upstream `GetNatureFromPersonality` —
    /// `personality % NUM_NATURES`, `pokeemerald/src/pokemon.c:5498`) rather
    /// than accepted separately: upstream never stores a nature, so a
    /// battler whose nature contradicts its personality cannot exist there
    /// and is unrepresentable here too. Callers that want a *specific*
    /// nature supply a personality that derives it, exactly as upstream's
    /// `CreateMonWithNature` does ([`crate::wild::roll_personality_for_nature`]).
    ///
    /// `level` must be in [`MIN_LEVEL`]`..=`[`MAX_LEVEL`], every IV in
    /// `0..=`[`MAX_IV`], and `moves` must be `1..=`[`MAX_MON_MOVES`] real
    /// moves; PP for each slot starts at the move's base PP
    /// (`dex.move_data(id)?.pp`).
    ///
    /// # Errors
    ///
    /// - [`BattleError::InvalidLevel`] if `level` is outside
    ///   `MIN_LEVEL..=MAX_LEVEL` (`pokeemerald/include/constants/pokemon.h:145`-`:146`)
    ///   — `CalculateMonStats` is never handed one, and an out-of-range level
    ///   feeds straight into the stat and damage formulas.
    /// - [`BattleError::InvalidIv`] if any IV exceeds [`MAX_IV`]
    ///   (`MAX_IV_MASK`, `include/constants/pokemon.h:201`): upstream stores
    ///   IVs in 5-bit fields, so a larger value is unrepresentable there.
    /// - [`BattleError::InvalidMoveCount`] if `moves` is empty or longer than
    ///   [`MAX_MON_MOVES`] — neither is representable upstream, and the
    ///   non-empty half is what makes the wild opponent's move-choice
    ///   rejection loop terminate (see
    ///   `crate::battle::opponent_ai::choose_enemy_move`).
    /// - [`BattleError::PlaceholderMove`] if any slot is [`MOVE_NONE`], the
    ///   empty-slot placeholder (see that constant's docs).
    /// - [`BattleError::PlaceholderSpecies`] if `species` is [`SPECIES_NONE`]
    ///   (the reserved all-zero `gSpeciesInfo` row) or falls in the reserved
    ///   old-Unown range [`SPECIES_OLD_UNOWN_B`]`..=`[`SPECIES_OLD_UNOWN_Z`]
    ///   (see those constants' docs).
    /// - [`BattleError::UnknownSpecies`] / [`BattleError::UnknownMove`] if
    ///   `species`/any of `moves` is not in `dex`.
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
        if !ivs.is_valid() {
            let offender = ivs
                .as_array()
                .into_iter()
                .find(|individual_value| *individual_value > MAX_IV)
                .unwrap_or(MAX_IV);
            return Err(BattleError::InvalidIv(offender));
        }
        let base = dex.species(species)?;
        let experience = experience_for_level(base.growth_rate, level)
            .map_err(|_| BattleError::InvalidLevel(level))?;
        let stats = compute_stats(base, level, nature, ivs);
        let mut slots = Vec::with_capacity(moves.len());
        for move_id in moves {
            let pp = dex.move_data(move_id)?.pp;
            slots.push(MoveSlot { move_id, pp });
        }
        Ok(Self {
            species,
            level,
            nature,
            ivs,
            personality,
            // `CreateBoxMon` (`pokeemerald/src/pokemon.c:2296`-`:2300`):
            // `abilityNum = personality & 1`, written only when the species
            // has a second ability — a one-ability species keeps the bit
            // clear, and so does this field, keeping the serialized save
            // bit (`party::to_save_pokemon`'s misc-word bit 31) exactly
            // what Emerald would have stored. Stored once at creation time
            // rather than re-derived on every read: a save round trip (the
            // `pokeemerald-rs` crate's `party` module, which owns the
            // `battle`/`engine::save` boundary) can hand back a slot that
            // disagrees with this default, via [`Self::with_ability_slot`].
            ability_slot: u8::from(base.abilities[1].0 != 0 && personality & 1 != 0),
            original_trainer_id: 0,
            types: base.types,
            base_stats: *base,
            experience,
            stats,
            current_hp: stats.max_hp,
            moves: slots,
            // `CreateBoxMon` zeroes the box before writing the fields it
            // sets, and `ppBonuses` is not one of them: a freshly built mon
            // has no PP Ups. A saved one restores its own byte through
            // [`BattlePokemon::with_pp_bonuses`].
            pp_bonuses: PpBonuses::NONE,
            stages: StatStages::default(),
            volatiles: Volatiles::default(),
        })
    }

    /// Adopt a saved `ppBonuses` byte (`MON_DATA_PP_BONUSES`) at the
    /// boundary that restores this Pokémon, refilling every known slot to
    /// its new [`BattlePokemon::max_pp`].
    ///
    /// A construction-time builder like
    /// [`BattlePokemon::with_original_trainer_id`], and meant for the same
    /// position: immediately after [`BattlePokemon::new`], which leaves
    /// every slot full at its *base* PP. Refilling is what makes those slots
    /// full at the PP-Up-adjusted maximum instead, so the save decoder can
    /// then spend the difference down to the saved value through
    /// [`BattlePokemon::deduct_pp`] and land on the right remaining PP for a
    /// mon that really does hold PP Ups.
    ///
    /// Every byte is accepted: the two-bit fields cannot overflow, so there
    /// is no invalid `ppBonuses` to reject (see [`PpBonuses`]).
    ///
    /// # Errors
    ///
    /// Whatever [`crate::dex::Dex::move_data`] reports for a move this mon
    /// already knows — unreachable, since [`BattlePokemon::new`] resolved
    /// every slot through that same lookup.
    pub fn with_pp_bonuses(mut self, dex: &Dex, bonuses: PpBonuses) -> Result<Self, BattleError> {
        self.pp_bonuses = bonuses;
        for index in 0..self.moves.len() {
            let full = self.max_pp(dex, index)?;
            self.moves[index].pp = full;
        }
        Ok(self)
    }

    /// The species this mon was built from.
    #[must_use]
    pub const fn species(&self) -> SpeciesId {
        self.species
    }

    /// Current level ([`MIN_LEVEL`]`..=`[`MAX_LEVEL`], enforced by
    /// [`BattlePokemon::new`]).
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }

    /// Total accumulated experience on this species' growth curve.
    #[must_use]
    pub const fn experience(&self) -> u32 {
        self.experience
    }

    /// This mon's nature — always `personality % 25`
    /// ([`Nature::from_personality`]), derived at construction.
    #[must_use]
    pub const fn nature(&self) -> Nature {
        self.nature
    }

    /// This mon's individual values (the Gen-3 stat rolls — see [`Ivs`]).
    #[must_use]
    pub const fn ivs(&self) -> Ivs {
        self.ivs
    }

    /// Personality value. [`BattlePokemon::nature`] is derived from it at
    /// construction; upstream also derives gender/shininess/unown letter
    /// from it, none of which this slice consumes.
    #[must_use]
    pub const fn personality(&self) -> u32 {
        self.personality
    }

    /// This mon's ability: the species' two-slot ability table indexed
    /// with [`BattlePokemon::ability_slot`], upstream's
    /// `GetAbilityBySpecies` (`pokeemerald/src/pokemon.c:4546`-`:4554`).
    /// The slot-0 fallback for a single-ability species is this port's own
    /// hardening, not upstream's: `CreateBoxMon` never sets `abilityNum`
    /// for such a species (`:2296`-`:2300`), and construction mirrors that
    /// guard, so the fallback only matters for a save-loaded slot. Consumed
    /// by [`crate::ability`]'s Overgrow/Liquid Ooze checks (issue #321) and
    /// by [`crate::stat_change`]'s ability
    /// guards (issue #322: Clear Body, White Smoke, Keen Eye, Hyper
    /// Cutter) — see this type's module docs for why the rest of the
    /// ability system stays out.
    #[must_use]
    pub const fn ability(&self) -> AbilityId {
        let [first, second] = self.base_stats.abilities;
        if second.0 == 0 || self.ability_slot == 0 {
            first
        } else {
            second
        }
    }

    /// The raw `abilityNum` bit (`0` or `1`) [`BattlePokemon::ability`]
    /// indexes with — `0` or `1`, never anything else
    /// ([`BattlePokemon::with_ability_slot`] masks it).
    ///
    /// Defaults to `personality & 1` at construction
    /// (`CreateBoxMon`, `pokeemerald/src/pokemon.c:2296`-`:2300`) and is
    /// otherwise immutable — the boundary that restores a saved mon
    /// (`pokeemerald-rs`'s `party` module) is the one caller expected to
    /// override it, via [`BattlePokemon::with_ability_slot`], with the bit
    /// the save itself stored rather than one re-derived from personality.
    #[must_use]
    pub const fn ability_slot(&self) -> u8 {
        self.ability_slot
    }

    /// Overwrite the stored ability slot — the boundary a save/load round
    /// trip uses to restore upstream's `abilityNum` bit exactly as stored,
    /// rather than leaving it at [`BattlePokemon::new`]'s personality-parity
    /// default (which is only *this constructor's* choice for a freshly
    /// built mon, not a re-derivation upstream itself performs on load —
    /// `LoadPlayerParty` is a struct copy, not a `CreateBoxMon` call).
    /// Masked to its single bit, so this can never desync
    /// [`BattlePokemon::ability`] onto an out-of-range slot.
    #[must_use]
    pub const fn with_ability_slot(mut self, ability_slot: u8) -> Self {
        self.ability_slot = ability_slot & 1;
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

    /// The species' one or two types, captured at construction so combat code
    /// does not need a [`Dex`] lookup mid-battle.
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

    /// The known moves and their remaining PP, in slot order.
    ///
    /// Non-empty and at most [`MAX_MON_MOVES`] long, with no [`MOVE_NONE`]
    /// entry — guaranteed by [`BattlePokemon::new`] and preserved by every
    /// method here, since PP deduction is the only mutation.
    #[must_use]
    pub fn moves(&self) -> &[MoveSlot] {
        &self.moves
    }

    /// The move in slot `index`, or `None` for a slot this mon does not know
    /// — upstream's `MOVE_NONE` slot (see [`MOVE_NONE`]).
    #[must_use]
    pub fn move_at(&self, index: usize) -> Option<MoveId> {
        self.moves.get(index).map(|slot| slot.move_id)
    }

    /// This mon's PP Ups, as upstream's packed `ppBonuses` byte
    /// (`MON_DATA_PP_BONUSES`).
    ///
    /// Exposed whole so the save encoder can write the same byte back out
    /// (`crates/pokeemerald-rs/src/party.rs`): the bits belonging to slots
    /// this mon has no move for are carried through untouched rather than
    /// re-emitted as zero.
    #[must_use]
    pub const fn pp_bonuses(&self) -> PpBonuses {
        self.pp_bonuses
    }

    /// Slot `index`'s maximum PP — `CalculatePPWithBonus(move, ppBonuses,
    /// index)` (`pokeemerald/src/pokemon.c:4650`-`:4654`): the move's base PP
    /// plus 20% of it per PP Up applied to *that slot*.
    ///
    /// This, not the move's base PP, is what a heal restores to
    /// ([`BattlePokemon::heal`]) and the ceiling
    /// [`BattlePokemon::deduct_pp`] counts down from.
    ///
    /// # Errors
    ///
    /// [`BattleError::InvalidMoveSlot`] if `index` is not a slot this mon
    /// has, or whatever [`crate::dex::Dex::move_data`] reports for the move
    /// in it (unreachable — [`BattlePokemon::new`] already resolved it).
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

    /// Mutable access to the in-battle stat stages.
    ///
    /// No move in this slice changes a stage (stat-changing effects are
    /// deferred — see the crate root docs), so this exists for callers that
    /// want to exercise the stage-aware damage/accuracy/turn-order paths from
    /// a non-neutral starting position. Each [`StatStage`] enforces its own
    /// `-6..=+6` range, so this cannot break any invariant of this type.
    pub fn stages_mut(&mut self) -> &mut StatStages {
        &mut self.stages
    }

    /// Whether this mon has fainted (`current_hp == 0`).
    #[must_use]
    pub const fn is_fainted(&self) -> bool {
        self.current_hp == 0
    }

    /// Subtract `amount` from current HP, saturating at `0` (upstream never
    /// lets `gBattleMons[].hp` underflow either: `Cmd_healthbarupdate`/damage
    /// application clamps the same way).
    pub fn apply_damage(&mut self, amount: u32) {
        self.current_hp = self.current_hp.saturating_sub(amount);
    }

    /// Add `amount` to current HP, clamped at maximum HP —
    /// `Cmd_datahpupdate`'s negative-damage branch
    /// (`pokeemerald/src/battle_script_commands.c:1896`-`:1900`):
    ///
    /// ```text
    /// gBattleMons[].hp += -gBattleMoveDamage;
    /// if (gBattleMons[].hp > gBattleMons[].maxHP)
    ///     gBattleMons[].hp = gBattleMons[].maxHP;
    /// ```
    ///
    /// Added by issue #321 for [`crate::drain`]'s heal half. Note that
    /// upstream applies no fainted-battler guard here: the clamp is the only
    /// bound, so this method has only the one too.
    pub fn heal_hp(&mut self, amount: u32) {
        self.current_hp = self
            .current_hp
            .saturating_add(amount)
            .min(self.stats.max_hp);
    }

    /// The volatile conditions this battler currently carries
    /// (`gBattleMons[].status2` / `gStatuses3[]`, as far as this crate
    /// models them — see [`Volatiles`]).
    #[must_use]
    pub const fn volatiles(&self) -> Volatiles {
        self.volatiles
    }

    /// Mutable access to [`BattlePokemon::volatiles`], for the pipelines
    /// that set a bit ([`crate::flag_move`]) and for the end-of-turn tick
    /// ([`Volatiles::tick_charge`]).
    ///
    /// Every [`Volatiles`] field is independently valid, so no invariant of
    /// this type can be broken through the reference — the same reasoning
    /// [`BattlePokemon::stages_mut`] rests on.
    pub const fn volatiles_mut(&mut self) -> &mut Volatiles {
        &mut self.volatiles
    }

    /// Reset every field that is battle scratch rather than party data:
    /// [`BattlePokemon::stages`] and [`BattlePokemon::volatiles`].
    ///
    /// Neither survives upstream's own battle boundary — `stages` because
    /// `struct Pokemon` has no such field at all (module docs on
    /// [`StatStages`]), `volatiles` because `BattleStartClearSetData`
    /// zeroes `gBattleMons[].status2` and `gStatuses3[]` before every
    /// battle (`src/battle_main.c:3034`, cited on [`Volatiles`]). A flow
    /// layer that clones the battle's player mon back into the party after
    /// the battle ends must call this before the clone reaches the party
    /// record, or the *next* battle's `BattlePokemon::new` — which already
    /// starts at [`Volatiles::default`] and [`StatStages::default`] — would
    /// be fighting a mon that inherited scratch state no upstream battle
    /// ever carries forward. Exists as one method rather than three
    /// call-site resets so the two fields cannot drift apart the way
    /// [`BattlePokemon::stages_mut`] alone did before [`Volatiles`] existed.
    pub fn clear_battle_scratch(&mut self) {
        self.stages = StatStages::default();
        self.volatiles = Volatiles::default();
    }

    /// Add earned experience, consuming the award **one level threshold at
    /// a time** the way upstream's controller/`Cmd_getexp` loop does
    /// (`Task_GiveExpToMon`'s cap at `nextLvlExp`,
    /// `src/battle_controller_player.c:1154`-`:1181`, looped by case 5 at
    /// `src/battle_script_commands.c:3505`-`:3509`), capping both level and
    /// total experience at level 100. Each level reached this way also
    /// teaches that level's learnset moves the way upstream's
    /// `MonTryLearningNewMove` does — **unscreened**, exactly like
    /// upstream: a move this crate cannot execute yet is still learned,
    /// sits in the moveset, and is refused per turn when it is *selected*
    /// rather than at learn time (see
    /// [`BattlePokemon::walk_level_learnset`] for why that is the
    /// consistent posture, and what else it does and does not reproduce).
    ///
    /// # The return value is a question, and it has to be answered
    ///
    /// `Some(`[`PendingMoveLearn`]`)` means the walk **stopped**: a crossed
    /// level offered a move and all four slots were full, which is where
    /// upstream opens `BattleScript_AskToLearnMove`'s yes/no box
    /// (`src/battle_script_commands.c:5368`-`:5370`). The mon holds *at*
    /// that level — the award's remainder rides on the token, unconsumed,
    /// like upstream's leftover `gBattleMoveDamage` — and nothing further
    /// is learned or levelled until the caller answers with
    /// [`BattlePokemon::resolve_move_learn`]. Dropping the token silently
    /// discards the question, the rest of that level's learnset entries,
    /// *and* the rest of the award — which is why it is `#[must_use]`
    /// rather than a field this type answers on the player's behalf.
    ///
    /// Stat recalculation follows `CalculateMonStats`, per level. If
    /// maximum HP grows, the increase is also added to current HP,
    /// preserving the absolute amount of damage the mon had taken before
    /// levelling up.
    ///
    /// # Recorded divergence: EVs and friendship still do not move
    ///
    /// Upstream's level-up path does more than raise the number and teach
    /// moves. From `Cmd_getexp`, `BattleScript_LevelUp`
    /// (`pokeemerald/data/battle_scripts_1.s`) plays the level-up
    /// fanfare/message, and `Cmd_getexp` itself also applies `MonGainEVs`
    /// (`src/battle_script_commands.c:3420`) and
    /// `AdjustFriendship(FRIENDSHIP_EVENT_GROW_LEVEL)` (`:3463`). **Neither
    /// is modelled here**: EVs (this crate carries none, see the module
    /// docs) and friendship are out of this slice's scope, so only the
    /// numeric level, stats, and — as of issue #252 — learnset moves change
    /// across a level-up. Recorded on the `Cmd_getexp` ledger entry.
    #[must_use = "a full moveset pauses the level-up walk for a player \
                  decision; dropping the token declines it *and* abandons \
                  the rest of the level-up's learnset entries and the \
                  award's unconsumed remainder"]
    pub fn apply_experience(&mut self, dex: &Dex, amount: u32) -> Option<PendingMoveLearn> {
        self.advance_experience(dex, amount)
    }

    /// `GetLevelFromMonExp` + `CalculateMonStats`
    /// (`pokeemerald/src/pokemon.c`) as the **save decoder** reaches them:
    /// adopt a stored experience total and re-derive the level from it,
    /// teaching **nothing**. Upstream's load path copies the attacks
    /// substructure verbatim and never runs `MonTryLearningNewMove` — the
    /// learnset walk belongs to `Cmd_getexp`'s in-battle award
    /// ([`BattlePokemon::apply_experience`]) alone — so a decode that
    /// taught crossed-level moves would mutate the save's own authoritative
    /// moveset merely by loading it.
    ///
    /// The level never moves down and the total never drops below the
    /// current level's own floor: a stored total under that floor is a
    /// level/experience pair upstream could never write, and reconciling it
    /// upward mirrors the decoder's fail-toward-consistency posture. In the
    /// ordinary consistent-bytes case the stored total sits inside the
    /// current level's own band and this is a plain assignment.
    pub fn reconcile_saved_experience(&mut self, total: u32) {
        let max_experience =
            experience_for_level(self.base_stats.growth_rate, MAX_LEVEL).unwrap_or(u32::MAX);
        self.experience = self.experience.max(total.min(max_experience));
        self.raise_level_to_experience();
    }

    /// Raise the level (and stats, preserving damage taken) to match the
    /// current experience total, returning `Some((old_level, new_level))`
    /// when at least one threshold was crossed. Shared by the in-battle
    /// award ([`BattlePokemon::apply_experience`], which then teaches the
    /// crossed learnset moves) and the save decoder
    /// ([`BattlePokemon::reconcile_saved_experience`], which must not).
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
        self.stats = compute_stats(&self.base_stats, self.level, self.nature, self.ivs);
        self.current_hp = self
            .current_hp
            .saturating_add(self.stats.max_hp.saturating_sub(old_max_hp));
        Some((old_level, new_level))
    }

    /// The attacking stat (Attack or Sp. Attack) and its stage for a move of
    /// `category`, matching [`crate::damage::MoveCategory::for_type`]'s
    /// physical/special split.
    #[must_use]
    pub fn attacking_stat(&self, category: crate::damage::MoveCategory) -> (u32, StatStage) {
        match category {
            crate::damage::MoveCategory::Physical => (self.stats.attack, self.stages.attack),
            crate::damage::MoveCategory::Special => (self.stats.sp_attack, self.stages.sp_attack),
        }
    }

    /// The defending stat (Defense or Sp. Defense) and its stage for a move
    /// of `category`.
    #[must_use]
    pub fn defending_stat(&self, category: crate::damage::MoveCategory) -> (u32, StatStage) {
        match category {
            crate::damage::MoveCategory::Physical => (self.stats.defense, self.stages.defense),
            crate::damage::MoveCategory::Special => (self.stats.sp_defense, self.stages.sp_defense),
        }
    }

    /// This mon's effective Speed: [`Stats::speed`] scaled by the Speed
    /// [`StatStage`] (`gStatStageRatios`-style `APPLY_STAT_MOD`, matching
    /// [`crate::turn_order`]'s inputs). Weather/ability/item/paralysis speed
    /// modifiers (`GetWhoStrikesFirst`, `pokeemerald/src/battle_main.c:4595`)
    /// are not modelled this slice.
    #[must_use]
    pub const fn effective_speed(&self) -> u32 {
        self.stages.speed.apply(self.stats.speed)
    }

    /// Deduct one PP from move slot `index` — the deducting arm of
    /// `Cmd_ppreduce` (`battle_script_commands.c:1205`; Pressure's
    /// PP-doubling and the `HITMARKER_NO_PPDEDUCT` guard are not modelled).
    ///
    /// Upstream never errors here, but not because a 0-PP move proceeds:
    /// `Cmd_attackcanceler` — the first command of the hit script — aborts
    /// a 0-PP move to `BattleScript_NoPPForMove` (`:934`-`:939`) before
    /// `ppreduce` ever runs, so on the ordinary path this function is only
    /// reached with PP to spend. (`ppreduce`'s own `:1230` guard covers the
    /// Struggle/multi-turn continuations that legitimately reach it at 0 —
    /// none modelled this slice.) The turn engine reproduces the abort on
    /// the wild side ([`crate::battle::BattleEvent::FailedNoPp`]); this
    /// method's `NoPpRemaining` error is a *caller* boundary — the player
    /// path pre-validates its slot, so draining a slot below zero is a bug,
    /// not a battle event.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::InvalidMoveSlot`] if `index` is out of range,
    /// or [`BattleError::NoPpRemaining`] if the slot's PP is already `0`.
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

    /// `HealPlayerParty`'s per-mon effect
    /// (`pokeemerald/src/script_pokemon_util.c:30-59`, issue #261's
    /// white-out): restore current HP to [`Stats::max_hp`] and every known
    /// move's PP to [`BattlePokemon::max_pp`] — upstream's own
    /// `CalculatePPWithBonus(move, ppBonuses, i)` (`:44`-`:50`), so a slot
    /// carrying PP Ups heals to the *upgraded* maximum rather than to the
    /// move's base PP (issue #304).
    ///
    /// Upstream's third effect — zeroing `MON_DATA_STATUS` — has no
    /// counterpart to run: this crate models no non-volatile status
    /// conditions at all (module docs), so there is nothing left to clear.
    /// The owned type this method lives on, not a free function, so a
    /// future multi-slot party (this crate's module docs already flag the
    /// one-mon-party limit) only has to call it once per member.
    ///
    /// # Errors
    ///
    /// Whatever [`crate::dex::Dex::move_data`] reports for a move id this
    /// mon already knows — unreachable in practice, since every
    /// [`MoveSlot::move_id`] this crate ever writes ([`BattlePokemon::new`],
    /// [`BattlePokemon::walk_level_learnset`]) already passed that same lookup,
    /// but surfaced as a caller-visible `Result` instead of an `expect` in
    /// case a future change to either ever desyncs them.
    pub fn heal(&mut self, dex: &Dex) -> Result<(), BattleError> {
        self.current_hp = self.stats.max_hp;
        for index in 0..self.moves.len() {
            let full = self.max_pp(dex, index)?;
            self.moves[index].pp = full;
        }
        Ok(())
    }
}
