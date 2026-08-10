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
//! player mon, both realistic for a first encounter), abilities, held items,
//! non-volatile status conditions, and the Shedinja 1-HP special case in
//! `CalculateMonStats`.

use assets::{BaseStats, MoveId, SpeciesId, Type};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::{Nature, Stat};
use crate::stat_stage::StatStage;

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
    /// PP remaining (starts at the move's base PP; this slice does not model
    /// PP Up bonuses).
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
    original_trainer_id: u32,
    types: [Type; 2],
    stats: Stats,
    current_hp: u32,
    moves: Vec<MoveSlot>,
    stages: StatStages,
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
            original_trainer_id: 0,
            types: base.types,
            stats,
            current_hp: stats.max_hp,
            moves: slots,
            stages: StatStages::default(),
        })
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
}

#[cfg(test)]
mod tests {
    use super::{
        calc_max_hp, calc_stat, compute_stats, BattlePokemon, Ivs, MoveSlot, StatStages, MAX_IV,
        MAX_LEVEL, MIN_LEVEL, MOVE_NONE, SPECIES_NONE, SPECIES_OLD_UNOWN_B, SPECIES_OLD_UNOWN_Z,
    };
    use crate::damage::MoveCategory;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::nature::{Nature, Stat};
    use crate::stat_stage::StatStage;
    use assets::{MoveId, SpeciesId, SpeciesTable};

