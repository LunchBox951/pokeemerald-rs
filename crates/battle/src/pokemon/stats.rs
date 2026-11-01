//! The stat formula: [`Ivs`], [`Stats`], and the pure functions that turn
//! base stats, level, nature, IVs, and EVs into a battler's computed stats.
//!
//! Shedinja's 1-HP special case and Deoxys's single-player Speed Forme base
//! stats are modelled here. [`BattlePokemon`] reads this module to build and
//! refresh its stat cache but owns none of the arithmetic.

use assets::{BaseStats, SpeciesId};

use super::{BattlePokemon, Evs, MAX_IV, SPECIES_SHEDINJA};
use crate::nature::{Nature, Stat};

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

    pub(super) const fn first_invalid(self) -> Option<u8> {
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

/// Computed battle stats before applying [`super::StatStages`].
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

const STAT_FORMULA_SCALE: u32 = 100;
const EFFORT_VALUES_PER_STAT_POINT: u32 = 4;
const NON_HP_STAT_OFFSET: u32 = 5;
const HP_STAT_OFFSET: u32 = 10;
const SHEDINJA_MAX_HP: u32 = 1;

pub(super) fn calc_stat(
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

pub(super) fn calc_max_hp(
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

/// Emerald's Speed Forme base stats, applied to Deoxys outside link battles.
const DEOXYS_SPEED_FORME_ATTACK: u8 = 95;
const DEOXYS_SPEED_FORME_DEFENSE: u8 = 90;
const DEOXYS_SPEED_FORME_SPEED: u8 = 180;
const DEOXYS_SPEED_FORME_SP_ATTACK: u8 = 95;
const DEOXYS_SPEED_FORME_SP_DEFENSE: u8 = 90;

/// Base stats a single-player battle reads for `species`.
///
/// Upstream `GetDeoxysStat` replaces Deoxys's five non-HP base stats with the
/// Speed Forme values outside link battles (`pokemon.c:2698-2712`); the dex row
/// and the save-time `CalculateMonStats` cache stay on the raw Normal Forme data.
pub(super) fn effective_battle_base_stats(species: SpeciesId, base: &BaseStats) -> BaseStats {
    if species != SpeciesId::DEOXYS {
        return *base;
    }
    BaseStats {
        attack: DEOXYS_SPEED_FORME_ATTACK,
        defense: DEOXYS_SPEED_FORME_DEFENSE,
        speed: DEOXYS_SPEED_FORME_SPEED,
        sp_attack: DEOXYS_SPEED_FORME_SP_ATTACK,
        sp_defense: DEOXYS_SPEED_FORME_SP_DEFENSE,
        ..*base
    }
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

impl BattlePokemon {
    /// Feeds Deoxys's current EVs into its five Speed Forme stats.
    ///
    /// Upstream `GetDeoxysStat` reads each stat's EV byte alongside its IV
    /// (`pokemon.c:2707-2711`), so the substitution is EV-aware even though
    /// every other species' live block stays `0`-EV. Max HP is untouched:
    /// `GetDeoxysStat` never reads it, and the save-side HP bookkeeping in
    /// [`super::evs`] relies on that `0`-EV floor.
    ///
    /// [`BattlePokemon::gain_evs`] does not call this, so EVs earned in a
    /// battle leave that battle's stats alone; each battle constructor calls
    /// it on the player's side instead, where upstream's battle-start copy
    /// re-reads the party member through `GetMonData`
    /// (`CopyPlayerMonData`, `battle_controller_player.c:1641`-`:1645`).
    pub(crate) fn apply_speed_forme_evs(&mut self) {
        if self.species != SpeciesId::DEOXYS {
            return;
        }
        let ev_aware = compute_stats_with_evs(
            self.species,
            &effective_battle_base_stats(self.species, &self.base_stats),
            self.level,
            self.nature,
            self.ivs,
            self.evs,
        );
        self.stats = Stats {
            max_hp: self.stats.max_hp,
            ..ev_aware
        };
    }
}
