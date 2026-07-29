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

/// The six individual values (`0..=31`) rolled for a Pokémon
/// (`MAX_IV_MASK`, `pokeemerald/include/constants/pokemon.h:201`).
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
fn calc_stat(base: u8, iv: u8, level: u8, nature: Nature, stat: Stat) -> u32 {
    let n = (2 * u32::from(base) + u32::from(iv)) * u32::from(level) / 100 + 5;
    nature.modify_stat(stat, n)
}

/// The max-HP half of `CalculateMonStats` (`pokeemerald/src/pokemon.c:2851`):
/// `(((2*base + iv) * level) / 100) + level + 10`. HP is never nature-modified
/// (`ModifyStatByNature` special-cases `statIndex <= STAT_HP`). The Shedinja
/// 1-HP special case (`species == SPECIES_SHEDINJA`) is not modelled.
fn calc_max_hp(base: u8, iv: u8, level: u8) -> u32 {
    let n = 2 * u32::from(base) + u32::from(iv);
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattlePokemon {
    /// The species this mon was built from.
    pub species: SpeciesId,
    /// Current level (`1..=100`).
    pub level: u8,
    /// Nature.
    pub nature: Nature,
    /// Individual values.
    pub ivs: Ivs,
    /// Personality value — carried for fidelity (it selects nature, and
    /// upstream also derives gender/shininess/unown letter from it, none of
    /// which this slice consumes) even though nothing in this crate reads it
    /// back yet.
    pub personality: u32,
    /// The species' one or two types, captured at construction so combat
    /// code does not need a [`Dex`] lookup mid-battle.
    pub types: [Type; 2],
    /// Computed battle stats.
    pub stats: Stats,
    /// Current HP (`0` means fainted).
    pub current_hp: u32,
    /// Known moves, in slot order (`0..MAX_MON_MOVES`).
    pub moves: Vec<MoveSlot>,
    /// In-battle stat stages.
    pub stages: StatStages,
}

impl BattlePokemon {
    /// Build a full-HP battler at the given species/level/nature/IVs/moves.
    ///
    /// `moves` must be non-empty and at most [`MAX_MON_MOVES`] long; PP for
    /// each slot starts at the move's base PP (`dex.move_data(id)?.pp`).
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownSpecies`] or [`BattleError::UnknownMove`]
    /// if `species`/any of `moves` is not in `dex`, or
    /// [`BattleError::InvalidMoveCount`] if `moves` is empty or longer than
    /// [`MAX_MON_MOVES`] — neither is representable upstream, and the
    /// non-empty half is what makes the wild opponent's move-choice rejection
    /// loop terminate (see [`crate::battle::Battle::take_turn`]).
    pub fn new(
        dex: &Dex,
        species: SpeciesId,
        level: u8,
        nature: Nature,
        ivs: Ivs,
        personality: u32,
        moves: Vec<MoveId>,
    ) -> Result<Self, BattleError> {
        if moves.is_empty() || moves.len() > MAX_MON_MOVES {
            return Err(BattleError::InvalidMoveCount(moves.len()));
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
            types: base.types,
            stats,
            current_hp: stats.max_hp,
            moves: slots,
            stages: StatStages::default(),
        })
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

    /// Deduct one PP from move slot `index` (`Cmd_ppreduce`'s common case —
    /// Pressure's PP-doubling is not modelled).
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
    use super::{calc_max_hp, calc_stat, compute_stats, BattlePokemon, Ivs, MoveSlot, StatStages};
    use crate::damage::MoveCategory;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::nature::{Nature, Stat};
    use crate::stat_stage::StatStage;
    use assets::{MoveId, SpeciesId, SpeciesTable};

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
        let ivs = Ivs {
            hp: 31,
            attack: 31,
            defense: 31,
            speed: 31,
            sp_attack: 31,
            sp_defense: 31,
        };
        let stats = compute_stats(bulbasaur, 5, Nature::Hardy, ivs);
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
            Nature::Hardy,
            Ivs::default(),
            0x1234_5678,
            vec![MoveId(33)], // Tackle
        )
        .unwrap()
    }

    #[test]
    fn new_starts_at_full_hp_with_neutral_stages() {
        let dex = Dex::new();
        let mon = sample_mon(&dex);
        assert_eq!(mon.current_hp, mon.stats.max_hp);
        assert!(!mon.is_fainted());
        assert_eq!(mon.stages, StatStages::default());
        assert_eq!(
            mon.moves,
            vec![MoveSlot {
                move_id: MoveId(33),
                pp: 35, // Tackle's base PP
            }]
        );
    }

    #[test]
    fn new_reports_unknown_species_and_moves() {
        let dex = Dex::new();
        let bad_species = SpeciesId(SpeciesTable::LEN_U16);
        assert_eq!(
            BattlePokemon::new(
                &dex,
                bad_species,
                5,
                Nature::Hardy,
                Ivs::default(),
                0,
                vec![MoveId(33)]
            ),
            Err(BattleError::UnknownSpecies(bad_species))
        );

        let bad_move = MoveId(60_000);
        assert_eq!(
            BattlePokemon::new(
                &dex,
                SpeciesId(1),
                5,
                Nature::Hardy,
                Ivs::default(),
                0,
                vec![bad_move]
            ),
            Err(BattleError::UnknownMove(bad_move))
        );
    }

    #[test]
    fn apply_damage_saturates_at_zero_and_marks_fainted() {
        let dex = Dex::new();
        let mut mon = sample_mon(&dex);
        let max_hp = mon.stats.max_hp;
        mon.apply_damage(max_hp + 1000);
        assert_eq!(mon.current_hp, 0);
        assert!(mon.is_fainted());
    }

    #[test]
    fn attacking_and_defending_stat_select_by_category() {
        let dex = Dex::new();
        let mon = sample_mon(&dex);
        assert_eq!(
            mon.attacking_stat(MoveCategory::Physical),
            (mon.stats.attack, StatStage::NEUTRAL)
        );
        assert_eq!(
            mon.attacking_stat(MoveCategory::Special),
            (mon.stats.sp_attack, StatStage::NEUTRAL)
        );
        assert_eq!(
            mon.defending_stat(MoveCategory::Physical),
            (mon.stats.defense, StatStage::NEUTRAL)
        );
        assert_eq!(
            mon.defending_stat(MoveCategory::Special),
            (mon.stats.sp_defense, StatStage::NEUTRAL)
        );
    }

    #[test]
    fn effective_speed_applies_the_speed_stage() {
        let dex = Dex::new();
        let mut mon = sample_mon(&dex);
        assert_eq!(mon.effective_speed(), mon.stats.speed);
        mon.stages.speed = StatStage::new(2).unwrap();
        assert_eq!(mon.effective_speed(), mon.stats.speed * 2);
    }

    #[test]
    fn deduct_pp_decrements_and_reports_exhaustion() {
        let dex = Dex::new();
        let mut mon = sample_mon(&dex);
        let starting_pp = mon.moves[0].pp;
        mon.deduct_pp(0).unwrap();
        assert_eq!(mon.moves[0].pp, starting_pp - 1);

        assert_eq!(mon.deduct_pp(5), Err(BattleError::InvalidMoveSlot(5)));

        mon.moves[0].pp = 0;
        assert_eq!(mon.deduct_pp(0), Err(BattleError::NoPpRemaining(0)));
    }
}
