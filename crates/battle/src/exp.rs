//! Experience awarded to one eligible player Pokémon after an opponent faints.

const FAINT_EXP_DIVISOR: u32 = 7;
const MINIMUM_FAINT_EXP: u32 = 1;
const TRAINER_EXP_PERCENT: u32 = 150;
const PERCENT_SCALE: u32 = 100;

/// Returns the wild-battle award from the opponent's base yield and level.
/// The award is at least one.
#[must_use]
pub fn wild_faint_exp(enemy_base_exp: u8, enemy_level: u8) -> u32 {
    let divided_award = u32::from(enemy_base_exp) * u32::from(enemy_level) / FAINT_EXP_DIVISOR;
    divided_award.max(MINIMUM_FAINT_EXP)
}

/// Returns the trainer-battle award after applying the bonus to the wild award.
#[must_use]
pub fn trainer_faint_exp(enemy_base_exp: u8, enemy_level: u8) -> u32 {
    wild_faint_exp(enemy_base_exp, enemy_level) * TRAINER_EXP_PERCENT / PERCENT_SCALE
}

#[cfg(test)]
mod tests {
    use super::{trainer_faint_exp, wild_faint_exp};

    const BULBASAUR_BASE_EXP: u8 = 64;
    const TREECKO_BASE_EXP: u8 = 65;
    const ROUTE_103_RIVAL_STARTER_LEVEL: u8 = 5;

    #[test]
    fn wild_award_multiplies_base_exp_by_level_before_dividing() {
        assert_eq!(wild_faint_exp(BULBASAUR_BASE_EXP, 5), 45);
    }

    #[test]
    fn floors_a_truncated_zero_to_one() {
        assert_eq!(wild_faint_exp(1, 1), 1);
    }

    #[test]
    fn scales_with_level_and_base_exp() {
        assert_eq!(wild_faint_exp(70, 10), 100);
        assert_eq!(wild_faint_exp(140, 10), 200);
    }

    #[test]
    fn a_trainer_battle_pays_one_and_a_half_times_the_wild_award() {
        assert_eq!(
            wild_faint_exp(TREECKO_BASE_EXP, ROUTE_103_RIVAL_STARTER_LEVEL),
            46
        );
        assert_eq!(
            trainer_faint_exp(TREECKO_BASE_EXP, ROUTE_103_RIVAL_STARTER_LEVEL),
            69
        );
    }

    #[test]
    fn the_bonus_multiplies_the_already_floored_award_not_the_raw_quotient() {
        assert_eq!(wild_faint_exp(1, 1), 1);
        assert_eq!(trainer_faint_exp(1, 1), 1);

        let trainer_bonus_applied_before_division = u32::from(1u8) * 150 / 100 / 7;
        assert_eq!(trainer_bonus_applied_before_division, 0);
    }
}
