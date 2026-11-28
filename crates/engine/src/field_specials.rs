//! Field-script specials that read party state.
//!
//! Only the friendship classifier and the effort-value maxed-out predicate are
//! modelled here. Choosing the lead Pokemon, dispatching the special and
//! branching on its result belong to callers.

mod effort;

pub use effort::evs_are_maxed_out;

/// Friendship category returned by upstream's `GetLeadMonFriendshipScore`
/// (`src/field_specials.c:949-966`).
///
/// The discriminants are the `FRIENDSHIP_*` ids of
/// `include/constants/pokemon.h:184-191`, which field scripts compare
/// `VAR_RESULT` against, so they must not be renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FriendshipScore {
    /// `FRIENDSHIP_NONE`: friendship 0.
    None = 0,
    /// `FRIENDSHIP_1_TO_49`.
    OneTo49 = 1,
    /// `FRIENDSHIP_50_TO_99`.
    FiftyTo99 = 2,
    /// `FRIENDSHIP_100_TO_149`.
    HundredTo149 = 3,
    /// `FRIENDSHIP_150_TO_199`.
    HundredFiftyTo199 = 4,
    /// `FRIENDSHIP_200_TO_254`.
    TwoHundredTo254 = 5,
    /// `FRIENDSHIP_MAX`: friendship 255 (`MAX_FRIENDSHIP`).
    Max = 6,
}

/// Classifies a friendship byte as upstream's `GetLeadMonFriendshipScore`
/// does after it has read the lead Pokemon's friendship.
#[must_use]
pub const fn friendship_score(friendship: u8) -> FriendshipScore {
    match friendship {
        255 => FriendshipScore::Max,
        200..=254 => FriendshipScore::TwoHundredTo254,
        150..=199 => FriendshipScore::HundredFiftyTo199,
        100..=149 => FriendshipScore::HundredTo149,
        50..=99 => FriendshipScore::FiftyTo99,
        1..=49 => FriendshipScore::OneTo49,
        0 => FriendshipScore::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_ids_match_the_upstream_constants() {
        assert_eq!(FriendshipScore::None as u8, 0);
        assert_eq!(FriendshipScore::OneTo49 as u8, 1);
        assert_eq!(FriendshipScore::FiftyTo99 as u8, 2);
        assert_eq!(FriendshipScore::HundredTo149 as u8, 3);
        assert_eq!(FriendshipScore::HundredFiftyTo199 as u8, 4);
        assert_eq!(FriendshipScore::TwoHundredTo254 as u8, 5);
        assert_eq!(FriendshipScore::Max as u8, 6);
    }

    #[test]
    fn every_friendship_byte_maps_to_its_range_id() {
        // (lowest friendship in the category, category id), as written in
        // GetLeadMonFriendshipScore's thresholds.
        let lower_bounds: [(u8, u8); 7] = [
            (0, 0),
            (1, 1),
            (50, 2),
            (100, 3),
            (150, 4),
            (200, 5),
            (255, 6),
        ];
        for friendship in 0..=u8::MAX {
            let expected = lower_bounds
                .iter()
                .rev()
                .find(|(lowest, _)| friendship >= *lowest)
                .map(|(_, id)| *id);
            assert_eq!(
                Some(friendship_score(friendship) as u8),
                expected,
                "friendship {friendship}"
            );
        }
    }
}
