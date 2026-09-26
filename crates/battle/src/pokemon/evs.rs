//! Effort-value storage and gains for [`BattlePokemon`].
//!
//! A new battler starts with zero EVs. Loaded EVs retain their raw save values,
//! while battle rewards cap new gains. Reward EVs are applied before
//! [`BattlePokemon::apply_experience`], so a level-up snapshot includes the
//! latest gain. This battle model has no Pokérus or held-item EV multipliers.
//!
//! Live [`BattlePokemon::stats`] remain zero-EV calculations. Save-time stat
//! recomputation instead reads [`BattlePokemon::evs_at_last_level_up`].

use assets::EvYield;

use super::BattlePokemon;

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

    /// Returns the EV snapshot captured at the most recent level-up.
    ///
    /// Save-time stat recomputation uses this snapshot rather than the current
    /// [`BattlePokemon::evs`].
    #[must_use]
    pub const fn evs_at_last_level_up(&self) -> Evs {
        self.evs_at_last_level_up
    }

    /// Adopts raw loaded EVs without validating caps or recomputing live stats.
    #[must_use]
    pub const fn with_evs(mut self, evs: Evs) -> Self {
        self.evs = evs;
        self
    }

    /// Adds a defeated species' base EV yield to the current values.
    ///
    /// Each stat's gain is limited by the remaining total capacity and then by
    /// that stat's capacity. Stats are processed in [`Evs::as_array`] order,
    /// and processing stops when the total reaches [`MAX_TOTAL_EVS`]. This does
    /// not recompute live stats.
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
