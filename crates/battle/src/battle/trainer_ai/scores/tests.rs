//! Highest-score selection and its tie-break.

use crate::battle::trainer_ai::scores::select_highest_scoring_move;
use crate::battle::trainer_ai::test_support::pokemon;
use crate::battle::trainer_ai::test_support::spend_move;
use crate::battle::trainer_ai::test_support::GROWL;
use crate::battle::trainer_ai::test_support::MUDKIP;
use crate::battle::trainer_ai::test_support::SCRATCH;
use crate::battle::trainer_ai::test_support::SELECT_FIRST_TIED_MOVE;
use crate::battle::trainer_ai::test_support::TACKLE;
use crate::pokemon::MAX_MON_MOVES;
use crate::script_rng::SequenceRng;

#[test]
fn zero_score_usable_move_excludes_spent_slot_zero() {
    let mut enemy = pokemon(MUDKIP, vec![TACKLE, GROWL]);
    spend_move(&mut enemy, 0);
    let mut rng = SequenceRng::new([SELECT_FIRST_TIED_MOVE]);

    assert_eq!(
        select_highest_scoring_move(&enemy, [0; MAX_MON_MOVES], &mut rng),
        1
    );
    assert_eq!(rng.draws(), 1);
    rng.assert_exhausted();
}

#[test]
fn zero_score_tie_includes_only_pp_positive_real_moves() {
    let mut enemy = pokemon(MUDKIP, vec![TACKLE, GROWL, SCRATCH]);
    spend_move(&mut enemy, 2);

    for (draw, expected_slot) in [(2_u16, 0_usize), (3, 1)] {
        let mut rng = SequenceRng::new([draw]);
        assert_eq!(
            select_highest_scoring_move(&enemy, [0; MAX_MON_MOVES], &mut rng),
            expected_slot
        );
        assert_eq!(rng.draws(), 1);
        rng.assert_exhausted();
    }
}
