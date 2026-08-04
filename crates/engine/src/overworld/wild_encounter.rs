//! Per-step wild-encounter rolls (I-4, issue #169), ported from
//! `pokeemerald/src/wild_encounter.c` and the step bookkeeping around it in
//! `src/field_control_avatar.c`.
//!
//! This module owns the *roll* — the engine-domain decision "does a wild
//! Pokémon appear on this step, and if so which species at which level"
//! `(oop-boundaries)`. It stops at a [`WildEncounter`] value: constructing the
//! actual battle-ready mon and starting a `battle::Battle` around it is the
//! integration layer's job (`crates/pokeemerald-rs/src/flow/wild_encounter.rs`),
//! because that crosses into the `battle` crate this one deliberately does not
//! depend on.
//!
//! # The draw order this reproduces
//!
//! Upstream's land path, from the top of `StandardWildEncounter`
//! (`wild_encounter.c:552`), against a fresh save standing in Route 101's
//! grass:
//!
//! 1. `GetCurrentMapWildMonHeaderId` (`:305`) — a table lookup, no draw. Here
//!    the caller passes the already-resolved [`WildEncounterHeader`].
//! 2. `MetatileBehavior_IsLandWildEncounter(curMetatileBehavior)` (`:595`) —
//!    no draw. [`super::metatile_behavior::is_land_wild_encounter`].
//! 3. `landMonsInfo == NULL` (`:597`) — no draw.
//! 4. `prevMetatileBehavior != curMetatileBehavior && !AllowWildCheckOnNewMetatile()`
//!    (`:599`) — **one draw, only when the behavior changed**
//!    ([`allow_wild_check_on_new_metatile`], `:533-539`).
//! 5. `WildEncounterCheck(encounterRate, FALSE)` (`:601`) — **one draw**
//!    ([`wild_encounter_check`] -> [`encounter_odds_check`], `:502-529`).
//! 6. `TryStartRoamerEncounter()` (`:604`) — no draw on a fresh save: it draws
//!    only once a roamer is actually on this map (`roamer.c:218` short-circuits
//!    on `IsRoamerAt` before its `Random() % 4`), and no roamer exists until a
//!    legendary is released.
//! 7. `DoMassOutbreakEncounterTest()` (`:615`, `:481-491`) — no draw on a
//!    fresh save either: its `Random() % 100` sits behind
//!    `outbreakPokemonSpecies != SPECIES_NONE`, and a new game has none.
//! 8. `TryGenerateWildMon(..., WILD_AREA_LAND, ...)` (`:622`, `:422-455`) —
//!    **two draws**: `ChooseWildMonIndex_Land` (`:182-210`) then
//!    `ChooseWildMonLevel` (`:268-303`).
//!
//! So a same-behavior step that produces an encounter costs exactly **three**
//! draws here (rate, slot, level), and a step onto a *different* behavior
//! costs **four** (the new-metatile check first). A step that fails the rate
//! check costs one (or two). Those counts are pinned by this module's own
//! tests, because a caller downstream of them — the wild mon's own
//! nature/personality/IV draws (`battle::wild::build_wild_pokemon`) — reads
//! the very next values off the same stream.
//!
//! # What is deliberately NOT modelled
//!
//! Every omission below is an upstream branch that this port cannot reach yet
//! (no abilities, no repel, no bike, no surfing, no roamers, no outbreaks, no
//! Safari/Battle Frontier modes) and that, in the state a v1 first encounter
//! actually happens in, upstream itself does not take — so the draw counts
//! above hold exactly rather than approximately:
//!
//! - **`WildEncounterCheck`'s rate modifiers** (`:504-525`): the
//!   Mach/Acro bike `* 80 / 100`, `ApplyFluteEncounterRateMod`
//!   (`FLAG_SYS_ENC_UP_ITEM`/`FLAG_SYS_ENC_DOWN_ITEM`, White/Black Flute),
//!   `ApplyCleanseTagEncounterRateMod` (a held Cleanse Tag), and the lead
//!   mon's Stench / Illuminate / White Smoke / Arena Trap / Sand Veil ability
//!   scaling. None draw; all are inert for a player on foot with no flute,
//!   no held item, and no such ability. The `* 16` scaling and the
//!   [`MAX_ENCOUNTER_RATE`] clamp around them *are* modelled.
//! - **`TryGenerateWildMon`'s ability-influenced slot pick** (`:430-434`):
//!   `TryGetAbilityInfluencedWildMonIndex` for Magnet Pull (Steel) and Static
//!   (Electric). It *does* draw (`Random() % 2`, `:945`) — but only after
//!   confirming the lead mon's ability is the one in question, which no
//!   starter's is (Overgrow/Blaze/Torrent).
//! - **`WILD_CHECK_REPEL`** (`:450`, `IsWildLevelAllowedByRepel` `:874-895`):
//!   no draw; inert with `VAR_REPEL_STEP_COUNT == 0`.
//! - **`WILD_CHECK_KEEN_EYE`** (`:452`, `IsAbilityAllowingEncounter`
//!   `:897-913`): draws `Random() % 2`, but only for a lead mon with Keen Eye
//!   or Intimidate that also out-levels the wild mon by 5+.
//! - **`sWildEncountersDisabled`** (`:557`, set by `DisableWildEncounters`
//!   from scripts) — there is no script engine to set it.
//! - **The water/rock-smash/fishing/Feebas paths**, the Battle Pike and
//!   Battle Pyramid header branches (`:564-591`), `SweetScentWildEncounter`,
//!   and `AreLegendariesInSootopolisPreventingEncounters`.
//! - **Altering Cave's `VAR_ALTERING_CAVE_WILD_SET` header offset**
//!   (`:321`): header resolution is the caller's, and
//!   `assets::WildEncounterTable` returns the *first* of that map's nine
//!   entries (its own module docs) — correct for set 0, the value a fresh
//!   save has.
//!
//! Each of these is enumerated again on the ledger entry for
//! `src/wild_encounter.c#StandardWildEncounter` so the gap is recorded where
//! coverage is tracked, not only here.

