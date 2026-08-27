//! The shared validation shape behind `drain`, `fixed_damage`, and
//! `multi_hit`'s `ensure_resolvable` (issue #406): each of those pipelines
//! documents the same contract — checked before any state or RNG is touched
//! — and until this module existed, each also hand-rolled the same
//! three-step sequence to enforce it, differing only in which effect
//! predicate rejected the move.
//!
//! [`crate::hit::ensure_resolvable`] is deliberately *not* folded in here:
//! its own doc comment covers its different checks and ordering (power,
//! then type, then effect, with a `STRUGGLE` exception), so sharing this
//! helper with it would force one of the two orderings to bend to fit the
//! other for no real gain.

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;

/// The lookup → effect-predicate rejection → untyped-move rejection
/// sequence every `ensure_resolvable` in this crate (bar [`crate::hit`]'s)
/// follows, parameterised over which effect the caller's pipeline accepts.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if `is_effect` rejects the
///   move's `EFFECT_*`.
/// - [`BattleError::UnsupportedMoveType`] for a `???`-typed move, which
///   `Cmd_typecalc` could not classify.
pub(crate) fn ensure_resolvable_effect(
    dex: &Dex,
    move_id: MoveId,
    is_effect: impl Fn(MoveEffect) -> bool,
) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_effect(mv.effect) {
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

    /// `MOVE_CURSE`, the sole `???`-typed move (see
    /// [`BattleError::UnsupportedMoveType`]'s doc comment) — and, because no
    /// move both carries an accepted effect and is `???`-typed, the only
    /// fixture that can reach the type check at all. A predicate that always
    /// accepts proves the sequence continues past the effect check to reject
    /// the type; a predicate that always rejects proves the effect check
    /// still wins first, exactly as every wrapper's doc comment claims.
    const CURSE: MoveId = MoveId(174);

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

    /// An unknown move never reaches the predicate at all: `dex.move_data`
    /// fails first, via `?`, before `is_effect` is ever called.
    #[test]
    fn unknown_move_propagates_before_the_predicate_runs() {
        let dex = Dex::new();
        let unknown = MoveId(60_000);
        assert_eq!(
            ensure_resolvable_effect(&dex, unknown, |_| panic!(
                "the predicate must not be called for an unknown move"
            )),
            Err(BattleError::UnknownMove(unknown))
        );
    }
}
