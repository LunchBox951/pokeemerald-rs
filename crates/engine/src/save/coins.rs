//! `SaveBlock1::coins`: the Game Corner coin balance and its four operations
//! (`GetCoins`, `SetCoins`, `AddCoins`, `RemoveCoins`).

/// `MAX_COINS` (`pokeemerald/include/constants/coins.h:4`).
pub const MAX_COINS: u16 = 9999;

/// The plaintext coin balance. The key's low 16 bits are applied when the block
/// is serialized, never here. No cap is applied on load or by [`Self::set`]:
/// an imported balance above [`MAX_COINS`] reads back as stored, and only
/// [`Self::add`] pulls a total down to the cap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Coins(u16);

impl Coins {
    /// A balance holding `amount`, uncapped.
    #[must_use]
    pub const fn new(amount: u16) -> Self {
        Self(amount)
    }

    /// `GetCoins`: the stored balance (`pokeemerald/src/coins.c:41-44`).
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// `SetCoins`: stores `amount` as given, with no cap
    /// (`pokeemerald/src/coins.c:46-49`).
    pub const fn set(&mut self, amount: u16) {
        self.0 = amount;
    }

    /// `AddCoins`: refuses at or above [`MAX_COINS`], leaving the balance
    /// unchanged. Otherwise the sum caps at [`MAX_COINS`] and the call
    /// succeeds, so adding zero below the cap succeeds and changes nothing
    /// (`pokeemerald/src/coins.c:51-71`).
    ///
    /// Upstream's `ownedCoins > ownedCoins + toAdd` overflow guard compares
    /// `int` sums and so never fires. The sum is narrowed back to 16 bits
    /// before the cap check instead, so an add whose total passes `0xFFFF`
    /// wraps: from 9998, adding 60000 leaves 4462, not the cap.
    pub const fn add(&mut self, amount: u16) -> bool {
        if self.0 >= MAX_COINS {
            return false;
        }
        let total = self.0.wrapping_add(amount);
        self.0 = if total > MAX_COINS { MAX_COINS } else { total };
        true
    }

    /// `RemoveCoins`: subtracts `amount` and succeeds only when the balance
    /// covers it; otherwise the balance is unchanged. Removing zero always
    /// succeeds. A balance above [`MAX_COINS`] is not capped by a removal
    /// (`pokeemerald/src/coins.c:73-82`).
    pub const fn remove(&mut self, amount: u16) -> bool {
        if self.0 >= amount {
            self.0 -= amount;
            true
        } else {
            false
        }
    }

    /// Zeroes the balance, as when the key that encrypted it is lost.
    pub const fn clear(&mut self) {
        self.0 = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(amount: u16) -> Coins {
        Coins::new(amount)
    }

    #[test]
    fn a_new_balance_is_zero_and_set_stores_any_value_uncapped() {
        assert_eq!(Coins::default().get(), 0);
        assert_eq!(with(MAX_COINS + 1).get(), 10_000);
        assert_eq!(with(u16::MAX).get(), 0xFFFF);
    }

    #[test]
    fn add_sums_below_the_cap_and_reports_success() {
        let mut coins = with(100);
        assert!(coins.add(250));
        assert_eq!(coins.get(), 350);
        assert!(coins.add(0));
        assert_eq!(coins.get(), 350);
    }

    #[test]
    fn add_caps_the_total_at_9999_and_then_refuses() {
        let mut coins = with(9998);
        assert!(coins.add(1));
        assert_eq!(coins.get(), 9999);
        assert!(!coins.add(1));
        assert!(!coins.add(0));
        assert_eq!(coins.get(), 9999);

        let mut over = with(9000);
        assert!(over.add(5000));
        assert_eq!(over.get(), 9999);
    }

    #[test]
    fn add_refuses_an_imported_balance_above_the_cap_without_touching_it() {
        let mut coins = with(10_000);
        assert!(!coins.add(1));
        assert_eq!(coins.get(), 10_000);
        let mut max = with(u16::MAX);
        assert!(!max.add(0));
        assert_eq!(max.get(), 0xFFFF);
    }

    /// Upstream's overflow check never fires (its sum is an `int`), so the
    /// sum wraps at 16 bits before the cap check.
    #[test]
    fn add_wraps_at_sixteen_bits_before_capping() {
        let mut coins = with(9998);
        assert!(coins.add(60_000));
        assert_eq!(coins.get(), 4462);

        let mut exact = with(1);
        assert!(exact.add(u16::MAX));
        assert_eq!(exact.get(), 0);

        let mut capped = with(9998);
        assert!(capped.add(u16::MAX));
        assert_eq!(capped.get(), 9997);

        let mut large = with(5000);
        assert!(large.add(u16::MAX));
        assert_eq!(large.get(), 4999);

        let mut just_under = with(9998);
        assert!(just_under.add(u16::MAX - 9998));
        assert_eq!(just_under.get(), 9999);
    }

    #[test]
    fn remove_subtracts_when_covered_and_fails_when_short() {
        let mut coins = with(500);
        assert!(coins.remove(200));
        assert_eq!(coins.get(), 300);
        assert!(!coins.remove(301));
        assert_eq!(coins.get(), 300);
        assert!(coins.remove(300));
        assert_eq!(coins.get(), 0);
        assert!(!coins.remove(1));
        assert_eq!(coins.get(), 0);
    }

    #[test]
    fn remove_of_zero_always_succeeds() {
        let mut empty = Coins::default();
        assert!(empty.remove(0));
        assert_eq!(empty.get(), 0);
        let mut funded = with(42);
        assert!(funded.remove(0));
        assert_eq!(funded.get(), 42);
    }

    #[test]
    fn remove_from_an_imported_balance_above_the_cap_does_not_cap_it() {
        let mut coins = with(20_000);
        assert!(coins.remove(5));
        assert_eq!(coins.get(), 19_995);
        let mut max = with(u16::MAX);
        assert!(max.remove(u16::MAX));
        assert_eq!(max.get(), 0);
    }
}