use assets::{LandEncounters, SpeciesId, WildEncounterHeader, WildPokemon};

use super::metatile_behavior::is_land_wild_encounter;
use crate::rng::RandomSource;

/// `MAX_ENCOUNTER_RATE` (`pokeemerald/src/wild_encounter.c:27`): the modulus
/// [`encounter_odds_check`] draws against, and the ceiling
/// [`wild_encounter_check`] clamps a scaled rate to.
pub const MAX_ENCOUNTER_RATE: u32 = 2880;

/// The per-slot chances of a land encounter table's twelve slots, in slot
/// order — upstream's `ENCOUNTER_CHANCE_LAND_MONS_SLOT_*` thresholds.
///
/// Upstream spells these as a chain of *cumulative* `#define`s generated from
/// `src/data/wild_encounters.json`'s `fields[0].encounter_rates`
/// (`land_mons`) into `constants/wild_encounter.h` — a build-generated header
/// this reference checkout does not carry, exactly as
/// `assets::wild_encounters`' own module docs note for the slot counts. The
/// per-slot chances are transcribed from that JSON (the canonical source) and
/// accumulated by [`land_slot_for_roll`] instead of restating twelve
/// pre-summed constants `(no-verbatim)`.
pub const LAND_SLOT_CHANCES: [u8; assets::LAND_SLOTS] = [20, 20, 10, 10, 10, 10, 5, 5, 4, 4, 1, 1];

/// `ENCOUNTER_CHANCE_LAND_MONS_TOTAL` — the sum of [`LAND_SLOT_CHANCES`], and
/// the modulus `ChooseWildMonIndex_Land` draws against
/// (`wild_encounter.c:184`).
pub const LAND_SLOT_TOTAL: u16 = {
    let mut total = 0u16;
    let mut i = 0;
    while i < LAND_SLOT_CHANCES.len() {
        total += LAND_SLOT_CHANCES[i] as u16;
        i += 1;
    }
    total
};