    /// Max Gen-3 individual values (stat rolls — *not* a cryptographic
    /// initialization vector; see [`Ivs`]): every `31` below is `MAX_IV_MASK`.
    const MAX_IVS: Ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };

    #[test]
    fn calc_max_hp_matches_hand_computed_bulbasaur_at_level_5() {
        // Bulbasaur base HP 45, IV 31 (max), level 5:
        // n = 2*45+31 = 121; 121*5/100 = 6 (605/100 truncated); +5+10 = 21.
        assert_eq!(calc_max_hp(45, 31, 5), 21);
    }

    #[test]
    fn calc_stat_applies_the_nature_modifier_after_the_plus_five() {
        // Bulbasaur base Attack 49, IV 31, level 5, Adamant (+Attack):
        // n = 2*49+31 = 129; 129*5/100 = 6 (645/100); +5 = 11; *110/100 = 12
        // (1210/100 truncated).
        let n = calc_stat(49, 31, 5, Nature::Adamant, Stat::Attack);
        assert_eq!(n, 12);
        // Same base/IV/level, neutral nature: no scaling, stays 11.
        assert_eq!(calc_stat(49, 31, 5, Nature::Hardy, Stat::Attack), 11);
    }

    #[test]
    fn compute_stats_bundles_all_six_stats() {
        let dex = Dex::new();
        let bulbasaur = dex.species(SpeciesId(1)).unwrap();
        let stats = compute_stats(bulbasaur, 5, Nature::Hardy, MAX_IVS);
        assert_eq!(stats.max_hp, calc_max_hp(bulbasaur.hp, 31, 5));
        assert_eq!(
            stats.attack,
            calc_stat(bulbasaur.attack, 31, 5, Nature::Hardy, Stat::Attack)
        );
        assert_eq!(
            stats.speed,
            calc_stat(bulbasaur.speed, 31, 5, Nature::Hardy, Stat::Speed)
        );
    }

    fn sample_mon(dex: &Dex) -> BattlePokemon {
        BattlePokemon::new(
            dex,
            SpeciesId(1), // Bulbasaur
            5,
            Ivs::default(),
            0x1234_5663,      // % 25 == 0, so the derived nature is neutral Hardy
            vec![MoveId(33)], // Tackle
        )
        .unwrap()
    }

    #[test]
    fn new_starts_at_full_hp_with_neutral_stages() {
        let dex = Dex::new();
        let mon = sample_mon(&dex);
        assert_eq!(mon.current_hp(), mon.stats().max_hp);
        assert!(!mon.is_fainted());
        assert_eq!(mon.stages(), StatStages::default());
        assert_eq!(
            mon.moves(),
            [MoveSlot {
                move_id: MoveId(33),
                pp: 35, // Tackle's base PP
            }]
        );
        assert_eq!(mon.move_at(0), Some(MoveId(33)));
        assert_eq!(
            mon.move_at(1),
            None,
            "an unknown slot is upstream MOVE_NONE"
        );
    }

    #[test]
    fn new_rejects_a_moveset_that_upstream_cannot_represent() {
        let dex = Dex::new();
        // Empty: `struct BattlePokemon` always has four slots and a battler
        // with none of them filled never reaches the engine -- and an empty
        // moveset would make the wild opponent's rejection loop spin forever.
        assert_eq!(
            BattlePokemon::new(&dex, SpeciesId(1), 5, Ivs::default(), 0, vec![]),
            Err(BattleError::InvalidMoveCount(0))
        );
        // Overfull: MAX_MON_MOVES is 4 (`include/constants/global.h:82`).
        assert_eq!(
            BattlePokemon::new(
                &dex,
                SpeciesId(1),
                5,
                Ivs::default(),
                0,
                vec![MoveId(33); 5]
            ),
            Err(BattleError::InvalidMoveCount(5))
        );
    }

    #[test]
    fn new_rejects_move_none_placeholder_slots() {
        let dex = Dex::new();
        // MOVE_NONE is the *empty slot* marker, never a known move:
        // `CheckMoveLimitations` rules it out (`battle_util.c:1098`) and the
        // wild rejection loop retries past it
        // (`battle_controller_opponent.c:1599`-`:1601`).
        assert_eq!(
            BattlePokemon::new(
                &dex,
                SpeciesId(1),
                5,
                Ivs::default(),
                0,
                vec![MOVE_NONE, MoveId(33)]
            ),
            Err(BattleError::PlaceholderMove(0))
        );
        // An all-placeholder moveset passes the non-empty count check, so the
        // placeholder check is what actually rejects it.
        assert_eq!(
            BattlePokemon::new(&dex, SpeciesId(1), 5, Ivs::default(), 0, vec![MOVE_NONE]),
            Err(BattleError::PlaceholderMove(0))
        );
    }

    #[test]
    fn new_rejects_levels_outside_the_upstream_range() {
        let dex = Dex::new();
        let build = |level| {
            BattlePokemon::new(
                &dex,
                SpeciesId(1),
                level,
                Ivs::default(),
                0,
                vec![MoveId(33)],
            )
        };
        // MIN_LEVEL..=MAX_LEVEL is 1..=100 (`include/constants/pokemon.h:145`-`:146`).
        assert_eq!(build(0), Err(BattleError::InvalidLevel(0)));
        assert_eq!(build(101), Err(BattleError::InvalidLevel(101)));
        assert_eq!(build(255), Err(BattleError::InvalidLevel(255)));
        assert!(build(MIN_LEVEL).is_ok());
        assert!(build(MAX_LEVEL).is_ok());
    }

    #[test]
    fn new_rejects_ivs_above_the_five_bit_maximum() {
        let dex = Dex::new();
        let build = |ivs| BattlePokemon::new(&dex, SpeciesId(1), 5, ivs, 0, vec![MoveId(33)]);
        // Upstream stores each IV in five bits (MAX_IV_MASK = 31,
        // `include/constants/pokemon.h:201`), so 32+ is unrepresentable.
        for over in [
            Ivs {
                hp: 32,
                ..Ivs::default()
            },
            Ivs {
                sp_defense: 255,
                ..Ivs::default()
            },
        ] {
            assert!(matches!(build(over), Err(BattleError::InvalidIv(_))));
        }
        assert_eq!(
            build(Ivs {
                speed: MAX_IV + 1,
                ..Ivs::default()
            }),
            Err(BattleError::InvalidIv(MAX_IV + 1))
        );
        assert!(build(MAX_IVS).is_ok(), "31 across the board is legal");
    }

    #[test]
    fn new_reports_unknown_species_and_moves() {
        let dex = Dex::new();
        let bad_species = SpeciesId(SpeciesTable::LEN_U16);
        assert_eq!(
            BattlePokemon::new(&dex, bad_species, 5, Ivs::default(), 0, vec![MoveId(33)]),
            Err(BattleError::UnknownSpecies(bad_species))
        );

        let bad_move = MoveId(60_000);
        assert_eq!(
            BattlePokemon::new(&dex, SpeciesId(1), 5, Ivs::default(), 0, vec![bad_move]),
            Err(BattleError::UnknownMove(bad_move))
        );
    }

    #[test]
    fn new_rejects_the_species_none_placeholder() {
        let dex = Dex::new();
        // Slot 0 of `gSpeciesInfo` exists but is the all-zero SPECIES_NONE
        // placeholder: addressable is not the same as real, so construction
        // refuses it rather than building a fightable mon from zeroes.
        assert_eq!(
            BattlePokemon::new(&dex, SPECIES_NONE, 5, Ivs::default(), 0, vec![MoveId(33)]),
            Err(BattleError::PlaceholderSpecies)
        );
    }

    #[test]
    fn new_rejects_the_old_unown_reserved_range_but_not_its_neighbours() {
        let dex = Dex::new();
        // 252..=276 are the Gen-2 compatibility holes carrying the dummy
        // OLD_UNOWN_SPECIES_INFO row; the ids on either side are Celebi
        // (251) and Treecko (277), which must keep working.
        for species in [SPECIES_OLD_UNOWN_B, SpeciesId(260), SPECIES_OLD_UNOWN_Z] {
            assert_eq!(
                BattlePokemon::new(&dex, species, 5, Ivs::default(), 0, vec![MoveId(33)]),
                Err(BattleError::PlaceholderSpecies),
                "reserved id {} must be refused",
                species.0
            );
        }
        for species in [SpeciesId(251), SpeciesId(277)] {
            assert!(
                BattlePokemon::new(&dex, species, 5, Ivs::default(), 0, vec![MoveId(33)]).is_ok(),
                "real neighbour id {} must construct",
                species.0
            );
        }
    }

    #[test]
    fn nature_is_derived_from_the_personality_value() {
        let dex = Dex::new();
        let build = |personality| {
            BattlePokemon::new(
                &dex,
                SpeciesId(1),
                5,
                MAX_IVS,
                personality,
                vec![MoveId(33)],
            )
            .unwrap()
        };
        // GetNatureFromPersonality (`pokemon.c:5498`): personality % 25.
        // Nature id 3 is Adamant (+Atk), so a mon built at personality 3
        // carries Adamant *and* Adamant-modified stats — a contradictory
        // nature/personality pair is unrepresentable by construction.
        let adamant = build(3);
        assert_eq!(adamant.nature(), Nature::Adamant);
        let bulbasaur = dex.species(SpeciesId(1)).unwrap();
        assert_eq!(
            adamant.stats(),
            compute_stats(bulbasaur, 5, Nature::Adamant, MAX_IVS)
        );
        // 28 % 25 == 3 wraps to the same nature.
        assert_eq!(build(28).nature(), Nature::Adamant);
        assert_eq!(build(0).nature(), Nature::Hardy);
    }

    #[test]
    fn apply_damage_saturates_at_zero_and_marks_fainted() {
        let dex = Dex::new();
        let mut mon = sample_mon(&dex);
        let max_hp = mon.stats().max_hp;
        mon.apply_damage(max_hp + 1000);
        assert_eq!(mon.current_hp(), 0);
        assert!(mon.is_fainted());
    }

    #[test]
    fn attacking_and_defending_stat_select_by_category() {
        let dex = Dex::new();
        let mon = sample_mon(&dex);
        assert_eq!(
            mon.attacking_stat(MoveCategory::Physical),
            (mon.stats().attack, StatStage::NEUTRAL)
        );
        assert_eq!(
            mon.attacking_stat(MoveCategory::Special),
            (mon.stats().sp_attack, StatStage::NEUTRAL)
        );
        assert_eq!(
            mon.defending_stat(MoveCategory::Physical),
            (mon.stats().defense, StatStage::NEUTRAL)
        );
        assert_eq!(
            mon.defending_stat(MoveCategory::Special),
            (mon.stats().sp_defense, StatStage::NEUTRAL)
        );
    }

    #[test]
    fn effective_speed_applies_the_speed_stage() {
        let dex = Dex::new();
        let mut mon = sample_mon(&dex);
        assert_eq!(mon.effective_speed(), mon.stats().speed);
        mon.stages_mut().speed = StatStage::new(2).unwrap();
        assert_eq!(mon.effective_speed(), mon.stats().speed * 2);
    }

    #[test]
    fn deduct_pp_decrements_and_reports_exhaustion() {
        let dex = Dex::new();
        let mut mon = sample_mon(&dex);
        let starting_pp = mon.moves()[0].pp;
        mon.deduct_pp(0).unwrap();
        assert_eq!(mon.moves()[0].pp, starting_pp - 1);

        assert_eq!(mon.deduct_pp(5), Err(BattleError::InvalidMoveSlot(5)));

        // Drain the slot through the only mutation the type offers: the
        // moveset itself is not reachable for writing (`oop-boundaries`).
        for _ in 0..(starting_pp - 1) {
            mon.deduct_pp(0).unwrap();
        }
        assert_eq!(mon.moves()[0].pp, 0);
        assert_eq!(mon.deduct_pp(0), Err(BattleError::NoPpRemaining(0)));
    }

    #[test]
    fn ivs_report_their_upstream_five_bit_range() {
        // Gen-3 stat rolls, not cryptographic initialization vectors.
        assert!(Ivs::default().is_valid());
        assert!(MAX_IVS.is_valid());
        assert_eq!(MAX_IVS.as_array(), [MAX_IV; 6]);
        assert!(!Ivs {
            attack: MAX_IV + 1,
            ..Ivs::default()
        }
        .is_valid());
    }
}
