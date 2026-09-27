//! Focus Energy, Charge, Defense Curl, and Confusion state carried by one
//! battler.
//!
//! Ordinary switches clear all four. Baton Pass preserves Focus Energy but
//! clears Charge's timer and Defense Curl's flag: its `status2` mask keeps
//! `STATUS2_FOCUS_ENERGY` but omits `STATUS2_DEFENSE_CURL`
//! (`src/battle_main.c:3173`-`:3217`), the opposite of how Focus Energy
//! itself survives the pass. Switching is outside this crate's boundary, so
//! nothing here currently reads that distinction.
//!
//! Charge's timer is also its active flag because upstream raises and clears
//! `STATUS3_CHARGED_UP` in lockstep with `chargeTimer`
//! (`src/battle_script_commands.c:9102`; `src/battle_util.c:1743`).
//!
//! Confusion decrements once per attacker action, not once per end-of-turn
//! tick like Charge (`CANCELER_CONFUSED`, `src/battle_util.c:2157`-`:2187`).
//! [`Volatiles::tick_confusion`] reports only the terminal zero transition.

/// The transient conditions carried by one battler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Volatiles {
    /// Whether Focus Energy is active.
    pub focus_energy: bool,
    /// The remaining end-of-turn ticks before Charge expires.
    pub charge_timer: u8,
    /// Whether Defense Curl has been used.
    pub defense_curl: bool,
    /// The remaining action-gated ticks before confusion expires. Zero means
    /// inactive.
    pub confusion_turns: u8,
}

impl Volatiles {
    /// The number of end-of-turn ticks before Charge expires.
    pub const CHARGE_TURNS: u8 = 2;

    /// Whether Charge's Electric-type power boost is active.
    #[must_use]
    pub const fn charged_up(self) -> bool {
        self.charge_timer > 0
    }

    /// Activates Focus Energy.
    pub const fn set_focus_energy(&mut self) {
        self.focus_energy = true;
    }

    /// Activates Defense Curl. `Cmd_setdefensecurlbit`
    /// (`src/battle_script_commands.c:8858`-`:8862`) unconditionally ORs in
    /// `STATUS2_DEFENSE_CURL`, with no already-set guard and no `Random()`.
    pub const fn set_defense_curl(&mut self) {
        self.defense_curl = true;
    }

    /// Activates or refreshes Charge.
    pub const fn set_charge(&mut self) {
        self.charge_timer = Self::CHARGE_TURNS;
    }

    /// Advances Charge by one end-of-turn tick.
    pub const fn tick_charge(&mut self) {
        if self.charge_timer > 0 {
            self.charge_timer -= 1;
        }
    }

    /// Whether confusion is active.
    #[must_use]
    pub const fn confused(self) -> bool {
        self.confusion_turns > 0
    }

    /// Activates confusion for `turns` actions.
    pub const fn set_confusion(&mut self, turns: u8) {
        self.confusion_turns = turns;
    }

    /// Advances confusion by one attacker action, reporting whether this
    /// tick is the one that ended it. Ticking an already-inactive confusion
    /// draws nothing and reports no transition, matching
    /// `AtkCanceler_UnableToUseMove`'s own `&&` short-circuit on a zero
    /// duration (`src/battle_util.c:2157`-`:2187`).
    pub const fn tick_confusion(&mut self) -> bool {
        if self.confusion_turns == 0 {
            return false;
        }
        self.confusion_turns -= 1;
        self.confusion_turns == 0
    }
}

#[cfg(test)]
mod tests {
    use super::Volatiles;

    #[test]
    fn a_fresh_battler_carries_no_volatiles() {
        let volatiles = Volatiles::default();
        assert!(!volatiles.focus_energy);
        assert!(!volatiles.charged_up());
        assert_eq!(volatiles.charge_timer, 0);
        assert!(!volatiles.defense_curl);
        assert!(!volatiles.confused());
        assert_eq!(volatiles.confusion_turns, 0);
    }

    #[test]
    fn charge_covers_its_own_turn_and_one_more() {
        let mut volatiles = Volatiles::default();
        volatiles.set_charge();
        assert!(volatiles.charged_up(), "the Charge turn itself");
        volatiles.tick_charge();
        assert!(volatiles.charged_up(), "the turn after");
        volatiles.tick_charge();
        assert!(!volatiles.charged_up(), "and no further");
        volatiles.tick_charge();
        assert_eq!(volatiles.charge_timer, 0);
    }

    #[test]
    fn focus_energy_is_a_latch() {
        let mut volatiles = Volatiles::default();
        volatiles.set_focus_energy();
        assert!(volatiles.focus_energy);
        volatiles.set_focus_energy();
        assert!(volatiles.focus_energy);
    }

    #[test]
    fn defense_curl_is_a_latch() {
        let mut volatiles = Volatiles::default();
        volatiles.set_defense_curl();
        assert!(volatiles.defense_curl);
        volatiles.set_defense_curl();
        assert!(volatiles.defense_curl);
    }

    #[test]
    fn confusion_expires_on_the_tick_that_reaches_zero() {
        let mut volatiles = Volatiles::default();
        volatiles.set_confusion(2);
        assert!(volatiles.confused());
        assert!(!volatiles.tick_confusion(), "one turn remains after this");
        assert!(volatiles.confused());
        assert!(volatiles.tick_confusion(), "this tick reaches zero");
        assert!(!volatiles.confused());
        assert_eq!(volatiles.confusion_turns, 0);
    }

    #[test]
    fn ticking_inactive_confusion_reports_no_transition() {
        let mut volatiles = Volatiles::default();
        assert!(!volatiles.tick_confusion());
        assert!(!volatiles.confused());
    }

    #[test]
    fn set_confusion_refreshes_an_already_active_duration() {
        let mut volatiles = Volatiles::default();
        volatiles.set_confusion(1);
        volatiles.set_confusion(3);
        assert_eq!(volatiles.confusion_turns, 3);
    }
}