/// How many steps after a map transition are immune to encounters —
/// `CheckStandardWildEncounter`'s `sWildEncounterImmunitySteps < 4` gate
/// (`pokeemerald/src/field_control_avatar.c:670`).
pub const WILD_ENCOUNTER_IMMUNITY_STEPS: u8 = 4;

/// A rolled land encounter: everything `TryGenerateWildMon`
/// (`wild_encounter.c:422-455`) decides before handing off to `CreateWildMon`.
///
/// Upstream writes the result straight into `gEnemyParty[0]`; here it is a
/// value the caller builds a battle mon from, so the roll stays free of
/// global mutable state `(oop-boundaries)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WildEncounter {
    /// The species that appeared.
    pub species: SpeciesId,
    /// The level `ChooseWildMonLevel` picked.
    pub level: u8,
    /// Which of the table's twelve slots was picked (`0..`[`assets::LAND_SLOTS`]).
    /// Not something upstream keeps past the call, but the single most useful
    /// thing to assert a slot-pick draw against — several Route 101 slots
    /// share a species, so the species alone cannot identify the slot.
    pub slot: usize,
}

/// Which land slot a `0..`[`LAND_SLOT_TOTAL`] roll selects —
/// `ChooseWildMonIndex_Land`'s threshold chain (`wild_encounter.c:182-210`),
/// re-expressed as a cumulative scan over [`LAND_SLOT_CHANCES`]
/// `(no-verbatim)`.
///
/// A `roll` at or above [`LAND_SLOT_TOTAL`] falls out as the last slot,
/// matching upstream's own unguarded final `else return 11` — unreachable
/// from [`choose_land_mon_index`], which takes the roll modulo the total.
#[must_use]
pub fn land_slot_for_roll(roll: u16) -> usize {
    let mut cumulative = 0u16;
    for (slot, chance) in LAND_SLOT_CHANCES.iter().enumerate() {
        cumulative += u16::from(*chance);
        if roll < cumulative {
            return slot;
        }
    }
    LAND_SLOT_CHANCES.len() - 1
}

/// `ChooseWildMonIndex_Land` (`wild_encounter.c:182-210`): one `Random()`
/// draw, taken modulo [`LAND_SLOT_TOTAL`] and resolved by
/// [`land_slot_for_roll`].
pub fn choose_land_mon_index(rng: &mut impl RandomSource) -> usize {
    land_slot_for_roll(rng.next_u16() % LAND_SLOT_TOTAL)
}

/// The level a `roll` picks out of a slot's `min_level..=max_level` band —
/// `ChooseWildMonLevel`'s arithmetic (`wild_encounter.c:268-303`) without its
/// draw.
///
/// Upstream orders the two bounds first ("make sure minimum level is less
/// than maximum level", `:275-285`), so an inverted table entry still yields a
/// level inside the band rather than underflowing; that swap is reproduced
/// here even though no real `gWildMonHeaders` entry needs it.
///
/// The Hustle / Vital Spirit / Pressure branch that follows (`:293-300`,
/// a further `Random() % 2` biasing the result toward `max`) is **not**
/// modelled — abilities are out of scope, and no starter has one of the
/// three.
#[must_use]
pub fn level_for_roll(min_level: u8, max_level: u8, roll: u16) -> u8 {
    let (min, max) = if max_level >= min_level {
        (min_level, max_level)
    } else {
        (max_level, min_level)
    };
    let range = u16::from(max - min) + 1;
    let level = u16::from(min) + (roll % range);
    // `level <= max <= u8::MAX` by construction, so the fallback is
    // unreachable; it is `max` rather than a panic so a future caller cannot
    // turn a table typo into a crash.
    u8::try_from(level).unwrap_or(max)
}

/// `ChooseWildMonLevel` (`wild_encounter.c:268-303`): one `Random()` draw
/// resolved by [`level_for_roll`].
pub fn choose_wild_mon_level(mon: &WildPokemon, rng: &mut impl RandomSource) -> u8 {
    level_for_roll(mon.min_level, mon.max_level, rng.next_u16())
}

