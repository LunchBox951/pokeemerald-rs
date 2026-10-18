//! The battle-time combat stat reads of one battler: the attacking and
//! defending stat with their stage, and the speed used for move and turn
//! ordering.
//!
//! Ability and status modifiers that upstream folds in before a stage applies
//! are applied here; modifiers that depend on the opposing battler are left to
//! the caller.

use super::{BattlePokemon, StatStage};

const PARALYSIS_SPEED_DIVISOR: u32 = 4;

impl BattlePokemon {
    /// Attacking stat and stage for the move category.
    ///
    /// A Hustle holder's, then a statused Guts holder's, raw physical Attack
    /// is each raised 150% before the stage is applied, matching upstream's
    /// `CalculateBaseDamage` (`pokeemerald/src/pokemon.c:3205-3206, :3211-3212`);
    /// special Attack is returned raw: Thick Fat and Plus/Minus depend on the
    /// other battler, so callers apply them before the stage.
    #[must_use]
    pub fn attacking_stat(&self, category: crate::damage::MoveCategory) -> (u32, StatStage) {
        match category {
            crate::damage::MoveCategory::Physical => {
                let raw_attack = crate::ability::hustle_attack(self.ability(), self.stats.attack);
                let raw_attack =
                    crate::ability::guts_attack(self.ability(), self.status1, raw_attack);
                (raw_attack, self.stages.attack)
            }
            crate::damage::MoveCategory::Special => (self.stats.sp_attack, self.stages.sp_attack),
        }
    }

    /// Defending stat and stage for the move category.
    ///
    /// A statused Marvel Scale holder's raw physical Defense is raised 150%
    /// before the stage is applied, matching upstream's
    /// `CalculateBaseDamage` (`pokeemerald/src/pokemon.c:3213-3214`); special
    /// Defense never changes.
    #[must_use]
    pub fn defending_stat(&self, category: crate::damage::MoveCategory) -> (u32, StatStage) {
        match category {
            crate::damage::MoveCategory::Physical => {
                let raw_defense = crate::ability::marvel_scale_defense(
                    self.ability(),
                    self.status1,
                    self.stats.defense,
                );
                (raw_defense, self.stages.defense)
            }
            crate::damage::MoveCategory::Special => (self.stats.sp_defense, self.stages.sp_defense),
        }
    }

    /// Speed after applying its stat stage.
    ///
    /// Weather, ability, and item modifiers are not applied here; paralysis'
    /// quarter modifier is [`BattlePokemon::speed_for_turn_order`]'s alone,
    /// since only turn ordering reads it (`battle_main.c:4650`-`:4651`).
    #[must_use]
    pub const fn effective_speed(&self) -> u32 {
        self.stages.speed.apply(self.stats.speed)
    }

    /// [`BattlePokemon::effective_speed`] with paralysis' quarter modifier
    /// applied after stage scaling, truncating independently
    /// (`speedBattler /= 4`, `battle_main.c:4650`-`:4651`,
    /// `:4684`-`:4685`). Turn order is the only reader of this modifier —
    /// see [`BattlePokemon::effective_speed`]'s own doc.
    #[must_use]
    pub const fn speed_for_turn_order(&self) -> u32 {
        let speed = self.effective_speed();
        if self.status1.is_paralysed() {
            speed / PARALYSIS_SPEED_DIVISOR
        } else {
            speed
        }
    }
}
