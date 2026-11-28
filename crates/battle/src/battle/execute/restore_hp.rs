//! Recover and Slack Off (`BattleScript_EffectRestoreHp`,
//! `data/battle_scripts_1.s:672`-`:684`) with the attacker branch of
//! `Cmd_tryhealhalfhealth` (`src/battle_script_commands.c:6615`-`:6631`) and
//! the full-HP failure script (`BattleScript_AlreadyAtFullHp`,
//! `data/battle_scripts_1.s:2042`-`:2046`).
//!
//! The script has no accuracy check, so neither outcome draws from the RNG,
//! whatever the move data's accuracy says. Snatch, which would redirect the
//! move at `Cmd_attackcanceler`, is outside this battle model.

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

use super::{Battle, BattleEvent};

/// Whether `effect` uses [`Battle::execute_restore_hp_move`].
pub(super) fn is_restore_hp_effect(effect: MoveEffect) -> bool {
    effect == MoveEffect::RESTORE_HP
}

/// Validates move lookup, the restore-HP effect and combat type; consumes no
/// RNG.
pub(super) fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_restore_hp_effect)
}

/// What `Cmd_tryhealhalfhealth` decides for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreHpOutcome {
    /// Current HP equals maximum HP (`battle_script_commands.c:6627`).
    AlreadyFull,
    /// Half of maximum HP, before the cap at maximum HP.
    Heal(u32),
}

/// Half of `max_hp` rounded down, raised to one when that is zero
/// (`battle_script_commands.c:6622`-`:6624`).
const fn half_max_hp(max_hp: u32) -> u32 {
    let half = max_hp / 2;
    if half == 0 {
        1
    } else {
        half
    }
}

fn resolve_restore_hp(user: &BattlePokemon) -> RestoreHpOutcome {
    let max_hp = user.stats().max_hp;
    if user.current_hp() == max_hp {
        RestoreHpOutcome::AlreadyFull
    } else {
        RestoreHpOutcome::Heal(half_max_hp(max_hp))
    }
}

impl Battle {
    /// Runs the restore-HP script after the shared canceller and PP steps.
    /// Draws nothing from the RNG and leaves the user's status alone.
    pub(super) fn execute_restore_hp_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        match resolve_restore_hp(self.battlers(attacker_is_player).0) {
            RestoreHpOutcome::AlreadyFull => {
                events.push(BattleEvent::HpAlreadyFull {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            RestoreHpOutcome::Heal(amount) => {
                let user = self.battler_mut(attacker_is_player);
                let before = user.current_hp();
                user.heal_hp(amount);
                events.push(BattleEvent::RegainedHealth {
                    by_player: attacker_is_player,
                    move_id,
                    healed: user.current_hp().saturating_sub(before),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use assets::{MoveId, SpeciesId};

    use super::super::last_landed_from;
    use super::{half_max_hp, resolve_restore_hp, RestoreHpOutcome};
    use crate::battle::trainer::fixed_ivs;
    use crate::battle::{Battle, BattleEvent, LastLandedMove};
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::pokemon::BattlePokemon;
    use crate::status1::Status1;

    /// Draws zero, which the constructor's single seed draw needs.
    struct ZeroRng;
    impl BattleRng for ZeroRng {
        fn next_u16(&mut self) -> u16 {
            0
        }
    }

    fn mon(dex: &Dex, species: SpeciesId, level: u8) -> BattlePokemon {
        BattlePokemon::new(
            dex,
            species,
            level,
            fixed_ivs(255),
            0,
            vec![MoveId::RECOVER],
        )
        .expect("dex-resident")
    }

    #[test]
    fn half_rounds_down_and_never_falls_below_one() {
        for (max_hp, half) in [
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 2),
            (105, 52),
            (106, 53),
            (999, 499),
        ] {
            assert_eq!(half_max_hp(max_hp), half, "max HP {max_hp}");
        }
    }

    #[test]
    fn the_outcome_is_full_only_at_exactly_maximum_hp() {
        let dex = Dex::new();
        let mut user = mon(&dex, SpeciesId::RATTATA, 50);
        assert_eq!(resolve_restore_hp(&user), RestoreHpOutcome::AlreadyFull);
        user.apply_damage(1);
        assert_eq!(resolve_restore_hp(&user), RestoreHpOutcome::Heal(52));
    }

    #[test]
    fn the_move_alone_heals_the_user_without_a_draw_or_a_status_change() {
        let dex = Dex::new();
        let mut battle = Battle::new(
            dex.clone(),
            mon(&dex, SpeciesId::RATTATA, 50),
            mon(&dex, SpeciesId::ZIGZAGOON, 5),
            false,
            &mut ZeroRng,
        )
        .unwrap();
        battle.player.apply_damage(100);
        battle.player.set_status1(Status1::Poisoned);
        let enemy_hp = battle.enemy.current_hp();
        let mut events = Vec::new();

        battle.execute_restore_hp_move(true, MoveId::RECOVER, &mut events);

        assert_eq!(battle.player.current_hp(), 57);
        assert_eq!(battle.player.status1(), Status1::Poisoned);
        assert_eq!(battle.enemy.current_hp(), enemy_hp);
        assert_eq!(
            events,
            [BattleEvent::RegainedHealth {
                by_player: true,
                move_id: MoveId::RECOVER,
                healed: 52
            }]
        );
    }

    #[test]
    fn a_full_user_reports_the_failure_and_keeps_every_field() {
        let dex = Dex::new();
        let mut battle = Battle::new(
            dex.clone(),
            mon(&dex, SpeciesId::ZIGZAGOON, 5),
            mon(&dex, SpeciesId::RATTATA, 50),
            false,
            &mut ZeroRng,
        )
        .unwrap();
        battle.enemy.set_status1(Status1::Burned);
        let before = battle.enemy.clone();
        let mut events = Vec::new();

        battle.execute_restore_hp_move(false, MoveId::SLACK_OFF, &mut events);

        assert_eq!(battle.enemy.current_hp(), before.current_hp());
        assert_eq!(battle.enemy.status1(), Status1::Burned);
        assert_eq!(
            events,
            [BattleEvent::HpAlreadyFull {
                by_player: false,
                move_id: MoveId::SLACK_OFF
            }]
        );
    }

    #[test]
    fn a_saved_hp_above_maximum_is_clamped_and_reports_no_gain() {
        let dex = Dex::new();
        let mut battle = Battle::new(
            dex.clone(),
            mon(&dex, SpeciesId::RATTATA, 50),
            mon(&dex, SpeciesId::ZIGZAGOON, 5),
            false,
            &mut ZeroRng,
        )
        .unwrap();
        let stats = battle.player.stats();
        battle.player = battle
            .player
            .clone()
            .with_cached_stats(stats, stats.max_hp + 1);
        let mut events = Vec::new();

        battle.execute_restore_hp_move(true, MoveId::RECOVER, &mut events);

        assert_eq!(battle.player.current_hp(), stats.max_hp);
        assert_eq!(
            events,
            [BattleEvent::RegainedHealth {
                by_player: true,
                move_id: MoveId::RECOVER,
                healed: 0
            }]
        );
    }

    #[test]
    fn neither_outcome_marks_the_move_as_having_no_effect() {
        for event in [
            BattleEvent::RegainedHealth {
                by_player: true,
                move_id: MoveId::RECOVER,
                healed: 1,
            },
            BattleEvent::HpAlreadyFull {
                by_player: true,
                move_id: MoveId::RECOVER,
            },
        ] {
            assert_eq!(
                last_landed_from(&[event], MoveId::RECOVER),
                LastLandedMove::Move(MoveId::RECOVER)
            );
        }
    }
}
