//! Selects ordinary land encounters while preserving the shared RNG stream.
//!
//! A same-behavior encounter draws for rate, slot, then level. A metatile
//! behavior change first draws whether an encounter check is allowed. Each
//! failed eligibility check stops before later draws. The caller resolves the
//! map header and builds a battle participant from the returned
//! [`WildEncounter`].
//!
//! This module does not apply bike, item, ability, or repel modifiers, and it
//! does not select water, fishing, rock-smash, roamer, outbreak, or special-mode
//! encounters.

use assets::{LandEncounters, SpeciesId, WildEncounterHeader, WildPokemon};

use super::metatile_behavior::{is_land_wild_encounter, MB_NORMAL};
use crate::rng::RandomSource;

const ENCOUNTER_RATE_SCALE: u32 = 16;
const NEW_METATILE_ROLL_RANGE: u16 = 100;
const NEW_METATILE_CHECK_CHANCE: u16 = 60;

/// Maximum scaled encounter rate and modulus for encounter-rate draws.
pub const MAX_ENCOUNTER_RATE: u32 = 2880;

/// Percentage chance for each land encounter slot, indexed by slot number.
pub const LAND_SLOT_CHANCES: [u8; assets::LAND_SLOTS] = {
    let mut percentage_by_slot = [0; assets::LAND_SLOTS];
    percentage_by_slot[0] = 20;
    percentage_by_slot[1] = 20;
    percentage_by_slot[2] = 10;
    percentage_by_slot[3] = 10;
    percentage_by_slot[4] = 10;
    percentage_by_slot[5] = 10;
    percentage_by_slot[6] = 5;
    percentage_by_slot[7] = 5;
    percentage_by_slot[8] = 4;
    percentage_by_slot[9] = 4;
    percentage_by_slot[10] = 1;
    percentage_by_slot[11] = 1;
    percentage_by_slot
};

/// Total percentage represented by [`LAND_SLOT_CHANCES`].
pub const LAND_SLOT_TOTAL: u16 = {
    let mut total = 0u16;
    let mut slot = 0;
    while slot < LAND_SLOT_CHANCES.len() {
        total += LAND_SLOT_CHANCES[slot] as u16;
        slot += 1;
    }
    total
};

/// Number of encounter-free steps granted after a transition or encounter.
pub const WILD_ENCOUNTER_IMMUNITY_STEPS: u8 = 4;

/// Species, level, and source slot selected by a land encounter roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WildEncounter {
    /// Encountered species.
    pub species: SpeciesId,
    /// Encountered level.
    pub level: u8,
    /// Selected slot in `0..`[`assets::LAND_SLOTS`].
    pub slot: usize,
}

/// Selects a land slot for a percentage roll.
///
/// Out-of-range rolls select the last slot, matching the final fallback in
/// `src/wild_encounter.c:182-210`. [`choose_land_mon_index`] cannot reach that
/// fallback because it reduces its draw modulo [`LAND_SLOT_TOTAL`].
#[must_use]
pub fn land_slot_for_roll(roll: u16) -> usize {
    let mut cumulative_chance = 0u16;
    for (slot, chance_percent) in LAND_SLOT_CHANCES.iter().enumerate() {
        cumulative_chance += u16::from(*chance_percent);
        if roll < cumulative_chance {
            return slot;
        }
    }
    LAND_SLOT_CHANCES.len() - 1
}

/// Draws and selects one land encounter slot.
pub fn choose_land_mon_index(rng: &mut impl RandomSource) -> usize {
    land_slot_for_roll(rng.next_u16() % LAND_SLOT_TOTAL)
}

/// Maps a roll into the inclusive level band after ordering inverted bounds.
#[must_use]
pub fn level_for_roll(min_level: u8, max_level: u8, roll: u16) -> u8 {
    let (min, max) = if max_level >= min_level {
        (min_level, max_level)
    } else {
        (max_level, min_level)
    };
    let level_count = u16::from(max - min) + 1;
    let level = u16::from(min) + (roll % level_count);
    u8::try_from(level).unwrap_or(max)
}

/// Draws one level from a wild Pokémon slot's inclusive level band.
pub fn choose_wild_mon_level(mon: &WildPokemon, rng: &mut impl RandomSource) -> u8 {
    level_for_roll(mon.min_level, mon.max_level, rng.next_u16())
}

