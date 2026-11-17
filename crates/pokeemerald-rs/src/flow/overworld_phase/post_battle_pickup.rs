//! The Pickup ability's item roll once a battle is won (`Cmd_pickup`,
//! `pokeemerald/src/battle_script_commands.c:9654-9726`), the last step of
//! `BattleScript_PayDayMoneyAndPickUpItems`
//! (`pokeemerald/data/battle_scripts_1.s:2940-2943`).
//!
//! Upstream runs the script from `HandleEndTurn_BattleWon` for a won wild
//! battle and, through `BattleScript_LocalBattleWonReward`, for a won local
//! trainer battle (`pokeemerald/src/battle_main.c:4983-5015`). A lost or fled
//! battle never reaches it. It follows the experience award, which is paid
//! as the opponent faints, and a trainer's prize money, and it precedes the
//! evolution scene and the Pokérus roll (`:5155-5222`), neither of which the
//! port runs after a battle.
//!
//! The roll is [`battle::roll_pickup`]; this module supplies each saved slot's
//! facts and files what is found. Every one of the six saved slots rolls,
//! whether or not it fought or has fainted, and a party with no Pickup holder
//! draws nothing from the RNG.
//!
//! # A found item the battle cannot carry is withheld
//!
//! Upstream gives the member whatever the tables name. This port's battles
//! refuse a member holding an item whose effect they do not model
//! ([`battle::ensure_held_item_playable`]): a lead that holds one disables wild
//! encounters and is refused every trainer and first battle until the player
//! takes the item off, and a reserve that holds one is silently left out of the
//! wild roster with no way to take it off yet
//! (`flow::lead_screen`, module docs). Three rare table entries are refused
//! today: King's Rock (levels 11-20 at percentile 98, 21-30 at 99), White Herb
//! (41-50 at 98, 51-60 at 99) and Leftovers (81-90 at 98, 91-100 at 99).
//! Both draws are still made, in upstream's order, but such an item is not
//! stored: the member keeps holding nothing, and the withholding is logged. The
//! check is the battle's own screen, so it ends for an item by itself once that
//! item's battle effect is ported; `withheld_items_are_exactly_those_the_battle_refuses`
//! names the three and fails then, as the reminder to drop this paragraph.

use battle::{Dex, PickupFind, PickupSlotFacts};

use crate::flow::wild_encounter::SharedRng;
use crate::party;

use super::OverworldPhase;

impl OverworldPhase {
    /// Rolls Pickup over the saved party after a won battle and files each
    /// item found. Call it once per win, with the battle's lead back in the
    /// phase and every reserve already merged into the save.
    pub(super) fn roll_pickup_after_win(&mut self) {
        let dex = Dex::new();
        let facts = self.pickup_slot_facts(&dex);
        let finds = battle::roll_pickup(&facts, &mut SharedRng::new(&mut self.rng));
        for PickupFind { slot, item } in finds {
            // Both draws are already made, so the stream is upstream's either way.
            if let Err(error) = battle::ensure_held_item_playable(item) {
                eprintln!(
                    "pickup: slot {slot} found {item:?}, but a battle cannot carry it yet \
                     ({error:?}) -- withheld, the member keeps its empty hand"
                );
                continue;
            }
            eprintln!("pickup: slot {slot} found {item:?}");
            self.file_pickup_find(slot, item);
        }
    }

    /// The facts of all six saved slots, in slot order.
    ///
    /// The resident lead's level and held item are read from its live
    /// battler, which a lone lead's battle has advanced past the saved record
    /// (experience, a consumed item); everything else about it, and every
    /// other slot, comes from the saved record.
    fn pickup_slot_facts(&self, dex: &Dex) -> Vec<Option<PickupSlotFacts>> {
        let lead = self
            .lead
            .as_ref()
            .filter(|lead| !lead.is_lent())
            .map(|lead| (lead.slot(), lead));
        self.save1
            .player_party
            .iter()
            .enumerate()
            .map(|(slot, saved)| match lead {
                // The lead's own record is its slot, saved or not yet.
                Some((lead_slot, lead)) if lead_slot == slot => {
                    party::pickup_slot_facts(dex, lead.record()).map(|facts| PickupSlotFacts {
                        held_item: lead.battler().held_item(),
                        level: lead.battler().level(),
                        ..facts
                    })
                }
                _ => party::pickup_slot_facts(dex, saved),
            })
            .collect()
    }

    /// Gives `item` to the member saved in `slot`. The resident lead takes it
    /// through its battler and is filed, so the next save does not write its
    /// old, empty hand back over the record.
    fn file_pickup_find(&mut self, slot: usize, item: assets::items::ItemId) {
        if self.owns_lead_slot(slot) {
            if let Some(lead) = self.lead_battler_mut() {
                *lead = lead.clone().with_held_item(item);
            }
            self.flush_lead_to_save();
        } else if let Err(error) =
            party::set_record_held_item(&mut self.save1.player_party[slot], item)
        {
            eprintln!("pickup: slot {slot}'s record {error} -- item not filed");
        }
    }
}

#[cfg(test)]
mod tests;
