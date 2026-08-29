//! Transient conditions that are cleared when a battler leaves the field.
//!
//! Charge's timer is also its active flag because upstream raises and clears
//! `STATUS3_CHARGED_UP` in lockstep with `chargeTimer`
//! (`src/battle_script_commands.c:9102`; `src/battle_util.c:1743`).

/// The transient conditions carried by one battler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Volatiles {
    /// Whether Focus Energy is active.
    pub focus_energy: bool,
    /// The remaining end-of-turn ticks before Charge expires.
    pub charge_timer: u8,
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
}