/// Draws against [`MAX_ENCOUNTER_RATE`] and compares the result with `encounter_rate`.
pub fn encounter_odds_check(encounter_rate: u32, rng: &mut impl RandomSource) -> bool {
    u32::from(rng.next_u16()) % MAX_ENCOUNTER_RATE < encounter_rate
}

/// Scales and clamps a table encounter rate, then draws once against it.
pub fn wild_encounter_check(encounter_rate: u8, rng: &mut impl RandomSource) -> bool {
    let scaled_rate = (u32::from(encounter_rate) * ENCOUNTER_RATE_SCALE).min(MAX_ENCOUNTER_RATE);
    encounter_odds_check(scaled_rate, rng)
}

/// Draws whether a check may continue after metatile behavior changes.
pub fn allow_wild_check_on_new_metatile(rng: &mut impl RandomSource) -> bool {
    rng.next_u16() % NEW_METATILE_ROLL_RANGE < NEW_METATILE_CHECK_CHANCE
}

/// Draws a land encounter's slot and then its level.
pub fn try_generate_wild_mon_land(
    land: &LandEncounters,
    rng: &mut impl RandomSource,
) -> WildEncounter {
    let slot = choose_land_mon_index(rng);
    let mon = land.mons[slot];
    let level = choose_wild_mon_level(&mon, rng);
    WildEncounter {
        species: mon.species,
        level,
        slot,
    }
}

/// Rolls an ordinary land encounter for one completed step.
///
/// `header` is the already-resolved entry for the current map. `None`, a
/// non-land metatile, a missing land table, a rejected behavior change, or a
/// failed rate draw returns `None` before consuming any later draw.
pub fn standard_wild_encounter(
    header: Option<&WildEncounterHeader>,
    current_metatile_behavior: u8,
    previous_metatile_behavior: u8,
    rng: &mut impl RandomSource,
) -> Option<WildEncounter> {
    let header = header?;
    if !is_land_wild_encounter(current_metatile_behavior) {
        return None;
    }
    let land = header.land.as_ref()?;
    if previous_metatile_behavior != current_metatile_behavior
        && !allow_wild_check_on_new_metatile(rng)
    {
        return None;
    }
    if !wild_encounter_check(land.encounter_rate, rng) {
        return None;
    }
    Some(try_generate_wild_mon_land(land, rng))
}

/// Per-step encounter immunity and previous-metatile state.
#[derive(Debug, Clone)]
pub struct WildEncounterState {
    immunity_steps: u8,
    prev_metatile_behavior: u8,
}

impl Default for WildEncounterState {
    fn default() -> Self {
        Self::new()
    }
}

impl WildEncounterState {
    /// Creates boot-time encounter state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            immunity_steps: 0,
            prev_metatile_behavior: MB_NORMAL,
        }
    }

    /// Restarts immunity without changing the remembered metatile behavior.
    pub fn restart_immunity_steps(&mut self) {
        self.immunity_steps = 0;
    }

    /// Returns the number of immune steps already consumed.
    #[must_use]
    pub const fn immunity_steps(&self) -> u8 {
        self.immunity_steps
    }

    /// Returns the behavior recorded for the previous completed step.
    #[must_use]
    pub const fn prev_metatile_behavior(&self) -> u8 {
        self.prev_metatile_behavior
    }

    /// Rolls for an encounter and records the completed step's behavior.
    ///
    /// Immune steps consume no RNG. A successful encounter restarts immunity;
    /// failed eligible rolls leave the counter at its ceiling.
    pub fn check_standard_wild_encounter(
        &mut self,
        current_metatile_behavior: u8,
        header: Option<&WildEncounterHeader>,
        rng: &mut impl RandomSource,
    ) -> Option<WildEncounter> {
        let encounter = if self.immunity_steps < WILD_ENCOUNTER_IMMUNITY_STEPS {
            self.immunity_steps += 1;
            None
        } else {
            standard_wild_encounter(
                header,
                current_metatile_behavior,
                self.prev_metatile_behavior,
                rng,
            )
        };
        if encounter.is_some() {
            self.restart_immunity_steps();
        }
        self.prev_metatile_behavior = current_metatile_behavior;
        encounter
    }
}

#[cfg(test)]
mod tests;
