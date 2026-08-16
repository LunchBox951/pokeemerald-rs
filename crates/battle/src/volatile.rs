//! Volatile in-battle state (S-6, issue #293): the `gBattleMons[].status2`
//! and `gStatuses3[]` bits this crate models, plus the `gDisableStructs[]`
//! timers that go with them.
//!
//! Upstream keeps three parallel stores of "condition" per battler and this
//! module owns the two that vanish when the battler leaves the field:
//!
//! | store | lives in | survives switching out? | modelled by |
//! |---|---|---|---|
//! | `status1` | `struct Pokemon` (the party record) | **yes** | [`crate::status`] |
//! | `status2` | `gBattleMons[]` only | mostly no | this module |
//! | `gStatuses3[]` + `gDisableStructs[]` | battle scratch | no | this module |
//!
//! Only the bits a modelled move can actually set are represented, for the
//! same reason [`crate::stat_change`] transcribes only real thunks: an
//! unreachable bit is dead data, and a bit that *is* reachable has to be
//! carried in whatever the rest of the engine reads it with.
//!
//! # The three bits, and who reads each
//!
//! - **`STATUS2_FOCUS_ENERGY`** (`1 << 20`, `include/constants/battle.h:144`),
//!   set by `Cmd_setfocusenergy` (`src/battle_script_commands.c:7758`). Read
//!   only by `Cmd_critcalc`, which adds `2 * (status2 & STATUS2_FOCUS_ENERGY
//!   != 0)` to the crit-chance stage (`:1267`) —
//!   [`crate::critical::crit_stage`]. It is one of the five bits
//!   `SwitchInClearSetData` deliberately **preserves** across a switch
//!   (`src/battle_main.c:3175`), which this crate has no way to exercise
//!   (the only switch it performs replaces a *fainted* mon with a fresh one
//!   from the bench) but which its docs record so a future switching slice
//!   does not clear it.
//! - **`STATUS2_DEFENSE_CURL`** (`1 << 30`, `:154`), set by
//!   `Cmd_setdefensecurlbit` (`:8860`). Read only by
//!   `Cmd_setrolloutcounter`'s power ramp (`:8564`-`:8565`), which doubles
//!   Rollout's already-doubling base power. Rollout is **not** modelled this
//!   slice, so nothing reads this bit yet — it is carried because Defense
//!   Curl sets it unconditionally (before, and independently of, its own
//!   stat raise) and dropping it would make a later Rollout slice silently
//!   half-powered. Unlike Focus Energy it is *not* in the switch-preserve
//!   mask.
//! - **`STATUS3_CHARGED_UP`** (`1 << 9`, `:166`) plus
//!   `gDisableStructs[].chargeTimer`, both set by `Cmd_setcharge` (`:9102`-
//!   `:9104`). Read by `Cmd_damagecalc`: `if (charged && move type ==
//!   TYPE_ELECTRIC) gBattleMoveDamage *= 2` (`:1298`-`:1299`) — see
//!   [`crate::damage`]'s pipeline and [`Volatiles::charged_up`]. The timer is
//!   decremented once per end-of-turn by `ENDTURN_CHARGE`
//!   (`src/battle_util.c:1743`-`:1745`), so the boost covers the Charge turn
//!   itself and exactly one turn after it.
//!
//! # RNG
//!
//! Nothing in this module draws. Every setter is a plain flag write, every
//! reader is arithmetic, and the one timer ticks deterministically.

/// The volatile conditions one battler carries — `status2`/`gStatuses3`
/// bits and their timers, as far as this crate models them
/// `(oop-boundaries)`.
///
/// Every field is `Default`-empty, matching `BattleStartClearSetData`
/// zeroing the whole `gBattleMons[]` entry (`src/battle_main.c:3034`) and
/// `SwitchInClearSetData` doing the same on send-out (`:3161`), and a
/// battler that leaves the field is simply replaced by a fresh
/// [`crate::pokemon::BattlePokemon`] rather than having its volatiles
/// cleared in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Volatiles {
    /// `STATUS2_FOCUS_ENERGY`: this battler is "getting pumped", worth `+2`
    /// crit-chance stages ([`crate::critical::crit_stage`]).
    pub focus_energy: bool,
    /// `STATUS2_DEFENSE_CURL`: this battler has used Defense Curl, which
    /// upstream's Rollout reads to double its power. Nothing reads it here
    /// yet — see the module docs.
    pub defense_curl: bool,
    /// `gDisableStructs[].chargeTimer` — `2` the moment Charge resolves,
    /// decremented once at the end of each turn. `STATUS3_CHARGED_UP` is not
    /// stored separately: upstream sets and clears the flag in lockstep with
    /// this timer reaching `0` (`Cmd_setcharge` at
    /// `src/battle_script_commands.c:9102`-`:9104`, `ENDTURN_CHARGE` at
    /// `src/battle_util.c:1744`-`:1745`), so one counter represents both
    /// without any state the other could contradict `(oop-boundaries)`.
    pub charge_timer: u8,
}

