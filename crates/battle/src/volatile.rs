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
//!   `SwitchInClearSetData` preserves — but **only on Baton Pass**. The
//!   `status2 &= (STATUS2_CONFUSION | STATUS2_FOCUS_ENERGY | …)` mask at
//!   `src/battle_main.c:3175` sits inside that function's
//!   `if (gBattleMoves[gCurrentMove].effect == EFFECT_BATON_PASS)` branch
//!   (`:3173`); an **ordinary** switch takes the `else` at `:3189`-`:3192`,
//!   which zeroes `status2` and `gStatuses3[]` outright. This crate can
//!   exercise neither (the only switch it performs replaces a *fainted* mon
//!   with a fresh one from the bench), but the distinction is recorded so a
//!   future switching slice clears the bit by default and spares it only for
//!   Baton Pass.
//! - **`STATUS2_DEFENSE_CURL`** (`1 << 30`, `:154`), set by
//!   `Cmd_setdefensecurlbit` (`:8860`). Read only by
//!   `Cmd_setrolloutcounter`'s power ramp (`:8564`-`:8565`), which doubles
//!   Rollout's already-doubling base power. Rollout is **not** modelled this
//!   slice, so nothing reads this bit yet — it is carried because Defense
//!   Curl sets it unconditionally (before, and independently of, its own
//!   stat raise) and dropping it would make a later Rollout slice silently
//!   half-powered. Unlike Focus Energy it is *not* in the switch-preserve
//!   mask.
//! - **`STATUS2_CONFUSION`** (the low three bits, `1 << 0 | 1 << 1 | 1 << 2`,
//!   `:129`-`:130`) — a *counter*, not a flag: `SetMoveEffect`'s
//!   `MOVE_EFFECT_CONFUSION` case stores
//!   `STATUS2_CONFUSION_TURN(((Random()) % 4) + 2)`
//!   (`src/battle_script_commands.c:2541`), i.e. 2..5 turns. Read by
//!   `CANCELLER_CONFUSED` (`src/battle_util.c:2156`-`:2187`), which
//!   decrements it *first* and then, only if any turns remain, rolls
//!   `Random() & 1` for whether the battler hurts itself — see
//!   [`Volatiles::tick_confusion`] and [`confusion_self_hit_roll`].
//!   Like Focus Energy it is in `SwitchInClearSetData`'s **Baton Pass**
//!   preserve mask (`src/battle_main.c:3175`) and, like it, is cleared by
//!   an ordinary switch (`:3189`-`:3192`).
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
    /// `STATUS2_CONFUSION`'s remaining turns (`0` when not confused).
    ///
    /// Set to `2..=5` by [`roll_confusion_turns`] and decremented by
    /// [`Volatiles::tick_confusion`] at the top of each of this battler's
    /// move attempts — **not** at end of turn, which is where a reader might
    /// expect it: upstream decrements inside `CANCELLER_CONFUSED`, so a
    /// battler that never gets to act never burns a confusion turn.
    pub confusion_turns: u8,
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

    /// Whether `STATUS2_CONFUSION` is set at all.
    #[must_use]
    pub const fn is_confused(self) -> bool {
        self.confusion_turns > 0
    }

    /// `CANCELLER_CONFUSED`'s opening statement
    /// (`src/battle_util.c:2159`): `status2 -= STATUS2_CONFUSION_TURN(1)`,
    /// run **before** the self-hit roll and unconditionally on any confused
    /// battler that is about to act.
    ///
    /// Returns `true` if the battler is still confused afterwards — the
    /// `if (status2 & STATUS2_CONFUSION)` at `:2160` that gates the roll.
    /// A `false` return is upstream's "snapped out of confusion", which
    /// costs **no draw** and lets the move proceed.
    pub const fn tick_confusion(&mut self) -> bool {
        if self.confusion_turns > 0 {
            self.confusion_turns -= 1;
        }
        self.confusion_turns > 0
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

/// `SetMoveEffect`'s `MOVE_EFFECT_CONFUSION` duration
/// (`src/battle_script_commands.c:2541`):
/// `STATUS2_CONFUSION_TURN(((Random()) % 4) + 2)`, i.e. **2..=5** turns for
/// one draw.
///
/// Written `% 4` rather than `& 3` because that is what the source says —
/// the two agree for every `u16`, but the sibling cases at `:2481`
/// (`MOVE_EFFECT_SLEEP`) and `:2573` (`MOVE_EFFECT_UPROAR`) genuinely use
/// `& 3`, so keeping each spelling faithful is what makes the difference
/// visible if one of them is ever changed upstream.
#[must_use]
pub fn roll_confusion_turns(rng: &mut impl crate::damage::BattleRng) -> u8 {
    // No truncation suppression needed: `% 4` leaves 0..=3, so the sum is
    // 2..=5 and clippy can see the narrowing is exact.
    let turns = (rng.next_u16() % 4) as u8;
    turns + 2
}

/// `CANCELLER_CONFUSED`'s self-hit roll (`src/battle_util.c:2162`):
/// `if (Random() & 1)` — **odd means the move goes through**, even means the
/// battler hurts itself instead.
///
/// Returns `true` for the self-hit (upstream's `else` at `:2169`). Only
/// reached when [`Volatiles::tick_confusion`] left turns on the clock, so a
/// battler whose confusion just expired makes no draw here.
#[must_use]
pub fn confusion_self_hit_roll(rng: &mut impl crate::damage::BattleRng) -> bool {
    rng.next_u16() & 1 == 0
}

/// `CalculateBaseDamage(&attacker, &attacker, MOVE_POUND, 0, 40, 0, ...)` —
/// the confusion self-hit's own damage call (`src/battle_util.c:2173`).
///
/// A **40-power physical hit the battler takes from itself**, and three
/// things about it are easy to get wrong, all visible in that one call:
///
/// - `powerOverride` is `40`, so the move's own power is irrelevant (it is
///   `MOVE_POUND` only to give `CalculateBaseDamage` a move id to read
///   flags from);
/// - `typeOverride` is `0` and no `typecalc` command ever runs on it, so
///   there is **no STAB and no type chart** — not even an immunity;
/// - attacker and defender are the same battler, so its own Attack and its
///   own Defense (and their stages) are both inputs.
///
/// The `85..=100%` roll *does* apply: the script runs
/// `adjustnormaldamage2` (`data/battle_scripts_1.s:3803`), which calls
/// `ApplyRandomDmgMultiplier`. That draw is the caller's — see
/// [`crate::battle::Battle`]'s canceller.
pub const CONFUSION_SELF_HIT_POWER: u32 = 40;

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

    /// The confusion counter's own life: 2..=5 on infliction, one tick per
    /// *move attempt*, and a final tick that snaps the battler out.
    #[test]
    fn confusion_counts_down_once_per_move_attempt() {
        let mut v = Volatiles::default();
        assert!(!v.is_confused());
        v.confusion_turns = 2;
        assert!(v.is_confused());

        assert!(v.tick_confusion(), "2 -> 1, still confused");
        assert_eq!(v.confusion_turns, 1);
        assert!(!v.tick_confusion(), "1 -> 0, snapped out");
        assert_eq!(v.confusion_turns, 0);
        assert!(!v.is_confused());
        // Upstream's guard: ticking an already-clear counter cannot
        // underflow.
        assert!(!v.tick_confusion());
        assert_eq!(v.confusion_turns, 0);
    }

    #[test]
    fn the_confusion_duration_roll_is_two_through_five_for_one_draw() {
        use crate::damage::BattleRng;
        struct One(u16, usize);
        impl BattleRng for One {
            fn next_u16(&mut self) -> u16 {
                self.1 += 1;
                self.0
            }
        }
        for (draw, expected) in [(0u16, 2u8), (1, 3), (2, 4), (3, 5), (4, 2), (0xFFFF, 5)] {
            let mut rng = One(draw, 0);
            assert_eq!(
                super::roll_confusion_turns(&mut rng),
                expected,
                "draw {draw}"
            );
            assert_eq!(rng.1, 1, "exactly one draw");
        }
    }

    /// `Random() & 1`: **odd lets the move through**, even is the self-hit.
    #[test]
    fn the_confusion_self_hit_roll_fires_on_an_even_draw() {
        use crate::damage::BattleRng;
        struct One(u16);
        impl BattleRng for One {
            fn next_u16(&mut self) -> u16 {
                self.0
            }
        }
        assert!(super::confusion_self_hit_roll(&mut One(0)));
        assert!(super::confusion_self_hit_roll(&mut One(2)));
        assert!(!super::confusion_self_hit_roll(&mut One(1)));
        assert!(!super::confusion_self_hit_roll(&mut One(3)));
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
