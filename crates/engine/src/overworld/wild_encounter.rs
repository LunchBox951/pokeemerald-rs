//! Selects ordinary land encounters while preserving the shared RNG stream.
//!
//! A same-behavior encounter draws for rate, slot, then level, plus one more
//! coin when party slot 0 holds Hustle, Vital Spirit or Pressure. A Magnet Pull
//! or Static lead first draws a coin and may pick the slot itself. A metatile
//! behavior change first draws whether an encounter check is allowed. Each
//! failed eligibility check stops before later draws. The caller resolves the
//! map header and slot 0's ability, and builds a battle participant from the
//! returned [`WildEncounter`].
//!
//! An active saved Repel then rejects a generated level below the first usable party
//! member's ([`RepelGate`]); the gate draws nothing.
//!
//! The rate roll applies the two flute flags and a Cleanse Tag held by slot 0
//! ([`EncounterRateModifiers`]) before its clamp; none of them draws. This module
//! does not apply bike, other item, or other ability modifiers, and it
//! does not select water, fishing, rock-smash, roamer, outbreak, or special-mode
//! encounters.

use assets::{
    AbilityId, LandEncounters, SpeciesId, SpeciesTable, Type, WildEncounterHeader, WildPokemon,
};

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
///
/// `slot_zero_ability` is the ability of party slot 0 when it is not an egg,
/// whether or not it is fainted or leading the battle (`ChooseWildMonLevel`,
/// `src/wild_encounter.c:268-303`). After the level draw, Hustle, Vital Spirit
/// and Pressure draw a second coin, even for a one-level band: an even coin
/// selects the band's maximum, an odd one lowers a level above the minimum by one.
pub fn choose_wild_mon_level(
    mon: &WildPokemon,
    slot_zero_ability: Option<AbilityId>,
    rng: &mut impl RandomSource,
) -> u8 {
    let level = level_for_roll(mon.min_level, mon.max_level, rng.next_u16());
    if !matches!(
        slot_zero_ability,
        Some(AbilityId::HUSTLE | AbilityId::VITAL_SPIRIT | AbilityId::PRESSURE)
    ) {
        return level;
    }
    let (min, max) = (
        mon.min_level.min(mon.max_level),
        mon.min_level.max(mon.max_level),
    );
    if rng.next_u16().is_multiple_of(2) {
        max
    } else if level > min {
        level - 1
    } else {
        level
    }
}

/// Draws against [`MAX_ENCOUNTER_RATE`] and compares the result with `encounter_rate`.
pub fn encounter_odds_check(encounter_rate: u32, rng: &mut impl RandomSource) -> bool {
    u32::from(rng.next_u16()) % MAX_ENCOUNTER_RATE < encounter_rate
}

/// The field inputs that scale an encounter rate between its table scaling and
/// its clamp (`WildEncounterCheck`, `src/wild_encounter.c:502-528`). None of
/// them draws from the RNG.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncounterRateModifiers {
    /// `FLAG_SYS_ENC_UP_ITEM`: adds half the current rate.
    pub encounter_up_flute: bool,
    /// `FLAG_SYS_ENC_DOWN_ITEM`: halves the current rate unless the up flag is set.
    pub encounter_down_flute: bool,
    /// Whether party slot 0 holds a Cleanse Tag, with no HP or egg condition.
    pub slot_zero_holds_cleanse_tag: bool,
}

impl EncounterRateModifiers {
    /// Applies the flute flags, then the Cleanse Tag, to a rate already scaled
    /// by 16. The up flag wins over the down flag (`ApplyFluteEncounterRateMod`,
    /// `:946-952`); a Tag multiplies by 2 and divides by 3 with integer
    /// truncation (`ApplyCleanseTagEncounterRateMod`, `:961-967`). The result is
    /// not clamped.
    #[must_use]
    pub fn apply(self, scaled_rate: u32) -> u32 {
        let mut rate = scaled_rate;
        if self.encounter_up_flute {
            rate += rate / 2;
        } else if self.encounter_down_flute {
            rate /= 2;
        }
        if self.slot_zero_holds_cleanse_tag {
            rate = rate * 2 / 3;
        }
        rate
    }
}

/// [`wild_encounter_check_with_rate_modifiers`] with no flute flag and no Cleanse Tag.
pub fn wild_encounter_check(encounter_rate: u8, rng: &mut impl RandomSource) -> bool {
    wild_encounter_check_with_rate_modifiers(encounter_rate, EncounterRateModifiers::default(), rng)
}

/// Scales a table encounter rate by 16, applies `modifiers`, clamps to
/// [`MAX_ENCOUNTER_RATE`], then draws once against it (`WildEncounterCheck`,
/// `src/wild_encounter.c:502-528`). The clamp follows the modifiers.
pub fn wild_encounter_check_with_rate_modifiers(
    encounter_rate: u8,
    modifiers: EncounterRateModifiers,
    rng: &mut impl RandomSource,
) -> bool {
    let scaled_rate = u32::from(encounter_rate) * ENCOUNTER_RATE_SCALE;
    encounter_odds_check(modifiers.apply(scaled_rate).min(MAX_ENCOUNTER_RATE), rng)
}

