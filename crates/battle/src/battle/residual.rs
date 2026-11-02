//! End-of-turn residual processing: the ordered per-battler effects, and the check that stops them once a side is exhausted.

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::status1::{burn_residual_damage, draws_shed_skin_cure, poison_residual_damage, Status1};
use crate::turn_order::{resolve_order, Order};
use assets::AbilityId;

use super::{Battle, BattleEvent, BattleKind, ChosenMoves};

impl Battle {
    pub(super) fn residual_effects(
        &mut self,
        chosen: ChosenMoves,
        events: &mut Vec<BattleEvent>,
        rng: &mut impl BattleRng,
    ) -> Result<(), BattleError> {
        if self.outcome.is_some() {
            return Ok(());
        }
        // `DoFieldEndTurnEffects` rebuilds `gBattlerByTurnOrder` from battler
        // identity and compares the pair afresh, at the battlers' current
        // Speed, so a Speed-stage change or a surviving tie is re-decided
        // rather than inherited from the action phase
        // (`src/battle_util.c:1199`-`:1210`).
        let priorities = self.priorities_of(chosen, true)?;
        let order = resolve_order(
            priorities.player,
            priorities.enemy,
            self.player.speed_for_turn_order(),
            self.enemy.speed_for_turn_order(),
            rng,
        );
        // `DoBattlerEndTurnEffects` walks that rebuilt order and within each battler's pass reaches
        // ENDTURN_ABILITIES (Shed Skin) before ENDTURN_POISON, and
        // ENDTURN_POISON before ENDTURN_BURN, and ENDTURN_BURN before
        // ENDTURN_CHARGE (`src/battle_util.c:1442`-`:1474`, `:1494`-`:1560`).
        let player_first = matches!(order, Order::AttackerFirst);
        for is_player in [player_first, !player_first] {
            let already_fainted = if is_player {
                self.player.is_fainted()
            } else {
                self.enemy.is_fainted()
            };
            if already_fainted {
                continue;
            }
            self.apply_shed_skin_residual(is_player, rng, events);
            self.apply_poison_residual(is_player, events);
            self.apply_burn_residual(is_player, events);
            if is_player {
                self.player.volatiles_mut().tick_charge();
            } else {
                self.enemy.volatiles_mut().tick_charge();
            }
            let now_fainted = if is_player {
                self.player.is_fainted()
            } else {
                self.enemy.is_fainted()
            };
            if now_fainted {
                self.settle_faint(is_player, events);
                if self.fainting_decides_the_battle(is_player) {
                    break;
                }
            }
        }
        Ok(())
    }

    fn fainting_decides_the_battle(&self, is_player: bool) -> bool {
        // `Cmd_checkteamslost` totals a whole party's HP
        // (`src/battle_script_commands.c:3534`-`:3577`); the player side is
        // exhausted only once its active member and every reserve have
        // fainted.
        if is_player {
            return self.usable_player_reserves() == 0;
        }
        match &self.kind {
            BattleKind::Trainer(context) => context.bench().iter().all(BattlePokemon::is_fainted),
            BattleKind::Wild | BattleKind::FirstBattle => true,
        }
    }

    /// `ABILITYEFFECT_ENDTURN`'s `ABILITY_SHED_SKIN` case: a living battler
    /// whose ability is Shed Skin draws to cure its own primary status
    /// (`pokeemerald/src/battle_util.c:2601`-`:2602`, `:2620`-`:2640`).
    fn apply_shed_skin_residual(
        &mut self,
        is_player: bool,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) {
        let battler = if is_player { &self.player } else { &self.enemy };
        if battler.is_fainted() || battler.ability() != AbilityId::SHED_SKIN {
            return;
        }
        let status = battler.status1();
        if !draws_shed_skin_cure(status, rng) {
            return;
        }
        let target = if is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        target.set_status1(Status1::Healthy);
        events.push(BattleEvent::ShedSkinCured {
            by_player: is_player,
            status,
        });
    }

    fn apply_poison_residual(&mut self, is_player: bool, events: &mut Vec<BattleEvent>) {
        let battler = if is_player { &self.player } else { &self.enemy };
        if battler.is_fainted() || !battler.status1().is_poisoned() {
            return;
        }
        let damage = poison_residual_damage(battler.stats().max_hp);
        let target = if is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let dealt = damage.min(target.current_hp());
        target.apply_damage(dealt);
        events.push(BattleEvent::HurtByPoison {
            by_player: is_player,
            damage: dealt,
        });
    }

    /// `ENDTURN_BURN`: a living burned battler loses an eighth of its max HP,
    /// at least one, with no RNG draw (`pokeemerald/src/battle_util.c:1550`-
    /// `:1560`).
    fn apply_burn_residual(&mut self, is_player: bool, events: &mut Vec<BattleEvent>) {
        let battler = if is_player { &self.player } else { &self.enemy };
        if battler.is_fainted() || !battler.status1().is_burned() {
            return;
        }
        let damage = burn_residual_damage(battler.stats().max_hp);
        let target = if is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let dealt = damage.min(target.current_hp());
        target.apply_damage(dealt);
        events.push(BattleEvent::HurtByBurn {
            by_player: is_player,
            damage: dealt,
        });
    }
}
