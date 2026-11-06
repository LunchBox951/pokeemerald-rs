//! Which held items a battle can carry without ignoring their effect.
//!
//! A held item rides on its [`BattlePokemon`] from construction onward. Every
//! effect upstream hangs off it lives in `ItemBattleEffects` and the damage,
//! turn-order and experience consumers (`src/battle_util.c:3260-3340`). This
//! engine ports only Oran Berry's end-turn heal ([`Battle::apply_held_item_residual`]),
//! so a battle admits an item that has no effect to ignore, and Oran Berry.
//!
//! The screen keys on the extracted [`HoldEffect`], not on the item's bag
//! behaviour. Anything with a nonzero effect except Oran Berry is refused,
//! which includes the other berries, the herbs and the type-boosting items
//! trainers hold, and also the few effects that happen never to reach a
//! battle: a refusal of those costs a trainer fight, a silent admission of an
//! unported effect costs correctness. A trainer's bag is a separate boundary
//! ([`ensure_trainer_bag_unused`]).
//!
//! Enigma Berry carries [`HoldEffect::NONE`] in the static table, but upstream
//! reads its effect from the save block (`battle_util.c:3266-3287`), so it is
//! refused by name.

use assets::items::{HoldEffect, ItemId, ItemTable};

use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

use super::{Battle, BattleEvent};

/// Refuses an item whose in-battle effect this engine would silently drop.
///
/// # Errors
///
/// Returns [`BattleError::UnsupportedHeldItem`] for an item outside the item
/// table, Enigma Berry, or any item with a nonzero hold effect other than
/// Oran Berry.
pub fn ensure_held_item_playable(item: ItemId) -> Result<(), BattleError> {
    if item == ItemId::NONE || item == ItemId::ORAN_BERRY {
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

/// `HOLD_EFFECT_RESTORE_HP`'s `ITEMEFFECT_NORMAL` case for the end-turn item
/// pass: the HP `item` restores to a living holder at or below half its max
/// HP, its param capped at the HP the holder is missing
/// (`src/battle_util.c:3331`-`:3346`). `None` when the item does not trigger.
fn end_turn_heal(item: ItemId, current_hp: u32, max_hp: u32) -> Option<u32> {
    if item != ItemId::ORAN_BERRY || current_hp == 0 || current_hp > max_hp / 2 {
        return None;
    }
    let param = ItemTable::new().get(item).ok()?.hold_effect_param;
    Some(u32::from(param).min(max_hp - current_hp))
}

impl Battle {
    /// `ENDTURN_ITEMS1` for one battler: `ItemBattleEffects(ITEMEFFECT_NORMAL,
    /// battler, FALSE)` heals a holder of Oran Berry at or below half HP and
    /// removes the Berry, with no RNG draw
    /// (`src/battle_util.c:1498`-`:1502`, `:3331`-`:3346`;
    /// `data/battle_scripts_1.s:4356`-`:4364`). It follows the battler's
    /// `ENDTURN_ABILITIES` and precedes `ENDTURN_LEECH_SEED`
    /// (`src/battle_util.c:1442`-`:1461`).
    pub(super) fn apply_held_item_residual(
        &mut self,
        is_player: bool,
        events: &mut Vec<BattleEvent>,
    ) {
        let holder = if is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let item = holder.held_item();
        let Some(heal) = end_turn_heal(item, holder.current_hp(), holder.stats().max_hp) else {
            return;
        };
        let before = holder.current_hp();
        holder.heal_hp(heal);
        holder.consume_held_item();
        events.push(BattleEvent::ItemRestoredHealth {
            by_player: is_player,
            item,
            healed: holder.current_hp() - before,
        });
    }
}

/// Refuses a trainer whose bag holds any item: `ShouldUseItem` would consider
/// each one on every turn (`battle_ai_switch_items.c`), and trainer action
/// selection here never uses items.
///
/// # Errors
///
/// Returns [`BattleError::UnsupportedTrainerBagItem`] for the first item that
/// is not [`ItemId::NONE`].
pub fn ensure_trainer_bag_unused(bag: &[ItemId]) -> Result<(), BattleError> {
    match bag.iter().find(|item| **item != ItemId::NONE) {
        Some(item) => Err(BattleError::UnsupportedTrainerBagItem(*item)),
        None => Ok(()),
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
    fn oran_berry_is_the_only_hold_effect_item_admitted() {
        assert_eq!(ensure_held_item_playable(ItemId::ORAN_BERRY), Ok(()));
    }

    #[test]
    fn every_item_with_a_hold_effect_is_refused_by_name() {
        for item in [
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
    fn any_trainer_bag_item_is_refused_and_an_empty_bag_is_not() {
        assert_eq!(ensure_trainer_bag_unused(&[ItemId::NONE; 4]), Ok(()));
        assert_eq!(
            ensure_trainer_bag_unused(&[ItemId::NONE, ItemId::SUPER_POTION, ItemId::FULL_RESTORE]),
            Err(BattleError::UnsupportedTrainerBagItem(ItemId::SUPER_POTION))
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
            mon(SpeciesId::TREECKO, ItemId::SITRUS_BERRY),
            false,
            &mut rng,
        );
        assert_eq!(
            refused.err(),
            Some(BattleError::UnsupportedHeldItem(ItemId::SITRUS_BERRY))
        );
        assert_eq!(rng.draws(), 0);
    }

    #[test]
    fn end_turn_heal_follows_oran_berrys_threshold_param_and_cap() {
        let oran = ItemId::ORAN_BERRY;
        assert_eq!(end_turn_heal(oran, 50, 100), Some(10));
        assert_eq!(end_turn_heal(oran, 51, 100), None);
        assert_eq!(end_turn_heal(oran, 49, 99), Some(10));
        assert_eq!(end_turn_heal(oran, 50, 99), None);
        assert_eq!(end_turn_heal(oran, 8, 16), Some(8));
        assert_eq!(end_turn_heal(oran, 9, 19), Some(10));
        assert_eq!(end_turn_heal(oran, 0, 100), None, "a fainted holder");
        assert_eq!(end_turn_heal(oran, 1, 1), None, "half of one is zero");
        assert_eq!(end_turn_heal(ItemId::NONE, 1, 100), None);
        assert_eq!(end_turn_heal(ItemId::SITRUS_BERRY, 1, 100), None);
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
