//! The move-end status-immunity cure (`MOVEEND_IMMUNITY_ABILITIES`).

use crate::status_immunity::{clear_condition, forbidden_condition};

use super::{Battle, BattleEvent, BattlerSide};

impl Battle {
    /// `MOVEEND_IMMUNITY_ABILITIES`: cures every active battler, player
    /// first, whose current ability forbids a condition it carries. The
    /// player's holder is scanned before the enemy's whichever battler moved,
    /// and a miss, a failure or a cancelled action reaches this pass too.
    pub(in crate::battle) fn apply_move_end_immunity(&mut self, events: &mut Vec<BattleEvent>) {
        for side in [BattlerSide::Player, BattlerSide::Enemy] {
            let by_player = side == BattlerSide::Player;
            let holder = self.battler_mut(by_player);
            if let Some(condition) = forbidden_condition(holder) {
                let ability = holder.ability();
                clear_condition(holder, condition);
                events.push(BattleEvent::AbilityCuredStatus {
                    by_player,
                    ability,
                    condition,
                });
            }
        }
    }
}
