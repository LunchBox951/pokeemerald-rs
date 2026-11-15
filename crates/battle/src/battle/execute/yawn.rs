//! Yawn's drowsiness move (`BattleScript_EffectYawn`,
//! `data/battle_scripts_1.s:2451`-`:2473`).
//!
//! The move spends no `Random()` call: its accuracy check takes
//! `Cmd_accuracycheck`'s no-calculation branch despite the move data's
//! accuracy of 100 (`src/battle_script_commands.c:1099`-`:1110`), and the
//! sleep duration is drawn only when the drowsiness expires at end of turn.
//! Substitute, Safeguard and Uproar are outside this battle model.

use assets::{AbilityId, MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

use super::{Battle, BattleEvent};

/// Whether `effect` uses [`Battle::execute_yawn_move`].
pub(super) fn is_yawn_effect(effect: MoveEffect) -> bool {
    effect == MoveEffect::YAWN
}

/// Validates move lookup, the Yawn effect and combat type; consumes no RNG.
pub(super) fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_yawn_effect)
}

/// What Yawn does to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YawnOutcome {
    /// Insomnia or Vital Spirit, checked before anything else
    /// (`battle_scripts_1.s:2455`-`:2456`; the sleep-prevention slice of
    /// `Cmd_jumpifcantmakeasleep`, `battle_script_commands.c:6837`-`:6841`).
    AbilityPrevented(AbilityId),
    /// A primary status or existing drowsiness (`Cmd_setyawn`,
    /// `battle_script_commands.c:9354`-`:9357`).
    Failed,
    Drowsy,
}

fn resolve_yawn(target: &BattlePokemon) -> YawnOutcome {
    let ability = target.ability();
    if ability == AbilityId::INSOMNIA || ability == AbilityId::VITAL_SPIRIT {
        YawnOutcome::AbilityPrevented(ability)
    } else if target.volatiles().drowsy() || !target.status1().is_healthy() {
        YawnOutcome::Failed
    } else {
        YawnOutcome::Drowsy
    }
}

impl Battle {
    /// Runs the Yawn script after the shared canceller and PP steps. Draws
    /// nothing from the RNG.
    pub(super) fn execute_yawn_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let outcome = resolve_yawn(self.battlers(attacker_is_player).1);
        match outcome {
            YawnOutcome::AbilityPrevented(ability) => {
                events.push(BattleEvent::YawnAbilityProtected {
                    by_player: attacker_is_player,
                    move_id,
                    ability,
                });
            }
            YawnOutcome::Failed => {
                events.push(BattleEvent::ButItFailed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            YawnOutcome::Drowsy => {
                let target = if attacker_is_player {
                    &mut self.enemy
                } else {
                    &mut self.player
                };
                target.volatiles_mut().set_yawn();
                events.push(BattleEvent::MadeDrowsy {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use assets::{AbilityId, MoveId, SpeciesId};

    use super::{resolve_yawn, YawnOutcome};
    use crate::battle::trainer::fixed_ivs;
    use crate::battle::{Battle, BattleEvent};
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
        BattlePokemon::new(dex, species, level, fixed_ivs(255), 0, vec![MoveId::YAWN])
            .expect("dex-resident")
    }

    #[test]
    fn the_move_alone_sets_the_counter_to_two_with_one_event() {
        let dex = Dex::new();
        let mut battle = Battle::new(
            dex.clone(),
            mon(&dex, SpeciesId::RATTATA, 50),
            mon(&dex, SpeciesId::ZIGZAGOON, 5),
            false,
            &mut ZeroRng,
        )
        .unwrap();
        let mut events = Vec::new();

        battle.execute_yawn_move(true, MoveId::YAWN, &mut events);

        assert_eq!(battle.enemy().volatiles().yawn_turns, 2);
        assert_eq!(battle.enemy().status1(), Status1::Healthy);
        assert_eq!(
            events,
            [BattleEvent::MadeDrowsy {
                by_player: true,
                move_id: MoveId::YAWN
            }]
        );
    }

    #[test]
    fn the_outcome_checks_the_ability_before_the_status_and_the_drowsiness() {
        let dex = Dex::new();
        let target = |species, status, drowsy| {
            let mut mon = mon(&dex, species, 5);
            mon.set_status1(status);
            if drowsy {
                mon.volatiles_mut().set_yawn();
            }
            mon
        };
        assert_eq!(
            resolve_yawn(&target(SpeciesId::ZIGZAGOON, Status1::Healthy, false)),
            YawnOutcome::Drowsy
        );
        for status in [Status1::Poisoned, Status1::Asleep { turns: 2 }] {
            assert_eq!(
                resolve_yawn(&target(SpeciesId::ZIGZAGOON, status, false)),
                YawnOutcome::Failed
            );
        }
        assert_eq!(
            resolve_yawn(&target(SpeciesId::ZIGZAGOON, Status1::Healthy, true)),
            YawnOutcome::Failed
        );
        for (species, ability) in [
            (SpeciesId::HOOTHOOT, AbilityId::INSOMNIA),
            (SpeciesId::MANKEY, AbilityId::VITAL_SPIRIT),
        ] {
            assert_eq!(
                resolve_yawn(&target(species, Status1::Poisoned, true)),
                YawnOutcome::AbilityPrevented(ability)
            );
        }
    }
}