/// Draws whether a check may continue after metatile behavior changes.
pub fn allow_wild_check_on_new_metatile(rng: &mut impl RandomSource) -> bool {
    rng.next_u16() % NEW_METATILE_ROLL_RANGE < NEW_METATILE_CHECK_CHANCE
}

/// `TryGetRandomWildMonIndexByType`, `src/wild_encounter.c:914-934`.
fn try_random_land_slot_by_type(
    land: &LandEncounters,
    wanted: Type,
    rng: &mut impl RandomSource,
) -> Option<usize> {
    let species_table = SpeciesTable::new();
    let mut matching_slots = [0usize; assets::LAND_SLOTS];
    let mut matching_count = 0;
    for (slot, mon) in land.mons.iter().enumerate() {
        let has_type = species_table
            .get(mon.species)
            .is_some_and(|stats| stats.types.contains(&wanted));
        if has_type {
            matching_slots[matching_count] = slot;
            matching_count += 1;
        }
    }
    if matching_count == 0 || matching_count == assets::LAND_SLOTS {
        return None;
    }
    let pick = usize::from(rng.next_u16()) % matching_count;
    Some(matching_slots[pick])
}

/// `TryGetAbilityInfluencedWildMonIndex`, `src/wild_encounter.c:936-953`; an egg lead is `None`.
fn try_ability_influenced_land_slot(
    land: &LandEncounters,
    slot_zero_ability: Option<AbilityId>,
    ability: AbilityId,
    wanted: Type,
    rng: &mut impl RandomSource,
) -> Option<usize> {
    if slot_zero_ability != Some(ability) || !rng.next_u16().is_multiple_of(2) {
        return None;
    }
    try_random_land_slot_by_type(land, wanted, rng)
}

/// Draws a land encounter's slot and then its level for `slot_zero_ability`
/// (see [`choose_wild_mon_level`]).
///
/// The slot phase tries Magnet Pull with Steel, then Static with Electric,
/// before the ordinary draw (`TryGenerateWildMon`, `src/wild_encounter.c:430-436`).
/// A pulled slot skips the ordinary draw; a failed pull still takes it.
pub fn try_generate_wild_mon_land(
    land: &LandEncounters,
    slot_zero_ability: Option<AbilityId>,
    rng: &mut impl RandomSource,
) -> WildEncounter {
    let slot = try_ability_influenced_land_slot(
        land,
        slot_zero_ability,
        AbilityId::MAGNET_PULL,
        Type::Steel,
        rng,
    )
    .or_else(|| {
        try_ability_influenced_land_slot(
            land,
            slot_zero_ability,
            AbilityId::STATIC,
            Type::Electric,
            rng,
        )
    })
    .unwrap_or_else(|| choose_land_mon_index(rng));
    let mon = land.mons[slot];
    let level = choose_wild_mon_level(&mon, slot_zero_ability, rng);
    WildEncounter {
        species: mon.species,
        level,
        slot,
    }
}

/// The saved Repel state `IsWildLevelAllowedByRepel` consults
/// (`src/wild_encounter.c:874-895`). Evaluating it draws nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepelGate {
    /// `VAR_REPEL_STEP_COUNT` is zero: every level is accepted without a party read.
    #[default]
    Inactive,
    /// `VAR_REPEL_STEP_COUNT` is nonzero.
    Active {
        /// Level of the first party member (over all six slots) with nonzero HP and
        /// `MON_DATA_IS_EGG` clear, or `None` when no member qualifies.
        first_qualifying_level: Option<u8>,
    },
}

impl RepelGate {
    /// Whether a wild Pokémon of `wild_level` may be encountered: a level strictly
    /// below the first usable member's is rejected, and so is every level when no
    /// member is usable.
    #[must_use]
    pub const fn allows(self, wild_level: u8) -> bool {
        match self {
            Self::Inactive => true,
            Self::Active {
                first_qualifying_level: Some(party_level),
            } => wild_level >= party_level,
            Self::Active {
                first_qualifying_level: None,
            } => false,
        }
    }
}

/// [`try_generate_wild_mon_land`], then `repel` (`:448-455`): `None` when the gate
/// rejects the finished level. The gate draws nothing, so a rejection has spent the slot
/// and level draws and none of the later construction draws.
pub fn try_generate_wild_mon_land_with_repel(
    land: &LandEncounters,
    slot_zero_ability: Option<AbilityId>,
    repel: RepelGate,
    rng: &mut impl RandomSource,
) -> Option<WildEncounter> {
    let encounter = try_generate_wild_mon_land(land, slot_zero_ability, rng);
    repel.allows(encounter.level).then_some(encounter)
}

