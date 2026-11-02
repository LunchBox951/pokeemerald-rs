//! Dependent crates must reach canonical move, item, and species ids by named constant.

use assets::{items::ItemId, MoveId, SpeciesId};
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

#[test]
fn species_id_treecko_is_reachable_outside_assets() {
    assert_eq!(SpeciesId::TREECKO, SpeciesId(277));
}
