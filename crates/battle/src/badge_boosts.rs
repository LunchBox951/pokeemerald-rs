//! The earned-badge stat boosts of an ordinary single-player battle.
//!
//! Four gym badges lift one battle stat each by 10 %: badge 1 Attack, badge 3
//! Speed, badge 5 Defense and badge 7 both special stats. Upstream applies
//! them to the player's side only, as a truncating `110 * stat / 100`: inside
//! `CalculateBaseDamage` for the four damage stats, after Huge Power and
//! before the type-boosting held items and the Hustle, Guts, Marvel Scale,
//! Thick Fat and Plus/Minus multipliers
//! (`pokeemerald/src/pokemon.c:3161`-`:3168`), and inside `GetWhoStrikesFirst`
//! for Speed, after the stage ratio and before Macho Brace and the paralysis
//! quarter (`pokeemerald/src/battle_main.c:4639`-`:4651`, repeated for the
//! second battler at `:4673`-`:4679`). The modifier never touches a stored
//! stat: it exists only in those two reads.
//!
//! `ShouldGetStatBadgeBoost` (`pokemon.c:3407`-`:3420`) also refuses link,
//! e-Reader, recorded-link and Frontier battles and Secret Base trainers, and
//! `GetWhoStrikesFirst` refuses link, recorded-link and Frontier battles;
//! none of those battle kinds exist in this engine, so the only gate left is
//! which badges the caller hands over and which side the battler fights on.
//! [`crate::Battle`] hands them to the player's side and clears every enemy
//! battler.

/// Percentage a held badge scales its stat to (`110 * stat / 100`).
const BADGE_PERCENT: u64 = 110;
const PERCENT_SCALE: u64 = 100;

/// Which of the four stat badges the player holds.
///
/// [`BadgeBoosts::NONE`] is the default, so a battler that was never given a
/// badge context reads every stat unchanged.
#[expect(
    clippy::struct_excessive_bools,
    reason = "four independent badge flags, one per boosted stat"
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct BadgeBoosts {
    attack: bool,
    speed: bool,
    defense: bool,
    special: bool,
}

impl BadgeBoosts {
    /// No stat badge held.
    pub const NONE: Self = Self {
        attack: false,
        speed: false,
        defense: false,
        special: false,
    };

    /// `badges` is `[badge 1, badge 3, badge 5, badge 7]`
    /// (`FLAG_BADGE01_GET`, `FLAG_BADGE03_GET`, `FLAG_BADGE05_GET`,
    /// `FLAG_BADGE07_GET`): the badges that boost a stat. Badges 2, 4, 6 and 8
    /// are [`crate::PlayerObedience::new`]'s.
    #[must_use]
    pub const fn from_badges(badges: [bool; 4]) -> Self {
        Self {
            attack: badges[0],
            speed: badges[1],
            defense: badges[2],
            special: badges[3],
        }
    }

    /// Whether no stat badge is held.
    #[must_use]
    pub const fn is_none(self) -> bool {
        !(self.attack || self.speed || self.defense || self.special)
    }

    /// Raw Attack after badge 1 (`pokemon.c:3161`-`:3162`).
    #[must_use]
    pub const fn boost_attack(self, attack: u32) -> u32 {
        scale_if(self.attack, attack)
    }

    /// Raw Defense after badge 5 (`pokemon.c:3163`-`:3164`).
    #[must_use]
    pub const fn boost_defense(self, defense: u32) -> u32 {
        scale_if(self.defense, defense)
    }

    /// Raw Sp. Attack or Sp. Defense after badge 7 (`pokemon.c:3165`-`:3168`).
    #[must_use]
    pub const fn boost_special(self, special: u32) -> u32 {
        scale_if(self.special, special)
    }

    /// Stage-scaled Speed after badge 3 (`battle_main.c:4639`-`:4645`).
    #[must_use]
    pub const fn boost_speed(self, speed: u32) -> u32 {
        scale_if(self.speed, speed)
    }
}

/// `(110 * stat) / 100`, truncating, in a wider type so a saturated stat does
/// not wrap.
const fn scale_if(held: bool, stat: u32) -> u32 {
    if !held {
        return stat;
    }
    let scaled = stat as u64 * BADGE_PERCENT / PERCENT_SCALE;
    if scaled > u32::MAX as u64 {
        u32::MAX
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "scaled is checked against u32::MAX above"
        )]
        let narrowed = scaled as u32;
        narrowed
    }
}

#[cfg(test)]
mod tests {
    use super::BadgeBoosts;

    const ALL: BadgeBoosts = BadgeBoosts::from_badges([true; 4]);

    #[test]
    fn each_badge_scales_only_its_own_stat() {
        let attack = BadgeBoosts::from_badges([true, false, false, false]);
        assert_eq!(attack.boost_attack(100), 110);
        assert_eq!(attack.boost_defense(100), 100);
        assert_eq!(attack.boost_special(100), 100);
        assert_eq!(attack.boost_speed(100), 100);

        let speed = BadgeBoosts::from_badges([false, true, false, false]);
        assert_eq!(speed.boost_speed(100), 110);
        assert_eq!(speed.boost_attack(100), 100);

        let defense = BadgeBoosts::from_badges([false, false, true, false]);
        assert_eq!(defense.boost_defense(100), 110);
        assert_eq!(defense.boost_attack(100), 100);

        let special = BadgeBoosts::from_badges([false, false, false, true]);
        assert_eq!(special.boost_special(100), 110);
        assert_eq!(special.boost_defense(100), 100);
    }

    #[test]
    fn the_boost_truncates() {
        assert_eq!(ALL.boost_attack(11), 12);
        assert_eq!(ALL.boost_attack(10), 11);
        assert_eq!(ALL.boost_attack(9), 9);
        assert_eq!(ALL.boost_speed(105), 115);
        assert_eq!(ALL.boost_speed(1), 1);
        assert_eq!(ALL.boost_speed(0), 0);
    }

    #[test]
    fn the_boost_saturates_instead_of_wrapping() {
        assert_eq!(ALL.boost_attack(u32::MAX), u32::MAX);
    }

    #[test]
    fn none_is_the_default_and_changes_nothing() {
        assert_eq!(BadgeBoosts::default(), BadgeBoosts::NONE);
        assert!(BadgeBoosts::NONE.is_none());
        assert!(!ALL.is_none());
        assert_eq!(BadgeBoosts::NONE.boost_attack(u32::MAX), u32::MAX);
        assert_eq!(BadgeBoosts::NONE.boost_speed(7), 7);
    }
}
