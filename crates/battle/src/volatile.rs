//! Volatile in-battle state (S-6, issue #321): the `gBattleMons[].status2`
//! and `gStatuses3[]` bits [`crate::flag_move`]'s scripts set, plus the
//! `gDisableStructs[]` timer that goes with one of them.
//!
//! Upstream keeps three parallel "condition" stores per battler. This module
//! owns the two that vanish when the battler leaves the field:
//!
//! | store | lives in | survives switching out? | modelled by |
//! |---|---|---|---|
//! | `status1` (sleep/poison/burn/…) | `struct Pokemon`, the party record | **yes** | not yet — issue #323 |
//! | `status2` | `gBattleMons[]` only | mostly no | this module |
//! | `gStatuses3[]` + `gDisableStructs[]` | battle scratch | no | this module |
//!
//! Only the bits a *modelled* move can actually set are represented, for the
//! same reason [`crate::stat_change`] transcribes only real script thunks: an
//! unreachable bit is dead data, and a reachable one has to be carried in
//! whatever reads it. Two so far.
//!
//! # `STATUS2_FOCUS_ENERGY` (`1 << 20`, `include/constants/battle.h:144`)
//!
//! Set by `Cmd_setfocusenergy` (`src/battle_script_commands.c:7758`), read
//! by exactly one place: `Cmd_critcalc`'s `critChance = 2 * ((status2 &
//! STATUS2_FOCUS_ENERGY) != 0) + …` (`:1267`) — [`crate::critical::crit_stage`].
//!
//! It is one of the five bits `SwitchInClearSetData` preserves — but **only
//! on Baton Pass**. The `status2 &= (STATUS2_CONFUSION |
//! STATUS2_FOCUS_ENERGY | …)` mask at `src/battle_main.c:3175` sits inside
//! that function's `if (gBattleMoves[gCurrentMove].effect ==
//! EFFECT_BATON_PASS)` branch (`:3173`); an **ordinary** switch takes the
//! `else` at `:3189`-`:3192`, which zeroes `status2` and `gStatuses3[]`
//! outright. This crate can exercise neither — the only switch it performs
//! replaces a *fainted* mon with a fresh [`crate::pokemon::BattlePokemon`]
//! from the bench, which starts at [`Volatiles::default`] — but the
//! distinction is recorded so a future switching slice clears the bit by
//! default and spares it only for Baton Pass.
//!
//! # `STATUS3_CHARGED_UP` (`1 << 9`, `:166`) and `chargeTimer`
//!
//! Both set by `Cmd_setcharge` (`:9102`-`:9104`). Read by `Cmd_damagecalc`:
//! `if (charged && move type == TYPE_ELECTRIC) gBattleMoveDamage *= 2`
//! (`:1298`-`:1299`) — [`crate::hit::damage_core`]. The timer is decremented
//! once per end-of-turn by `ENDTURN_CHARGE` (`src/battle_util.c:1743`-`:1745`),
//! so the boost covers the Charge turn itself and exactly one turn after it.
//!
//! One field carries both: upstream raises and clears the flag in lockstep
//! with the timer reaching `0`, so a second `bool` could only ever
//! contradict it `(oop-boundaries)`.
//!
//! # RNG
//!
//! **Nothing here draws.** Every setter is a flag write, every reader is
//! arithmetic, and the one timer ticks deterministically. The volatiles a
//! *roll* would produce — `STATUS2_CONFUSION`'s 2..5-turn counter above all
//! — are issue #323's, and are deliberately absent rather than stubbed.

/// The volatile conditions one battler carries — the `status2`/`gStatuses3`
/// bits and timers this crate models `(oop-boundaries)`.
///
/// [`Default`]-empty matches `BattleStartClearSetData` zeroing the whole
/// `gBattleMons[]` entry (`src/battle_main.c:3034`) and `SwitchInClearSetData`
/// doing the same on send-out (`:3161`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Volatiles {
    /// `STATUS2_FOCUS_ENERGY`: this battler is "getting pumped", worth `+2`
    /// crit-chance stages ([`crate::critical::crit_stage`]).
    pub focus_energy: bool,
    /// `gDisableStructs[].chargeTimer` — [`Volatiles::CHARGE_TURNS`] the
    /// moment Charge resolves, decremented once at the end of each turn.
    /// `STATUS3_CHARGED_UP` is not stored separately (module docs).
    pub charge_timer: u8,
}

impl Volatiles {
    /// `Cmd_setcharge`'s timer value
    /// (`src/battle_script_commands.c:9103`): `chargeTimer = 2`.
    pub const CHARGE_TURNS: u8 = 2;

    /// Whether `STATUS3_CHARGED_UP` is set — the flag `Cmd_damagecalc` tests
    /// before doubling an Electric move's damage
    /// (`src/battle_script_commands.c:1298`).
    #[must_use]
    pub const fn charged_up(self) -> bool {
        self.charge_timer > 0
    }

    /// `Cmd_setfocusenergy` (`src/battle_script_commands.c:7758`), reduced
    /// to the branch its script can reach: set the bit.
    ///
    /// The command's own `else` — "already pumped, fail" (`:7751`-`:7755`) —
    /// is dead code from `BattleScript_EffectFocusEnergy`, whose
    /// `jumpifstatus2 BS_ATTACKER, STATUS2_FOCUS_ENERGY,
    /// BattleScript_ButItFailed` (`data/battle_scripts_1.s:889`) diverts one
    /// instruction earlier. [`crate::flag_move`] reproduces the *script's*
    /// check, which is why this setter is unconditional.
    pub const fn set_focus_energy(&mut self) {
        self.focus_energy = true;
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
        assert!(!v.charged_up());
        assert_eq!(v.charge_timer, 0);
    }

    /// `Cmd_setcharge` starts the timer at 2 and `ENDTURN_CHARGE` decrements
    /// it once per end of turn, so the boost covers the Charge turn and
    /// exactly one turn after it — then stops, and stays stopped.
    #[test]
    fn charge_covers_its_own_turn_and_one_more() {
        let mut v = Volatiles::default();
        v.set_charge();
        assert!(v.charged_up(), "the Charge turn itself");
        v.tick_charge();
        assert!(v.charged_up(), "the turn after");
        v.tick_charge();
        assert!(!v.charged_up(), "and no further");
        // Idempotent once expired: upstream's `if (chargeTimer)` guard means
        // a further end-of-turn cannot underflow the counter.
        v.tick_charge();
        assert_eq!(v.charge_timer, 0);
    }

    #[test]
    fn focus_energy_is_a_latch() {
        let mut v = Volatiles::default();
        v.set_focus_energy();
        assert!(v.focus_energy);
        // The setter is unconditional (module docs): re-setting an already
        // set bit is upstream's `Cmd_setfocusenergy` behaviour, and the
        // "already pumped" refusal is the *script's*, not this type's.
        v.set_focus_energy();
        assert!(v.focus_energy);
    }
}
