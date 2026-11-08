//! Effort-value storage and gains for [`BattlePokemon`].
//!
//! A new battler starts with zero EVs. Loaded EVs retain their raw save values,
//! while battle rewards cap new gains. Reward EVs are applied before
//! [`BattlePokemon::apply_experience`], so a level-up recalculation sees the
//! latest gain. This battle model has no Pokérus or held-item EV multipliers.
//!
//! [`BattlePokemon::cached_stats`] is the authoritative stat block: built at
//! zero EVs for a new battler, adopted verbatim by
//! [`BattlePokemon::with_cached_stats`] for a loaded one, and replaced from the
//! current EVs only by a level-up (`CalculateMonStats`, `src/pokemon.c:2823-2895`).
//! Gaining EVs never recomputes it. [`BattlePokemon::stats`] is the same block
//! except Deoxys's five Speed Forme stats, which upstream `GetDeoxysStat`
//! computes from the current EVs.

use assets::EvYield;

use super::{BattlePokemon, Stats};

/// Maximum value [`BattlePokemon::gain_evs`] can store for one stat.
pub const MAX_PER_STAT_EVS: u16 = 255;

/// Maximum total that [`BattlePokemon::gain_evs`] can grant across all stats.
pub const MAX_TOTAL_EVS: u16 = 510;

/// Raw stored effort values for all six stats.
///
/// Saved values are not required to satisfy [`MAX_TOTAL_EVS`].
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
    /// Returns values in HP, Attack, Defense, Speed, Special Attack, and
    /// Special Defense order.
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

    const fn from_array([hp, attack, defense, speed, sp_attack, sp_defense]: [u8; 6]) -> Self {
        Self {
            hp,
            attack,
            defense,
            speed,
            sp_attack,
            sp_defense,
        }
    }
}

impl BattlePokemon {
    /// Returns the current effort values.
    #[must_use]
    pub const fn evs(&self) -> Evs {
        self.evs
    }

    /// Adopts raw loaded EVs without validating caps.
    ///
    /// The stat cache is not recomputed; only Deoxys's live Speed Forme stats
    /// are derived from `evs`.
    #[must_use]
    pub fn with_evs(mut self, evs: Evs) -> Self {
        self.evs = evs;
        self.apply_speed_forme_evs();
        self
    }

    /// Adopts a loaded, authoritative stat block and current HP verbatim.
    ///
    /// Upstream copies a party record into battle without recalculating it
    /// (`src/load_save.c:160-178`), so the block may legitimately lag the
    /// mon's current [`BattlePokemon::evs`]; neither value is recomputed,
    /// clamped, or normalized here. Call this before
    /// [`BattlePokemon::with_evs`] when both are loaded: `with_evs` then
    /// derives Deoxys's Speed Forme live view from the adopted EVs, while the
    /// stored block ([`BattlePokemon::cached_stats`]) stays as loaded.
    #[must_use]
    pub fn with_cached_stats(mut self, stats: Stats, current_hp: u32) -> Self {
        self.stats = stats;
        self.cached_stats = stats;
        self.current_hp = current_hp;
        self
    }

    /// Adds a defeated species' base EV yield to the current values.
    ///
    /// Each stat's gain is limited by the remaining total capacity and then by
    /// that stat's capacity. Stats are processed in [`Evs::as_array`] order,
    /// and processing stops when the total reaches [`MAX_TOTAL_EVS`]. This does
    /// not recompute live stats; Deoxys's Speed Forme stats pick the new EVs up
    /// when the next battle is constructed.
    ///
    /// Battle reward settlement calls this before
    /// [`BattlePokemon::apply_experience`].
    pub fn gain_evs(&mut self, ev_yield: EvYield) {
        let stat_yields = [
            ev_yield.hp,
            ev_yield.attack,
            ev_yield.defense,
            ev_yield.speed,
            ev_yield.sp_attack,
            ev_yield.sp_defense,
        ];
        let mut stat_evs = self.evs.as_array();
        let mut total_evs: u16 = stat_evs.iter().copied().map(u16::from).sum();

        for (stat_ev, stat_yield) in stat_evs.iter_mut().zip(stat_yields) {
            if total_evs >= MAX_TOTAL_EVS {
                break;
            }

            let remaining_total_capacity = MAX_TOTAL_EVS - total_evs;
            let remaining_stat_capacity = MAX_PER_STAT_EVS - u16::from(*stat_ev);
            let increase = u16::from(stat_yield)
                .min(remaining_total_capacity)
                .min(remaining_stat_capacity);

            *stat_ev = u8::try_from(u16::from(*stat_ev) + increase).unwrap_or(u8::MAX);
            total_evs += increase;
        }

        self.evs = Evs::from_array(stat_evs);
    }
}
