//! Canonical move and item identities must be nameable from dependent crates.
//!
//! `assets` owns the upstream numbering, so a crate that needs a specific move
//! or item has to reach the named constant rather than restate its index
//! `(self-explanatory-code)`.

use assets::{items::ItemId, MoveId};
use battle::damage::STRUGGLE;

#[test]
fn move_id_struggle_is_reachable_outside_assets() {
    assert_eq!(MoveId::STRUGGLE, MoveId(165));
}

#[test]
fn item_id_oran_berry_is_reachable_outside_assets() {
    assert_eq!(ItemId::ORAN_BERRY, ItemId(139));
}

#[test]
fn battle_struggle_is_the_canonical_move_identity() {
    assert_eq!(STRUGGLE, MoveId::STRUGGLE);
}