impl Volatiles {
    /// `Cmd_setcharge`'s timer value (`src/battle_script_commands.c:9103`):
    /// `chargeTimer = 2`.
    pub const CHARGE_TURNS: u8 = 2;

    /// Whether `STATUS3_CHARGED_UP` is set — the flag `Cmd_damagecalc` tests
    /// before doubling an Electric move's damage
    /// (`src/battle_script_commands.c:1298`).
    #[must_use]
    pub const fn charged_up(self) -> bool {
        self.charge_timer > 0
    }

    /// `Cmd_setfocusenergy` (`src/battle_script_commands.c:7758`), reduced to
    /// the branch its script can actually reach: set the bit.
    ///
    /// The command's own `else` — "already pumped, fail" (`:7751`-`:7755`) —
    /// is dead code from `BattleScript_EffectFocusEnergy`, whose
    /// `jumpifstatus2 BS_ATTACKER, STATUS2_FOCUS_ENERGY,
    /// BattleScript_ButItFailed` (`data/battle_scripts_1.s:889`) diverts one
    /// instruction earlier. [`crate::status_move`] reproduces the *script's*
    /// check, which is why this setter is unconditional.
    pub const fn set_focus_energy(&mut self) {
        self.focus_energy = true;
    }

    /// `Cmd_setdefensecurlbit` (`:8860`): set the bit, unconditionally.
    ///
    /// Unconditional matters: `BattleScript_EffectDefenseCurl` runs
    /// `setdefensecurlbit` **before** its `setstatchanger`/`statbuffchange`
    /// pair (`data/battle_scripts_1.s:2018`-`:2020`), so a user already at
    /// `+6` Defense still gets the Rollout flag even though the stat raise
    /// reports "won't go any higher!".
    pub const fn set_defense_curl(&mut self) {
        self.defense_curl = true;
    }

    /// `Cmd_setcharge` (`:9102`-`:9104`): raise the flag and (re)start the
    /// timer at [`Volatiles::CHARGE_TURNS`].
    ///
    /// Upstream writes `chargeTimer` and `chargeTimerStartValue` both to
    /// `2`; the start value is only ever read back by the battle-recording
    /// code this crate does not model, so one field carries it.
    pub const fn set_charge(&mut self) {
        self.charge_timer = Self::CHARGE_TURNS;
    }

    /// `ENDTURN_CHARGE` (`src/battle_util.c:1743`-`:1745`):
    /// `if (chargeTimer && --chargeTimer == 0) status3 &= ~STATUS3_CHARGED_UP`.
    ///
    /// Called once per battler per end of turn. A `0` timer is left alone,
    /// exactly as upstream's guard does, so this is idempotent once expired.
    pub const fn tick_charge(&mut self) {
        if self.charge_timer > 0 {
            self.charge_timer -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Volatiles;

    #[test]
    fn a_fresh_battler_carries_no_volatiles() {
        let v = Volatiles::default();
        assert!(!v.focus_energy);
        assert!(!v.defense_curl);
        assert!(!v.charged_up());
        assert_eq!(v.charge_timer, 0);
    }

    /// The timer's whole observable life: `2` on use, still charged after
    /// the Charge turn's own end-of-turn tick (so the *next* turn's Electric
    /// move is boosted), and expired after the one after that.
    #[test]
    fn charge_covers_its_own_turn_and_exactly_one_more() {
        let mut v = Volatiles::default();
        v.set_charge();
        assert_eq!(v.charge_timer, 2);
        assert!(v.charged_up(), "boosted for the rest of the Charge turn");

        v.tick_charge(); // end of the Charge turn
        assert_eq!(v.charge_timer, 1);
        assert!(v.charged_up(), "still boosted through the following turn");

        v.tick_charge(); // end of the following turn
        assert_eq!(v.charge_timer, 0);
        assert!(!v.charged_up(), "expired");

        // Upstream's `if (chargeTimer && ...)` guard: ticking an expired
        // timer is a no-op rather than an underflow.
        v.tick_charge();
        assert_eq!(v.charge_timer, 0);
    }

    #[test]
    fn the_two_flag_setters_are_idempotent() {
        let mut v = Volatiles::default();
        v.set_focus_energy();
        v.set_defense_curl();
        let once = v;
        v.set_focus_energy();
        v.set_defense_curl();
        assert_eq!(v, once);
    }
}
