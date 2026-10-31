//! Flag and move-effect admission.

use crate::battle::trainer_ai::ensure_scoreable;
use crate::battle::trainer_ai::ensure_supported_flags;
use crate::battle::trainer_ai::test_support::first_turn_setup_flags;
use crate::battle::trainer_ai::test_support::route_103_flags;
use crate::battle::trainer_ai::test_support::EMBER;
use crate::battle::trainer_ai::test_support::GROWL;
use crate::battle::trainer_ai::test_support::LEER;
use crate::battle::trainer_ai::test_support::POUND;
use crate::battle::trainer_ai::test_support::SEISMIC_TOSS;
use crate::dex::Dex;
use crate::error::BattleError;
use assets::trainers::AiFlags;
use assets::MoveId;

#[test]
fn flagless_admission_still_requires_a_dex_resident_move() {
    let dex = Dex::new();
    assert_eq!(ensure_scoreable(&dex, SEISMIC_TOSS, AiFlags::NONE), Ok(()));
    assert!(ensure_scoreable(&dex, MoveId(u16::MAX), AiFlags::NONE).is_err());
}

#[test]
fn scoring_flags_still_refuse_fixed_damage_moves() {
    let dex = Dex::new();
    for flags in [
        AiFlags::CHECK_BAD_MOVE,
        AiFlags::TRY_TO_FAINT,
        AiFlags::CHECK_VIABILITY,
        AiFlags::SETUP_FIRST_TURN,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, SEISMIC_TOSS, flags),
            Err(BattleError::UnscoreableMoveEffect(SEISMIC_TOSS))
        );
    }
}

#[test]
fn route_103_move_effects_are_scoreable_and_ember_is_not() {
    let dex = Dex::new();
    let flags = route_103_flags();

    assert!(ensure_scoreable(&dex, POUND, flags).is_ok());
    assert!(ensure_scoreable(&dex, GROWL, flags).is_ok());
    assert!(ensure_scoreable(&dex, LEER, flags).is_ok());
    assert_eq!(
        ensure_scoreable(&dex, EMBER, flags),
        Err(BattleError::UnscoreableMoveEffect(EMBER))
    );
}

#[test]
fn only_the_four_lowest_ai_script_bits_are_supported() {
    assert!(ensure_supported_flags(route_103_flags()).is_ok());
    assert!(ensure_supported_flags(first_turn_setup_flags()).is_ok());
    assert_eq!(
        ensure_supported_flags(AiFlags::CHECK_BAD_MOVE.union(AiFlags::RISKY)),
        Err(BattleError::UnsupportedAiFlags(AiFlags::RISKY))
    );
}