/// `EncounterOddsCheck` (`wild_encounter.c:493-500`): one `Random()` draw,
/// `Random() % MAX_ENCOUNTER_RATE < encounter_rate`.
pub fn encounter_odds_check(encounter_rate: u32, rng: &mut impl RandomSource) -> bool {
    u32::from(rng.next_u16()) % MAX_ENCOUNTER_RATE < encounter_rate
}

/// `WildEncounterCheck` (`wild_encounter.c:502-529`): scale the table's
/// `encounterRate` by 16, clamp to [`MAX_ENCOUNTER_RATE`], then
/// [`encounter_odds_check`] — one draw, always taken.
///
/// The bike / flute / Cleanse Tag / ability modifiers between the scaling and
/// the clamp are not modelled (module docs) — none of them draws, so the draw
/// count is upstream's regardless, and every one of them is inert for a
/// player on foot with no flute, held item, or encounter-rate ability.
///
/// Route 101's `encounter_rate` of `20` therefore becomes `320/2880`
/// (≈11.1% per eligible step), which is the real game's Route 101 rate.
pub fn wild_encounter_check(encounter_rate: u8, rng: &mut impl RandomSource) -> bool {
    let rate = (u32::from(encounter_rate) * 16).min(MAX_ENCOUNTER_RATE);
    encounter_odds_check(rate, rng)
}

/// `AllowWildCheckOnNewMetatile` (`wild_encounter.c:531-539`): stepping onto
/// a tile whose *behavior* differs from the previous step's skips the whole
/// encounter check 40% of the time. One `Random()` draw; `true` means "go
/// ahead and check".
pub fn allow_wild_check_on_new_metatile(rng: &mut impl RandomSource) -> bool {
    rng.next_u16() % 100 < 60
}

/// `TryGenerateWildMon(..., WILD_AREA_LAND, ...)` (`wild_encounter.c:422-455`)
/// down to its slot and level picks: two draws, slot then level.
///
/// The ability-influenced slot pick before them and the repel / Keen Eye
/// gates after them are not modelled (module docs), so this cannot fail the
/// way upstream's `bool8` can — an eligible step that reaches here always
/// produces an encounter.
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

/// `StandardWildEncounter`'s land branch (`wild_encounter.c:552-628`) for one
/// completed step: `None` when no wild Pokémon appears.
///
/// `header` is the already-resolved `gWildMonHeaders` entry for the current
/// map (`GetCurrentMapWildMonHeaderId`, `:305-333`); `None` models
/// `HEADER_NONE` — a map with no wild table at all, where upstream falls
/// through to its Battle Pike / Battle Pyramid branches and otherwise returns
/// `FALSE`.
///
/// See the module docs for the exact draw order and for everything between
/// these gates that upstream would evaluate but this port does not.
pub fn standard_wild_encounter(
    header: Option<&WildEncounterHeader>,
    cur_metatile_behavior: u8,
    prev_metatile_behavior: u8,
    rng: &mut impl RandomSource,
) -> Option<WildEncounter> {
    // `headerId == HEADER_NONE` (`:559`) and
    // `MetatileBehavior_IsLandWildEncounter` (`:595`) — both before any draw,
    // so an ordinary step on ordinary ground costs nothing.
    let header = header?;
    if !is_land_wild_encounter(cur_metatile_behavior) {
        return None;
    }
    // `gWildMonHeaders[headerId].landMonsInfo == NULL` (`:597`): a map with a
    // wild table but no *land* table (a purely aquatic one) rolls nothing on
    // foot.
    let land = header.land.as_ref()?;
    // `:599` — the new-metatile skip only draws when the behavior changed.
    if prev_metatile_behavior != cur_metatile_behavior && !allow_wild_check_on_new_metatile(rng) {
        return None;
    }
    if !wild_encounter_check(land.encounter_rate, rng) {
        return None;
    }
    Some(try_generate_wild_mon_land(land, rng))
}

