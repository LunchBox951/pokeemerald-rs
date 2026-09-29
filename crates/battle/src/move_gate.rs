//! Shared move admission for effect-specific execution pipelines.

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;

/// Validates move lookup, supported effect, and combat type in that order.
/// The order determines which failure is reported before execution can
/// mutate state or consume randomness.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if `supports_effect` rejects the
///   move's `EFFECT_*`.
/// - [`BattleError::UnsupportedMoveType`] if the move has no combat type.
pub(crate) fn ensure_resolvable_effect(
    dex: &Dex,
    move_id: MoveId,
    supports_effect: impl Fn(MoveEffect) -> bool,
) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !supports_effect(mv.effect) {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    if mv.move_type.battle_type().is_none() {
        return Err(BattleError::UnsupportedMoveType(move_id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_resolvable_effect;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use assets::MoveId;

    const CURSE: MoveId = MoveId(174);
    const UNKNOWN_MOVE: MoveId = MoveId(60_000);

    #[test]
    fn effect_rejection_takes_precedence_over_type_rejection() {
        let dex = Dex::new();
        assert_eq!(
            ensure_resolvable_effect(&dex, CURSE, |_| false),
            Err(BattleError::UnsupportedMoveEffect(CURSE)),
            "an effect predicate that rejects everything must win before the type check runs"
        );
        assert_eq!(
            ensure_resolvable_effect(&dex, CURSE, |_| true),
            Err(BattleError::UnsupportedMoveType(CURSE)),
            "once the effect predicate accepts, the ???-typed move is still rejected"
        );
    }

    #[test]
    fn unknown_move_propagates_before_the_predicate_runs() {
        let dex = Dex::new();
        assert_eq!(
            ensure_resolvable_effect(&dex, UNKNOWN_MOVE, |_| panic!(
                "the predicate must not be called for an unknown move"
            )),
            Err(BattleError::UnknownMove(UNKNOWN_MOVE))
        );
    }
}
