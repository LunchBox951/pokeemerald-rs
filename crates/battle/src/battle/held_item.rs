//! Which held items a battle can carry without ignoring their effect.
//!
//! A held item rides on its [`BattlePokemon`] from construction onward. Every
//! effect upstream hangs off it lives in `ItemBattleEffects` and the damage,
//! turn-order and experience consumers (`src/battle_util.c:3260-3340`), none of
//! which this engine ports, so a battle admits only an item that has no
//! effect to ignore.
//!
//! The screen keys on the extracted [`HoldEffect`], not on the item's bag
//! behaviour. Anything with a nonzero effect is refused, which includes the
//! berries, the herbs and the type-boosting items trainers hold, and also the
//! few effects that happen never to reach a battle: a refusal of those costs a
//! trainer fight, a silent admission of an unported effect costs correctness.
//! Enigma Berry carries [`HoldEffect::NONE`] in the static table, but upstream
//! reads its effect from the save block (`battle_util.c:3266-3287`), so it is
//! refused by name.

use assets::items::{HoldEffect, ItemId, ItemTable};

use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

/// Refuses an item whose in-battle effect this engine would silently drop.
///
/// # Errors
///
/// Returns [`BattleError::UnsupportedHeldItem`] for an item outside the item
/// table, Enigma Berry, or any item with a nonzero hold effect.
pub fn ensure_held_item_playable(item: ItemId) -> Result<(), BattleError> {
    if item == ItemId::NONE {
        return Ok(());
    }
    let effect_free = item != ItemId::ENIGMA_BERRY
        && ItemTable::new()
            .get(item)
            .is_ok_and(|data| data.hold_effect == HoldEffect::NONE);
    if effect_free {
        Ok(())
    } else {
        Err(BattleError::UnsupportedHeldItem(item))
    }
}

/// Screens the held item of each participant that can still enter the battle.
pub(super) fn ensure_participants_hold_playable_items<'a>(
    participants: impl IntoIterator<Item = &'a BattlePokemon>,
) -> Result<(), BattleError> {
    participants
        .into_iter()
        .try_for_each(|participant| ensure_held_item_playable(participant.held_item()))
}

#[cfg(test)]
mod tests {
    use assets::items::ItemId;
    use assets::{MoveId, SpeciesId};

    use super::*;
    use crate::battle::Battle;
    use crate::dex::Dex;
    use crate::pokemon::Ivs;
    use crate::script_rng::SequenceRng;

    fn mon(species: SpeciesId, item: ItemId) -> BattlePokemon {
        BattlePokemon::new(
            &Dex::new(),
            species,
            10,
            Ivs::default(),
            0,
            vec![MoveId::TACKLE],
        )
        .unwrap()
        .with_held_item(item)
    }

    #[test]
    fn an_empty_hand_and_an_effect_free_item_are_playable() {
        assert_eq!(ensure_held_item_playable(ItemId::NONE), Ok(()));
        assert_eq!(ensure_held_item_playable(ItemId::NUGGET), Ok(()));
    }

    #[test]
    fn every_item_with_a_hold_effect_is_refused_by_name() {
        for item in [
            ItemId::ORAN_BERRY,
            ItemId::SITRUS_BERRY,
            ItemId::CHESTO_BERRY,
            ItemId::WHITE_HERB,
            ItemId::SMOKE_BALL,
            ItemId::SILVER_POWDER,
            ItemId::DRAGON_FANG,
            ItemId::BLACK_BELT,
        ] {
            assert_eq!(
                ensure_held_item_playable(item),
                Err(BattleError::UnsupportedHeldItem(item)),
                "{item:?}"
            );
        }
    }

    /// Enigma Berry's table row says "no effect" but upstream reads its effect
    /// from the save block (`battle_util.c:3266-3287`).
    #[test]
    fn enigma_berry_is_refused_despite_its_empty_static_effect() {
        let static_effect = ItemTable::new()
            .get(ItemId::ENIGMA_BERRY)
            .unwrap()
            .hold_effect;
        assert_eq!(static_effect, HoldEffect::NONE, "fixture sanity");
        assert_eq!(
            ensure_held_item_playable(ItemId::ENIGMA_BERRY),
            Err(BattleError::UnsupportedHeldItem(ItemId::ENIGMA_BERRY))
        );
    }

    #[test]
    fn an_item_outside_the_table_is_refused() {
        let unknown = ItemId(u16::MAX);
        assert_eq!(
            ensure_held_item_playable(unknown),
            Err(BattleError::UnsupportedHeldItem(unknown))
        );
    }

    #[test]
    fn the_held_item_is_part_of_a_mons_identity() {
        let empty = mon(SpeciesId::MUDKIP, ItemId::NONE);
        let holding = mon(SpeciesId::MUDKIP, ItemId::NUGGET);
        assert_eq!(holding.held_item(), ItemId::NUGGET);
        assert_ne!(empty, holding);
        assert_eq!(holding.clone(), holding);
    }

    #[test]
    fn a_wild_battle_refuses_a_berry_holder_before_any_draw() {
        let mut rng = SequenceRng::new([]);
        let refused = Battle::new(
            Dex::new(),
            mon(SpeciesId::MUDKIP, ItemId::NONE),
            mon(SpeciesId::TREECKO, ItemId::ORAN_BERRY),
            false,
            &mut rng,
        );
        assert_eq!(
            refused.err(),
            Some(BattleError::UnsupportedHeldItem(ItemId::ORAN_BERRY))
        );
        assert_eq!(rng.draws(), 0);
    }

    #[test]
    fn an_effect_free_holder_starts_the_battle() {
        let mut rng = SequenceRng::new([0, 0]);
        let started = Battle::new(
            Dex::new(),
            mon(SpeciesId::MUDKIP, ItemId::NUGGET),
            mon(SpeciesId::TREECKO, ItemId::NUGGET),
            false,
            &mut rng,
        );
        assert!(started.is_ok(), "{:?}", started.err());
    }
}
