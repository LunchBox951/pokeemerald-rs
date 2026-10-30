//! A held item consumed in battle must not come back from the save.

use super::super::LoadedLead;
use super::common::{stored_record_with_retained_fields, EXPECTED_GROWTH_HELD_ITEM};
use assets::items::ItemId;
use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattlePokemon, BattleRng, Dex, Ivs, PlayerAction};

const WURMPLE: SpeciesId = SpeciesId(290);

/// Every draw is zero: no tie and a hit, whatever the move.
struct AllZero;

impl BattleRng for AllZero {
    fn next_u16(&mut self) -> u16 {
        0
    }
}

fn held_item_bytes(record: &engine::save::Pokemon) -> ItemId {
    let substructures = record.box_data.substructures().unwrap();
    ItemId(u16::from_le_bytes(
        substructures.growth[EXPECTED_GROWTH_HELD_ITEM]
            .try_into()
            .unwrap(),
    ))
}

#[test]
fn a_berry_consumed_in_battle_stays_consumed_after_save_and_reload() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_HELD_ITEM]
        .copy_from_slice(&ItemId::ORAN_BERRY.0.to_le_bytes());
    stored.box_data.set_substructures(&substructures);
    let mut loaded = LoadedLead::load(&dex, std::slice::from_ref(&stored)).unwrap();
    assert_eq!(loaded.battler().held_item(), ItemId::ORAN_BERRY);

    let mut lead = loaded.take_battler();
    lead.apply_damage(lead.current_hp() - lead.stats().max_hp / 2);
    let enemy =
        BattlePokemon::new(&dex, WURMPLE, 50, Ivs::default(), 0, vec![MoveId::SPLASH]).unwrap();
    let mut rng = AllZero;
    let mut battle = Battle::new(dex.clone(), lead, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::ItemRestoredHealth {
                by_player: true,
                ..
            }
        )),
        "the Berry must be eaten: {events:?}"
    );
    assert_eq!(battle.player().held_item(), ItemId::NONE);
    loaded.restore_battler(battle.player().clone());

    let saved = loaded.merge_and_save(&dex);

    assert_eq!(held_item_bytes(&saved), ItemId::NONE);
    let reloaded = LoadedLead::load(&dex, std::slice::from_ref(&saved)).unwrap();
    assert_eq!(reloaded.battler().held_item(), ItemId::NONE);
}

#[test]
fn a_record_whose_held_item_did_not_change_keeps_its_item_bytes() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let mut loaded = LoadedLead::load(&dex, std::slice::from_ref(&stored)).unwrap();
    let kept = loaded.battler().held_item();
    assert_ne!(kept, ItemId::NONE, "fixture sanity");

    let saved = loaded.merge_and_save(&dex);

    assert_eq!(held_item_bytes(&saved), kept);
    assert_eq!(saved.to_bytes(), stored.to_bytes());
}