/// The per-step encounter bookkeeping `CheckStandardWildEncounter` keeps in
/// two file statics (`pokeemerald/src/field_control_avatar.c:38-39`), as an
/// owned value instead `(oop-boundaries)`.
///
/// Upstream's `sWildEncounterImmunitySteps` counts the first four steps after
/// a map transition and suppresses the roll entirely on each, and
/// `sPrevMetatileBehavior` remembers the behavior the previous step ended on
/// so `AllowWildCheckOnNewMetatile` can tell a behavior change from a
/// continuation. Both are updated on *every* step, including suppressed ones.
#[derive(Debug, Clone, Default)]
pub struct WildEncounterState {
    /// `sWildEncounterImmunitySteps` (`field_control_avatar.c:38`): counts up
    /// to [`WILD_ENCOUNTER_IMMUNITY_STEPS`], then stops and stays there.
    immunity_steps: u8,
    /// `sPrevMetatileBehavior` (`:39`): the behavior the *previous* step
    /// ended on. Starts at `MB_NORMAL` (`0`), matching upstream's zeroed
    /// EWRAM, and is deliberately **not** reset by
    /// [`Self::restart_immunity_steps`] — upstream's
    /// `RestartWildEncounterImmunitySteps` (`:662-666`) touches only the
    /// step counter.
    prev_metatile_behavior: u8,
}

impl WildEncounterState {
    /// A freshly zeroed state — upstream's EWRAM statics at boot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            immunity_steps: 0,
            prev_metatile_behavior: 0,
        }
    }

    /// `RestartWildEncounterImmunitySteps` (`field_control_avatar.c:662-666`):
    /// grant four more encounter-free steps.
    ///
    /// Upstream calls this from every full map load — `LoadMapFromWarp`
    /// (`src/overworld.c:850`) and `LoadMapFromCameraTransition`, the
    /// map-edge connection crossing (`:800`) — and again when a battle
    /// starts (`src/battle_setup.c:370`, `:941`), so the walk back out of a
    /// battle is not immediately interrupted by another one.
    pub fn restart_immunity_steps(&mut self) {
        self.immunity_steps = 0;
    }

    /// How many of the [`WILD_ENCOUNTER_IMMUNITY_STEPS`] immune steps have
    /// been used. Exposed for tests and diagnostics; nothing branches on it
    /// outside [`Self::check_standard_wild_encounter`].
    #[must_use]
    pub const fn immunity_steps(&self) -> u8 {
        self.immunity_steps
    }

    /// The behavior the previous step ended on (`sPrevMetatileBehavior`).
    #[must_use]
    pub const fn prev_metatile_behavior(&self) -> u8 {
        self.prev_metatile_behavior
    }

    /// `CheckStandardWildEncounter` (`field_control_avatar.c:668-686`): roll
    /// for one completed step onto a tile with `cur_metatile_behavior`.
    ///
    /// The first [`WILD_ENCOUNTER_IMMUNITY_STEPS`] steps after a transition
    /// consume the counter and return `None` **without drawing** — upstream
    /// returns before `StandardWildEncounter` is even called, so the immunity
    /// window is invisible to the RNG stream, not merely to the player. A
    /// fired encounter resets the counter, granting four more immune steps
    /// after the battle. Either way `sPrevMetatileBehavior` is updated.
    pub fn check_standard_wild_encounter(
        &mut self,
        cur_metatile_behavior: u8,
        header: Option<&WildEncounterHeader>,
        rng: &mut impl RandomSource,
    ) -> Option<WildEncounter> {
        if self.immunity_steps < WILD_ENCOUNTER_IMMUNITY_STEPS {
            self.immunity_steps += 1;
            self.prev_metatile_behavior = cur_metatile_behavior;
            return None;
        }
        let encounter = standard_wild_encounter(
            header,
            cur_metatile_behavior,
            self.prev_metatile_behavior,
            rng,
        );
        if encounter.is_some() {
            self.immunity_steps = 0;
        }
        self.prev_metatile_behavior = cur_metatile_behavior;
        encounter
    }
}

#[cfg(test)]
mod tests;
