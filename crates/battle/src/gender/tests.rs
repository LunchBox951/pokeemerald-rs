use super::Gender;
use assets::GenderRatio;

#[test]
fn a_mixed_ratio_is_female_only_when_the_low_byte_is_below_it() {
    let ratio = GenderRatio::HALF_FEMALE;
    for (low_byte, expected) in [
        (0, Gender::Female),
        (126, Gender::Female),
        (127, Gender::Male),
        (255, Gender::Male),
    ] {
        assert_eq!(Gender::of_personality(ratio, low_byte), expected);
    }
}

#[test]
fn only_the_low_personality_byte_decides_a_mixed_ratio() {
    let ratio = GenderRatio::HALF_FEMALE;
    assert_eq!(Gender::of_personality(ratio, 0xABCD_EF7E), Gender::Female);
    assert_eq!(Gender::of_personality(ratio, 0xABCD_EF7F), Gender::Male);
}

#[test]
fn a_locked_ratio_ignores_the_personality() {
    for personality in [0, 127, 255, u32::MAX] {
        assert_eq!(
            Gender::of_personality(GenderRatio::AlwaysMale, personality),
            Gender::Male
        );
        assert_eq!(
            Gender::of_personality(GenderRatio::AlwaysFemale, personality),
            Gender::Female
        );
        assert_eq!(
            Gender::of_personality(GenderRatio::Genderless, personality),
            Gender::Genderless
        );
    }
}

#[test]
fn a_ratio_variant_holding_a_locked_byte_is_locked_like_upstream() {
    assert_eq!(
        Gender::of_personality(GenderRatio::Ratio(0), 255),
        Gender::Male
    );
    assert_eq!(
        Gender::of_personality(GenderRatio::Ratio(0xFE), 0),
        Gender::Female
    );
    assert_eq!(
        Gender::of_personality(GenderRatio::Ratio(0xFF), 0),
        Gender::Genderless
    );
}

#[test]
fn a_cute_charm_lead_hands_the_wild_mon_the_opposite_gender() {
    assert_eq!(Gender::Female.cute_charm_wild_gender(), Gender::Male);
    assert_eq!(Gender::Male.cute_charm_wild_gender(), Gender::Female);
    assert_eq!(Gender::Genderless.cute_charm_wild_gender(), Gender::Female);
}