/// [`standard_wild_encounter_with_rate_modifiers`] with no flute flag and no Cleanse Tag.
pub fn standard_wild_encounter(
    header: Option<&WildEncounterHeader>,
    current_metatile_behavior: u8,
    previous_metatile_behavior: u8,
    slot_zero_ability: Option<AbilityId>,
    rng: &mut impl RandomSource,
) -> Option<WildEncounter> {
    standard_wild_encounter_with_rate_modifiers(
        header,
        current_metatile_behavior,
        previous_metatile_behavior,
        slot_zero_ability,
        EncounterRateModifiers::default(),
        rng,
    )
}

/// Rolls an ordinary land encounter for one completed step.
///
/// `header` is the already-resolved entry for the current map. `None`, a
/// non-land metatile, a missing land table, a rejected behavior change, or a
/// failed rate draw returns `None` before consuming any later draw.
/// `slot_zero_ability` is passed to [`choose_wild_mon_level`] and `rate_modifiers`
/// scale the rate draw ([`wild_encounter_check_with_rate_modifiers`]).
pub fn standard_wild_encounter_with_rate_modifiers(
    header: Option<&WildEncounterHeader>,
    current_metatile_behavior: u8,
    previous_metatile_behavior: u8,
    slot_zero_ability: Option<AbilityId>,
    rate_modifiers: EncounterRateModifiers,
    rng: &mut impl RandomSource,
) -> Option<WildEncounter> {
    standard_wild_encounter_with_repel(
        header,
        current_metatile_behavior,
        previous_metatile_behavior,
        slot_zero_ability,
        rate_modifiers,
        RepelGate::Inactive,
        rng,
    )
}

/// [`standard_wild_encounter_with_rate_modifiers`] that also applies `repel` to the
/// finished level ([`try_generate_wild_mon_land_with_repel`]). A rejection returns
/// `None` after the rate, slot and level draws.
pub fn standard_wild_encounter_with_repel(
    header: Option<&WildEncounterHeader>,
    current_metatile_behavior: u8,
    previous_metatile_behavior: u8,
    slot_zero_ability: Option<AbilityId>,
    rate_modifiers: EncounterRateModifiers,
    repel: RepelGate,
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
    if !wild_encounter_check_with_rate_modifiers(land.encounter_rate, rate_modifiers, rng) {
        return None;
    }
    try_generate_wild_mon_land_with_repel(land, slot_zero_ability, repel, rng)
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

    /// [`Self::check_standard_wild_encounter_with_rate_modifiers`] with no flute flag and
    /// no Cleanse Tag.
    pub fn check_standard_wild_encounter(
        &mut self,
        current_metatile_behavior: u8,
        header: Option<&WildEncounterHeader>,
        slot_zero_ability: Option<AbilityId>,
        rng: &mut impl RandomSource,
    ) -> Option<WildEncounter> {
        self.check_standard_wild_encounter_with_rate_modifiers(
            current_metatile_behavior,
            header,
            slot_zero_ability,
            EncounterRateModifiers::default(),
            rng,
        )
    }

    /// Rolls for an encounter and records the completed step's behavior.
    ///
    /// Immune steps consume no RNG. A successful encounter restarts immunity;
    /// failed eligible rolls, including a Repel rejection, leave the counter at its ceiling.
    /// `slot_zero_ability` is passed to [`choose_wild_mon_level`] and `rate_modifiers`
    /// to [`wild_encounter_check_with_rate_modifiers`].
    pub fn check_standard_wild_encounter_with_rate_modifiers(
        &mut self,
        current_metatile_behavior: u8,
        header: Option<&WildEncounterHeader>,
        slot_zero_ability: Option<AbilityId>,
        rate_modifiers: EncounterRateModifiers,
        rng: &mut impl RandomSource,
    ) -> Option<WildEncounter> {
        self.check_standard_wild_encounter_with_repel(
            current_metatile_behavior,
            header,
            slot_zero_ability,
            rate_modifiers,
            RepelGate::Inactive,
            rng,
        )
    }

    /// [`Self::check_standard_wild_encounter_with_rate_modifiers`] with `repel` applied to
    /// the finished level; a rejected encounter does not restart immunity.
    pub fn check_standard_wild_encounter_with_repel(
        &mut self,
        current_metatile_behavior: u8,
        header: Option<&WildEncounterHeader>,
        slot_zero_ability: Option<AbilityId>,
        rate_modifiers: EncounterRateModifiers,
        repel: RepelGate,
        rng: &mut impl RandomSource,
    ) -> Option<WildEncounter> {
        let encounter = if self.immunity_steps < WILD_ENCOUNTER_IMMUNITY_STEPS {
            self.immunity_steps += 1;
            None
        } else {
            standard_wild_encounter_with_repel(
                header,
                current_metatile_behavior,
                self.prev_metatile_behavior,
                slot_zero_ability,
                rate_modifiers,
                repel,
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
