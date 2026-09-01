//! Effort values: how [`BattlePokemon`] adopts and gains them, and the one
//! line [`BattlePokemon::stats`] refuses to cross because of them.
//!
//! Every mon [`BattlePokemon::new`] builds starts at `0` EVs.
//! [`BattlePokemon::with_evs`] adopts a loaded record's own retained bytes
//! at the save boundary, and [`BattlePokemon::gain_evs`] is `MonGainEVs`
//! (`pokeemerald/src/pokemon.c:5988`-`:6064`), applied on every KO before
//! [`BattlePokemon::apply_experience`] -- upstream's own order. Neither the
//! Pokérus nor the Macho Brace doubling applies: this crate carries
//! neither, so every award reaching [`BattlePokemon::gain_evs`] is
//! upstream's un-multiplied base yield.
//!
//! [`BattlePokemon::stats`] itself never becomes EV-aware -- not at
//! construction, not after [`BattlePokemon::with_evs`], and not across any
//! in-battle level-up ([`BattlePokemon::raise_level_to_experience`]) --
//! however many real EVs this mon carries. `pokeemerald-rs::party`'s own
//! load-clamp rebase system depends on that floor never moving, to
//! consistently measure how many points a retained or freshly recomputed
//! save-file maximum sits above it; only that module's own save-time
//! recompute is EV-aware, and it is fed
//! [`BattlePokemon::evs_at_last_level_up`] rather than the live
//! [`BattlePokemon::evs`] -- see that field's own doc for why the two must
//! differ.

use assets::EvYield;

use super::BattlePokemon;

/// Largest effort value a single stat can hold (`MAX_PER_STAT_EVS`,
/// `pokeemerald/include/constants/pokemon.h:203`) — [`BattlePokemon::gain_evs`]'s
/// per-stat cap.
pub const MAX_PER_STAT_EVS: u16 = 255;

/// Largest sum of all six effort values (`MAX_TOTAL_EVS`,
/// `pokeemerald/include/constants/pokemon.h:204`) — [`BattlePokemon::gain_evs`]'s
/// whole-mon cap, applied before [`MAX_PER_STAT_EVS`] (upstream's own order).
pub const MAX_TOTAL_EVS: u16 = 510;

/// Stored effort values for all six stats.
///
/// Each byte accepts `0..=255`. The stat formula divides by four, so values
/// `252..=255` all provide the maximum contribution. [`BattlePokemon::evs`]'s
/// type, and also the standalone value [`super::compute_stats_with_evs`]
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
    /// order — the same order [`super::Ivs::as_array`] uses.
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

impl BattlePokemon {
    /// This mon's live effort values. See [`BattlePokemon::gain_evs`] and
    /// [`BattlePokemon::with_evs`] for how they change, and this module's
    /// own docs for what carrying them does, and does not, change about
    /// [`BattlePokemon::stats`].
    #[must_use]
    pub const fn evs(&self) -> Evs {
        self.evs
    }

    /// [`BattlePokemon::evs_at_last_level_up`]'s own field doc: the EV set
    /// the most recent level-up's `CalculateMonStats` would have cached,
    /// distinct from this mon's current, possibly KO-incremented-since,
    /// [`BattlePokemon::evs`]. `pokeemerald-rs::party`'s save-time recompute
    /// is this method's one caller.
    #[must_use]
    pub const fn evs_at_last_level_up(&self) -> Evs {
        self.evs_at_last_level_up
    }

    /// Adopts a saved record's own EV bytes at the boundary that restores
    /// this Pokémon — the same position as
    /// [`BattlePokemon::with_original_trainer_id`], immediately after
    /// [`BattlePokemon::new`], which otherwise leaves every mon at `0` EVs.
    /// Deliberately does **not** recompute [`BattlePokemon::stats`] (this
    /// module's own docs). Every byte is accepted: upstream's EV fields are
    /// unconstrained `u8`s -- [`MAX_PER_STAT_EVS`] and [`MAX_TOTAL_EVS`]
    /// bound only what [`BattlePokemon::gain_evs`] writes, not what a
    /// hand-edited save can already hold.
    #[must_use]
    pub const fn with_evs(mut self, evs: Evs) -> Self {
        self.evs = evs;
        self
    }

    /// `MonGainEVs` (`pokeemerald/src/pokemon.c:5988`-`:6064`): adds
    /// `ev_yield`'s per-stat award to [`BattlePokemon::evs`], capping the
    /// running total at [`MAX_TOTAL_EVS`] before each stat's own value at
    /// [`MAX_PER_STAT_EVS`] — upstream's own order (`:6051`-`:6059`). The
    /// loop stops entirely, not just for the current stat, once the running
    /// total reaches [`MAX_TOTAL_EVS`], exactly as upstream's `break`
    /// (`:6005`-`:6006`) does, so a later stat gets no award once the mon is
    /// full.
    ///
    /// Called from [`crate::battle::Battle::settle_win_reward`] on every KO,
    /// **before** [`BattlePokemon::apply_experience`] — upstream's own order
    /// (`Cmd_getexp` case 2's `MonGainEVs` call precedes case 3's stat
    /// recompute).
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
}
